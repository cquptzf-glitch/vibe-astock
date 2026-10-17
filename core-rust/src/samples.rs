//! 涨停样本统计(移植自 orchestrator/src/finance/shortline/samples.ts,口径来自 Vibe AStock v2.0.0 duanxian/backtest.py)。
//!
//! 几种经典短线打法(首板 / 早封 / 尾盘封 / 连板接力 …)在近 N 个交易日里的历史表现:整体、按入场那天的情绪强弱分开、按封板时间分档。
//!
//! 🔴 这是「市场现象统计」,不是策略回测,更不是打板收益:样本是前一天收盘还封在涨停板上的票(事后名单),
//!    冲板没封住的、排队买不到的、一字板根本买不进的都没区分;收益 = 前一天收盘 → 当天收盘,等权,没有滑点和成交概率。
//!    真去打板,结果只会更差。界面必须带着这层口径。
//! 🔴 分环境用**入场那天**(前一交易日)的情绪,不用当天的:决策发生在前一天收盘,那时看不到当天的情绪。
//!
//! 原料:每个交易日一次东财「昨日涨停表现」池(逐只明细),已收盘的日子存本机(东财池子只留约一个月,缓存让样本越用越长);
//! 入场日情绪读数和情绪历史共用同一份每日读数。与桌面版的不同:
//! - 窗口截止到页面选的那天;那天还没收盘就不进统计(盘中的涨跌幅收盘前还会变)。
//! - 是否再涨停按那一天的涨跌幅制度判(主板 ST 7 月 6 日前后、创业板 2020-08-24 前后)。
//! - 涨停数前二行业并列时按行业名排。
//! 只读公开行情,明细缓存不进备份。

use crate::{
    discovery::{self, DiscoveryError},
    emotion::{self, PoolKind, Row},
    emotion_history::{self, DayRecord},
};
use anyhow::Result;
use chrono::Utc;
use futures_util::{stream, StreamExt};
use reqwest::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

const DAYS_DEFAULT: usize = 30;
const DAYS_MIN: usize = 10;
const DAYS_MAX: usize = 90;
/// 取到的交易日少于这么多就不出统计
const MIN_DAYS: usize = 5;
/// 入场日情绪分少于这么多天就不分环境:三等分要每档至少两天
pub(crate) const MIN_REGIME_DAYS: usize = 6;
const CONCURRENCY: usize = 3;
/// 「早封」「尾盘封」的界线(HHMMSS,比的是前一天的**最后**封板时间)
const EARLY_SEAL: &str = "100000";
const LATE_SEAL: &str = "143000";
const MAIN_SECTOR_TOP: usize = 2;
const SOURCE: &str = "东财昨日涨停表现池(每天一次,已收盘的日子存本机)+ 入场日情绪读数(和情绪历史共用)";
const CAVEAT: &str = "样本 = 前一天收盘还封在涨停板上的票(事后名单)。包含一字板等未必买得进的票,没排除排队买不到的情况;冲板没封住的不在样本里。\
收益按前一天收盘到当天收盘、等权计算,没有成交模拟,不能推出真实收益或打法期望。这是市场现象统计,不是策略回测。";
pub(crate) const REGIMES: [&str; 4] = ["情绪强", "情绪中", "情绪弱", "环境未覆盖"];
/// 首板按最后封板时间分档(HHMMSS 上界,含)。晚 = 封得晚,或中途炸开过又回封
const SEAL_BUCKETS: [(&str, &str); 5] = [
    ("开盘秒板", "093500"),
    ("9:35-10:00", "100000"),
    ("10:00-11:00", "110000"),
    ("11:00-14:00", "140000"),
    ("14:00后", "150500"),
];
const SEAL_UNKNOWN: &str = "封板时间缺失";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct SampleRow {
    pub code: String,
    pub name: String,
    /// 当天涨跌幅(%)
    pub ret: f64,
    /// 前一天的连板数
    pub prev_boards: i64,
    /// 前一天的最后封板时间(HHMMSS);缺失 = ""
    pub seal_time: String,
    /// 东财行业(不是题材);缺失 = ""
    pub sector: String,
    /// 当天收盘是否在涨停价;判不了 = None
    pub limit_up: Option<bool>,
}

struct DayCtx {
    main_sectors: Vec<String>,
}

