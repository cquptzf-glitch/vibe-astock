//! 情绪历史、近 5 天与明日验证。
//!
//! 口径移植自桌面版 orchestrator/src/finance/shortline/{history,weekly,verification,verify_metrics}.ts
//! (分位、两头阈值、噪声门槛、基准发生率、09:15 事后判定不变)。
//! 每个交易日的八项读数由东财涨停 / 炸板 / 跌停 / 昨日涨停池与前一交易日涨停池算出;
//! 已收盘且取全的日子写进 SQLite 缓存——东财池子只留约一个月,缓存让历史越用越长。
//! 与桌面版的差异:题材集中度按东财所属行业归;题材集中度、全市场跌停也有历史序列,所以同样给基准发生率。
use crate::{
    discovery::{self, DiscoveryError},
    emotion::{self, ChinaNow, PoolKind, Row},
    quote,
    symbol::normalize_stock_symbol,
};
use anyhow::Result;
use chrono::{DateTime, Datelike, Duration as ChronoDuration, NaiveDate, Utc, Weekday};
use futures_util::stream::{self, StreamExt};
use reqwest::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

/// 东财涨停 / 炸板 / 昨日涨停池能往回查多少个日历日(桌面版 2026-10-10 实测 29 天,留一天余量)。更早的只读缓存。
const SOURCE_RETENTION_DAYS: i64 = 28;
const MIN_SAMPLES: usize = 10;
const HIGH: f64 = 0.85;
const LOW: f64 = 0.15;
pub(crate) const BASELINE_DAYS: usize = 60;
const MIN_BASELINE_SAMPLES: usize = 12;
const TRIVIAL_BASELINE: f64 = 0.7;
const RARE_BASELINE: f64 = 0.3;
const REASON_MAX: usize = 200;
const CUTOFF_HM: u32 = 915;
const WEEKLY_DAYS: usize = 5;
const SOURCE: &str = "东财涨停 / 炸板 / 跌停 / 昨日涨停池(已收盘且取全的日子存本机缓存)";

pub(crate) struct Metric {
    pub key: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub kind: &'static str,
    pub hint: &'static str,
    /// 明日验证「变了才算变」的阈值
    pub eps: f64,
    /// 情绪历史「跟前一天比」的噪声门槛
    pub diff_eps: f64,
    pub higher_is_hotter: bool,
    /// 是否属于情绪历史的六项读数
    pub history: bool,
}

pub(crate) const METRICS: [Metric; 8] = [
    Metric {
        key: "limit_up_count",
        label: "涨停家数",
        unit: "家",
        kind: "count",
        hint: "市场整体的赚钱意愿",
        eps: 5.0,
        diff_eps: 8.0,
        higher_is_hotter: true,
        history: true,
    },
    Metric {
        key: "highest_board",
        label: "最高连板高度",
        unit: "板",
        kind: "count",
        hint: "情绪的天花板,压缩=周期见顶",
        eps: 0.5,
        diff_eps: 0.5,
        higher_is_hotter: true,
        history: true,
    },
    Metric {
        key: "promotion_1to2",
        label: "1进2 晋级率",
        unit: "",
        kind: "rate",
        hint: "最敏感的一档,走低=接力意愿收缩",
        eps: 0.05,
        diff_eps: 0.08,
        higher_is_hotter: true,
        history: true,
    },
    Metric {
        key: "money_effect_median",
        label: "赚钱效应中位数",
        unit: "%",
        kind: "pct",
        hint: "多数人的体感",
        eps: 0.5,
        diff_eps: 1.0,
        higher_is_hotter: true,
        history: true,
    },
    Metric {
        key: "broken_rate",
        label: "炸板率",
        unit: "",
        kind: "rate",
        hint: "资金分歧程度(炸板未回封占尝试数)",
        eps: 0.05,
        diff_eps: 0.08,
        higher_is_hotter: false,
        history: true,
    },
    Metric {
        key: "deep_loss_count",
        label: "跌超5%家数",
        unit: "家",
        kind: "count",
        hint: "大面的量级,判退潮先看它",
        eps: 5.0,
        diff_eps: 8.0,
        higher_is_hotter: false,
        history: true,
    },
    Metric {
        key: "theme_concentration",
        label: "头部行业集中度",
        unit: "",
        kind: "rate",
        hint: "一枝独秀还是遍地开花",
        eps: 0.05,
        diff_eps: 0.05,
        higher_is_hotter: true,
        history: false,
    },
    Metric {
        key: "market_limit_down",
        label: "全市场跌停家数",
        unit: "家",
        kind: "count",
        hint: "极端风险的直接读数",
        eps: 5.0,
        diff_eps: 5.0,
        higher_is_hotter: false,
        history: false,
    },
];

pub(crate) fn is_metric(key: &str) -> bool {
    METRICS.iter().any(|m| m.key == key)
}

pub(crate) fn metric(key: &str) -> Option<&'static Metric> {
    METRICS.iter().find(|m| m.key == key)
}

fn metric_view(m: &Metric) -> Value {
    json!({"key": m.key, "label": m.label, "unit": m.unit, "kind": m.kind, "hint": m.hint, "eps": m.eps,
        "diff_eps": m.diff_eps, "higher_is_hotter": m.higher_is_hotter, "history": m.history})
}

