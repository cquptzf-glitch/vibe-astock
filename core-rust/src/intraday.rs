//! 盯盘 · 竞价与盘中路径(移植自 orchestrator/src/finance/shortline/intraday.ts)。
//!
//! 昨晚判了「退潮」,今早到底应验没有?09:25 竞价一出来就有一部分答案;同样收盘 40 家涨停,
//! 早上 60 家炸到 40 家和 15 家扩散到 40 家含义相反 —— 只有按时点留下快照才看得出来。
//!
//! - 时点:09:25 / 09:35 / 10:00 / 11:30 / 14:00 / 15:00(北京时间),每个时点之后 8 分钟内有效,过了不补。
//!   09:25 只到 09:29(进了连续竞价,竞价读数就没了)。
//! - 拿的是**此刻**的行情,只能当天、当场存;历史日绝不现抓。每只行情的时间必须是今天(节假日腾讯给的是节前收盘价)。
//! - 每张快照:昨日涨停股此刻的整体 / 分板位强弱、最高标读数(09:25 用开盘价,其余用最新价);
//!   除 09:25 外再存盘中情绪(东财涨停 / 炸板 / 跌停池)。只给市场层面的聚合,个股只列客观读数,不排序、不评分。
//!
//! 手机上没有常驻的定时任务:App 开着「实时动态」或「竞价与盘中」时,每分钟检查一次到没到时点窗口,到了就存;
//! 也可以在窗口里手动存一张。快照是机器存的公开盘面数据,存 SQLite,不进备份。

use crate::{
    discovery::DiscoveryError,
    emotion::{self, PoolKind, Row},
    emotion_history::{self, Item},
    watch::{self, Clock, WQuote},
};
use anyhow::Result;
use futures_util::lock::Mutex;
use reqwest::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
};

pub(crate) const SLOTS: [&str; 6] = ["09:25", "09:35", "10:00", "11:30", "14:00", "15:00"];
/// 时点与真实抓取时刻允许的最大偏差(分钟):10:30 打开页面抓一张标着 09:25 的快照,竞价读数就成了假事实
const MAX_DRIFT_MIN: i64 = 8;
const SCHEMA: i64 = 1;
/// 一个时点窗口里最多试几次(每分钟一次)
const MAX_TRIES: i64 = 6;
/// 快照只留一年
const RETENTION_DAYS: i64 = 400;
const SOURCE: &str =
    "腾讯实时行情(昨日涨停股此刻涨跌)+ 东财涨停 / 炸板 / 跌停池(盘中情绪);名单为上一交易日涨停池";
const MISSING_REASON: &str = "这个时点没存到快照(当时 App 没开着盯盘、不是交易日,或取数失败)";
const TIER_ORDER: [&str; 3] = ["首板", "2板", "3板及以上"];

// ---------------------------------------------------------------- 时点窗口(纯计算)

fn minutes(hm: &str) -> Option<i64> {
    let (h, m) = hm.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (i64, i64) = (h.parse().ok()?, m.parse().ok()?);
    ((0..24).contains(&h) && (0..60).contains(&m)).then_some(h * 60 + m)
}

fn hm_of(total: i64) -> String {
    format!("{:02}:{:02}", total / 60, total % 60)
}

/// now 距 slot 过了几分钟;还没到 / 格式坏了 = None
pub(crate) fn drift_minutes(slot: &str, hm: &str) -> Option<i64> {
    let d = minutes(hm)? - minutes(slot)?;
    (d >= 0).then_some(d)
}

/// 这个时刻能不能算作这个时点的快照:只收成交时段,午收 / 日收允许取数晚 8 分钟
pub(crate) fn valid_window(slot: &str, hm: &str) -> bool {
    match drift_minutes(slot, hm) {
        Some(d) if d <= MAX_DRIFT_MIN => {}
        _ => return false,
    }
    if slot == "09:25" {
        return ("09:25".."09:30").contains(&hm);
    }
    if slot == "11:30" || slot == "15:00" {
        return true;
    }
    (("09:30".."11:30").contains(&slot) && hm < "11:30")
        || (("13:00".."15:00").contains(&slot) && hm < "15:00")
}

/// 此刻落在哪个时点的窗口里
pub(crate) fn slot_due(hm: &str) -> Option<&'static str> {
    SLOTS.into_iter().find(|s| valid_window(s, hm))
}

/// 这个时点的窗口最后一分钟
pub(crate) fn window_end(slot: &str) -> String {
    if slot == "09:25" {
        "09:29".into()
    } else {
        hm_of(minutes(slot).unwrap_or(0) + MAX_DRIFT_MIN)
    }
}

/// 时点和此刻对不上的原因;对得上 = None
pub(crate) fn timing_problem(slot: &str, hm: &str) -> Option<String> {
    if hm < slot {
        return Some(format!("现在 {hm},还没到 {slot}"));
    }
    if slot == "09:25" && hm >= "09:30" {
        return Some("已进入连续竞价,09:25 竞价快照补不回来".into());
    }
    match drift_minutes(slot, hm) {
        Some(d) if d <= MAX_DRIFT_MIN => {}
        _ => {
            return Some(format!(
                "现在 {hm},距 {slot} 已超过 {MAX_DRIFT_MIN} 分钟,不能当作这个时点的快照"
            ))
        }
    }
    (!valid_window(slot, hm)).then(|| "现在不在有效成交时段,午休的行情不能存成盘中快照".into())
}