fn is_early(r: &SampleRow) -> bool {
    !r.seal_time.is_empty() && r.seal_time.as_str() <= EARLY_SEAL
}

fn is_late(r: &SampleRow) -> bool {
    !r.seal_time.is_empty() && r.seal_time.as_str() >= LATE_SEAL
}

struct Strategy {
    name: &'static str,
    desc: &'static str,
    filter: fn(&SampleRow, &DayCtx) -> bool,
}

/// 每条 = 对「前一天涨停的票」这个群体的一条过滤规则,不挑个股
const STRATEGIES: [Strategy; 7] = [
    Strategy { name: "首板打板", desc: "前一天首次涨停的全部个股,持有到当天收盘", filter: |r, _| r.prev_boards == 1 },
    Strategy { name: "首板·早封", desc: "前一天首板且 10:00 前最终封板(早盘封住且之后没再炸开)", filter: |r, _| r.prev_boards == 1 && is_early(r) },
    Strategy { name: "首板·尾盘封", desc: "前一天首板但 14:30 后才最终封板(含尾盘偷袭和全天反复炸板后回封)", filter: |r, _| r.prev_boards == 1 && is_late(r) },
    // 叫「涨停数前二行业」不叫「主线」:短线说的主线是题材,不是东财行业分类
    Strategy {
        name: "首板·涨停数前二行业",
        desc: "前一天首板且属于那批涨停股里家数最多的两个行业(行业 ≠ 题材)",
        filter: |r, c| r.prev_boards == 1 && !r.sector.is_empty() && c.main_sectors.contains(&r.sector),
    },
    Strategy { name: "连板接力", desc: "前一天已经 2 板及以上的个股,持有到当天收盘", filter: |r, _| r.prev_boards >= 2 },
    Strategy { name: "高标接力", desc: "前一天 3 板及以上的高位股,持有到当天收盘", filter: |r, _| r.prev_boards >= 3 },
    Strategy { name: "全体涨停", desc: "前一天涨停的全部个股(基准线,对照其它打法有没有多赚)", filter: |_, _| true },
];

// ---------------------------------------------------------------- 纯计算

/// 一组样本的统计。没有样本时各项给 null,不伪装成 0
pub(crate) fn stats(rows: &[&SampleRow]) -> Value {
    if rows.is_empty() {
        return json!({"sample": 0, "win_rate": null, "avg": null, "median": null, "best": null, "worst": null, "limit_up_rate": null});
    }
    let rets: Vec<f64> = rows.iter().map(|r| r.ret).collect();
    let judged: Vec<&&SampleRow> = rows.iter().filter(|r| r.limit_up.is_some()).collect();
    json!({
        "sample": rows.len(),
        "win_rate": emotion::r3(rets.iter().filter(|x| **x > 0.0).count() as f64 / rets.len() as f64),
        "avg": emotion::r2(rets.iter().sum::<f64>() / rets.len() as f64),
        "median": emotion::r2(emotion::median(&rets)),
        "best": emotion::r2(rets.iter().cloned().fold(f64::MIN, f64::max)),
        "worst": emotion::r2(rets.iter().cloned().fold(f64::MAX, f64::min)),
        "limit_up_rate": if judged.is_empty() { Value::Null } else { json!(emotion::r3(judged.iter().filter(|r| r.limit_up == Some(true)).count() as f64 / judged.len() as f64)) },
    })
}

/// 那一天的群体上下文:那批涨停股里家数最多的两个行业(并列按行业名)
fn day_context(rows: &[SampleRow]) -> DayCtx {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for r in rows.iter().filter(|r| !r.sector.is_empty()) {
        *counts.entry(r.sector.as_str()).or_default() += 1;
    }
    let mut top: Vec<(&str, usize)> = counts.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    DayCtx { main_sectors: top.into_iter().take(MAIN_SECTOR_TOP).map(|(s, _)| s.to_string()).collect() }
}