// ---------------------------------------------------------------- 一天的读数

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct Leader {
    pub code: String,
    pub name: String,
    pub boards: i64,
    pub sector: String,
    pub first_seal: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct DayRecord {
    pub date: String,
    pub values: BTreeMap<String, Option<f64>>,
    /// ok = 八项都有;partial = 缺几项;missing = 一项都没取到(不进统计)
    pub status: String,
    pub settled: Option<bool>,
    pub note: String,
    #[serde(default)]
    pub cached: bool,
    #[serde(default)]
    pub leaders: Vec<Leader>,
    #[serde(default)]
    pub leader_total: usize,
    /// 当天各行业涨停家数,降序
    #[serde(default)]
    pub sectors: Vec<(String, usize)>,
}

impl DayRecord {
    fn missing(date: &str, note: impl Into<String>) -> Self {
        Self {
            date: date.into(),
            values: METRICS.iter().map(|m| (m.key.to_string(), None)).collect(),
            status: "missing".into(),
            note: note.into(),
            ..Self::default()
        }
    }

    pub fn value(&self, key: &str) -> Option<f64> {
        self.values.get(key).copied().flatten()
    }
}

pub(crate) struct DayPools {
    pub zt: Result<Vec<Row>, String>,
    pub zb: Result<Vec<Row>, String>,
    pub dt: Result<Vec<Row>, String>,
    pub yzt: Result<Vec<Row>, String>,
    pub prev_zt: Result<Vec<Row>, String>,
}

fn sector_of(row: &Row) -> String {
    let s = row.sector.trim();
    if s.is_empty() {
        "未分类".into()
    } else {
        s.into()
    }
}

/// 八项读数。每项单独可缺:取不到的写 None 并在 note 里说原因,不补零。
pub(crate) fn compute_day(date: &str, settled: Option<bool>, pools: &DayPools) -> DayRecord {
    let zt = match &pools.zt {
        Ok(rows) if !rows.is_empty() => rows,
        Ok(_) => {
            return DayRecord {
                settled,
                ..DayRecord::missing(date, "涨停池为空(非交易日或数据还没出)")
            }
        }
        Err(e) => {
            return DayRecord {
                settled,
                ..DayRecord::missing(date, e.clone())
            }
        }
    };
    let mut values: BTreeMap<String, Option<f64>> = BTreeMap::new();
    let mut notes: Vec<String> = Vec::new();
    let highest = zt.iter().map(|r| r.boards.max(1)).max().unwrap_or(1);
    values.insert("limit_up_count".into(), Some(zt.len() as f64));
    values.insert("highest_board".into(), Some(highest as f64));
    values.insert(
        "broken_rate".into(),
        match &pools.zb {
            Ok(zb) => Some(emotion::r3(zb.len() as f64 / (zb.len() + zt.len()) as f64)),
            Err(e) => {
                notes.push(e.clone());
                None
            }
        },
    );
    let (money, deep) = match &pools.yzt {
        Ok(rows) => {
            let rets: Vec<f64> = rows.iter().filter_map(|r| r.ret).collect();
            if rets.is_empty() {
                notes.push("昨日涨停表现为空".into());
                (None, None)
            } else {
                (
                    Some(emotion::r2(emotion::median(&rets))),
                    Some(rets.iter().filter(|v| **v <= -5.0).count() as f64),
                )
            }
        }
        Err(e) => {
            notes.push(e.clone());
            (None, None)
        }
    };
    values.insert("money_effect_median".into(), money);
    values.insert("deep_loss_count".into(), deep);
    let today: HashSet<&str> = zt.iter().map(|r| r.code.as_str()).collect();
    values.insert(
        "promotion_1to2".into(),
        match &pools.prev_zt {
            Ok(prev) => {
                let firsts: Vec<&Row> = prev.iter().filter(|r| r.boards <= 1).collect();
                if firsts.is_empty() {
                    notes.push("前一交易日没有首板".into());
                    None
                } else {
                    Some(emotion::r3(
                        firsts
                            .iter()
                            .filter(|r| today.contains(r.code.as_str()))
                            .count() as f64
                            / firsts.len() as f64,
                    ))
                }
            }
            Err(e) => {
                notes.push(e.clone());
                None
            }
        },
    );
    let mut counts: HashMap<String, usize> = HashMap::new();
    for row in zt {
        *counts.entry(sector_of(row)).or_default() += 1;
    }
    let mut sectors: Vec<(String, usize)> = counts.into_iter().collect();
    sectors.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    values.insert(
        "theme_concentration".into(),
        sectors
            .first()
            .map(|(_, n)| emotion::r3(*n as f64 / zt.len() as f64)),
    );
    values.insert(
        "market_limit_down".into(),
        match &pools.dt {
            Ok(dt) => Some(dt.len() as f64),
            Err(e) => {
                notes.push(e.clone());
                None
            }
        },
    );
    let mut tops: Vec<&Row> = zt.iter().filter(|r| r.boards.max(1) == highest).collect();
    tops.sort_by(|a, b| {
        let key = |r: &Row| {
            if r.first_seal.is_empty() {
                "999999".to_string()
            } else {
                r.first_seal.clone()
            }
        };
        key(a).cmp(&key(b))
    });
    let leaders = tops
        .iter()
        .take(5)
        .map(|r| Leader {
            code: r.code.clone(),
            name: r.name.clone(),
            boards: r.boards.max(1),
            sector: r.sector.clone(),
            first_seal: if r.first_seal.len() == 6 {
                format!("{}:{}", &r.first_seal[0..2], &r.first_seal[2..4])
            } else {
                String::new()
            },
        })
        .collect();
    let lacks: Vec<&str> = METRICS
        .iter()
        .filter(|m| values.get(m.key).copied().flatten().is_none())
        .map(|m| m.label)
        .collect();
    if !lacks.is_empty() {
        notes.insert(0, format!("缺 {}", lacks.join("、")));
    }
    notes.dedup();
    DayRecord {
        date: date.into(),
        values,
        status: if lacks.is_empty() {
            "ok".into()
        } else {
            "partial".into()
        },
        settled,
        note: notes.join(";"),
        cached: false,
        leaders,
        leader_total: tops.len(),
        sectors,
    }
}

// ---------------------------------------------------------------- 取数与缓存

fn cache_load(db: &Path, dates: &[String]) -> Result<HashMap<String, DayRecord>> {
    let conn = Connection::open(db)?;
    let mut stmt = conn.prepare("SELECT payload FROM mobile_shortline_days WHERE date = ?1")?;
    let mut out = HashMap::new();
    for date in dates {
        let payload: Option<String> = stmt.query_row([date], |row| row.get(0)).ok();
        if let Some(record) = payload.and_then(|p| serde_json::from_str::<DayRecord>(&p).ok()) {
            if record.date == *date {
                out.insert(
                    date.clone(),
                    DayRecord {
                        cached: true,
                        ..record
                    },
                );
            }
        }
    }
    Ok(out)
}

fn cache_store(db: &Path, records: &[&DayRecord]) -> Result<()> {
    let conn = Connection::open(db)?;
    let now = Utc::now().to_rfc3339();
    for record in records {
        conn.execute(
            "INSERT INTO mobile_shortline_days(date, payload, fetched_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(date) DO UPDATE SET payload = excluded.payload, fetched_at = excluded.fetched_at",
            params![record.date, serde_json::to_string(record)?, now],
        )?;
    }
    Ok(())
}

pub(crate) fn retention_floor(now: ChinaNow) -> String {
    (now.0 - ChronoDuration::days(SOURCE_RETENTION_DAYS))
        .format("%Y-%m-%d")
        .to_string()
}

async fn fetch_days(
    client: &Client,
    dates: &[String],
    calendar: &[String],
    now: ChinaNow,
) -> Vec<DayRecord> {
    if dates.is_empty() {
        return Vec::new();
    }
    let prev_of = |d: &str| calendar.iter().rev().find(|x| x.as_str() < d).cloned();
    let mut jobs: Vec<(PoolKind, String)> = Vec::new();
    let mut zt_dates: HashSet<String> = HashSet::new();
    for d in dates {
        for kind in [PoolKind::Broken, PoolKind::LimitDown, PoolKind::Yesterday] {
            jobs.push((kind, d.clone()));
        }
        zt_dates.insert(d.clone());
        if let Some(p) = prev_of(d) {
            zt_dates.insert(p);
        }
    }
    jobs.extend(zt_dates.into_iter().map(|d| (PoolKind::LimitUp, d)));
    type PoolFetch = ((PoolKind, String), Result<Vec<Row>, String>);
    let results: Vec<PoolFetch> = stream::iter(jobs)
        .map(|(kind, date)| async move {
            let compact = date.replace('-', "");
            let result = emotion::fetch_pool(client, kind, &compact)
                .await
                .map_err(|e| emotion::error_message(kind.label(), &e));
            ((kind, date), result)
        })
        .buffer_unordered(6)
        .collect()
        .await;
    let mut by: HashMap<(u8, String), Result<Vec<Row>, String>> = HashMap::new();
    let tag = |k: PoolKind| match k {
        PoolKind::LimitUp => 0u8,
        PoolKind::Broken => 1,
        PoolKind::LimitDown => 2,
        PoolKind::Yesterday => 3,
    };
    for ((kind, date), result) in results {
        by.insert((tag(kind), date), result);
    }
    let take = |by: &HashMap<(u8, String), Result<Vec<Row>, String>>, k: u8, d: &str| {
        by.get(&(k, d.to_string()))
            .cloned()
            .unwrap_or_else(|| Err("未请求".into()))
    };
    dates
        .iter()
        .map(|d| {
            let prev_zt = match prev_of(d) {
                Some(p) => take(&by, 0, &p).and_then(|rows| {
                    if rows.is_empty() {
                        Err("前一交易日涨停池为空".into())
                    } else {
                        Ok(rows)
                    }
                }),
                None => Err("取不到前一交易日".into()),
            };
            let pools = DayPools {
                zt: take(&by, 0, d),
                zb: take(&by, 1, d),
                dt: take(&by, 2, d),
                yzt: take(&by, 3, d),
                prev_zt,
            };
            compute_day(d, Some(emotion::is_settled(d, calendar, now)), &pools)
        })
        .collect()
}

/// 截止 end(含)的最近 days 个交易日读数,升序;取不到的日子也列出来(值为 None)。`refresh_end` 只重取 end。
pub(crate) async fn series(
    client: &Client,
    db: &Path,
    calendar: &[String],
    end: &str,
    days: usize,
    refresh_end: bool,
    now: ChinaNow,
) -> Result<(Vec<DayRecord>, Vec<String>)> {
    let window: Vec<String> = {
        let upto: Vec<&String> = calendar.iter().filter(|d| d.as_str() <= end).collect();
        upto[upto.len().saturating_sub(days)..]
            .iter()
            .map(|d| (*d).clone())
            .collect()
    };
    let mut cached = cache_load(db, &window)?;
    if refresh_end {
        cached.remove(end);
    }
    let floor = retention_floor(now);
    let need: Vec<String> = window
        .iter()
        .filter(|d| !cached.contains_key(*d) && d.as_str() >= floor.as_str())
        .cloned()
        .collect();
    let fetched = fetch_days(client, &need, calendar, now).await;
    let store: Vec<&DayRecord> = fetched
        .iter()
        .filter(|r| r.settled == Some(true) && r.status == "ok")
        .collect();
    // 缓存写失败不影响本次结果;下次会重取。
    let _ = cache_store(db, &store);
    let mut by: HashMap<String, DayRecord> = cached;
    for record in fetched {
        by.insert(record.date.clone(), record);
    }
    let series: Vec<DayRecord> = window
        .iter()
        .map(|d| {
            by.remove(d)
                .unwrap_or_else(|| DayRecord::missing(d, "超过东财保留期(约一个月),当时也没缓存"))
        })
        .collect();
    let mut gaps = Vec::new();
    let older = series
        .iter()
        .filter(|s| s.status == "missing" && s.date != end)
        .count();
    if older > 0 {
        gaps.push(format!(
            "{older} 个交易日没取到读数(东财池子只留约一个月,更早又没缓存过的日子取不到),不进统计"
        ));
    }
    if let Some(last) = series.last().filter(|s| s.status == "missing") {
        gaps.insert(0, format!("{} 的读数没取到({})", last.date, last.note));
    }
    Ok((series, gaps))
}

fn days_param(path: &str, default: usize, min: usize, max: usize) -> Result<usize, DiscoveryError> {
    discovery::integer(path, "days", default, min, max)
}

fn refresh_param(path: &str) -> Result<bool, DiscoveryError> {
    match discovery::parameter(path, "refresh").as_deref() {
        None => Ok(false),
        Some("1") => Ok(true),
        Some(_) => Err(DiscoveryError::BadRequest("refresh 只能是 1".into())),
    }
}

async fn resolve(
    client: &Client,
    path: &str,
) -> Result<(Vec<String>, String, ChinaNow), DiscoveryError> {
    let requested = emotion::date_parameter(path)?;
    let now = emotion::now_china();
    let calendar = emotion::trade_dates(client).await?;
    let (date, _) = emotion::review_day(&calendar, requested.as_deref(), now)?;
    Ok((calendar, date, now))
}

// ---------------------------------------------------------------- 情绪历史

fn percentile(value: Option<f64>, hist: &[f64]) -> Option<f64> {
    let value = value?;
    if hist.len() < MIN_SAMPLES {
        return None;
    }
    Some(emotion::r3(
        hist.iter().filter(|v| **v <= value).count() as f64 / hist.len() as f64,
    ))
}

/// 复盘那天各项 + 在之前那些天里的分位 + 两头标记。`rows` 升序、只含有数的日子,最后一行应是那天。
pub(crate) fn context_for(date: &str, rows: &[&DayRecord]) -> Value {
    let Some(today) = rows.last() else {
        return json!({"available": false, "reason": "历史读数一天都没有"});
    };
    if today.date != date {
        return json!({"available": false, "reason": format!("{date} 的读数没取到")});
    }
    let past = &rows[..rows.len() - 1];
    let items: Vec<Value> = METRICS.iter().filter(|m| m.history).map(|m| {
        let cur = today.value(m.key);
        let hist: Vec<f64> = past.iter().filter_map(|r| r.value(m.key)).collect();
        let pct = percentile(cur, &hist);
        let extreme = pct.and_then(|p| {
            if p >= HIGH { Some(if m.higher_is_hotter { "偏热" } else { "偏冷" }) }
            else if p <= LOW { Some(if m.higher_is_hotter { "偏冷" } else { "偏热" }) }
            else { None }
        });
        json!({
            "key": m.key, "label": m.label, "unit": m.unit, "kind": m.kind, "higher_is_hotter": m.higher_is_hotter,
            "value": cur, "percentile": pct, "extreme": extreme,
            "hist_min": hist.iter().copied().reduce(f64::min), "hist_max": hist.iter().copied().reduce(f64::max),
            "hist_median": if hist.is_empty() { None } else { Some(emotion::r2(emotion::median(&hist))) },
        })
    }).collect();
    let extreme_count = items.iter().filter(|i| !i["extreme"].is_null()).count();
    json!({"available": true, "date": date, "sample_days": past.len(), "min_samples": MIN_SAMPLES, "items": items, "extreme_count": extreme_count})
}

/// 跟上一个有数的交易日比,只列超过噪声门槛的变化;按「变化 ÷ 门槛」从大到小排。
pub(crate) fn diff_for(date: &str, rows: &[&DayRecord]) -> Value {
    if rows.len() < 2 {
        return json!({"available": false, "reason": "历史不足两天,没法比较"});
    }
    let (prev, today) = (rows[rows.len() - 2], rows[rows.len() - 1]);
    if today.date != date {
        return json!({"available": false, "reason": format!("{date} 的读数没取到")});
    }
    let mut changes: Vec<(f64, Value)> = Vec::new();
    let mut compared = 0;
    for m in METRICS.iter().filter(|m| m.history) {
        let (Some(a), Some(b)) = (prev.value(m.key), today.value(m.key)) else {
            continue;
        };
        compared += 1;
        let delta = b - a;
        // 浮点容差:正好等于门槛也算噪声。
        if delta.abs() <= m.diff_eps + 1e-9 {
            continue;
        }
        changes.push((
            delta.abs() / m.diff_eps.max(1e-9),
            json!({
                "key": m.key, "label": m.label, "unit": m.unit, "kind": m.kind, "prev": a, "cur": b,
                "delta": emotion::r2(delta), "hotter": (delta > 0.0) == m.higher_is_hotter,
            }),
        ));
    }
    changes.sort_by(|a, b| b.0.total_cmp(&a.0));
    let n = changes.len();
    json!({"available": true, "date": date, "prev_date": prev.date, "changes": changes.into_iter().map(|c| c.1).collect::<Vec<_>>(), "unchanged_count": compared - n})
}

fn day_view(r: &DayRecord) -> Value {
    json!({"date": r.date, "values": r.values, "status": r.status, "settled": r.settled, "note": r.note, "cached": r.cached})
}

pub async fn history(client: &Client, db: &Path, path: &str) -> Result<Value, DiscoveryError> {
    let days = days_param(path, 30, 5, 60)?;
    let refresh = refresh_param(path)?;
    let (calendar, date, now) = resolve(client, path).await?;
    let (series, gaps) = series(client, db, &calendar, &date, days, refresh, now).await?;
    let rows: Vec<&DayRecord> = series.iter().filter(|r| r.status != "missing").collect();
    Ok(json!({
        "status": if series.last().is_some_and(|r| r.status == "missing") { "partial" } else { "ok" },
        "date": date, "days": days,
        "readings": METRICS.iter().filter(|m| m.history).map(metric_view).collect::<Vec<_>>(),
        "series": series.iter().map(day_view).collect::<Vec<_>>(),
        "context": context_for(&date, &rows),
        "diff": diff_for(&date, &rows),
        "gaps": gaps, "source": SOURCE, "generated_at": Utc::now().to_rfc3339(),
    }))
}

// ---------------------------------------------------------------- 近 5 天

/// 上榜那天收盘起到 end 的每日累计收益(%)。上榜那天没有收盘价 = 算不了。
pub(crate) fn followthrough(bars: &[(String, f64)], appear: &str, end: &str) -> Vec<(String, f64)> {
    let Some(base) = bars
        .iter()
        .find(|b| b.0 == appear)
        .map(|b| b.1)
        .filter(|v| *v > 0.0)
    else {
        return Vec::new();
    };
    bars.iter()
        .filter(|b| b.0.as_str() >= appear && b.0.as_str() <= end)
        .map(|b| (b.0.clone(), emotion::r2((b.1 / base - 1.0) * 100.0)))
        .collect()
}

/// (区间最高累计收益, 最新收盘距那个最高点的跌幅%),都按收盘价;不到两个点不算。
pub(crate) fn peak_drawdown(points: &[(String, f64)]) -> (Option<f64>, Option<f64>) {
    if points.len() < 2 {
        return (None, None);
    }
    let peak = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let cur = points.last().map(|p| p.1).unwrap_or(0.0);
    if 1.0 + peak / 100.0 <= 0.0 {
        return (Some(emotion::r2(peak)), None);
    }
    let dd = emotion::r2((((1.0 + cur / 100.0) / (1.0 + peak / 100.0)) - 1.0) * 100.0).min(0.0);
    (
        Some(emotion::r2(peak)),
        Some(if dd == 0.0 { 0.0 } else { dd }),
    )
}

/// 行业轮动矩阵:每个行业每天的涨停家数与名次;取不到的日子是 None(不是 0)。
pub(crate) fn rotation(days: &[DayRecord]) -> Value {
    let mut names: Vec<String> = Vec::new();
    for d in days {
        for (s, _) in &d.sectors {
            if !names.contains(s) {
                names.push(s.clone());
            }
        }
    }
    let last = days.len().saturating_sub(1);
    let mut themes: Vec<Value> = names.iter().map(|name| {
        let series: Vec<Option<usize>> = days.iter().map(|d| {
            if d.status == "missing" { None } else { Some(d.sectors.iter().find(|(s, _)| s == name).map(|(_, n)| *n).unwrap_or(0)) }
        }).collect();
        let ranks: Vec<Option<usize>> = days.iter().map(|d| d.sectors.iter().position(|(s, _)| s == name).map(|i| i + 1)).collect();
        let today = series.get(last).copied().flatten().unwrap_or(0);
        let total: usize = series.iter().flatten().sum();
        let active = series.iter().filter(|v| v.is_some_and(|n| n > 0)).count();
        let top3 = ranks.iter().filter(|r| r.is_some_and(|r| r <= 3)).count();
        let mut streak = 0;
        let mut streak_partial = false;
        for v in series.iter().rev() {
            match v {
                Some(n) if *n > 0 => streak += 1,
                None => { streak_partial = true; break; }
                _ => break,
            }
        }
        let status = if today > 0 {
            if streak >= 2 {
                format!("连续{streak}天")
            } else if let Some(prev) = series[..last].iter().rposition(|v| v.is_some_and(|n| n > 0)) {
                format!("隔{}天回流", last - prev - 1)
            } else {
                "首次出现".to_string()
            }
        } else if last > 0 && series[last - 1].is_some_and(|n| n > 0) {
            "今天退下去了".to_string()
        } else {
            "今天没有涨停".to_string()
        };
        json!({"sector": name, "series": series, "ranks": ranks, "today": today, "total": total, "active_days": active,
            "top3_days": top3, "streak": streak, "streak_partial": streak_partial, "status": status})
    }).collect();
    themes.sort_by(|a, b| {
        b["today"]
            .as_u64()
            .cmp(&a["today"].as_u64())
            .then(b["total"].as_u64().cmp(&a["total"].as_u64()))
    });
    themes.truncate(15);
    json!({"dates": days.iter().map(|d| d.date.clone()).collect::<Vec<_>>(), "themes": themes,
        "missing_days": days.iter().filter(|d| d.status == "missing").map(|d| d.date.clone()).collect::<Vec<_>>()})
}

pub async fn weekly(client: &Client, db: &Path, path: &str) -> Result<Value, DiscoveryError> {
    let refresh = refresh_param(path)?;
    let (calendar, date, now) = resolve(client, path).await?;
    let (series, mut gaps) =
        series(client, db, &calendar, &date, WEEKLY_DAYS, refresh, now).await?;
    let mut firsts: Vec<(Leader, String)> = Vec::new();
    let mut seen = HashSet::new();
    for d in &series {
        for l in &d.leaders {
            if seen.insert(l.code.clone()) {
                firsts.push((l.clone(), d.date.clone()));
            }
        }
    }
    let current: HashSet<String> = series
        .last()
        .map(|d| d.leaders.iter().map(|l| l.code.clone()).collect())
        .unwrap_or_default();
    let end = date.clone();
    let lineage: Vec<Value> = stream::iter(firsts.into_iter().enumerate())
        .map(|(index, (leader, appear))| {
            let end = end.clone();
            let is_top = current.contains(&leader.code);
            async move {
                let base = json!({"code": leader.code, "name": leader.name, "sector": leader.sector, "appear_date": appear,
                    "boards_then": leader.boards, "is_current_top": is_top});
                let points = match normalize_stock_symbol(&leader.code) {
                    Ok(key) => match quote::fetch_kline(client, &key, 40).await {
                        Ok(value) => {
                            let bars: Vec<(String, f64)> = value["extra"]["rows"].as_array().into_iter().flatten()
                                .filter_map(|r| Some((r.get(0)?.as_str()?.to_string(), r.get(2)?.as_f64()?))).collect();
                            Ok(followthrough(&bars, &appear, &end))
                        }
                        Err(e) => Err(emotion::error_message("后续收盘行情", &e)),
                    },
                    Err(_) => Err("代码无法识别".to_string()),
                };
                let mut item = base;
                match points {
                    Ok(points) if !points.is_empty() => {
                        let (peak, dd) = peak_drawdown(&points);
                        item["cum_return_since"] = json!(points.last().map(|p| p.1));
                        item["series"] = json!(points.iter().map(|p| json!({"date": p.0, "cum_ret": p.1})).collect::<Vec<_>>());
                        item["peak_cum_ret"] = json!(peak);
                        item["drawdown_from_peak"] = json!(dd);
                        item["series_warning"] = json!("");
                    }
                    Ok(_) => { item["series"] = json!([]); item["series_warning"] = json!("上榜那天的收盘价没取到,算不了走势"); }
                    Err(e) => { item["series"] = json!([]); item["series_warning"] = json!(e); }
                }
                (index, item)
            }
        })
        .buffer_unordered(4)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .fold(Vec::new(), |mut acc, (index, item)| { acc.push((index, item)); acc })
        .into_iter()
        .map(|(_, item)| item)
        .collect();
    let mut lineage = lineage;
    lineage.sort_by(|a, b| {
        a["appear_date"]
            .as_str()
            .cmp(&b["appear_date"].as_str())
            .then(b["boards_then"].as_i64().cmp(&a["boards_then"].as_i64()))
    });
    if series.last().is_some_and(|d| d.settled != Some(true)) {
        gaps.push(format!("{date} 还没收盘,最后一天的读数和走势收盘前还会变"));
    }
    Ok(json!({
        "status": if series.iter().any(|d| d.status == "missing") { "partial" } else { "ok" },
        "date": date,
        "days": series.iter().map(|d| json!({
            "date": d.date, "limit_up": d.value("limit_up_count"), "highest_board": d.value("highest_board"),
            "broken_rate": d.value("broken_rate"), "money_effect": d.value("money_effect_median"), "promotion_1to2": d.value("promotion_1to2"),
            "leaders": d.leaders, "leader_total": d.leader_total, "settled": d.settled, "status": d.status, "note": d.note,
        })).collect::<Vec<_>>(),
        "rotation": rotation(&series),
        "lineage": lineage,
        "gaps": gaps, "source": SOURCE, "generated_at": Utc::now().to_rfc3339(),
        "note": "行业按东财所属行业归;桌面版题材轮动用同花顺涨停原因。走势按前复权收盘价,从上榜那天收盘算起。",
    }))
}

// ---------------------------------------------------------------- 明日验证

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item {
    pub id: i64,
    pub date: String,
    pub metric: String,
    pub direction: String,
    pub reason: String,
    pub created_at: String,
    pub updated_at: String,
}

pub(crate) fn direction_label(d: &str) -> &'static str {
    match d {
        "up" => "上升",
        "down" => "下降",
        _ => "持平",
    }
}

