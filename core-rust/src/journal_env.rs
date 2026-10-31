//! 交易日志 · 按入场那天的市场环境分组(移植自 orchestrator/src/finance/journal/environment.ts,口径来自 Vibe AStock v2.0.0 交易日志的「按情绪环境」「按当时板位」两组)。
//!
//! 两样都按**入场日**取(一轮 = 第一笔买入那天,补记 = 填的日期):
//!   - 情绪强 / 中 / 弱:入场日情绪分(和涨停样本统计同一个算式:涨停家数 × (1 + 最高连板) × (1 − 炸板率)),
//!     在你交易过的这些日子之间三等分。是相对强弱:只在强势市场里做的人,也会有三分之一的日子分到「弱」。
//!   - 当时板位:入场那天收盘时这只票在不在东财涨停池里、几板;不在涨停池、在炸板池里 = 炸板(桌面版把炸板并进「非涨停」,这里分开)。
//!
//! 🔴 涨停池取不到时,那天的票归「没取到」,不当成「非涨停」。
//! 🔴 入场那天还没收盘(北京时间 15:30 前)的不分组:盘中的涨停池和情绪读数收盘前还会变。
//! 东财这几张池只留约一个月,更早的日子只读本机缓存(情绪历史缓存与本页缓存的收盘后涨停 / 炸板池);读不到的日子如实列出。
//! 与桌面版的差别:每日读数走手机端的情绪历史缓存(`mobile_shortline_days`),涨停 / 炸板池的逐只成员存在 `mobile_journal_env_days`
//! (公开行情,不进备份);手机端只有清仓后才算已实现盈亏,所以分组统计里没卖完的交易只有笔数、没有盈亏。
//! ⛔ 这些数据是用户自己的交易衍生出来的,只留本机,不进任何 AI。

use crate::discovery::DiscoveryError;
use crate::emotion::{self, PoolKind};
use crate::emotion_history::{self, DayRecord};
use crate::journal_risk::{self, entry_day, Row};
use crate::samples;
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use futures_util::stream::{self, StreamExt};
use reqwest::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::path::Path;

/// 只看最近这么多笔(同复查清单)
pub(crate) const ENV_LIMIT: usize = 500;
const CONCURRENCY: usize = 3;
/// 最多往回看这么多个交易日的读数(上证指数日线日历只有约一年)
const MAX_WINDOW_DAYS: usize = 260;
const UNCLASSIFIED: &str = "未分类";

pub(crate) const BOARD_GROUPS: [&str; 7] = [
    "首板",
    "2板",
    "3板及以上",
    "炸板",
    "没碰涨停",
    "非涨停(没分出炸板)",
    "没取到",
];

/// 涨停 / 炸板池里的一只票(只存算板位要用的几项)
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct Pooled {
    pub code: String,
    pub boards: i64,
    pub first_seal: String,
    pub broken_times: i64,
    pub industry: Option<String>,
}