/// 首板按最后封板时间分档的表现。只看首板:连板股的封板时间含义不同,混进来会污染结论
pub(crate) fn seal_curve(per_day: &BTreeMap<String, Vec<SampleRow>>) -> Vec<Value> {
    let mut buckets: Vec<Vec<&SampleRow>> = vec![Vec::new(); SEAL_BUCKETS.len()];
    let mut unknown: Vec<&SampleRow> = Vec::new();
    for rows in per_day.values() {
        for r in rows.iter().filter(|r| r.prev_boards == 1) {
            if r.seal_time.is_empty() {
                unknown.push(r);
                continue;
            }
            let idx = SEAL_BUCKETS.iter().position(|(_, upper)| r.seal_time.as_str() <= *upper).unwrap_or(SEAL_BUCKETS.len() - 1);
            buckets[idx].push(r);
        }
    }
    let mut out: Vec<Value> = SEAL_BUCKETS.iter().zip(&buckets).map(|((name, _), rs)| with_bucket(name, stats(rs))).collect();
    if !unknown.is_empty() {
        out.push(with_bucket(SEAL_UNKNOWN, stats(&unknown)));
    }
    out
}

fn with_bucket(name: &str, mut stat: Value) -> Value {
    stat["bucket"] = json!(name);
    stat
}

/// 入场日情绪分:涨停家数 × (1 + 最高连板) × (1 − 炸板率)。只要相对强弱;三项缺一项就不打分
pub(crate) fn entry_score(day: Option<&DayRecord>) -> Option<f64> {
    let day = day?;
    let (lu, hb, br) = (day.value("limit_up_count")?, day.value("highest_board")?, day.value("broken_rate")?);
    Some((lu * (1.0 + hb) * (1.0 - br) * 10.0).round() / 10.0)
}

pub(crate) fn regime_cuts(scores: &[Option<f64>]) -> (Option<f64>, Option<f64>) {
    let mut valid: Vec<f64> = scores.iter().flatten().copied().collect();
    if valid.len() < MIN_REGIME_DAYS {
        return (None, None);
    }
    valid.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    (Some(valid[valid.len() / 3]), Some(valid[valid.len() * 2 / 3]))
}

pub(crate) fn regime_of(score: Option<f64>, cuts: (Option<f64>, Option<f64>)) -> &'static str {
    match (score, cuts) {
        (Some(s), (Some(low), Some(high))) => {
            if s <= low { "情绪弱" } else if s >= high { "情绪强" } else { "情绪中" }
        }
        _ => "环境未覆盖",
    }
}

fn run_strategy(s: &Strategy, per_day: &BTreeMap<String, Vec<SampleRow>>, regimes: &HashMap<String, &'static str>) -> Value {
    let mut all: Vec<&SampleRow> = Vec::new();
    let mut by_regime: HashMap<&str, Vec<&SampleRow>> = HashMap::new();
    let mut daily = Vec::new();
    let mut equity = 100.0_f64;
    for (date, rows) in per_day {
        let ctx = day_context(rows);
        let hit: Vec<&SampleRow> = rows.iter().filter(|r| (s.filter)(r, &ctx)).collect();
        let regime = regimes.get(date).copied().unwrap_or("环境未覆盖");
        all.extend(hit.iter().copied());
        by_regime.entry(regime).or_default().extend(hit.iter().copied());
        let avg = (!hit.is_empty()).then(|| emotion::r2(hit.iter().map(|r| r.ret).sum::<f64>() / hit.len() as f64));
        if let Some(avg) = avg {
            equity *= 1.0 + avg / 100.0;
        }
        daily.push(json!({"date": date, "sample": hit.len(), "avg": avg, "regime": regime, "equity": emotion::r2(equity)}));
    }
    let by_regime: serde_json::Map<String, Value> = REGIMES.iter().map(|r| (r.to_string(), stats(by_regime.get(r).map(Vec::as_slice).unwrap_or_default()))).collect();
    json!({"name": s.name, "desc": s.desc, "overall": stats(&all), "by_regime": by_regime, "daily": daily, "final_equity": emotion::r2(equity)})
}

/// 昨日涨停表现池 → 逐只明细。没有当天涨跌幅的行不要
pub(crate) fn rows_of(pool: &[Row], day: &str) -> Vec<SampleRow> {
    pool.iter()
        .filter_map(|r| {
            Some(SampleRow {
                code: r.code.clone(),
                name: r.name.clone(),
                ret: r.ret?,
                prev_boards: r.prev_boards.max(1),
                seal_time: r.last_seal.clone(),
                sector: r.sector.clone(),
                limit_up: emotion::is_limit_up(r, day),
            })
        })
        .collect()
}