pub(crate) fn actual_direction(
    cur: Option<f64>,
    prev: Option<f64>,
    eps: f64,
) -> Option<&'static str> {
    let diff = cur? - prev?;
    if diff.abs() <= eps + 1e-9 {
        Some("flat")
    } else if diff > 0.0 {
        Some("up")
    } else {
        Some("down")
    }
}

/// 某个读数各方向在 rows(升序)里逐日 d−1 → d 的发生频率。跨过取不到的日子不配对。
pub(crate) fn baseline(m: &Metric, rows: &[&DayRecord]) -> Value {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut n = 0;
    for pair in rows.windows(2) {
        if let Some(d) = actual_direction(pair[1].value(m.key), pair[0].value(m.key), m.eps) {
            *counts.entry(d).or_default() += 1;
            n += 1;
        }
    }
    if n < MIN_BASELINE_SAMPLES {
        return json!({"available": false, "samples": n, "reason": format!("只有 {n} 对可比日,样本太少,基准发生率会误导")});
    }
    let rate = |k: &str| emotion::r3(*counts.get(k).unwrap_or(&0) as f64 / n as f64);
    json!({"available": true, "samples": n, "rates": {"up": rate("up"), "down": rate("down"), "flat": rate("flat")}})
}

fn baseline_note(rate: Option<f64>, m: &Metric) -> String {
    match rate {
        None => String::new(),
        Some(r) if r <= 1e-9 => format!(
            "历史上一次都没出现过这个方向,这条几乎不可能被验证成立({}的「持平」要求变化不超过 {})",
            m.label, m.eps
        ),
        Some(r) if r >= TRIVIAL_BASELINE => "这个方向历史上本来就常发生,判对了信息量低".into(),
        Some(r) if r <= RARE_BASELINE => "少见方向,判对一次含量高".into(),
        _ => String::new(),
    }
}