// ---------------------------------------------------------------- 快照计算(纯计算)

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LadderStock {
    pub code: String,
    pub name: String,
    pub boards: i64,
    pub sector: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct TierStat {
    pub sample: usize,
    pub avg: f64,
    pub median: f64,
    pub up_rate: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct TopBoard {
    pub code: String,
    pub name: String,
    pub boards: i64,
    pub sector: String,
    pub pct: Option<f64>,
    pub at_limit: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct Cohort {
    pub expected: usize,
    pub sample: usize,
    pub coverage_rate: f64,
    pub avg: f64,
    pub median: f64,
    pub up_count: usize,
    pub down_count: usize,
    pub up_rate: f64,
    pub deep_loss_5: usize,
    pub by_tier: BTreeMap<String, TierStat>,
    pub top_board_level: i64,
    pub top_boards: Vec<TopBoard>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct LiveCounts {
    pub limit_up: Option<f64>,
    pub broken: Option<f64>,
    pub limit_down: Option<f64>,
    pub highest_board: Option<f64>,
    pub consec_count: Option<f64>,
    pub seal_rate: Option<f64>,
    pub broken_rate: Option<f64>,
    pub promotion_1to2: Option<f64>,
    pub promotion_all: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct Snapshot {
    pub schema: i64,
    pub date: String,
    pub slot: String,
    pub captured_at: String,
    pub clock: String,
    pub drift_minutes: i64,
    pub basis: String,
    pub prev_date: String,
    pub cohort: Cohort,
    pub live: Option<LiveCounts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_reason: Option<String>,
    pub gaps: Vec<String>,
}

pub(crate) fn tier_of(boards: i64) -> &'static str {
    match boards {
        b if b <= 1 => "首板",
        2 => "2板",
        _ => "3板及以上",
    }
}

/// 一只票在这个口径下的涨跌幅;行情时间不是这一天(节假日、停牌、取数失败)一律不算
pub(crate) fn quote_pct(q: Option<&WQuote>, basis: &str, day: &str) -> Option<f64> {
    let q = q?;
    let ymd = day.replace('-', "");
    if q.quote_time.len() != 14 || !q.quote_time.starts_with(&ymd) {
        return None;
    }
    if basis == "open" {
        let (open, last) = (q.open?, q.last_close?);
        if &q.quote_time[8..12] < "0925" || open <= 0.0 || last <= 0.0 {
            return None;
        }
        return Some((open / last - 1.0) * 100.0);
    }
    q.pct
}

fn r2(v: f64) -> f64 {
    emotion::r2(v)
}

fn stat(values: &[f64]) -> TierStat {
    TierStat {
        sample: values.len(),
        avg: r2(values.iter().sum::<f64>() / values.len() as f64),
        median: r2(emotion::median(values)),
        up_rate: emotion::r3(
            values.iter().filter(|v| **v > 0.0).count() as f64 / values.len() as f64,
        ),
    }
}

/// 昨日涨停股此刻的强弱。一只都没取到 = None(不存快照)
pub(crate) fn cohort_stats(
    stocks: &[LadderStock],
    quotes: &HashMap<String, WQuote>,
    basis: &str,
    day: &str,
) -> Option<Cohort> {
    let got: Vec<(&LadderStock, f64)> = stocks
        .iter()
        .filter_map(|s| quote_pct(quotes.get(&s.code), basis, day).map(|v| (s, v)))
        .collect();
    if got.is_empty() {
        return None;
    }
    let values: Vec<f64> = got.iter().map(|(_, v)| *v).collect();
    let mut tiers: HashMap<&str, Vec<f64>> = HashMap::new();
    for (s, v) in &got {
        tiers.entry(tier_of(s.boards)).or_default().push(*v);
    }
    let top = stocks.iter().map(|s| s.boards).max().unwrap_or(1);
    let pct_of: HashMap<&str, f64> = got.iter().map(|(s, v)| (s.code.as_str(), *v)).collect();
    let all = stat(&values);
    Some(Cohort {
        expected: stocks.len(),
        sample: values.len(),
        coverage_rate: emotion::r3(values.len() as f64 / stocks.len() as f64),
        avg: all.avg,
        median: all.median,
        up_count: values.iter().filter(|v| **v > 0.0).count(),
        down_count: values.iter().filter(|v| **v < 0.0).count(),
        up_rate: all.up_rate,
        deep_loss_5: values.iter().filter(|v| **v <= -5.0).count(),
        by_tier: TIER_ORDER
            .iter()
            .filter_map(|k| tiers.get(k).map(|vs| (k.to_string(), stat(vs))))
            .collect(),
        top_board_level: top,
        top_boards: stocks
            .iter()
            .filter(|s| s.boards == top)
            .map(|s| {
                let q = quotes.get(&s.code);
                let v = pct_of.get(s.code.as_str()).copied();
                // 封没封只在最新价口径下说;竞价价位碰到涨停价不等于封住
                let at_limit = (basis == "latest" && v.is_some())
                    .then(|| {
                        q.and_then(|q| q.price.zip(q.limit_up))
                            .map(|(p, l)| p >= l - 1e-6)
                    })
                    .flatten();
                TopBoard {
                    code: s.code.clone(),
                    name: if s.name.is_empty() {
                        q.map(|q| q.name.clone()).unwrap_or_else(|| s.code.clone())
                    } else {
                        s.name.clone()
                    },
                    boards: s.boards,
                    sector: s.sector.clone(),
                    pct: v.map(r2),
                    at_limit,
                }
            })
            .collect(),
    })
}

/// 盘中情绪:今天三张池 + 上一交易日涨停池。只有涨停池必需;其余各自可缺,缺的项是 None,不当 0
pub(crate) fn live_counts(
    zt: &[Row],
    zb: Option<&[Row]>,
    dt: Option<&[Row]>,
    prev_zt: Option<&[Row]>,
) -> LiveCounts {
    let n = zt.len() as f64;
    let today: HashSet<&str> = zt.iter().map(|r| r.code.as_str()).collect();
    let tries = zb.map(|zb| zb.len() as f64 + n).filter(|t| *t > 0.0);
    LiveCounts {
        limit_up: Some(n),
        broken: zb.map(|zb| zb.len() as f64),
        limit_down: dt.map(|dt| dt.len() as f64),
        highest_board: Some(zt.iter().map(|r| r.boards.max(1)).max().unwrap_or(0) as f64),
        consec_count: Some(zt.iter().filter(|r| r.boards >= 2).count() as f64),
        seal_rate: tries.map(|t| emotion::r3(n / t)),
        broken_rate: zb
            .zip(tries)
            .map(|(zb, t)| emotion::r3(zb.len() as f64 / t)),
        promotion_1to2: prev_zt.and_then(|prev| {
            let firsts: Vec<&Row> = prev.iter().filter(|r| r.boards <= 1).collect();
            (!firsts.is_empty()).then(|| {
                emotion::r3(
                    firsts
                        .iter()
                        .filter(|r| today.contains(r.code.as_str()))
                        .count() as f64
                        / firsts.len() as f64,
                )
            })
        }),
        promotion_all: prev_zt.filter(|p| !p.is_empty()).map(|prev| {
            emotion::r3(
                prev.iter()
                    .filter(|r| today.contains(r.code.as_str()))
                    .count() as f64
                    / prev.len() as f64,
            )
        }),
    }
}

/// 快照里能给出明日验证某项的盘中读数吗(头部行业集中度只有收盘后才有)
pub(crate) fn slot_reading(s: &Snapshot, metric: &str) -> Option<f64> {
    let live = s.live.as_ref();
    match metric {
        "money_effect_median" => Some(s.cohort.median),
        "deep_loss_count" => Some(s.cohort.deep_loss_5 as f64),
        "limit_up_count" => live.and_then(|l| l.limit_up),
        "highest_board" => live.and_then(|l| l.highest_board),
        "promotion_1to2" => live.and_then(|l| l.promotion_1to2),
        "broken_rate" => live.and_then(|l| l.broken_rate),
        "market_limit_down" => live.and_then(|l| l.limit_down),
        _ => None,
    }
}

const CLOSE_ONLY: [&str; 1] = ["theme_concentration"];

// ---------------------------------------------------------------- 存储

fn load_day(db: &Path, day: &str) -> Result<(Vec<Snapshot>, Vec<String>)> {
    let conn = Connection::open(db)?;
    let mut stmt =
        conn.prepare("SELECT slot, payload FROM mobile_intraday_snapshots WHERE date = ?1")?;
    let rows: Vec<(String, String)> = stmt
        .query_map([day], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let mut by: HashMap<String, String> = rows.into_iter().collect();
    let mut snapshots = Vec::new();
    let mut warnings = Vec::new();
    for slot in SLOTS {
        let Some(payload) = by.remove(slot) else {
            continue;
        };
        match serde_json::from_str::<Snapshot>(&payload) {
            Ok(s)
                if s.schema == SCHEMA
                    && s.date == day
                    && s.slot == slot
                    && s.clock.len() >= 5
                    && valid_window(slot, &s.clock[..5])
                    && s.basis == if slot == "09:25" { "open" } else { "latest" } =>
            {
                snapshots.push(s)
            }
            Ok(_) => warnings.push(format!(
                "{slot} 的快照不是这个时点当场存的(或格式不对),原件保留,没算进来"
            )),
            Err(_) => warnings.push(format!("{slot} 的快照读不出来,原件保留,没算进来")),
        }
    }
    Ok((snapshots, warnings))
}

/// 只新建、不覆盖:同一天同一时点已经有一张时返回 false(先存的那张保留)
fn insert_snapshot(db: &Path, s: &Snapshot) -> Result<bool> {
    let changed = Connection::open(db)?.execute(
        "INSERT OR IGNORE INTO mobile_intraday_snapshots(date, slot, payload, captured_at) VALUES (?1, ?2, ?3, ?4)",
        params![s.date, s.slot, serde_json::to_string(s)?, s.captured_at],
    )?;
    Ok(changed == 1)
}

fn has_snapshot(db: &Path, day: &str, slot: &str) -> Result<bool> {
    Ok(Connection::open(db)?
        .query_row(
            "SELECT 1 FROM mobile_intraday_snapshots WHERE date = ?1 AND slot = ?2",
            [day, slot],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SlotStatus {
    pub state: String,
    pub reason: Option<String>,
    pub tries: i64,
}

fn read_status(db: &Path, day: &str) -> Result<HashMap<String, SlotStatus>> {
    let conn = Connection::open(db)?;
    let mut stmt = conn
        .prepare("SELECT slot, state, reason, tries FROM mobile_intraday_status WHERE date = ?1")?;
    let rows = stmt.query_map([day], |r| {
        Ok((
            r.get::<_, String>(0)?,
            SlotStatus {
                state: r.get(1)?,
                reason: r.get(2)?,
                tries: r.get(3)?,
            },
        ))
    })?;
    Ok(rows.flatten().collect())
}

fn write_status(db: &Path, day: &str, slot: &str, st: &SlotStatus, at: &str) -> Result<()> {
    Connection::open(db)?.execute(
        "INSERT INTO mobile_intraday_status(date, slot, state, reason, at, tries) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(date, slot) DO UPDATE SET state = excluded.state, reason = excluded.reason, at = excluded.at, tries = excluded.tries",
        params![day, slot, st.state, st.reason, at, st.tries],
    )?;
    Ok(())
}

fn prune(db: &Path, day: &str) -> Result<()> {
    let today = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")?;
    let floor = (today - chrono::Duration::days(RETENTION_DAYS))
        .format("%Y-%m-%d")
        .to_string();
    let conn = Connection::open(db)?;
    conn.execute(
        "DELETE FROM mobile_intraday_snapshots WHERE date < ?1",
        [&floor],
    )?;
    conn.execute(
        "DELETE FROM mobile_intraday_status WHERE date < ?1",
        [&floor],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- 抓取

#[derive(Default)]
pub struct IntradayState {
    calendar: Option<Vec<String>>,
    calendar_at: i64,
    calendar_ok: bool,
    last_beat: i64,
}

pub type SharedIntraday = Mutex<IntradayState>;

async fn calendar(client: &Client, st: &mut IntradayState, clock: &Clock) -> Option<Vec<String>> {
    let ttl = if st.calendar_ok { 30 * 60_000 } else { 60_000 };
    if st.calendar_at == 0 || clock.ms - st.calendar_at >= ttl {
        st.calendar_at = clock.ms;
        match emotion::trade_dates(client).await {
            Ok(dates) => {
                st.calendar = Some(dates);
                st.calendar_ok = true;
            }
            Err(_) => st.calendar_ok = false,
        }
    }
    st.calendar.clone()
}

pub(crate) enum Capture {
    Saved(Box<Snapshot>),
    Failed(String),
    /// 这个时点在取数期间已经被别的请求存上了;先存的那张保留
    Already,
}

async fn capture(client: &Client, db: &Path, slot: &str, cal: Option<&[String]>) -> Capture {
    let start = Clock::now();
    let day = start.day.clone();
    if let Some(problem) = timing_problem(slot, &start.hms[..5]) {
        return Capture::Failed(problem);
    }
    let Some(cal) = cal else {
        return Capture::Failed("交易日历没取到,不知道昨日梯队该取哪一天".into());
    };
    let Some(prev) = cal
        .iter()
        .rev()
        .find(|d| d.as_str() < day.as_str())
        .cloned()
    else {
        return Capture::Failed("交易日历里找不到上一交易日".into());
    };
    let prev_compact = prev.replace('-', "");
    let prev_rows = match emotion::fetch_pool(client, PoolKind::LimitUp, &prev_compact).await {
        Ok(rows) => rows,
        Err(e) => {
            return Capture::Failed(format!(
                "昨日梯队没取到({})",
                emotion::error_message(&format!("{prev} 涨停池"), &e)
            ))
        }
    };
    let ladder = match watch::ladder_from_pool(&prev_rows, &prev) {
        Ok(rows) => rows,
        Err(reason) => return Capture::Failed(format!("昨日梯队没取到({reason})")),
    };
    let stocks: Vec<LadderStock> = ladder
        .into_iter()
        .map(|r| LadderStock {
            code: r.code,
            name: r.name,
            boards: r.boards,
            sector: r.sector,
        })
        .collect();
    let basis = if slot == "09:25" { "open" } else { "latest" };
    let codes: Vec<String> = stocks.iter().map(|s| s.code.clone()).collect();
    let (quotes, failed, batches) = watch::fetch_quotes(client, &codes).await;
    let mut gaps = Vec::new();
    if failed > 0 {
        gaps.push(format!(
            "行情分 {batches} 批取(每批最多 50 只),其中 {failed} 批没取到"
        ));
    }
    let Some(cohort) = cohort_stats(&stocks, &quotes, basis, &day) else {
        return Capture::Failed(if basis == "open" {
            "没取到今天 09:25 的开盘价(可能还没出,窗口内会再试)".into()
        } else {
            "行情时间都不是今天(可能今天没开市,或行情没取到)".into()
        });
    };
    if cohort.coverage_rate < 1.0 {
        gaps.push(format!(
            "昨日涨停 {} 只,取到今天行情的 {} 只",
            cohort.expected, cohort.sample
        ));
    }

    let (live, live_reason) = if basis == "latest" {
        let compact = day.replace('-', "");
        let (zt, zb, dt) = futures_util::join!(
            emotion::fetch_pool(client, PoolKind::LimitUp, &compact),
            emotion::fetch_pool(client, PoolKind::Broken, &compact),
            emotion::fetch_pool(client, PoolKind::LimitDown, &compact),
        );
        match zt {
            Ok(zt) if !zt.is_empty() => {
                let mut missing = Vec::new();
                if zb.is_err() {
                    missing.push("炸板池");
                }
                if dt.is_err() {
                    missing.push("跌停池");
                }
                let live = live_counts(
                    &zt,
                    zb.as_deref().ok(),
                    dt.as_deref().ok(),
                    Some(&prev_rows),
                );
                (
                    Some(live),
                    (!missing.is_empty())
                        .then(|| format!("{}没取到,对应家数为 —", missing.join("、"))),
                )
            }
            Ok(_) => (None, Some("盘中涨停池是空的(数据还没出)".to_string())),
            Err(e) => (None, Some(emotion::error_message("盘中涨停池", &e))),
        }
    } else {
        (
            None,
            Some("集合竞价阶段还没有当天的涨停 / 炸板池".to_string()),
        )
    };

    // 取数可能跨过窗口(尤其 09:25 跨进连续竞价):按取完的时刻再核一次,跨了就不存
    let end = Clock::now();
    if end.day != day {
        return Capture::Failed("取数跨过了零点,本次没存".into());
    }
    if let Some(late) = timing_problem(slot, &end.hms[..5]) {
        return Capture::Failed(format!("取数用了太久:{late},本次没存"));
    }
    let snapshot = Snapshot {
        schema: SCHEMA,
        date: day,
        slot: slot.into(),
        captured_at: chrono::Utc::now().to_rfc3339(),
        clock: end.hms.clone(),
        drift_minutes: drift_minutes(slot, &end.hms[..5]).unwrap_or(0),
        basis: basis.into(),
        prev_date: prev,
        cohort,
        live,
        live_reason,
        gaps,
    };
    match insert_snapshot(db, &snapshot) {
        Ok(true) => Capture::Saved(Box::new(snapshot)),
        Ok(false) => Capture::Already,
        Err(_) => Capture::Failed("快照没存上(本机存储有问题)".into()),
    }
}

/// 每分钟最多一次:到了某个时点的窗口、今天这个时点还没存、没试满次数,就存一张。盯盘页轮询时顺带调用。
pub async fn beat(client: &Client, db: &Path, state: &SharedIntraday) -> Option<String> {
    let clock = Clock::now();
    let slot = slot_due(&clock.hms[..5])?;
    if clock.weekend {
        return None;
    }
    let mut st = state.lock().await;
    if clock.ms - st.last_beat < 55_000 || has_snapshot(db, &clock.day, slot).unwrap_or(true) {
        return None;
    }
    st.last_beat = clock.ms;
    let prev = read_status(db, &clock.day).ok()?.remove(slot);
    if prev
        .as_ref()
        .is_some_and(|p| p.state != "failed" || p.tries >= MAX_TRIES)
    {
        return None;
    }
    let tries = prev.map_or(0, |p| p.tries) + 1;
    let at = chrono::Utc::now().to_rfc3339();
    let cal = calendar(client, &mut st, &clock).await;
    // 日历里没有今天、而且已经过了 09:35:节假日。取不到日历照样试(每只行情的时间必须是今天,节假日存不进去)
    if cal.as_ref().is_some_and(|c| !c.contains(&clock.day)) && clock.hm >= 935 {
        let _ = write_status(
            db,
            &clock.day,
            slot,
            &SlotStatus {
                state: "skipped".into(),
                reason: Some("今天休市".into()),
                tries,
            },
            &at,
        );
        return None;
    }
    let _ = prune(db, &clock.day);
    let result = capture(client, db, slot, cal.as_deref()).await;
    let status = match &result {
        Capture::Saved(_) => SlotStatus {
            state: "saved".into(),
            reason: None,
            tries,
        },
        Capture::Failed(reason) => SlotStatus {
            state: "failed".into(),
            reason: Some(reason.clone()),
            tries,
        },
        Capture::Already => return None,
    };
    let _ = write_status(db, &clock.day, slot, &status, &at);
    Some(match result {
        Capture::Saved(s) => format!(
            "盘中快照 {slot}:已存(昨日涨停 {}/{} 只)",
            s.cohort.sample, s.cohort.expected
        ),
        Capture::Failed(reason) => format!("盘中快照 {slot}:没存,{reason}"),
        Capture::Already => unreachable!(),
    })
}

/// 手动存一张:`{ "action": "capture" }`。只能存此刻所在时点的那张;已经存过的不重抓。
pub async fn act(
    client: &Client,
    db: &Path,
    state: &SharedIntraday,
    body: &Value,
) -> Result<Value, DiscoveryError> {
    if body.get("action").and_then(Value::as_str) != Some("capture") {
        return Err(DiscoveryError::BadRequest("action 只能是 capture".into()));
    }
    let clock = Clock::now();
    let hm = &clock.hms[..5];
    let Some(slot) = slot_due(hm) else {
        let windows: Vec<String> = SLOTS
            .iter()
            .map(|s| format!("{s}–{}", window_end(s)))
            .collect();
        return Err(DiscoveryError::BadRequest(format!(
            "现在({hm})不在任何快照时点的有效窗口里:{}",
            windows.join("、")
        )));
    };
    let mut st = state.lock().await;
    if has_snapshot(db, &clock.day, slot).map_err(DiscoveryError::Upstream)? {
        return Ok(json!({"slot": slot, "ok": true, "already": true, "reason": null}));
    }
    let tries = read_status(db, &clock.day)
        .map_err(DiscoveryError::Upstream)?
        .remove(slot)
        .map_or(0, |p| p.tries)
        + 1;
    let cal = calendar(client, &mut st, &clock).await;
    let result = capture(client, db, slot, cal.as_deref()).await;
    let at = chrono::Utc::now().to_rfc3339();
    Ok(match result {
        Capture::Saved(_) => {
            let _ = write_status(
                db,
                &clock.day,
                slot,
                &SlotStatus {
                    state: "saved".into(),
                    reason: None,
                    tries,
                },
                &at,
            );
            json!({"slot": slot, "ok": true, "already": false, "reason": null})
        }
        Capture::Already => json!({"slot": slot, "ok": true, "already": true, "reason": null}),
        Capture::Failed(reason) => {
            let _ = write_status(
                db,
                &clock.day,
                slot,
                &SlotStatus {
                    state: "failed".into(),
                    reason: Some(reason.clone()),
                    tries,
                },
                &at,
            );
            json!({"slot": slot, "ok": false, "already": false, "reason": reason})
        }
    })
}

// ---------------------------------------------------------------- 视图

/// 这一天开不开市:周末不开;交易日历里有 = 开;日历覆盖到了、却没有这一天 = 休市;今天 09:35 前日历还没有今天 = 说不准
pub(crate) fn trading_day(
    date: &str,
    weekend: bool,
    today: &str,
    hm: u32,
    cal: Option<&[String]>,
) -> Option<bool> {
    if weekend {
        return Some(false);
    }
    let cal = cal?;
    if cal.iter().any(|d| d == date) {
        return Some(true);
    }
    if date == today {
        return (hm >= 935).then_some(false);
    }
    cal.first()
        .is_some_and(|first| date >= first.as_str())
        .then_some(false)
}

fn is_weekend(date: &str) -> bool {
    use chrono::Datelike;
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .is_ok_and(|d| matches!(d.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun))
}

pub(crate) fn slot_rows(
    snapshots: &[Snapshot],
    status: &HashMap<String, SlotStatus>,
    closed: Option<&str>,
    date: &str,
    today: &str,
    hm: &str,
) -> Vec<Value> {
    SLOTS
        .iter()
        .map(|slot| {
            let snap = snapshots.iter().find(|s| s.slot == *slot);
            let st = status.get(*slot).filter(|s| s.state != "saved");
            let past = date < today || hm > window_end(slot).as_str();
            let (state, reason) = match (snap, st, closed) {
                (Some(_), _, _) => ("saved".to_string(), None),
                (None, Some(s), _) => (s.state.clone(), s.reason.clone()),
                (None, None, Some(c)) => ("skipped".into(), Some(c.to_string())),
                (None, None, None) if past => ("missing".into(), Some(MISSING_REASON.to_string())),
                _ => ("upcoming".into(), None),
            };
            json!({"slot": slot, "window_end": window_end(slot), "state": state, "reason": reason, "snapshot": snap})
        })
        .collect()
}

pub(crate) fn check_items(
    items: &[&Item],
    base: Option<&emotion_history::DayRecord>,
    snapshots: &[Snapshot],
) -> Vec<Value> {
    items
        .iter()
        .filter_map(|it| {
            let m = emotion_history::metric(&it.metric)?;
            let base_value = base.and_then(|b| b.value(m.key));
            let close_only = CLOSE_ONLY.contains(&m.key);
            let points: Vec<Value> = snapshots
                .iter()
                .map(|s| {
                    let value = if close_only { None } else { slot_reading(s, m.key) };
                    let actual = emotion_history::actual_direction(value, base_value, m.eps);
                    json!({
                        "slot": s.slot, "value": value, "actual": actual, "actual_label": actual.map(emotion_history::direction_label),
                        "verified": actual.map(|a| a == it.direction),
                    })
                })
                .collect();
            Some(json!({
                "id": it.id.to_string(), "metric": m.key, "label": m.label, "unit": m.unit, "kind": m.kind, "eps": m.eps,
                "direction": it.direction, "direction_label": emotion_history::direction_label(&it.direction), "reason": it.reason,
                "base_value": base_value, "close_only": close_only, "points": points,
            }))
        })
        .collect()
}

pub async fn view(
    client: &Client,
    db: &Path,
    state: &SharedIntraday,
    path: &str,
) -> Result<Value, DiscoveryError> {
    let clock = Clock::now();
    let date = emotion::date_parameter(path)?.unwrap_or_else(|| clock.day.clone());
    if date > clock.day {
        return Err(DiscoveryError::BadRequest(format!("{date} 还没到")));
    }
    let hm = clock.hms[..5].to_string();
    let (snapshots, warnings) = load_day(db, &date).map_err(DiscoveryError::Upstream)?;
    let status = read_status(db, &date).map_err(DiscoveryError::Upstream)?;
    let cal = {
        let mut st = state.lock().await;
        calendar(client, &mut st, &clock).await
    };
    let weekend = is_weekend(&date);
    let trading = trading_day(&date, weekend, &clock.day, clock.hm, cal.as_deref());
    let closed =
        (trading == Some(false)).then_some(if weekend { "周末休市" } else { "休市" });
    let slots = slot_rows(&snapshots, &status, closed, &date, &clock.day, &hm);
    let auction = snapshots.iter().find(|s| s.slot == "09:25");
    let auction_reason = if auction.is_some() {
        None
    } else if let Some(c) = closed {
        Some(format!("{c},没有竞价"))
    } else if slots[0]["state"] == "upcoming" {
        Some("09:25 竞价结果出来后,App 开着盯盘就会存(09:25–09:29)".to_string())
    } else {
        Some(format!(
            "09:25 竞价快照没有:{}。过了时点补不回来",
            slots[0]["reason"].as_str().unwrap_or(MISSING_REASON)
        ))
    };

    // 盘中核验:前一晚写下的条件,逐个时点给读数(暂定)
    let mut gaps = Vec::new();
    let set_date = snapshots
        .iter()
        .map(|s| s.prev_date.clone())
        .find(|d| !d.is_empty());
    let mut items = Vec::new();
    if let Some(set) = &set_date {
        let all = emotion_history::load_items(db).map_err(DiscoveryError::Upstream)?;
        let conds: Vec<&Item> = all.iter().filter(|it| it.date == *set).collect();
        if !conds.is_empty() {
            let base = match &cal {
                Some(cal) => match emotion_history::series(
                    client,
                    db,
                    cal,
                    set,
                    1,
                    false,
                    emotion::now_china(),
                )
                .await
                {
                    Ok((rows, _)) => rows
                        .into_iter()
                        .find(|r| r.date == *set && r.status != "missing"),
                    Err(_) => None,
                },
                None => None,
            };
            if base.is_none() {
                gaps.push(format!("{set} 的每日读数没取到,对应条件没有基准值"));
            }
            items = check_items(&conds, base.as_ref(), &snapshots);
        }
    }

    Ok(json!({
        "date": date,
        "today": date == clock.day,
        "trading_day": trading,
        "slots": slots,
        "auction": auction,
        "auction_reason": auction_reason,
        "checks": { "set_date": set_date, "items": items },
        "warnings": warnings,
        "gaps": gaps,
        "basis_labels": { "open": "当日开盘价 ÷ 昨收", "latest": "抓取时最新价 ÷ 昨收" },
        "source": SOURCE,
        "generated_at": chrono::Utc::now().to_rfc3339(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(
        pct: Option<f64>,
        open: Option<f64>,
        last: Option<f64>,
        time: &str,
        price: Option<f64>,
        limit_up: Option<f64>,
    ) -> WQuote {
        WQuote {
            name: String::new(),
            price,
            pct,
            amount_yuan: None,
            limit_up,
            limit_down: None,
            open,
            last_close: last,
            quote_time: time.into(),
        }
    }

    fn stock(code: &str, boards: i64) -> LadderStock {
        LadderStock {
            code: code.into(),
            name: format!("N{code}"),
            boards,
            sector: "汽车".into(),
        }
    }

    #[test]
    fn windows_match_the_desktop_rules() {
        assert!(valid_window("09:25", "09:25") && valid_window("09:25", "09:29"));
        assert!(
            !valid_window("09:25", "09:30"),
            "进了连续竞价就不是竞价快照"
        );
        assert!(valid_window("09:35", "09:43") && !valid_window("09:35", "09:44"));
        assert!(valid_window("11:30", "11:38"), "午收允许晚 8 分钟");
        assert!(valid_window("15:00", "15:08") && !valid_window("15:00", "15:09"));
        assert!(!valid_window("10:00", "09:59"));
        assert_eq!(slot_due("10:03"), Some("10:00"));
        assert_eq!(slot_due("12:00"), None);
        assert_eq!(slot_due("13:30"), None);
        assert_eq!(window_end("09:25"), "09:29");
        assert_eq!(window_end("14:00"), "14:08");
        assert_eq!(
            timing_problem("10:00", "09:58").unwrap(),
            "现在 09:58,还没到 10:00"
        );
        assert!(timing_problem("09:25", "09:31")
            .unwrap()
            .contains("补不回来"));
        assert!(timing_problem("10:00", "10:20")
            .unwrap()
            .contains("超过 8 分钟"));
        assert_eq!(timing_problem("14:00", "14:05"), None);
    }

    #[test]
    fn cohort_uses_open_for_auction_and_skips_stale_quotes() {
        let stocks = vec![
            stock("600001", 1),
            stock("600002", 1),
            stock("600003", 3),
            stock("600004", 2),
        ];
        let mut quotes = HashMap::new();
        quotes.insert(
            "600001".to_string(),
            q(
                Some(1.0),
                Some(10.5),
                Some(10.0),
                "20261016092503",
                Some(10.5),
                Some(11.0),
            ),
        );
        quotes.insert(
            "600002".to_string(),
            q(
                Some(-1.0),
                Some(9.4),
                Some(10.0),
                "20261016092503",
                Some(9.4),
                Some(11.0),
            ),
        );
        quotes.insert(
            "600003".to_string(),
            q(
                Some(10.0),
                Some(11.0),
                Some(10.0),
                "20261016092503",
                Some(11.0),
                Some(11.0),
            ),
        );
        // 节前收盘价:行情时间不是今天,不算
        quotes.insert(
            "600004".to_string(),
            q(
                Some(5.0),
                Some(10.0),
                Some(9.0),
                "20261015150000",
                Some(10.0),
                Some(11.0),
            ),
        );
        let c = cohort_stats(&stocks, &quotes, "open", "2026-10-16").unwrap();
        assert_eq!(
            (
                c.expected,
                c.sample,
                c.up_count,
                c.down_count,
                c.deep_loss_5
            ),
            (4, 3, 2, 1, 1)
        );
        assert_eq!(c.median, 5.0);
        assert_eq!(c.coverage_rate, 0.75);
        assert_eq!(
            c.by_tier.keys().cloned().collect::<Vec<_>>(),
            vec!["3板及以上".to_string(), "首板".into()]
        );
        assert_eq!(c.by_tier["首板"].sample, 2);
        assert_eq!(c.top_board_level, 3);
        assert_eq!(c.top_boards[0].pct, Some(10.0));
        assert_eq!(c.top_boards[0].at_limit, None, "竞价价位碰到涨停价不算封住");
        let latest = cohort_stats(&stocks, &quotes, "latest", "2026-10-16").unwrap();
        assert_eq!(latest.top_boards[0].at_limit, Some(true));
        assert!(cohort_stats(&stocks, &quotes, "open", "2026-10-17").is_none());
        let early = q(
            Some(1.0),
            Some(10.5),
            Some(10.0),
            "20261016092000",
            None,
            None,
        );
        assert_eq!(
            quote_pct(Some(&early), "open", "2026-10-16"),
            None,
            "09:25 之前没有开盘价"
        );
    }

    #[test]
    fn live_counts_keep_missing_pools_as_none() {
        let row = |code: &str, boards: i64| Row {
            code: code.into(),
            boards,
            ..Row::default()
        };
        let zt = vec![row("1", 1), row("2", 2), row("3", 4)];
        let zb = vec![row("9", 0)];
        let prev = vec![row("1", 1), row("5", 1), row("2", 1), row("6", 3)];
        let l = live_counts(&zt, Some(&zb), None, Some(&prev));
        assert_eq!(
            (
                l.limit_up,
                l.broken,
                l.limit_down,
                l.highest_board,
                l.consec_count
            ),
            (Some(3.0), Some(1.0), None, Some(4.0), Some(2.0))
        );
        assert_eq!((l.seal_rate, l.broken_rate), (Some(0.75), Some(0.25)));
        assert_eq!(l.promotion_1to2, Some(0.667));
        assert_eq!(l.promotion_all, Some(0.5));
        let none = live_counts(&zt, None, None, None);
        assert_eq!(
            (none.broken_rate, none.seal_rate, none.promotion_1to2),
            (None, None, None)
        );
    }

    fn snapshot(slot: &str, clock: &str, median: f64, live: Option<LiveCounts>) -> Snapshot {
        Snapshot {
            schema: SCHEMA,
            date: "2026-10-16".into(),
            slot: slot.into(),
            captured_at: "2026-10-16T02:00:00Z".into(),
            clock: clock.into(),
            drift_minutes: 1,
            basis: if slot == "09:25" {
                "open".into()
            } else {
                "latest".into()
            },
            prev_date: "2026-10-15".into(),
            cohort: Cohort {
                expected: 2,
                sample: 2,
                coverage_rate: 1.0,
                avg: median,
                median,
                up_count: 1,
                down_count: 1,
                up_rate: 0.5,
                deep_loss_5: 3,
                by_tier: BTreeMap::new(),
                top_board_level: 1,
                top_boards: vec![],
            },
            live,
            live_reason: None,
            gaps: vec![],
        }
    }

    #[test]
    fn slots_and_checks_view() {
        let snaps = vec![
            snapshot("09:25", "09:25:40", 1.2, None),
            snapshot(
                "10:00",
                "10:01:10",
                -0.4,
                Some(LiveCounts {
                    limit_up: Some(40.0),
                    broken_rate: Some(0.3),
                    ..LiveCounts::default()
                }),
            ),
        ];
        let mut status = HashMap::new();
        status.insert(
            "09:35".to_string(),
            SlotStatus {
                state: "failed".into(),
                reason: Some("行情时间都不是今天".into()),
                tries: 2,
            },
        );
        let rows = slot_rows(&snaps, &status, None, "2026-10-16", "2026-10-16", "10:30");
        let states: Vec<&str> = rows.iter().map(|r| r["state"].as_str().unwrap()).collect();
        assert_eq!(
            states,
            vec!["saved", "failed", "saved", "upcoming", "upcoming", "upcoming"]
        );
        let past = slot_rows(
            &[],
            &HashMap::new(),
            None,
            "2026-10-15",
            "2026-10-16",
            "10:30",
        );
        assert!(past.iter().all(|r| r["state"] == "missing"));
        let closed = slot_rows(
            &[],
            &HashMap::new(),
            Some("周末休市"),
            "2026-10-17",
            "2026-10-17",
            "10:30",
        );
        assert!(closed
            .iter()
            .all(|r| r["state"] == "skipped" && r["reason"] == "周末休市"));

        let item = |id: i64, metric: &str, direction: &str| Item {
            id,
            date: "2026-10-15".into(),
            metric: metric.into(),
            direction: direction.into(),
            reason: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let items = [
            item(1, "money_effect_median", "down"),
            item(2, "limit_up_count", "up"),
            item(3, "theme_concentration", "up"),
        ];
        let refs: Vec<&Item> = items.iter().collect();
        let mut base = emotion_history::DayRecord {
            date: "2026-10-15".into(),
            status: "ok".into(),
            ..Default::default()
        };
        base.values.insert("money_effect_median".into(), Some(0.5));
        base.values.insert("limit_up_count".into(), Some(36.0));
        let checks = check_items(&refs, Some(&base), &snaps);
        assert_eq!(checks[0]["points"][0]["actual"], "up");
        assert_eq!(checks[0]["points"][0]["verified"], false);
        assert_eq!(
            checks[0]["points"][1]["verified"], true,
            "-0.4 比昨天 0.5 低过 0.5 的门槛"
        );
        assert_eq!(
            checks[1]["points"][0]["value"],
            Value::Null,
            "竞价没有盘中家数"
        );
        assert_eq!(checks[1]["points"][0]["verified"], Value::Null);
        assert_eq!(
            checks[1]["points"][1]["verified"], false,
            "40 比 36 只多 4 家,没过 5 家的门槛,算持平"
        );
        assert_eq!(checks[2]["close_only"], true);
        assert_eq!(checks[2]["points"][1]["value"], Value::Null);
    }

    /// 对真实上游:cargo test --locked intraday::tests::live_intraday_probe -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_intraday_probe() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-intraday-live-{nonce}"));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let get = |path: &str| crate::MobileApiRequest {
            path: path.into(),
            method: "GET".into(),
            body: Value::Null,
        };
        let today = app.handle(get("/mobile/intraday")).await;
        println!(
            "today status {} trading {} auction_reason {}",
            today.status, today.body["trading_day"], today.body["auction_reason"]
        );
        println!(
            "slots {:?}",
            today.body["slots"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| format!("{} {}", s["slot"], s["state"]))
                .collect::<Vec<_>>()
        );
        if let Ok(out) = std::env::var("VIBE_INTRADAY_FIXTURE") {
            std::fs::write(out, serde_json::to_string_pretty(&today.body).unwrap()).unwrap();
        }
        let capture = app
            .handle(crate::MobileApiRequest {
                path: "/mobile/intraday".into(),
                method: "POST".into(),
                body: json!({"action": "capture"}),
            })
            .await;
        println!("capture status {} {}", capture.status, capture.body);
        assert_eq!(today.status, 200);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn trading_day_rules() {
        let cal: Vec<String> = ["2026-10-14", "2026-10-15", "2026-10-16"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            trading_day("2026-10-17", true, "2026-10-17", 1000, Some(&cal)),
            Some(false)
        );
        assert_eq!(
            trading_day("2026-10-16", false, "2026-10-16", 1000, Some(&cal)),
            Some(true)
        );
        assert_eq!(
            trading_day("2026-10-19", false, "2026-10-19", 900, Some(&cal)),
            None
        );
        assert_eq!(
            trading_day("2026-10-19", false, "2026-10-19", 1000, Some(&cal)),
            Some(false)
        );
        assert_eq!(
            trading_day("2026-10-01", false, "2026-10-19", 1000, Some(&cal)),
            None,
            "早于日历范围,说不准"
        );
        assert_eq!(
            trading_day("2026-10-16", false, "2026-10-19", 1000, None),
            None
        );
    }

    #[test]
    fn snapshots_are_insert_only_and_validated_on_load() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("vibe-intraday-{nonce}-{}", std::process::id()));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let db = app.db_path().to_path_buf();
        let first = snapshot("10:00", "10:01:10", 1.0, None);
        assert!(insert_snapshot(&db, &first).unwrap());
        assert!(
            !insert_snapshot(&db, &snapshot("10:00", "10:02:00", 9.0, None)).unwrap(),
            "先存的那张保留"
        );
        // 窗口外存的(不是当场存的)读出来不算
        assert!(insert_snapshot(&db, &snapshot("14:00", "14:30:00", 2.0, None)).unwrap());
        let (snaps, warnings) = load_day(&db, "2026-10-16").unwrap();
        assert_eq!(snaps, vec![first]);
        assert_eq!(warnings.len(), 1);
        assert!(has_snapshot(&db, "2026-10-16", "10:00").unwrap());
        write_status(
            &db,
            "2026-10-16",
            "09:35",
            &SlotStatus {
                state: "failed".into(),
                reason: Some("x".into()),
                tries: 1,
            },
            "t",
        )
        .unwrap();
        write_status(
            &db,
            "2026-10-16",
            "09:35",
            &SlotStatus {
                state: "failed".into(),
                reason: Some("y".into()),
                tries: 2,
            },
            "t",
        )
        .unwrap();
        assert_eq!(
            read_status(&db, "2026-10-16").unwrap()["09:35"],
            SlotStatus {
                state: "failed".into(),
                reason: Some("y".into()),
                tries: 2
            }
        );
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