// ---------------------------------------------------------------- 缓存与取数

fn cache_load(db: &Path, date: &str) -> Option<Vec<SampleRow>> {
    let conn = Connection::open(db).ok()?;
    let payload: String = conn.query_row("SELECT payload FROM mobile_samples_days WHERE date = ?1", [date], |r| r.get(0)).ok()?;
    serde_json::from_str(&payload).ok().filter(|rows: &Vec<SampleRow>| !rows.is_empty())
}

fn cache_store(db: &Path, date: &str, rows: &[SampleRow]) -> Result<()> {
    Connection::open(db)?.execute(
        "INSERT INTO mobile_samples_days(date, payload, fetched_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(date) DO UPDATE SET payload = excluded.payload, fetched_at = excluded.fetched_at",
        params![date, serde_json::to_string(rows)?, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

struct Fetched {
    date: String,
    rows: Option<Vec<SampleRow>>,
    settled: bool,
    note: String,
}

fn param_days(path: &str) -> Result<usize, DiscoveryError> {
    discovery::integer(path, "days", DAYS_DEFAULT, DAYS_MIN, DAYS_MAX)
}

fn param_refresh(path: &str) -> Result<bool, DiscoveryError> {
    match discovery::parameter(path, "refresh").as_deref() {
        None => Ok(false),
        Some("1") => Ok(true),
        Some(_) => Err(DiscoveryError::BadRequest("refresh 只能是 1".into())),
    }
}

pub async fn fetch(client: &Client, db: &Path, path: &str) -> Result<Value, DiscoveryError> {
    let requested = emotion::date_parameter(path)?;
    let days = param_days(path)?;
    let refresh = param_refresh(path)?;
    let now = emotion::now_china();
    let calendar = emotion::trade_dates(client).await?;
    let (date, _) = emotion::review_day(&calendar, requested.as_deref(), now)?;
    let mut gaps: Vec<String> = Vec::new();

    // 多取一天:窗口第一天的入场日是它前一个交易日
    let (series, _) = emotion_history::series(client, db, &calendar, &date, days + 1, refresh, now).await.map_err(DiscoveryError::Upstream)?;
    if series.len() < 2 {
        gaps.push("交易日历里列不出之前的交易日,只统计这一天".into());
    }
    let window: Vec<String> = series.iter().rev().take(days).rev().map(|s| s.date.clone()).collect();
    let entry_of: HashMap<&str, &DayRecord> = series.windows(2).map(|w| (w[1].date.as_str(), &w[0])).collect();

    let floor = emotion_history::retention_floor(now);
    let fetched: Vec<Fetched> = stream::iter(window.clone())
        .map(|d| {
            let (calendar, floor, target) = (&calendar, &floor, &date);
            async move {
                let settled = emotion::is_settled(&d, calendar, now);
                if !(refresh && d == *target) {
                    if let Some(rows) = cache_load(db, &d) {
                        return Fetched { date: d, rows: Some(rows), settled: true, note: String::new() };
                    }
                }
                if d.as_str() < floor.as_str() {
                    return Fetched { date: d, rows: None, settled, note: "超过东财保留期(约一个月),当时也没缓存".into() };
                }
                match emotion::fetch_pool(client, PoolKind::Yesterday, &d.replace('-', "")).await {
                    Ok(pool) => {
                        let rows = rows_of(&pool, &d);
                        if rows.is_empty() {
                            Fetched { date: d, rows: None, settled, note: "明细是空的".into() }
                        } else {
                            if settled {
                                let _ = cache_store(db, &d, &rows);
                            }
                            Fetched { date: d, rows: Some(rows), settled, note: String::new() }
                        }
                    }
                    Err(e) => Fetched { date: d.clone(), rows: None, settled, note: emotion::error_message("昨日涨停表现池", &e) },
                }
            }
        })
        .buffered(CONCURRENCY)
        .collect()
        .await;

    let mut per_day: BTreeMap<String, Vec<SampleRow>> = BTreeMap::new();
    let (mut missing, mut unsettled): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for f in fetched.iter() {
        match &f.rows {
            None => missing.push(f.date.clone()),
            Some(_) if !f.settled => unsettled.push(f.date.clone()),
            Some(rows) => {
                per_day.insert(f.date.clone(), rows.clone());
            }
        }
    }
    if let Some(target) = fetched.iter().find(|f| f.date == date).filter(|f| f.rows.is_none()) {
        gaps.push(format!("{date} 的涨停样本没取到{}", if target.note.is_empty() { String::new() } else { format!("({})", target.note) }));
    }
    let older = missing.iter().filter(|d| **d != date).count();
    if older > 0 {
        gaps.push(format!("{older} 个交易日的涨停样本没取到(东财只留最近一个月左右,更早又没缓存过的日子取不到),已剔除"));
    }
    for d in &unsettled {
        gaps.push(format!("{d} 还没收盘,不进统计"));
    }

    let used: Vec<&String> = per_day.keys().collect();
    let mut body = json!({
        "date": date, "days": days, "layer": "market_phenomenon", "sample_caveat": CAVEAT,
        "days_used": used.len(), "date_from": used.first(), "date_to": used.last(),
        "missing_days": missing, "unsettled_days": unsettled, "generated_at": Utc::now().to_rfc3339(), "source": SOURCE,
    });
    if used.len() < MIN_DAYS {
        body["available"] = json!(false);
        body["reason"] = json!(format!("取到样本的交易日不足({}/{},至少 {MIN_DAYS} 天)", used.len(), window.len()));
        body["regime_cuts"] = json!({"low": null, "high": null});
        body["seal_curve"] = json!([]);
        body["strategies"] = json!([]);
        body["gaps"] = json!(gaps);
        return Ok(body);
    }

    let scores: HashMap<&str, Option<f64>> = used.iter().map(|d| (d.as_str(), entry_score(entry_of.get(d.as_str()).copied()))).collect();
    let score_list: Vec<Option<f64>> = used.iter().map(|d| scores[d.as_str()]).collect();
    let cuts = regime_cuts(&score_list);
    let unscored = score_list.iter().filter(|s| s.is_none()).count();
    if cuts.0.is_none() {
        gaps.push(format!("入场日情绪读数只有 {} 天,不足 {MIN_REGIME_DAYS} 天,不分环境", used.len() - unscored));
    } else if unscored > 0 {
        gaps.push(format!("{unscored} 天的入场日情绪读数没取到,归「环境未覆盖」"));
    }
    let regimes: HashMap<String, &'static str> = used.iter().map(|d| ((*d).clone(), regime_of(scores[d.as_str()], cuts))).collect();

    body["available"] = json!(true);
    body["reason"] = Value::Null;
    body["regime_cuts"] = json!({"low": cuts.0, "high": cuts.1});
    body["seal_curve"] = json!(seal_curve(&per_day));
    body["strategies"] = json!(STRATEGIES.iter().map(|s| run_strategy(s, &per_day, &regimes)).collect::<Vec<_>>());
    body["gaps"] = json!(gaps);
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(code: &str, ret: f64, prev: i64, seal: &str, sector: &str, up: Option<bool>) -> SampleRow {
        SampleRow { code: code.into(), name: format!("N{code}"), ret, prev_boards: prev, seal_time: seal.into(), sector: sector.into(), limit_up: up }
    }

    #[test]
    fn stats_use_null_for_empty_and_skip_unjudged_limit_ups() {
        assert_eq!(stats(&[])["avg"], Value::Null);
        assert_eq!(stats(&[])["sample"], 0);
        let rows = [row("1", 10.0, 1, "", "", Some(true)), row("2", -4.0, 1, "", "", Some(false)), row("3", 2.0, 1, "", "", None)];
        let refs: Vec<&SampleRow> = rows.iter().collect();
        let s = stats(&refs);
        assert_eq!((s["sample"].clone(), s["win_rate"].clone(), s["avg"].clone(), s["median"].clone()), (json!(3), json!(0.667), json!(2.67), json!(2.0)));
        assert_eq!((s["best"].clone(), s["worst"].clone(), s["limit_up_rate"].clone()), (json!(10.0), json!(-4.0), json!(0.5)));
        let none = [row("9", 1.0, 1, "", "", None)];
        assert_eq!(stats(&none.iter().collect::<Vec<_>>())["limit_up_rate"], Value::Null, "全判不了不是 0");
    }

    #[test]
    fn strategies_filter_by_boards_seal_time_and_top_two_sectors() {
        let rows = vec![
            row("1", 1.0, 1, "093000", "汽车", None), row("2", 2.0, 1, "093000", "汽车", None), row("3", 3.0, 1, "100000", "电力", None),
            row("4", 4.0, 1, "143000", "电力", None), row("5", 5.0, 1, "144500", "银行", None), row("6", -1.0, 3, "", "", None), row("7", 0.5, 2, "", "银行", None),
        ];
        let ctx = day_context(&rows);
        assert_eq!(ctx.main_sectors, vec!["汽车".to_string(), "电力".to_string()], "家数并列(汽车、电力、银行各 2)时按行业名的字符顺序排,取前两个");
        let count = |name: &str| {
            let s = STRATEGIES.iter().find(|s| s.name == name).unwrap();
            rows.iter().filter(|r| (s.filter)(r, &ctx)).map(|r| r.code.as_str()).collect::<Vec<_>>()
        };
        assert_eq!(count("首板打板"), vec!["1", "2", "3", "4", "5"]);
        assert_eq!(count("首板·早封"), vec!["1", "2", "3"], "10:00 整含在内");
        assert_eq!(count("首板·尾盘封"), vec!["4", "5"], "14:30 整含在内");
        assert_eq!(count("首板·涨停数前二行业"), vec!["1", "2", "3", "4"]);
        assert_eq!(count("连板接力"), vec!["6", "7"]);
        assert_eq!(count("高标接力"), vec!["6"]);
        assert_eq!(count("全体涨停").len(), 7);
    }

    #[test]
    fn seal_curve_counts_only_first_boards_and_keeps_unknown_separate() {
        let mut per_day = BTreeMap::new();
        per_day.insert("2026-10-15".to_string(), vec![
            row("1", 1.0, 1, "092500", "", None), row("2", 2.0, 1, "093500", "", None), row("3", 3.0, 1, "093501", "", None),
            row("4", 4.0, 1, "150000", "", None), row("5", 9.0, 2, "092500", "", None), row("6", 6.0, 1, "", "", None),
        ]);
        let curve = seal_curve(&per_day);
        let sample = |i: usize| curve[i]["sample"].as_u64().unwrap();
        assert_eq!(curve.len(), 6);
        assert_eq!((sample(0), sample(1), sample(2), sample(3), sample(4), sample(5)), (2, 1, 0, 0, 1, 1));
        assert_eq!(curve[2]["avg"], Value::Null, "没有样本的档不是 0");
        assert_eq!(curve[5]["bucket"], SEAL_UNKNOWN);
    }

    #[test]
    fn regimes_split_by_entry_day_sentiment_with_thirds() {
        let mut day = DayRecord::default();
        day.values.insert("limit_up_count".into(), Some(50.0));
        day.values.insert("highest_board".into(), Some(5.0));
        day.values.insert("broken_rate".into(), Some(0.25));
        assert_eq!(entry_score(Some(&day)), Some(225.0));
        day.values.insert("broken_rate".into(), None);
        assert_eq!(entry_score(Some(&day)), None, "缺一项不打分");
        assert_eq!(entry_score(None), None);
        assert_eq!(regime_cuts(&[Some(1.0), Some(2.0), Some(3.0), Some(4.0), Some(5.0)]), (None, None), "不足 6 天不分环境");
        let cuts = regime_cuts(&[Some(60.0), Some(10.0), Some(20.0), Some(30.0), Some(40.0), Some(50.0), None]);
        assert_eq!(cuts, (Some(30.0), Some(50.0)));
        assert_eq!(regime_of(Some(30.0), cuts), "情绪弱");
        assert_eq!(regime_of(Some(40.0), cuts), "情绪中");
        assert_eq!(regime_of(Some(50.0), cuts), "情绪强");
        assert_eq!(regime_of(None, cuts), "环境未覆盖");
        assert_eq!(regime_of(Some(40.0), (None, None)), "环境未覆盖");
    }

    #[test]
    fn strategy_curve_compounds_daily_averages_and_splits_regimes() {
        let mut per_day = BTreeMap::new();
        per_day.insert("2026-10-14".to_string(), vec![row("1", 10.0, 1, "", "", None), row("2", 0.0, 1, "", "", None)]);
        per_day.insert("2026-10-15".to_string(), vec![row("3", 5.0, 2, "", "", None)]);
        per_day.insert("2026-10-16".to_string(), vec![row("4", -10.0, 1, "", "", None)]);
        let regimes: HashMap<String, &'static str> = [("2026-10-14", "情绪强"), ("2026-10-15", "情绪中")].into_iter().map(|(d, r)| (d.to_string(), r)).collect();
        let s = STRATEGIES.iter().find(|s| s.name == "首板打板").unwrap();
        let out = run_strategy(s, &per_day, &regimes);
        assert_eq!(out["overall"]["sample"], 3);
        assert_eq!(out["daily"][0]["avg"], 5.0);
        assert_eq!(out["daily"][1]["avg"], Value::Null, "当天没有样本不动权益");
        assert_eq!(out["daily"][1]["equity"], 105.0);
        assert_eq!(out["final_equity"], 94.5);
        assert_eq!(out["by_regime"]["情绪强"]["sample"], 2);
        assert_eq!(out["by_regime"]["情绪中"]["sample"], 0);
        assert_eq!(out["by_regime"]["环境未覆盖"]["sample"], 1, "没标环境的日子归未覆盖");
    }

    #[test]
    fn pool_rows_drop_missing_returns_and_judge_limit_up_by_day_rules() {
        let pool = vec![
            Row { code: "600001".into(), name: "A".into(), ret: Some(10.0), close: Some(11.0), limit_price: Some(11.0), prev_boards: 2, last_seal: "093000".into(), sector: "汽车".into(), ..Row::default() },
            Row { code: "600002".into(), name: "B".into(), ret: Some(3.0), close: Some(10.3), limit_price: Some(11.0), prev_boards: 1, ..Row::default() },
            Row { code: "600003".into(), name: "C".into(), ret: None, prev_boards: 1, ..Row::default() },
            Row { code: "600004".into(), name: "D".into(), ret: Some(9.8), prev_boards: 0, ..Row::default() },
        ];
        let rows = rows_of(&pool, "2026-10-16");
        assert_eq!(rows.iter().map(|r| r.code.as_str()).collect::<Vec<_>>(), vec!["600001", "600002", "600004"]);
        assert_eq!((rows[0].limit_up, rows[1].limit_up, rows[2].limit_up), (Some(true), Some(false), Some(true)), "没有涨停价时按当天制度的涨跌幅判");
        assert_eq!((rows[0].prev_boards, rows[2].prev_boards), (2, 1));
    }

    #[test]
    fn sample_cache_round_trips() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-samples-{nonce}-{}", std::process::id()));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let db = app.db_path().to_path_buf();
        let rows = vec![row("1", 1.5, 1, "093000", "汽车", Some(false))];
        cache_store(&db, "2026-10-15", &rows).unwrap();
        assert_eq!(cache_load(&db, "2026-10-15"), Some(rows));
        assert_eq!(cache_load(&db, "2026-10-14"), None);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 对真实上游:cargo test --locked samples::tests::live_samples_probe -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_samples_probe() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-samples-live-{nonce}"));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let started = std::time::Instant::now();
        let r = app.handle(crate::MobileApiRequest { path: "/mobile/shortline/samples?days=30".into(), method: "GET".into(), body: Value::Null }).await;
        println!("status {} in {:?}; available {} used {} {}~{} missing {} gaps {}", r.status, started.elapsed(), r.body["available"], r.body["days_used"], r.body["date_from"], r.body["date_to"], r.body["missing_days"].as_array().map_or(0, Vec::len), r.body["gaps"]);
        for s in r.body["strategies"].as_array().into_iter().flatten() {
            println!("{} overall {} final {}", s["name"], s["overall"], s["final_equity"]);
        }
        if let Ok(out) = std::env::var("VIBE_SAMPLES_FIXTURE") {
            std::fs::write(out, serde_json::to_string_pretty(&r.body).unwrap()).unwrap();
        }
        let again = std::time::Instant::now();
        let r2 = app.handle(crate::MobileApiRequest { path: "/mobile/shortline/samples?days=30".into(), method: "GET".into(), body: Value::Null }).await;
        println!("second call {:?}, same strategies: {}", again.elapsed(), r2.body["strategies"] == r.body["strategies"]);
        assert_eq!(r.status, 200);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