fn with_baseline(item: &mut Value, b: &Value, direction: &str, m: &Metric) {
    let rate = b["rates"][direction]
        .as_f64()
        .filter(|_| b["available"] == true);
    item["baseline"] = json!(rate);
    item["baseline_samples"] = if b["available"] == true {
        b["samples"].clone()
    } else {
        Value::Null
    };
    item["baseline_reason"] = if b["available"] == true {
        Value::Null
    } else {
        b["reason"].clone()
    };
    item["baseline_note"] = json!(baseline_note(rate, m));
}

/// 下一场 09:15 开盘竞价以后才写或改的条件(按最后一次修改算)。时间读不出来也按事后。
pub(crate) fn is_late(updated_at: &str, next_day: &str) -> bool {
    let Ok(at) = DateTime::parse_from_rfc3339(updated_at) else {
        return true;
    };
    let Ok(cutoff) = DateTime::parse_from_rfc3339(&format!(
        "{next_day}T{:02}:{:02}:00+08:00",
        CUTOFF_HM / 100,
        CUTOFF_HM % 100
    )) else {
        return true;
    };
    at >= cutoff
}

pub(crate) fn verify_items(
    items: &[Item],
    prev: &DayRecord,
    cur: &DayRecord,
    next_day: &str,
    rows: &[&DayRecord],
) -> Vec<Value> {
    items.iter().filter_map(|it| {
        let m = metric(&it.metric)?;
        let (p, c) = (prev.value(m.key), cur.value(m.key));
        let actual = actual_direction(c, p, m.eps);
        let verified = actual.map(|a| a == it.direction);
        let mut out = json!({
            "id": it.id, "metric": m.key, "label": m.label, "unit": m.unit, "kind": m.kind, "eps": m.eps, "higher_is_hotter": m.higher_is_hotter,
            "direction": it.direction, "direction_label": direction_label(&it.direction), "reason": it.reason,
            "prev_value": p, "cur_value": c, "actual": actual, "actual_label": actual.map(direction_label),
            "verified": verified, "late": is_late(&it.updated_at, next_day), "written_at": it.updated_at,
        });
        with_baseline(&mut out, &baseline(m, rows), &it.direction, m);
        out["edge"] = match (verified, out["baseline"].as_f64()) {
            (Some(v), Some(b)) => json!(emotion::r3(if v { 1.0 } else { 0.0 } - b)),
            _ => Value::Null,
        };
        Some(out)
    }).collect()
}