fn pooled(rows: &[emotion::Row]) -> Vec<Pooled> {
    rows.iter()
        .map(|r| {
            let sector = r.sector.trim();
            Pooled {
                code: r.code.clone(),
                boards: r.boards,
                first_seal: r.first_seal.clone(),
                broken_times: r.broken_times,
                industry: (!sector.is_empty() && sector != UNCLASSIFIED)
                    .then(|| sector.to_string()),
            }
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DayEnv {
    /// 入场日情绪分;None = 没取到或缺项
    pub score: Option<f64>,
    pub money_effect: Option<f64>,
    pub emotion_note: String,
    /// 收盘涨停池;None = 没取到
    pub zt: Option<Vec<Pooled>>,
    /// 炸板池;None = 没取到
    pub zb: Option<Vec<Pooled>>,
    pub zt_note: String,
    pub zb_note: String,
}

struct BoardInfo {
    board: &'static str,
    boards: Option<i64>,
    first_seal: Option<String>,
    broken_times: Option<i64>,
    industry: Option<String>,
}

fn seal_clock(raw: &str) -> Option<String> {
    (raw.len() == 6 && raw.bytes().all(|b| b.is_ascii_digit()))
        .then(|| format!("{}:{}", &raw[0..2], &raw[2..4]))
}

/// 这只票入场那天的板位。涨停池里没给连板数的按首板(桌面版同)
fn board_of(symbol: &str, zt: Option<&[Pooled]>, zb: Option<&[Pooled]>) -> BoardInfo {
    let none = |board| BoardInfo {
        board,
        boards: None,
        first_seal: None,
        broken_times: None,
        industry: None,
    };
    let Some(zt) = zt else {
        return none("没取到");
    };
    if let Some(hit) = zt.iter().find(|s| s.code == symbol) {
        let b = hit.boards.max(1);
        return BoardInfo {
            board: match b {
                1 => "首板",
                2 => "2板",
                _ => "3板及以上",
            },
            boards: Some(b),
            first_seal: seal_clock(&hit.first_seal),
            broken_times: Some(hit.broken_times),
            industry: hit.industry.clone(),
        };
    }
    let Some(zb) = zb else {
        return none("非涨停(没分出炸板)");
    };
    match zb.iter().find(|s| s.code == symbol) {
        Some(broke) => BoardInfo {
            board: "炸板",
            broken_times: Some(broke.broken_times),
            industry: broke.industry.clone(),
            ..none("炸板")
        },
        None => none("没碰涨停"),
    }
}

/// 卖出 / 入场那天 15:30(北京时间)以后才算收盘定稿
pub(crate) fn settled_day(day: &str, now: DateTime<Utc>) -> bool {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(7, 30, 0))
        .is_some_and(|t| now >= t.and_utc())
}

fn group_stats(rows: &[(&Row, &str)], order: &[&str]) -> Vec<Value> {
    let mut groups: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
    for (r, key) in rows {
        groups.entry(key).or_default().push(r);
    }
    let rank = |k: &str| order.iter().position(|x| *x == k).unwrap_or(order.len());
    let mut keys: Vec<&str> = groups.keys().copied().collect();
    keys.sort_by(|a, b| rank(a).cmp(&rank(b)).then(a.cmp(b)));
    keys.into_iter()
        .map(|k| {
            let mut b = journal_risk::bucket(&groups[k]);
            b["key"] = json!(k);
            b
        })
        .collect()
}

/// 纯计算:每笔挂上入场日环境,再分两组统计。`rows` 要是桌面版顺序(入场日新的在前);`days` = 入场日 → 那天的环境(收盘后的才给)
pub(crate) fn compute_environment(rows: &[Row], days: &HashMap<String, DayEnv>) -> Value {
    let recent = &rows[..rows.len().min(ENV_LIMIT)];
    let scored: Vec<Option<f64>> = days.values().map(|d| d.score).collect();
    let cuts = samples::regime_cuts(&scored);
    let mut out: Vec<Value> = Vec::new();
    let mut settled: Vec<(&Row, &'static str, &'static str)> = Vec::new();
    for r in recent {
        let day = entry_day(r);
        let key = format!("mobile:{}", r.e.id);
        let Some(d) = days.get(&day) else {
            out.push(json!({"key": key, "date": day, "regime": null, "score": null, "money_effect": null, "board": null, "boards": null, "first_seal": null, "broken_times": null, "industry": null}));
            continue;
        };
        let regime = samples::regime_of(d.score, cuts);
        let b = board_of(&r.e.symbol, d.zt.as_deref(), d.zb.as_deref());
        out.push(json!({"key": key, "date": day, "regime": regime, "score": d.score, "money_effect": d.money_effect, "board": b.board,
            "boards": b.boards, "first_seal": b.first_seal, "broken_times": b.broken_times, "industry": b.industry}));
        settled.push((r, regime, b.board));
    }
    let regime_rows: Vec<(&Row, &str)> = settled.iter().map(|(r, g, _)| (*r, *g)).collect();
    let board_rows: Vec<(&Row, &str)> = settled.iter().map(|(r, _, b)| (*r, *b)).collect();
    json!({
        "rows": out,
        "by_regime": group_stats(&regime_rows, &samples::REGIMES),
        "by_board": group_stats(&board_rows, &BOARD_GROUPS),
        "regime_cuts": {"low": cuts.0, "high": cuts.1},
        "scored_days": scored.iter().flatten().count(),
        "unsettled_trades": recent.len() - settled.len(),
        "scanned": recent.len(),
    })
}

// ---------------------------------------------------------------- 逐日池子:缓存与取数

fn open(db: &Path) -> Result<Connection> {
    Connection::open(db).context("open mobile journal env sqlite")
}

#[derive(Serialize, Deserialize)]
struct DayPayload {
    zt: Vec<Pooled>,
    zb: Vec<Pooled>,
}

fn cache_get(conn: &Connection, day: &str) -> Option<DayPayload> {
    let payload: String = conn
        .query_row(
            "SELECT payload FROM mobile_journal_env_days WHERE date = ?1",
            [day],
            |r| r.get(0),
        )
        .ok()?;
    serde_json::from_str(&payload).ok()
}

fn cache_put(conn: &Connection, day: &str, payload: &DayPayload, now: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO mobile_journal_env_days(date, payload, fetched_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(date) DO UPDATE SET payload = excluded.payload, fetched_at = excluded.fetched_at",
        params![day, serde_json::to_string(payload)?, now],
    )?;
    Ok(())
}

/// 一天的涨停 / 炸板池。已收盘且两张都取到的才存本机;超出东财保留期又没存过的,如实说取不到
pub(crate) async fn pools_for<F, Fut>(
    db: &Path,
    day: &str,
    floor: &str,
    now: &str,
    fetch: &F,
) -> (Option<Vec<Pooled>>, Option<Vec<Pooled>>, String, String)
where
    F: Fn(PoolKind, String) -> Fut,
    Fut: Future<Output = std::result::Result<Vec<emotion::Row>, String>>,
{
    if let Ok(conn) = open(db) {
        if let Some(p) = cache_get(&conn, day) {
            return (Some(p.zt), Some(p.zb), String::new(), String::new());
        }
    }
    if day < floor {
        return (
            None,
            None,
            "本机没存过这天的池子(超过东财保留期约一个月)".into(),
            String::new(),
        );
    }
    let compact = day.replace('-', "");
    let zt = match fetch(PoolKind::LimitUp, compact.clone()).await {
        // 东财超过保留期或日期不对时回空池、不报错:空池一律按没取到
        Ok(rows) if rows.is_empty() => {
            return (
                None,
                None,
                "池是空的(超过东财保留期或不是交易日时如此)".into(),
                String::new(),
            )
        }
        Ok(rows) => pooled(&rows),
        Err(e) => return (None, None, e, String::new()),
    };
    // 涨停池没取到,板位整天都是「没取到」,炸板池不必再取(少一次东财请求)
    match fetch(PoolKind::Broken, compact).await {
        Ok(rows) => {
            let zb = pooled(&rows);
            if let Ok(conn) = open(db) {
                // 缓存写失败不影响这次结果,下次会重取
                let _ = cache_put(
                    &conn,
                    day,
                    &DayPayload {
                        zt: zt.clone(),
                        zb: zb.clone(),
                    },
                    now,
                );
            }
            (Some(zt), Some(zb), String::new(), String::new())
        }
        Err(e) => (Some(zt), None, String::new(), e),
    }
}

/// 同一种原因的日子合成一条,最多列 3 天
fn gap_lines(items: &[(String, String)], what: &str) -> Vec<String> {
    let mut by: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (day, note) in items {
        by.entry(note).or_default().push(day);
    }
    by.into_iter()
        .map(|(note, ds)| {
            format!(
                "{} 天的{what}没取到({}):{}{}",
                ds.len(),
                if note.is_empty() { "没取到" } else { note },
                ds.iter().take(3).copied().collect::<Vec<_>>().join("、"),
                if ds.len() > 3 { " 等" } else { "" }
            )
        })
        .collect()
}

/// 把每个入场日的读数(情绪分)和池子拼成 DayEnv,再算分组与缺口说明
pub(crate) async fn build_view<F, Fut>(
    db: &Path,
    rows: &[Row],
    records: &HashMap<String, DayRecord>,
    dates: &[String],
    floor: &str,
    now: DateTime<Utc>,
    fetch: F,
) -> Value
where
    F: Fn(PoolKind, String) -> Fut,
    Fut: Future<Output = std::result::Result<Vec<emotion::Row>, String>>,
{
    let stamp = now.to_rfc3339();
    let fetch_ref = &fetch;
    let stamp_ref = stamp.as_str();
    let mut fetched: Vec<(String, DayEnv)> = stream::iter(dates.iter().cloned())
        .map(|day| async move {
            let rec = records.get(&day);
            let score = samples::entry_score(rec);
            let emotion_note = match (rec, score) {
                (_, Some(_)) => String::new(),
                (Some(r), None) if r.status == "missing" => {
                    if r.note.is_empty() {
                        "没取到".to_string()
                    } else {
                        r.note.clone()
                    }
                }
                (Some(_), None) => "情绪读数缺涨停家数、最高连板、炸板率中的一项".to_string(),
                (None, None) => "没取到".to_string(),
            };
            let (zt, zb, zt_note, zb_note) = pools_for(db, &day, floor, stamp_ref, fetch_ref).await;
            (
                day,
                DayEnv {
                    score,
                    money_effect: rec.and_then(|r| r.value("money_effect_median")),
                    emotion_note,
                    zt,
                    zb,
                    zt_note,
                    zb_note,
                },
            )
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;
    fetched.sort_by(|a, b| a.0.cmp(&b.0));
    let days: HashMap<String, DayEnv> = fetched.iter().cloned().collect();
    let mut v = compute_environment(rows, &days);
    let mut gaps: Vec<String> = Vec::new();
    if v["unsettled_trades"].as_u64().unwrap_or(0) > 0 {
        gaps.push(format!(
            "{} 笔的入场日还没收盘,不分组",
            v["unsettled_trades"]
        ));
    }
    gaps.extend(gap_lines(
        &fetched
            .iter()
            .filter(|(_, d)| d.score.is_none())
            .map(|(day, d)| (day.clone(), d.emotion_note.clone()))
            .collect::<Vec<_>>(),
        "情绪读数",
    ));
    if v["regime_cuts"]["low"].is_null() {
        gaps.push(format!(
            "取到情绪读数的交易日只有 {} 天,不足 {} 天,不分强中弱",
            v["scored_days"],
            samples::MIN_REGIME_DAYS
        ));
    }
    gaps.extend(gap_lines(
        &fetched
            .iter()
            .filter(|(_, d)| d.zt.is_none())
            .map(|(day, d)| (day.clone(), d.zt_note.clone()))
            .collect::<Vec<_>>(),
        "涨停池",
    ));
    gaps.extend(gap_lines(
        &fetched
            .iter()
            .filter(|(_, d)| d.zt.is_some() && d.zb.is_none())
            .map(|(day, d)| (day.clone(), d.zb_note.clone()))
            .collect::<Vec<_>>(),
        "炸板池",
    ));
    v["available"] = json!(true);
    v["days"] = json!(dates.len());
    v["gaps"] = json!(gaps);
    v["note"] = json!("按入场那天的市场环境分组。情绪强中弱是你交易过的这些日子之间相对比较(情绪分 = 涨停家数 × (1 + 最高连板) × (1 − 炸板率),同涨停样本统计);当时板位看入场那天收盘时这只票在不在涨停池、炸板池里。这是你自己的行为统计,不是对市场的预测。");
    v["source"] = json!("东财涨停池 / 炸板池 + 情绪历史读数(已收盘的日子存本机);东财只留约一个月,更早的日子只读本机缓存");
    v
}

/// `GET /mobile/journal/environment`
pub async fn view(client: &Client, db: &Path) -> std::result::Result<Value, DiscoveryError> {
    let up = DiscoveryError::Upstream;
    let a = journal_risk::analysis(db).map_err(up)?;
    let mut rows = a.rows;
    journal_risk::va_order(&mut rows);
    if rows.is_empty() {
        return Ok(json!({"available": false, "reason": "还没有交易记录"}));
    }
    let now_utc = Utc::now();
    let china_now = emotion::now_china();
    let today = china_now.0.format("%Y-%m-%d").to_string();
    let mut dates: Vec<String> = rows[..rows.len().min(ENV_LIMIT)]
        .iter()
        .map(entry_day)
        .filter(|d| d.as_str() <= today.as_str() && settled_day(d, now_utc))
        .collect();
    dates.sort();
    dates.dedup();
    if dates.is_empty() {
        let mut v = compute_environment(&rows, &HashMap::new());
        v["available"] = json!(true);
        v["days"] = json!(0);
        v["gaps"] = json!([format!(
            "{} 笔的入场日还没收盘,不分组",
            v["unsettled_trades"]
        )]);
        return Ok(v);
    }
    let calendar = emotion::trade_dates(client)
        .await
        .map_err(DiscoveryError::Upstream)?;
    let end = dates.last().cloned().unwrap_or_default();
    let upto: Vec<&String> = calendar
        .iter()
        .filter(|d| d.as_str() <= end.as_str())
        .collect();
    let first = dates.first().cloned().unwrap_or_default();
    let window = upto
        .iter()
        .filter(|d| d.as_str() >= first.as_str())
        .count()
        .clamp(10, MAX_WINDOW_DAYS);
    let (series, _) =
        emotion_history::series(client, db, &calendar, &end, window, false, china_now)
            .await
            .map_err(DiscoveryError::Upstream)?;
    let records: HashMap<String, DayRecord> =
        series.into_iter().map(|r| (r.date.clone(), r)).collect();
    let floor = emotion_history::retention_floor(china_now);
    let mut v = build_view(
        db,
        &rows,
        &records,
        &dates,
        &floor,
        now_utc,
        |kind, compact| async move {
            emotion::fetch_pool(client, kind, &compact)
                .await
                .map_err(|e| emotion::error_message(kind.label(), &e))
        },
    )
    .await;
    v["generated_at"] = json!(now_utc.to_rfc3339());
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal_risk::{Entry, Fill};
    use crate::journal_settings::Fees;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const ZERO: Fees = Fees {
        commission_rate: 0.0,
        commission_min: 0.0,
        stamp_tax_rate: 0.0,
        transfer_fee_rate: 0.0,
        fifo: false,
    };

    fn p(code: &str, boards: i64) -> Pooled {
        Pooled {
            code: code.into(),
            boards,
            first_seal: "093001".into(),
            broken_times: 1,
            industry: Some("半导体".into()),
        }
    }

    fn trade(id: i64, symbol: &str, day: &str, pnl_pct: f64) -> Row {
        let e = Entry {
            id,
            kind: "episode".into(),
            symbol: symbol.into(),
            date: day.into(),
            created_at: format!("{day}T09:00:00Z"),
            fills: vec![
                Fill {
                    side: "buy".into(),
                    date: day.into(),
                    price: 10.0,
                    shares: 100.0,
                    fee: Some(0.0),
                },
                Fill {
                    side: "sell".into(),
                    date: "2099-01-01".into(),
                    price: 10.0 + pnl_pct / 10.0,
                    shares: 100.0,
                    fee: Some(0.0),
                },
            ],
            ..Entry::default()
        };
        journal_risk::settle(e, &ZERO, id as usize)
    }

    fn env(score: Option<f64>, zt: Option<Vec<Pooled>>, zb: Option<Vec<Pooled>>) -> DayEnv {
        DayEnv {
            score,
            zt,
            zb,
            ..DayEnv::default()
        }
    }

    #[test]
    fn board_position_separates_broken_from_never_touched_and_unknown_from_non_limit_up() {
        let zt = vec![
            p("600001", 1),
            p("600002", 2),
            p("600003", 5),
            p("600004", 0),
        ];
        let zb = vec![Pooled {
            code: "600005".into(),
            broken_times: 3,
            industry: None,
            ..Pooled::default()
        }];
        let board = |code: &str, zt: Option<&Vec<Pooled>>, zb: Option<&Vec<Pooled>>| {
            board_of(code, zt.map(|v| v.as_slice()), zb.map(|v| v.as_slice()))
        };
        assert_eq!(board("600001", Some(&zt), Some(&zb)).board, "首板");
        assert_eq!(board("600002", Some(&zt), Some(&zb)).board, "2板");
        assert_eq!(board("600003", Some(&zt), Some(&zb)).board, "3板及以上");
        assert_eq!(
            board("600004", Some(&zt), Some(&zb)).board,
            "首板",
            "池里没给连板数的按首板"
        );
        let broke = board("600005", Some(&zt), Some(&zb));
        assert_eq!((broke.board, broke.broken_times), ("炸板", Some(3)));
        assert_eq!(board("600006", Some(&zt), Some(&zb)).board, "没碰涨停");
        assert_eq!(
            board("600006", Some(&zt), None).board,
            "非涨停(没分出炸板)",
            "炸板池没取到时不敢说没碰过"
        );
        assert_eq!(
            board("600001", None, None).board,
            "没取到",
            "涨停池取不到不能把票当成非涨停"
        );
        let hit = board("600001", Some(&zt), Some(&zb));
        assert_eq!(
            (
                hit.first_seal.as_deref(),
                hit.industry.as_deref(),
                hit.boards
            ),
            (Some("09:30"), Some("半导体"), Some(1))
        );
    }

    #[test]
    fn grouping_uses_thirds_of_the_users_own_days_and_leaves_unsettled_days_out() {
        // 8 个入场日,情绪分 10..80;三等分:弱 ≤ 第 3 个(30)、强 ≥ 第 6 个(60)
        let mut days = HashMap::new();
        let mut rows = Vec::new();
        for i in 1..=8 {
            let day = format!("2026-09-{:02}", i + 10);
            days.insert(
                day.clone(),
                env(
                    Some(i as f64 * 10.0),
                    Some(vec![p(&format!("60000{i}"), 1)]),
                    Some(vec![]),
                ),
            );
            rows.push(trade(
                i,
                &format!("60000{i}"),
                &day,
                if i <= 4 { -2.0 } else { 3.0 },
            ));
        }
        // 一笔入场日没有环境(还没收盘)
        rows.push(trade(9, "600099", "2026-10-16", 1.0));
        journal_risk::va_order(&mut rows);
        let v = compute_environment(&rows, &days);
        assert_eq!(
            (
                v["regime_cuts"]["low"].clone(),
                v["regime_cuts"]["high"].clone(),
                v["scored_days"].clone()
            ),
            (json!(30.0), json!(60.0), json!(8))
        );
        let count = |g: &str| {
            v["by_regime"]
                .as_array()
                .unwrap()
                .iter()
                .find(|x| x["key"] == g)
                .map(|x| x["count"].as_u64().unwrap())
        };
        assert_eq!(
            (count("情绪弱"), count("情绪中"), count("情绪强")),
            (Some(3), Some(2), Some(3))
        );
        assert_eq!(v["unsettled_trades"], 1);
        let keys: Vec<&str> = v["by_regime"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["key"].as_str().unwrap())
            .collect();
        assert_eq!(keys, vec!["情绪强", "情绪中", "情绪弱"]);
        let unsettled = v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["date"] == "2026-10-16")
            .unwrap();
        assert_eq!(
            (unsettled["regime"].clone(), unsettled["board"].clone()),
            (Value::Null, Value::Null)
        );
        assert_eq!(v["by_board"][0]["key"], "首板");
        assert_eq!(v["by_board"][0]["count"], 8);
        // 样本日不足 6 天:不分强中弱,统一「环境未覆盖」,但板位照给
        let few: HashMap<String, DayEnv> = days
            .iter()
            .take(4)
            .map(|(k, d)| (k.clone(), d.clone()))
            .collect();
        let v = compute_environment(&rows, &few);
        assert_eq!(v["regime_cuts"]["low"], Value::Null);
        assert!(v["by_regime"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["key"] == "环境未覆盖"));
    }

    fn db(label: &str) -> (crate::AppCore, std::path::PathBuf) {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("vibe-jenv-{label}-{nonce}-{}", std::process::id()));
        (crate::AppCore::new(dir.clone()).unwrap(), dir)
    }

    fn pool_row(code: &str, boards: i64) -> emotion::Row {
        emotion::Row {
            code: code.into(),
            name: code.into(),
            sector: "半导体".into(),
            boards,
            first_seal: "093001".into(),
            broken_times: 0,
            ..emotion::Row::default()
        }
    }

    #[tokio::test]
    async fn pools_are_cached_only_when_both_are_complete_and_old_uncached_days_are_reported() {
        let (app, dir) = db("pools");
        let path = app.db_path().to_path_buf();
        let calls = AtomicUsize::new(0);
        let ok = |kind: PoolKind, _d: String| {
            calls.fetch_add(1, Ordering::SeqCst);
            async move {
                Ok(vec![pool_row(
                    if kind == PoolKind::LimitUp {
                        "600001"
                    } else {
                        "600002"
                    },
                    2,
                )])
            }
        };
        let floor = "2026-09-20";
        let (zt, zb, a, b) = pools_for(&path, "2026-10-12", floor, "t", &ok).await;
        assert_eq!(
            (zt.unwrap()[0].boards, zb.unwrap()[0].code.as_str(), a, b),
            (2, "600002", String::new(), String::new())
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        // 第二次读缓存,不再联网
        assert!(pools_for(&path, "2026-10-12", floor, "t", &ok)
            .await
            .0
            .is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        // 超出保留期又没存过:不联网、说明原因
        let old = pools_for(&path, "2026-08-01", floor, "t", &ok).await;
        assert!(
            old.0.is_none()
                && old.2.contains("超过东财保留期")
                && calls.load(Ordering::SeqCst) == 2
        );
        // 涨停池取到、炸板池失败:不存缓存,只报炸板池原因
        let half = |kind: PoolKind, _d: String| async move {
            if kind == PoolKind::Broken {
                Err("炸板池:网络断了".to_string())
            } else {
                Ok(vec![pool_row("600001", 1)])
            }
        };
        let (zt, zb, _, note) = pools_for(&path, "2026-10-13", floor, "t", &half).await;
        assert!(zt.is_some() && zb.is_none() && note.contains("网络断了"));
        assert!(
            cache_get(&open(&path).unwrap(), "2026-10-13").is_none(),
            "不完整的不存"
        );
        // 涨停池空 = 没取到,不是「没有票涨停」
        let empty = |_k: PoolKind, _d: String| async { Ok(Vec::<emotion::Row>::new()) };
        let (zt, _, note, _) = pools_for(&path, "2026-10-14", floor, "t", &empty).await;
        assert!(zt.is_none() && note.contains("池是空的"));
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn record(day: &str, lu: f64, hb: f64, br: f64) -> DayRecord {
        DayRecord {
            date: day.into(),
            status: "ok".into(),
            settled: Some(true),
            values: [
                ("limit_up_count", lu),
                ("highest_board", hb),
                ("broken_rate", br),
                ("money_effect_median", 1.5),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), Some(*v)))
            .collect(),
            ..DayRecord::default()
        }
    }

    #[tokio::test]
    async fn build_view_lists_gaps_by_reason_and_never_fills_missing_days() {
        let (app, dir) = db("view");
        let path = app.db_path().to_path_buf();
        let days: Vec<String> = (12..=17).map(|d| format!("2026-10-{d}")).collect();
        let mut rows: Vec<Row> = days
            .iter()
            .enumerate()
            .map(|(i, d)| trade(i as i64 + 1, "600001", d, 1.0))
            .collect();
        journal_risk::va_order(&mut rows);
        let mut records = HashMap::new();
        for (i, d) in days.iter().enumerate().take(4) {
            records.insert(d.clone(), record(d, 40.0 + i as f64 * 10.0, 3.0, 0.2));
        }
        records.insert(
            days[4].clone(),
            DayRecord {
                date: days[4].clone(),
                status: "missing".into(),
                note: "涨停池为空(非交易日或数据还没出)".into(),
                ..DayRecord::default()
            },
        );
        let fetch = |kind: PoolKind, _d: String| async move {
            Ok(vec![pool_row(
                if kind == PoolKind::LimitUp {
                    "600001"
                } else {
                    "600009"
                },
                1,
            )])
        };
        let now = DateTime::parse_from_rfc3339("2026-10-18T02:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let v = build_view(&path, &rows, &records, &days, "2026-09-20", now, fetch).await;
        assert_eq!(
            (
                v["days"].clone(),
                v["scored_days"].clone(),
                v["regime_cuts"]["low"].clone()
            ),
            (json!(6), json!(4), Value::Null)
        );
        let gaps: Vec<&str> = v["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g.as_str().unwrap())
            .collect();
        assert!(
            gaps.iter()
                .any(|g| g.contains("取到情绪读数的交易日只有 4 天,不足 6 天,不分强中弱")),
            "{gaps:?}"
        );
        assert!(
            gaps.iter()
                .any(|g| g.starts_with("1 天的情绪读数没取到(涨停池为空")
                    && g.contains("2026-10-16")),
            "{gaps:?}"
        );
        assert!(
            gaps.iter()
                .any(|g| g.starts_with("1 天的情绪读数没取到(没取到)") && g.contains("2026-10-17")),
            "{gaps:?}"
        );
        assert_eq!(v["by_board"][0]["key"], "首板");
        assert_eq!(v["by_board"][0]["count"], 6, "每个入场日涨停池里都有它");
        assert!(v["note"].as_str().unwrap().contains("不是对市场的预测"));
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn settled_day_waits_for_half_past_three_beijing_time() {
        let at = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
        assert!(!settled_day("2026-10-16", at("2026-10-16T07:29:59Z")));
        assert!(settled_day("2026-10-16", at("2026-10-16T07:30:00Z")));
        assert!(!settled_day("garbage", at("2026-10-16T07:30:00Z")));
    }

    /// 生成前端冻结夹具(全部是合成数据,不含任何真实成交与真实行情):
    /// VIBE_JOURNAL_ENV_FIXTURE=路径 cargo test --locked journal_env::tests::dump_synthetic_fixture -- --ignored
    #[tokio::test]
    #[ignore]
    async fn dump_synthetic_fixture() {
        let (app, dir) = db("fixture");
        let path = app.db_path().to_path_buf();
        // (入场日, 代码, 盈亏 %, 当天这只票的位置: 0 首板 / 1 2板 / 2 3板+ / 3 炸板 / 4 没碰)
        let spec = [
            ("2026-09-21", 0, 4.0),
            ("2026-09-22", 3, -3.0),
            ("2026-09-23", 4, 1.0),
            ("2026-09-24", 1, 6.0),
            ("2026-09-25", 0, 2.5),
            ("2026-09-28", 2, 8.0),
            ("2026-09-29", 3, -5.0),
            ("2026-10-09", 4, -1.0),
            ("2026-10-12", 0, 3.0),
            ("2026-10-13", 1, 5.5),
            ("2026-10-14", 3, -6.0),
            ("2026-10-15", 4, 0.5),
        ];
        let mut rows: Vec<Row> = Vec::new();
        let mut records = HashMap::new();
        let mut pools: HashMap<String, (Vec<Pooled>, Vec<Pooled>)> = HashMap::new();
        for (i, (day, pos, pnl)) in spec.iter().enumerate() {
            let code = format!("6001{:02}", i + 1);
            rows.push(trade(i as i64 + 1, &code, day, *pnl));
            records.insert(
                day.to_string(),
                record(
                    day,
                    30.0 + (i as f64 * 9.0) % 70.0,
                    2.0 + (i % 5) as f64,
                    0.15 + (i % 4) as f64 * 0.07,
                ),
            );
            let (mut zt, mut zb) = (vec![p("600900", 1), p("600901", 2)], vec![]);
            match pos {
                0 => zt.push(Pooled {
                    boards: 1,
                    ..p(&code, 1)
                }),
                1 => zt.push(p(&code, 2)),
                2 => zt.push(Pooled {
                    industry: Some("算力".into()),
                    ..p(&code, 4)
                }),
                3 => zb.push(Pooled {
                    code: code.clone(),
                    boards: 0,
                    first_seal: String::new(),
                    broken_times: 2,
                    industry: Some("机器人".into()),
                }),
                _ => {}
            }
            pools.insert(day.to_string(), (zt, zb));
        }
        // 一笔入场日还没收盘
        rows.push(trade(13, "600113", "2026-10-17", 1.0));
        journal_risk::va_order(&mut rows);
        let dates: Vec<String> = spec.iter().map(|s| s.0.to_string()).collect();
        let pools_ref = &pools;
        let fetch = |kind: PoolKind, compact: String| async move {
            let day = format!("{}-{}-{}", &compact[0..4], &compact[4..6], &compact[6..8]);
            let (zt, zb) = &pools_ref[&day];
            let list = if kind == PoolKind::LimitUp { zt } else { zb };
            Ok(list
                .iter()
                .map(|x| emotion::Row {
                    code: x.code.clone(),
                    name: x.code.clone(),
                    sector: x.industry.clone().unwrap_or_default(),
                    boards: x.boards,
                    first_seal: x.first_seal.clone(),
                    broken_times: x.broken_times,
                    ..emotion::Row::default()
                })
                .collect())
        };
        let now = DateTime::parse_from_rfc3339("2026-10-17T02:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut v = build_view(&path, &rows, &records, &dates, "2026-09-01", now, fetch).await;
        v["generated_at"] = json!("2026-10-17T02:00:00Z");
        if let Ok(out) = std::env::var("VIBE_JOURNAL_ENV_FIXTURE") {
            std::fs::write(out, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        }
        println!(
            "days {} cuts {} gaps {}",
            v["days"], v["regime_cuts"], v["gaps"]
        );
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 实时探测(联网):合成一笔最近交易日的成交,走完整流程
    #[tokio::test]
    #[ignore]
    async fn live_environment_probe() {
        let (app, dir) = db("live");
        let path = app.db_path().to_path_buf();
        for (code, day) in [
            ("600519", "2026-10-16"),
            ("000001", "2026-10-15"),
            ("600036", "2026-10-14"),
        ] {
            crate::journal::act(&path, &json!({"action": "record", "symbol": code, "name": code,
                "fills": [{"type": "buy", "date": day, "price": 10, "shares": 100, "fee": 0}, {"type": "sell", "date": "2026-10-16", "price": 10.5, "shares": 100, "fee": 0}]})).unwrap();
        }
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap();
        let v = view(&client, &path).await.unwrap();
        println!("{}", serde_json::to_string(&json!({"days": v["days"], "scored": v["scored_days"], "cuts": v["regime_cuts"], "gaps": v["gaps"], "boards": v["by_board"], "rows": v["rows"]})).unwrap());
        let again = std::time::Instant::now();
        view(&client, &path).await.unwrap();
        println!("second call {:?}", again.elapsed());
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