/// 汇总。分母只算判定得了的条目;超额只在有基准的那批上算(实际命中率 − 按历史常态随口猜的命中率)。
pub(crate) fn summarize(results: &[&Value]) -> Value {
    let decided: Vec<&&Value> = results
        .iter()
        .filter(|r| r["verified"].is_boolean())
        .collect();
    let hit = decided.iter().filter(|r| r["verified"] == true).count();
    let with_b: Vec<&&&Value> = decided
        .iter()
        .filter(|r| r["baseline"].is_number())
        .collect();
    let expected = if with_b.is_empty() {
        None
    } else {
        Some(
            with_b
                .iter()
                .map(|r| r["baseline"].as_f64().unwrap_or(0.0))
                .sum::<f64>()
                / with_b.len() as f64,
        )
    };
    let hit_b = if with_b.is_empty() {
        None
    } else {
        Some(with_b.iter().filter(|r| r["verified"] == true).count() as f64 / with_b.len() as f64)
    };
    json!({
        "total": results.len(), "decided": decided.len(), "hit": hit,
        "rate": if decided.is_empty() { None } else { Some(emotion::r3(hit as f64 / decided.len() as f64)) },
        "undecidable": results.len() - decided.len(), "baseline_covered": with_b.len(),
        "expected_rate": expected.map(emotion::r3),
        "edge": match (expected, hit_b) { (Some(e), Some(h)) => Some(emotion::r3(h - e)), _ => None },
    })
}

pub(crate) fn load_items(db: &Path) -> Result<Vec<Item>> {
    let conn = Connection::open(db)?;
    let mut stmt = conn.prepare("SELECT id, date, metric, direction, reason, created_at, updated_at FROM mobile_verifications ORDER BY date, id")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Item {
                id: row.get(0)?,
                date: row.get(1)?,
                metric: row.get(2)?,
                direction: row.get(3)?,
                reason: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let order = |k: &str| METRICS.iter().position(|m| m.key == k).unwrap_or(99);
    let mut rows: Vec<Item> = rows.into_iter().filter(|r| is_metric(&r.metric)).collect();
    rows.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then(order(&a.metric).cmp(&order(&b.metric)))
    });
    Ok(rows)
}

/// 被检验的那一场:日历里 date 之后的第一个交易日。
pub(crate) fn next_session(calendar: &[String], date: &str) -> Option<String> {
    calendar.iter().find(|d| d.as_str() > date).cloned()
}

fn now_hm_after(now: ChinaNow, day: &str) -> bool {
    let today = now.0.format("%Y-%m-%d").to_string();
    today.as_str() > day || (today == day && now.1 >= CUTOFF_HM)
}

/// 纯计算部分:给定窗口读数和全部条件,算对账、计划、战绩。
pub(crate) fn verify_view(
    date: &str,
    series: &[DayRecord],
    items: &[Item],
    next: Option<&str>,
    now: ChinaNow,
) -> Value {
    let mut by_date: HashMap<&str, Vec<Item>> = HashMap::new();
    for it in items {
        by_date
            .entry(it.date.as_str())
            .or_default()
            .push(it.clone());
    }
    let mut check = json!({"available": false, "reason": "列不出前一个交易日"});
    let mut days = Vec::new();
    let mut counted: Vec<Value> = Vec::new();
    let (mut excluded_late, mut excluded_provisional) = (0, 0);
    for i in 0..series.len().saturating_sub(1) {
        let (set, next_day) = (&series[i], &series[i + 1]);
        let rows: Vec<&DayRecord> = series[..=i]
            .iter()
            .filter(|r| r.status != "missing")
            .collect();
        let list = by_date.get(set.date.as_str()).cloned().unwrap_or_default();
        let results = verify_items(&list, set, next_day, &next_day.date, &rows);
        let provisional = next_day.settled != Some(true);
        if next_day.date == date {
            let refs: Vec<&Value> = results.iter().collect();
            check = json!({"available": true, "set_date": set.date, "date": date, "provisional": provisional, "results": results, "summary": summarize(&refs)});
        }
        if results.is_empty() {
            continue;
        }
        let keep: Vec<&Value> = results
            .iter()
            .filter(|r| r["late"] == false && !provisional)
            .collect();
        excluded_late += results.iter().filter(|r| r["late"] == true).count();
        if provisional {
            excluded_provisional += results.iter().filter(|r| r["late"] == false).count();
        }
        let s = summarize(&keep);
        days.push(json!({"set_date": set.date, "date": next_day.date, "total": results.len(), "decided": s["decided"], "hit": s["hit"], "excluded": results.len() - keep.len()}));
        counted.extend(keep.into_iter().cloned());
    }
    let last = series.last();
    let rows_all: Vec<&DayRecord> = series.iter().filter(|r| r.status != "missing").collect();
    let plan_items: Vec<Value> = by_date.get(date).cloned().unwrap_or_default().iter().filter_map(|it| {
        let m = metric(&it.metric)?;
        let mut out = json!({"id": it.id, "metric": m.key, "label": m.label, "unit": m.unit, "kind": m.kind, "eps": m.eps,
            "direction": it.direction, "direction_label": direction_label(&it.direction), "reason": it.reason,
            "base_value": last.and_then(|l| l.value(m.key)), "written_at": it.updated_at});
        with_baseline(&mut out, &baseline(m, &rows_all), &it.direction, m);
        Some(out)
    }).collect();
    let locked = next.is_some_and(|n| now_hm_after(now, n));
    let counted_refs: Vec<&Value> = counted.iter().collect();
    json!({
        "date": date,
        "metrics": METRICS.iter().map(metric_view).collect::<Vec<_>>(),
        "directions": [{"key": "up", "label": "上升"}, {"key": "down", "label": "下降"}, {"key": "flat", "label": "持平"}],
        "check": check,
        "plan": {"date": date, "items": plan_items, "next_date": next, "locked": locked,
            "lock_note": if locked { format!("{} 已经开盘竞价,这晚的条件不能再写或改(看过盘再写的判断不算数)", next.unwrap_or("下一场")) }
                else { format!("下一场{} 09:15 起锁定", next.map(|n| format!(" {n}")).unwrap_or_default()) }},
        "track": {"from": series.first().map(|s| s.date.clone()), "to": date, "summary": summarize(&counted_refs), "days": days,
            "excluded_late": excluded_late, "excluded_provisional": excluded_provisional},
    })
}

pub async fn verify(client: &Client, db: &Path, path: &str) -> Result<Value, DiscoveryError> {
    let refresh = refresh_param(path)?;
    let (calendar, date, now) = resolve(client, path).await?;
    let (series, gaps) = series(client, db, &calendar, &date, BASELINE_DAYS, refresh, now).await?;
    let items = load_items(db)?;
    let next = next_session(&calendar, &date);
    let mut body = verify_view(&date, &series, &items, next.as_deref(), now);
    body["status"] = json!("ok");
    body["gaps"] = json!(gaps);
    body["source"] = json!(SOURCE);
    body["generated_at"] = json!(Utc::now().to_rfc3339());
    body["note"] = json!("只收市场层面读数,不涉及个股、点位或买卖时机。下一场 09:15 以后写或改的条件标「事后」,照样对账但不计入战绩;下一场没收盘时的对账标「暂定」。");
    Ok(body)
}

#[derive(Clone, Debug)]
pub(crate) struct Want {
    pub metric: String,
    pub direction: String,
    pub reason: String,
}

pub(crate) fn parse_wants(body: &Value) -> Result<(String, Vec<Want>), DiscoveryError> {
    let bad = |m: String| DiscoveryError::BadRequest(m);
    if body["action"] != "save" {
        return Err(bad("只支持 action=save".into()));
    }
    let date = body["date"].as_str().unwrap_or("").to_string();
    if NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_err() || date.len() != 10 {
        return Err(bad("date 要写成 YYYY-MM-DD".into()));
    }
    let Some(raw) = body["items"].as_array() else {
        return Err(bad("items 要是数组".into()));
    };
    if raw.len() > METRICS.len() {
        return Err(bad(format!("最多 {} 条(一个读数一条)", METRICS.len())));
    }
    let mut seen = HashSet::new();
    let mut wants = Vec::new();
    for (i, x) in raw.iter().enumerate() {
        let metric_key = x["metric"].as_str().unwrap_or("").trim().to_string();
        let Some(m) = metric(&metric_key) else {
            return Err(bad(format!("第 {} 条:读数只能从清单里选", i + 1)));
        };
        if !seen.insert(metric_key.clone()) {
            return Err(bad(format!(
                "「{}」写了两条,同一晚一个读数只写一条",
                m.label
            )));
        }
        let direction = x["direction"].as_str().unwrap_or("");
        if !matches!(direction, "up" | "down" | "flat") {
            return Err(bad(format!("第 {} 条:方向只能是 up / down / flat", i + 1)));
        }
        let reason = match &x["reason"] {
            Value::Null => String::new(),
            Value::String(s) => s.trim().to_string(),
            _ => return Err(bad(format!("第 {} 条:理由要是文字", i + 1))),
        };
        if reason.chars().count() > REASON_MAX {
            return Err(bad(format!("第 {} 条:理由最多 {REASON_MAX} 字", i + 1)));
        }
        wants.push(Want {
            metric: metric_key,
            direction: direction.into(),
            reason,
        });
    }
    Ok((date, wants))
}

/// 整晚一起替换;没变的条目不重写(updated_at 不动,免得重存一遍被当成事后改)。一个事务完成。
pub(crate) fn apply_save(
    db: &Path,
    date: &str,
    wants: &[Want],
    allow_delete: bool,
    now: &str,
) -> Result<Vec<Item>, DiscoveryError> {
    let mut conn = Connection::open(db).map_err(|e| DiscoveryError::Upstream(e.into()))?;
    let existing: Vec<Item> = load_items(db)
        .map_err(DiscoveryError::Upstream)?
        .into_iter()
        .filter(|it| it.date == date)
        .collect();
    let keep: HashSet<&str> = wants.iter().map(|w| w.metric.as_str()).collect();
    if !allow_delete && existing.iter().any(|it| !keep.contains(it.metric.as_str())) {
        return Err(DiscoveryError::BadRequest(format!(
            "暂时确认不了下一场是否已开盘,不能删 {date} 那晚的条件(删掉的不留记录);稍后再试"
        )));
    }
    let tx = conn
        .transaction()
        .map_err(|e| DiscoveryError::Upstream(e.into()))?;
    let run = |tx: &rusqlite::Transaction| -> rusqlite::Result<()> {
        for it in existing
            .iter()
            .filter(|it| !keep.contains(it.metric.as_str()))
        {
            tx.execute("DELETE FROM mobile_verifications WHERE id = ?1", [it.id])?;
        }
        for w in wants {
            match existing.iter().find(|it| it.metric == w.metric) {
                Some(it) if it.direction == w.direction && it.reason == w.reason => {}
                Some(it) => {
                    tx.execute("UPDATE mobile_verifications SET direction = ?1, reason = ?2, updated_at = ?3 WHERE id = ?4", params![w.direction, w.reason, now, it.id])?;
                }
                None => {
                    tx.execute("INSERT INTO mobile_verifications(date, metric, direction, reason, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                        params![date, w.metric, w.direction, w.reason, now])?;
                }
            }
        }
        Ok(())
    };
    run(&tx).map_err(|e| DiscoveryError::Upstream(e.into()))?;
    tx.commit()
        .map_err(|e| DiscoveryError::Upstream(e.into()))?;
    Ok(load_items(db)
        .map_err(DiscoveryError::Upstream)?
        .into_iter()
        .filter(|it| it.date == date)
        .collect())
}

/// 今天有没有开市:上证指数实时快照的更新时间是不是今天。取不到 = None。
async fn today_has_session(client: &Client, now: ChinaNow) -> Option<bool> {
    let root: Value = client
        .get("https://push2.eastmoney.com/api/qt/stock/get")
        .query(&[("secid", "1.000001"), ("fields", "f86")])
        .header("Referer", "https://quote.eastmoney.com/")
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let ts = root["data"]["f86"].as_i64()?;
    let day = DateTime::from_timestamp(ts, 0)?
        .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600)?)
        .date_naive();
    Some(day == now.0)
}

pub async fn save_verify(
    client: &Client,
    db: &Path,
    body: &Value,
) -> Result<Value, DiscoveryError> {
    let (date, wants) = parse_wants(body)?;
    let now = emotion::now_china();
    let today = now.0.format("%Y-%m-%d").to_string();
    if date > today {
        return Err(DiscoveryError::BadRequest(format!("{date} 还没到")));
    }
    let weekday = !matches!(now.0.weekday(), Weekday::Sat | Weekday::Sun);
    let calendar = emotion::trade_dates(client).await.unwrap_or_default();
    if !calendar.is_empty() && !calendar.contains(&date) && !(date == today && weekday) {
        return Err(DiscoveryError::BadRequest(format!(
            "{date} 不是交易日;条件写在复盘的那个交易日上"
        )));
    }
    let mut next = next_session(&calendar, &date);
    let mut unsure = calendar.is_empty();
    if next.is_none() && date < today && weekday && now.1 >= CUTOFF_HM {
        // 今天这根日线要开盘后才有:问实时快照今天开没开市(工作日也可能休市)。
        match today_has_session(client, now).await {
            Some(true) => next = Some(today.clone()),
            Some(false) => {}
            None => unsure = true,
        }
    }
    if let Some(n) = &next {
        if now_hm_after(now, n) {
            return Err(DiscoveryError::BadRequest(format!(
                "{n} 已经开盘,{date} 那晚的条件不能再写或改了(看过盘再写的判断不算数)"
            )));
        }
    }
    let items = apply_save(db, &date, &wants, !unsure, &Utc::now().to_rfc3339())?;
    Ok(
        json!({"status": "ok", "date": date, "next_date": next, "items": items.iter().map(|it| json!({
        "id": it.id, "metric": it.metric, "direction": it.direction, "reason": it.reason, "updated_at": it.updated_at,
    })).collect::<Vec<_>>()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(code: &str, boards: i64, ret: Option<f64>, sector: &str, first: &str) -> Row {
        Row {
            code: code.into(),
            name: format!("股{code}"),
            board: "10cm",
            ret,
            sector: sector.into(),
            first_seal: first.into(),
            boards,
            ..Row::default()
        }
    }

    fn pools() -> DayPools {
        DayPools {
            zt: Ok(vec![
                row("600001", 3, Some(10.0), "机器人", "093000"),
                row("600002", 3, Some(10.0), "机器人", "092500"),
                row("600003", 1, Some(10.0), "算力", "100000"),
                row("600004", 2, Some(10.0), "机器人", ""),
            ]),
            zb: Ok(vec![row("600009", 0, Some(2.0), "算力", "")]),
            dt: Ok(vec![
                row("600010", 0, Some(-10.0), "汽车", ""),
                row("600011", 0, Some(-10.0), "汽车", ""),
            ]),
            yzt: Ok(vec![
                row("600004", 1, Some(10.0), "", ""),
                row("600005", 1, Some(-6.0), "", ""),
                row("600006", 1, Some(1.0), "", ""),
                row("600012", 1, None, "", ""),
            ]),
            prev_zt: Ok(vec![
                row("600004", 1, None, "", ""),
                row("600005", 1, None, "", ""),
                row("600006", 1, None, "", ""),
                row("600001", 2, None, "", ""),
            ]),
        }
    }

    fn day(date: &str, pairs: &[(&str, f64)]) -> DayRecord {
        let mut r = DayRecord::missing(date, "");
        r.status = "ok".into();
        r.settled = Some(true);
        for (k, v) in pairs {
            r.values.insert((*k).into(), Some(*v));
        }
        r
    }

    #[test]
    fn day_readings_follow_desktop_definitions() {
        let r = compute_day("2026-10-16", Some(true), &pools());
        assert_eq!(r.status, "ok");
        assert_eq!(r.value("limit_up_count"), Some(4.0));
        assert_eq!(r.value("highest_board"), Some(3.0));
        assert_eq!(r.value("broken_rate"), Some(0.2));
        assert_eq!(r.value("money_effect_median"), Some(1.0));
        assert_eq!(r.value("deep_loss_count"), Some(1.0));
        assert_eq!(r.value("promotion_1to2"), Some(0.333));
        assert_eq!(r.value("theme_concentration"), Some(0.75));
        assert_eq!(r.value("market_limit_down"), Some(2.0));
        assert_eq!(
            r.leaders
                .iter()
                .map(|l| l.code.as_str())
                .collect::<Vec<_>>(),
            vec!["600002", "600001"]
        );
        assert_eq!(r.leaders[0].first_seal, "09:25");
        assert_eq!(r.sectors[0], ("机器人".to_string(), 3));
    }

    #[test]
    fn missing_pools_leave_readings_empty_with_reason() {
        let mut p = pools();
        p.zb = Err("炸板池:数据源请求超时".into());
        p.prev_zt = Err("取不到前一交易日".into());
        let r = compute_day("2026-10-16", Some(true), &p);
        assert_eq!(r.status, "partial");
        assert_eq!(r.value("broken_rate"), None);
        assert_eq!(r.value("promotion_1to2"), None);
        assert!(
            r.note.starts_with("缺 1进2 晋级率、炸板率") || r.note.contains("炸板率"),
            "{}",
            r.note
        );
        p.zt = Ok(vec![]);
        assert_eq!(compute_day("2026-10-16", Some(true), &p).status, "missing");
    }

    #[test]
    fn history_context_needs_ten_samples_and_marks_extremes() {
        let mut rows: Vec<DayRecord> = (1..=11)
            .map(|i| {
                day(
                    &format!("2026-09-{i:02}"),
                    &[("limit_up_count", 40.0 + i as f64), ("broken_rate", 0.3)],
                )
            })
            .collect();
        rows.push(day(
            "2026-09-12",
            &[("limit_up_count", 90.0), ("broken_rate", 0.3)],
        ));
        let refs: Vec<&DayRecord> = rows.iter().collect();
        let ctx = context_for("2026-09-12", &refs);
        let zt = ctx["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["key"] == "limit_up_count")
            .unwrap();
        assert_eq!(zt["percentile"], json!(1.0));
        assert_eq!(zt["extreme"], "偏热");
        let short = context_for("2026-09-12", &refs[refs.len() - 5..]);
        let zt = short["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["key"] == "limit_up_count")
            .unwrap();
        assert!(zt["percentile"].is_null());
        let diff = diff_for("2026-09-12", &refs);
        assert_eq!(diff["changes"].as_array().unwrap().len(), 1);
        assert_eq!(diff["changes"][0]["delta"], json!(39.0));
        assert_eq!(diff["changes"][0]["hotter"], true);
        // 正好等于门槛算噪声
        let a = day("2026-09-01", &[("broken_rate", 0.427)]);
        let b = day("2026-09-02", &[("broken_rate", 0.507)]);
        assert!(diff_for("2026-09-02", &[&a, &b])["changes"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn verification_direction_baseline_late_and_summary() {
        let m = metric("limit_up_count").unwrap();
        assert_eq!(
            actual_direction(Some(45.0), Some(40.0), m.eps),
            Some("flat")
        );
        assert_eq!(actual_direction(Some(46.0), Some(40.0), m.eps), Some("up"));
        assert_eq!(actual_direction(None, Some(40.0), m.eps), None);
        let rows: Vec<DayRecord> = (1..=14)
            .map(|i| {
                day(
                    &format!("2026-09-{i:02}"),
                    &[("limit_up_count", if i % 2 == 0 { 60.0 } else { 40.0 })],
                )
            })
            .collect();
        let refs: Vec<&DayRecord> = rows.iter().collect();
        let b = baseline(m, &refs);
        assert_eq!(b["available"], true);
        assert_eq!(b["samples"], 13);
        assert_eq!(b["rates"]["flat"], json!(0.0));
        assert!(baseline(m, &refs[..5])["available"] == false);
        assert!(!is_late("2026-10-16T09:14:59+08:00", "2026-10-16"));
        assert!(is_late("2026-10-16T01:15:00Z", "2026-10-16"));
        assert!(is_late("garbage", "2026-10-16"));
        let results = [
            json!({"verified": true, "baseline": 0.5}),
            json!({"verified": false, "baseline": 0.5}),
            json!({"verified": null, "baseline": null}),
        ];
        let refs: Vec<&Value> = results.iter().collect();
        let s = summarize(&refs);
        assert_eq!(
            (
                s["total"].as_u64(),
                s["decided"].as_u64(),
                s["hit"].as_u64()
            ),
            (Some(3), Some(2), Some(1))
        );
        assert_eq!(s["edge"], json!(0.0));
    }

    #[test]
    fn verify_view_checks_previous_night_and_excludes_late_and_provisional() {
        let mut series = vec![
            day(
                "2026-10-14",
                &[("limit_up_count", 40.0), ("broken_rate", 0.3)],
            ),
            day(
                "2026-10-15",
                &[("limit_up_count", 60.0), ("broken_rate", 0.3)],
            ),
            day(
                "2026-10-16",
                &[("limit_up_count", 50.0), ("broken_rate", 0.2)],
            ),
        ];
        series[2].settled = Some(false);
        let item = |id, date: &str, metric: &str, dir: &str, at: &str| Item {
            id,
            date: date.into(),
            metric: metric.into(),
            direction: dir.into(),
            reason: "r".into(),
            created_at: at.into(),
            updated_at: at.into(),
        };
        let items = vec![
            item(
                1,
                "2026-10-14",
                "limit_up_count",
                "up",
                "2026-10-14T20:00:00+08:00",
            ),
            item(
                2,
                "2026-10-14",
                "broken_rate",
                "down",
                "2026-10-15T10:00:00+08:00",
            ),
            item(
                3,
                "2026-10-15",
                "broken_rate",
                "down",
                "2026-10-15T20:00:00+08:00",
            ),
            item(
                4,
                "2026-10-16",
                "limit_up_count",
                "flat",
                "2026-10-16T20:00:00+08:00",
            ),
        ];
        let now = (NaiveDate::from_ymd_opt(2026, 10, 16).unwrap(), 1400);
        let v = verify_view("2026-10-16", &series, &items, Some("2026-10-19"), now);
        assert_eq!(v["check"]["set_date"], "2026-10-15");
        assert_eq!(v["check"]["provisional"], true);
        assert_eq!(v["check"]["results"][0]["verified"], true);
        assert_eq!(v["track"]["summary"]["decided"], 1);
        assert_eq!(v["track"]["summary"]["hit"], 1);
        assert_eq!(v["track"]["excluded_late"], 1);
        assert_eq!(v["track"]["excluded_provisional"], 1);
        assert_eq!(v["plan"]["items"][0]["base_value"], json!(50.0));
        assert_eq!(v["plan"]["locked"], false);
        let later = (NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(), 916);
        assert_eq!(
            verify_view("2026-10-16", &series, &items, Some("2026-10-19"), later)["plan"]["locked"],
            true
        );
    }

    #[test]
    fn save_validates_and_keeps_unchanged_rows_untouched() {
        assert!(parse_wants(&json!({"action": "save", "date": "2026-10-16", "items": [{"metric": "x", "direction": "up"}]})).is_err());
        assert!(parse_wants(&json!({"action": "save", "date": "2026-10-16", "items": [{"metric": "broken_rate", "direction": "up"}, {"metric": "broken_rate", "direction": "down"}]})).is_err());
        assert!(parse_wants(&json!({"action": "save", "date": "2026-10-16", "items": [{"metric": "broken_rate", "direction": "sideways"}]})).is_err());
        let long = "字".repeat(201);
        assert!(parse_wants(&json!({"action": "save", "date": "2026-10-16", "items": [{"metric": "broken_rate", "direction": "up", "reason": long}]})).is_err());
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-verify-{nonce}"));
        let core = crate::AppCore::new(dir.clone()).unwrap();
        drop(core);
        let db = dir.join("vibe-astock-mobile.sqlite3");
        let (date, wants) = parse_wants(&json!({"action": "save", "date": "2026-10-16", "items": [
            {"metric": "broken_rate", "direction": "down", "reason": "分歧收敛"}, {"metric": "limit_up_count", "direction": "up"}]})).unwrap();
        let first = apply_save(&db, &date, &wants, true, "2026-10-16T20:00:00+08:00").unwrap();
        assert_eq!(first.len(), 2);
        let (_, again) = parse_wants(&json!({"action": "save", "date": "2026-10-16", "items": [
            {"metric": "broken_rate", "direction": "down", "reason": "分歧收敛"}, {"metric": "limit_up_count", "direction": "flat"}]})).unwrap();
        let second = apply_save(&db, &date, &again, true, "2026-10-16T21:00:00+08:00").unwrap();
        let broken = second.iter().find(|i| i.metric == "broken_rate").unwrap();
        assert_eq!(
            broken.updated_at, "2026-10-16T20:00:00+08:00",
            "unchanged row keeps its time"
        );
        assert_eq!(
            second
                .iter()
                .find(|i| i.metric == "limit_up_count")
                .unwrap()
                .updated_at,
            "2026-10-16T21:00:00+08:00"
        );
        let (_, fewer) = parse_wants(&json!({"action": "save", "date": "2026-10-16", "items": [{"metric": "broken_rate", "direction": "down", "reason": "分歧收敛"}]})).unwrap();
        assert!(
            apply_save(&db, &date, &fewer, false, "2026-10-16T22:00:00+08:00").is_err(),
            "unsure session forbids deletion"
        );
        assert_eq!(
            apply_save(&db, &date, &fewer, true, "2026-10-16T22:00:00+08:00")
                .unwrap()
                .len(),
            1
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rotation_and_lineage_math() {
        let mut days: Vec<DayRecord> = [
            "2026-10-12",
            "2026-10-13",
            "2026-10-14",
            "2026-10-15",
            "2026-10-16",
        ]
        .iter()
        .map(|d| day(d, &[]))
        .collect();
        days[0].sectors = vec![("机器人".into(), 3)];
        days[1].sectors = vec![("机器人".into(), 2), ("算力".into(), 1)];
        days[2].status = "missing".into();
        days[3].sectors = vec![("算力".into(), 4), ("机器人".into(), 1)];
        days[4].sectors = vec![("机器人".into(), 5), ("算力".into(), 2)];
        let rot = rotation(&days);
        let robot = rot["themes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["sector"] == "机器人")
            .unwrap();
        assert_eq!(robot["series"], json!([3, 2, null, 1, 5]));
        assert_eq!(robot["status"], "连续2天");
        assert_eq!(robot["streak_partial"], true);
        assert_eq!(rot["missing_days"], json!(["2026-10-14"]));
        let bars = vec![
            ("2026-10-14".to_string(), 10.0),
            ("2026-10-15".to_string(), 12.0),
            ("2026-10-16".to_string(), 11.0),
        ];
        let pts = followthrough(&bars, "2026-10-14", "2026-10-16");
        assert_eq!(
            pts.iter().map(|p| p.1).collect::<Vec<_>>(),
            vec![0.0, 20.0, 10.0]
        );
        assert_eq!(peak_drawdown(&pts), (Some(20.0), Some(-8.33)));
        assert!(followthrough(&bars, "2026-10-13", "2026-10-16").is_empty());
    }

    /// 手动探测真实上游:cargo test --locked emotion_history::tests::live -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_history_weekly_verify_probe() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-live-history-{nonce}"));
        drop(crate::AppCore::new(dir.clone()).unwrap());
        let db = dir.join("vibe-astock-mobile.sqlite3");
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("Mozilla/5.0 VibeAStock-Mobile/test")
            .build()
            .unwrap();
        let start = std::time::Instant::now();
        let h = history(&client, &db, "/mobile/shortline/history?days=30")
            .await
            .unwrap();
        let missing = h["series"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["status"] == "missing")
            .count();
        println!(
            "history {} days, missing {missing}, {:?}, gaps={}",
            h["series"].as_array().unwrap().len(),
            start.elapsed(),
            h["gaps"]
        );
        println!(
            "context extremes={} diff changes={}",
            h["context"]["extreme_count"],
            h["diff"]["changes"].as_array().map_or(0, Vec::len)
        );
        let start = std::time::Instant::now();
        let again = history(&client, &db, "/mobile/shortline/history?days=30")
            .await
            .unwrap();
        let cached = again["series"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["cached"] == true)
            .count();
        println!("second call {:?}, cached days {cached}", start.elapsed());
        let w = weekly(&client, &db, "/mobile/shortline/weekly")
            .await
            .unwrap();
        println!(
            "weekly lineage={} rotation themes={}",
            w["lineage"].as_array().unwrap().len(),
            w["rotation"]["themes"].as_array().unwrap().len()
        );
        println!("lineage[0]={}", w["lineage"][0]);
        let v = verify(&client, &db, "/mobile/shortline/verify")
            .await
            .unwrap();
        println!(
            "verify date={} next={} locked={} check={}",
            v["date"], v["plan"]["next_date"], v["plan"]["locked"], v["check"]["available"]
        );
        if let Ok(out) = std::env::var("VIBE_EMOTION_DUMP") {
            std::fs::write(format!("{out}/history.json"), again.to_string()).unwrap();
            std::fs::write(format!("{out}/weekly.json"), w.to_string()).unwrap();
            std::fs::write(format!("{out}/verify.json"), v.to_string()).unwrap();
        }
        assert!(cached > 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn day_records_round_trip_through_the_cache() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-days-{nonce}"));
        drop(crate::AppCore::new(dir.clone()).unwrap());
        let db = dir.join("vibe-astock-mobile.sqlite3");
        let record = compute_day("2026-10-16", Some(true), &pools());
        cache_store(&db, &[&record]).unwrap();
        let loaded =
            cache_load(&db, &["2026-10-16".to_string(), "2026-10-15".to_string()]).unwrap();
        assert_eq!(loaded.len(), 1);
        let back = &loaded["2026-10-16"];
        assert!(back.cached);
        assert_eq!(
            DayRecord {
                cached: false,
                ..back.clone()
            },
            record
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
