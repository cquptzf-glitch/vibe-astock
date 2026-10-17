//! 交易日志 · 最大浮盈浮亏(MFE / MAE 与盈利回吐,移植自 orchestrator/src/finance/journal/excursion.ts,口径来自 Vibe AStock v2.0.0 `duanxian/excursion.py`)。
//!
//! 一笔交易的盈亏只说了终点,不说过程:
//!   - 最大浮盈(MFE):这笔最多曾赚到多少。远高于实际落袋 = 盈利回吐;
//!   - 最大浮亏(MAE):这笔最多曾亏到多少;
//!   - 捕获率 = 实际落袋 ÷ 最大浮盈。
//!
//! ⚠️ 日 K 的先天限制(必须如实标出):只有日线最高 / 最低价,看不出极值发生在盘中买入之前还是之后。
//!    同日进出整段存疑,最大浮盈只是上界;跨日持有时中间完整交易日的高低点一定在持仓期内,只有两头存疑。
//!    所以另给 `*_certain`(只取严格落在买入日与卖出日之间的完整交易日):`mfe_certain` = 确定至少赚到过这么多;
//!    `mae_certain` = 确定至少差到过这个程度(**可能是正数 = 那几天从未浮亏**)。
//! ⚠️ 用的是**不复权**日线对照成交成本(桌面版同):现金分红除息会让低点偏低,在说明里写明。
//! 🔴 和桌面版不同:
//!    - 手机端只有清仓后才算已实现,所以只算**卖光了的**一轮;北交所腾讯 K 线取不到,不算。
//!    - 手机端没有送转流水,没法像桌面版那样直接排除「期间有送转」的整轮;改成看日线:持仓期间任意一天收盘价比前一天低 21% 以上
//!      (A 股涨跌停最宽 20%,除权日才会这样),整轮标「疑似送转」不算(也可能是新股上市初期)。**只能识别大比例送转**,小比例送转看不出来。
//! 🔴 行情取到就永久存本机:卖出当天收盘前取的那份不算定稿,收盘后再打开会重取一次。
//! ⛔ 这些数据是用户自己的交易衍生出来的,只留本机,不进任何 AI。

use crate::journal_risk::{self, Row};
use crate::symbol::normalize_stock_symbol;
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use futures_util::stream::{self, StreamExt};
use reqwest::Client;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::path::Path;
use std::time::Duration;

/// 少于这么多笔不给汇总性描述(几笔的捕获率中位数没有意义)
pub(crate) const EXC_MIN_TRADES: usize = 6;
/// 最大浮盈低于这个值(%)时捕获率没有讨论价值:买进去就没怎么涨过,谈不上卖早了
pub(crate) const MEANINGFUL_MFE: f64 = 3.0;
/// 捕获率低于这个值提示「几乎没吃到」
pub(crate) const LOW_CAPTURE_HINT: f64 = 0.3;
/// 只看最近这么多笔(桌面版同)
pub(crate) const EXCURSION_LIMIT: usize = 300;
const CONCURRENCY: usize = 4;
/// 持仓期间单日收盘比前一天低到这个比例以下,疑似送转(涨跌停最宽 20%)
const SPLIT_RATIO: f64 = 0.79;

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn r4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

fn median(vals: &[f64]) -> f64 {
    crate::emotion::median(vals)
}

/// 北交所代码(4、8、92 开头):腾讯日 K 取不到
pub(crate) fn is_bj(symbol: &str) -> bool {
    symbol.starts_with('4') || symbol.starts_with('8') || symbol.starts_with("92")
}

/// 一天的行情:日期、最高、最低、收盘(不复权)
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Hl {
    pub date: String,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

/// 算这一轮要用的东西:都来自卖光了的一轮
#[derive(Clone, Debug)]
pub(crate) struct Input {
    pub key: String,
    pub symbol: String,
    pub name: String,
    pub cost: f64,
    pub buy: String,
    pub sell: String,
    pub realized_pct: f64,
}

/// 复查清单要用的几个读数
#[derive(Clone, Debug)]
pub(crate) struct Lite {
    pub mfe_pct: f64,
    pub mae_pct: f64,
    pub realized_pct: f64,
    pub capture_rate: Option<f64>,
    pub same_day: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Item {
    pub input: Input,
    pub mfe_pct: f64,
    pub mae_pct: f64,
    pub mfe_certain: Option<f64>,
    pub mae_certain: Option<f64>,
    pub certain_note: Option<String>,
    pub bars: usize,
    pub bars_inner: usize,
    pub same_day: bool,
    pub precision: &'static str,
    pub give_back_pct: f64,
    pub capture_rate: Option<f64>,
    pub capture_note: Option<&'static str>,
}

impl Item {
    pub(crate) fn lite(&self) -> Lite {
        Lite { mfe_pct: self.mfe_pct, mae_pct: self.mae_pct, realized_pct: self.input.realized_pct, capture_rate: self.capture_rate, same_day: self.same_day }
    }

    fn to_value(&self) -> Value {
        json!({
            "key": self.input.key, "symbol": self.input.symbol, "name": self.input.name,
            "date": self.input.buy, "exit_date": self.input.sell, "realized_pct": r2(self.input.realized_pct),
            "mfe_pct": self.mfe_pct, "mae_pct": self.mae_pct, "mfe_certain": self.mfe_certain, "mae_certain": self.mae_certain,
            "certain_note": self.certain_note, "bars": self.bars, "bars_inner": self.bars_inner, "same_day": self.same_day,
            "precision": self.precision, "give_back_pct": self.give_back_pct, "capture_rate": self.capture_rate, "capture_note": self.capture_note,
        })
    }
}

pub(crate) enum Computed {
    Item(Item),
    /// 持仓期间疑似送转,整轮不算
    Split,
    Empty,
}

fn pct(v: f64, cost: f64) -> f64 {
    r2((v / cost - 1.0) * 100.0)
}

fn signed2(x: f64) -> String {
    format!("{}{:.2}", if x < 0.0 { "" } else { "+" }, x)
}

/// 给定行情算最大浮盈浮亏。纯计算、不碰网络(只读缓存的调用方复用同一套算法)。`rows` 要已限定在买入日~卖出日之内
pub(crate) fn compute_excursion(t: &Input, rows: &[Hl]) -> Computed {
    if !(t.cost > 0.0) || t.buy.is_empty() || t.sell.is_empty() || rows.is_empty() {
        return Computed::Empty;
    }
    if rows.windows(2).any(|w| w[1].close < w[0].close * SPLIT_RATIO) {
        return Computed::Split;
    }
    let high = rows.iter().map(|r| r.high).fold(f64::MIN, f64::max);
    let low = rows.iter().map(|r| r.low).fold(f64::MAX, f64::min);
    let (mfe, mae) = (pct(high, t.cost), pct(low, t.cost));
    // 只取严格在买卖日之间的完整交易日:这几天一定在持仓期内
    let inner: Vec<&Hl> = rows.iter().filter(|r| t.buy < r.date && r.date < t.sell).collect();
    let mfe_certain = inner.iter().map(|r| r.high).fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.max(v)))).map(|v| pct(v, t.cost));
    let mae_certain = inner.iter().map(|r| r.low).fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.min(v)))).map(|v| pct(v, t.cost));
    let same_day = t.buy == t.sell;
    let realized = r2(t.realized_pct);
    let capture = (mfe >= MEANINGFUL_MFE).then(|| r3(realized / mfe));
    Computed::Item(Item {
        input: t.clone(),
        mfe_pct: mfe,
        mae_pct: mae,
        mfe_certain,
        mae_certain,
        certain_note: mae_certain.map(|m| {
            if m > 0.0 { format!("中间完整交易日里从未浮亏(最差时仍 {}%)", signed2(m)) } else { format!("中间完整交易日里确定浮亏过 {}%", signed2(m)) }
        }),
        bars: rows.len(),
        bars_inner: inner.len(),
        same_day,
        precision: if same_day {
            "上界(同日进出,日线分不清高点在买入前还是后)"
        } else if !inner.is_empty() {
            "买卖日两头存疑,中间完整交易日可靠"
        } else {
            "上界(相邻两日,没有完整的中间交易日)"
        },
        // 盈利回吐:曾经到过的最好 减 实际落袋
        give_back_pct: r2(mfe - realized),
        capture_rate: capture,
        // 捕获率可能是负数:这段确实涨过、最后却亏着走 —— 比「没吃到」更差的一档,得给一句人话
        capture_note: match capture {
            Some(c) if c < 0.0 => Some("这段涨过,但最后是亏损离场 —— 比「没吃到」更差的一档"),
            Some(c) if c < LOW_CAPTURE_HINT => Some("几乎没吃到这段行情"),
            _ => None,
        },
    })
}

/// 一笔交易能不能算最大浮盈浮亏;不能的给原因分类
pub(crate) enum Eligibility {
    Ok(Input),
    NotClosed,
    Bj,
}

pub(crate) fn eligibility(r: &Row) -> Eligibility {
    let (Some(buy), Some(sell), Some(pnl)) = (r.first_buy.as_ref(), r.last_sell.as_ref(), r.pnl_pct) else { return Eligibility::NotClosed };
    if r.e.kind != "episode" || !r.closed || !(r.avg_cost > 0.0) {
        return Eligibility::NotClosed;
    }
    if is_bj(&r.e.symbol) {
        return Eligibility::Bj;
    }
    Eligibility::Ok(Input { key: format!("mobile:{}", r.e.id), symbol: r.e.symbol.clone(), name: r.e.name.clone(), cost: r4(r.avg_cost), buy: buy.clone(), sell: sell.clone(), realized_pct: pnl })
}

// ---------------------------------------------------------------- 缓存(永久)与定稿判断

fn open(db: &Path) -> Result<Connection> {
    Connection::open(db).context("open mobile journal bars sqlite")
}

/// 卖出那天 15:30(北京时间)= 07:30 UTC
fn settle_at(sell: &str) -> Option<DateTime<Utc>> {
    let day = NaiveDate::parse_from_str(sell, "%Y-%m-%d").ok()?;
    Some(day.and_hms_opt(7, 30, 0)?.and_utc())
}

/// 这份行情是不是卖出那天收盘以后取的(之前取的高低点可能还会变)
pub(crate) fn settled_fetch(fetched_at: &str, sell: &str) -> bool {
    match (DateTime::parse_from_rfc3339(fetched_at), settle_at(sell)) {
        (Ok(at), Some(line)) => at.with_timezone(&Utc) >= line,
        _ => false,
    }
}

pub(crate) struct Cached {
    pub rows: Vec<Hl>,
    pub fetched_at: String,
}

pub(crate) fn cache_get(conn: &Connection, t: &Input) -> Result<Option<Cached>> {
    let raw: Option<(String, String)> = conn
        .query_row("SELECT payload, fetched_at FROM mobile_journal_bars WHERE symbol = ?1 AND start_day = ?2 AND end_day = ?3", params![t.symbol, t.buy, t.sell], |r| Ok((r.get(0)?, r.get(1)?)))
        .ok();
    let Some((payload, fetched_at)) = raw else { return Ok(None) };
    // 缓存损坏当没有,重取覆盖
    let Ok(lines) = serde_json::from_str::<Vec<(String, f64, f64, f64)>>(&payload) else { return Ok(None) };
    Ok(Some(Cached { rows: lines.into_iter().map(|(date, high, low, close)| Hl { date, high, low, close }).collect(), fetched_at }))
}

pub(crate) fn cache_put(conn: &Connection, t: &Input, rows: &[Hl], fetched_at: &str) -> Result<()> {
    let payload = serde_json::to_string(&rows.iter().map(|r| (&r.date, r.high, r.low, r.close)).collect::<Vec<_>>())?;
    conn.execute(
        "INSERT INTO mobile_journal_bars(symbol, start_day, end_day, payload, fetched_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(symbol, start_day, end_day) DO UPDATE SET payload = excluded.payload, fetched_at = excluded.fetched_at",
        params![t.symbol, t.buy, t.sell, payload, fetched_at],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- 取行情(腾讯不复权日 K)

/// 腾讯日 K 响应 → [start, end] 内的日线(不复权)。日期要是 YYYY-MM-DD,价格要合理,不补值
pub(crate) fn rows_from_tencent(root: &Value, symbol: &str, start: &str, end: &str) -> Result<Vec<Hl>> {
    let key = normalize_stock_symbol(symbol).map_err(|e| anyhow!("{e}"))?;
    let (bars, _, _) = crate::chart::parse_tencent_kline(root, &key, "day")?;
    Ok(bars.into_iter().filter(|b| b.0.as_str() >= start && b.0.as_str() <= end && b.0.len() == 10).map(|(date, _o, close, high, low, ..)| Hl { date, high, low, close }).collect())
}

async fn fetch_tencent(client: &Client, t: &Input) -> Result<Vec<Hl>> {
    let key = normalize_stock_symbol(&t.symbol).map_err(|e| anyhow!("{e}"))?;
    let days = (NaiveDate::parse_from_str(&t.sell, "%Y-%m-%d")? - NaiveDate::parse_from_str(&t.buy, "%Y-%m-%d")?).num_days();
    if days > 780 {
        return Err(anyhow!("持有超过两年,腾讯日 K 一次取不全"));
    }
    let count = (days + 10).clamp(30, 800);
    let root: Value = client
        .get("https://web.ifzq.gtimg.cn/appstock/app/fqkline/get")
        .query(&[("param", format!("{},day,{},{},{count},", key.tencent, t.buy, t.sell))])
        .timeout(Duration::from_secs(12))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    rows_from_tencent(&root, &t.symbol, &t.buy, &t.sell)
}

// ---------------------------------------------------------------- 只读缓存(复查清单用)与逐笔取行情

/// 只读缓存算(复查清单用):一次网络请求都不发。返回能算的(按行 key)与跳过的笔数(没缓存 / 缓存是卖出当天盘中取的)
pub(crate) fn cached_excursions(db: &Path, rows: &[Row]) -> Result<(BTreeMap<String, Lite>, usize)> {
    let conn = open(db)?;
    let (mut map, mut skipped) = (BTreeMap::new(), 0usize);
    for r in rows {
        let Eligibility::Ok(t) = eligibility(r) else { continue };
        match cache_get(&conn, &t)? {
            Some(c) if settled_fetch(&c.fetched_at, &t.sell) => match compute_excursion(&t, &c.rows) {
                Computed::Item(it) => {
                    map.insert(t.key.clone(), it.lite());
                }
                // 疑似送转:这一轮不能算,也不是「没取过行情」
                Computed::Split => {}
                Computed::Empty => skipped += 1,
            },
            _ => skipped += 1,
        }
    }
    Ok((map, skipped))
}

fn failed_value(t: &Input, why: &str) -> Value {
    json!({"key": t.key, "symbol": t.symbol, "name": t.name, "date": t.buy, "why": why})
}

/// 汇总(桌面版 `excursionSummary`)
pub(crate) fn summary(items: &[Item]) -> Value {
    let caps: Vec<f64> = items.iter().filter_map(|r| r.capture_rate).collect();
    let same_day = items.iter().filter(|r| r.same_day).count();
    let lost_with_move = caps.iter().filter(|c| **c < 0.0).count();
    let mut sorted: Vec<&Item> = items.iter().collect();
    // 入场日新的在前(稳定排序)
    sorted.sort_by(|a, b| b.input.buy.cmp(&a.input.buy));
    json!({
        "trades": items.len(),
        "enough_samples": items.len() >= EXC_MIN_TRADES,
        // 一律用中位数:少数极端值会把均值拉得看不出典型情况
        "median_capture_rate": if caps.is_empty() { Value::Null } else { json!(r3(median(&caps))) },
        "capture_samples": caps.len(),
        "median_give_back": if items.is_empty() { Value::Null } else { json!(r2(median(&items.iter().map(|r| r.give_back_pct).collect::<Vec<_>>()))) },
        "median_mae": if items.is_empty() { Value::Null } else { json!(r2(median(&items.iter().map(|r| r.mae_pct).collect::<Vec<_>>()))) },
        // 赚钱收场但中途深亏过 —— 这次扛出来了,但是坏习惯的样本
        "endured_count": items.iter().filter(|r| r.input.realized_pct > 0.0 && r.mae_pct <= -5.0).count(),
        // 亏钱收场且从没怎么浮盈过 —— 进场就不对,不是拿不住
        "bad_entry_count": items.iter().filter(|r| r.input.realized_pct < 0.0 && r.mfe_pct < MEANINGFUL_MFE).count(),
        "same_day_count": same_day,
        "lost_with_move_count": lost_with_move,
        "capture_note": if lost_with_move > 0 { json!(format!("其中 {lost_with_move} 笔捕获率是负数 —— 那段确实涨过,但最后是亏损离场,比「没吃到」更差的一档(所以中位数可能是负的,不是算错了)。")) } else { Value::Null },
        "items": sorted.iter().map(|i| i.to_value()).collect::<Vec<_>>(),
        // 偏差必须和结论摆在一起
        "bias_note": format!("最大浮盈取的是日线高点,含「买入之前」的那部分 → 捕获率被系统性低估。其中 {same_day} 笔是同日进出,整段只能给上界。真实的最大浮盈落在每笔的「确定赚到过」与上界之间。最大浮亏同样包含入场前或离场后的日线低点,只能当最不利的边界,不能当实际持仓里的最大浮亏。"),
        "caveat": "日线看不出最高点发生在盘中买入之前还是之后。跨日持有时,中间完整交易日的高低点在持仓期内。这里用不复权日线对照你的成交成本:遇到除息,低点会偏低,应自己核对,不据此判断真实得失;期间疑似大比例送转的整轮不算(手机端没有送转流水,只能从日线看出单日跌超 21% 这种大比例送转)。",
    })
}

/// 逐笔算(「复查」页的最大浮盈浮亏)。`fetch_online` = false 时一次网络请求都不发,只用已缓存的定稿行情。
/// `rows` 要是桌面版顺序(入场日新的在前),只看最近 EXCURSION_LIMIT 笔。`fetch` 是取行情的函数(测试里换成假的)。
pub(crate) async fn compute_with<F, Fut>(db: &Path, rows: &[Row], fetch_online: bool, now: DateTime<Utc>, fetch: F) -> Result<Value>
where
    F: Fn(Input) -> Fut,
    Fut: Future<Output = Result<Vec<Hl>>>,
{
    let recent = &rows[..rows.len().min(EXCURSION_LIMIT)];
    let (mut not_closed, mut bj) = (0usize, 0usize);
    let mut todo: Vec<Input> = Vec::new();
    for r in recent {
        match eligibility(r) {
            Eligibility::Ok(t) => todo.push(t),
            Eligibility::NotClosed => not_closed += 1,
            Eligibility::Bj => bj += 1,
        }
    }
    let conn = open(db)?;
    // 先看缓存:能直接用的直接用,要取的排队
    enum Plan {
        Ready(Vec<Hl>, String, bool),
        Fetch,
        Uncached,
    }
    let mut plans: Vec<(Input, Plan)> = Vec::new();
    for t in todo {
        let cached = cache_get(&conn, &t)?;
        let plan = match cached {
            // 已定稿,或者卖出当天还没收盘(取不到更好的了):用缓存
            Some(c) if settled_fetch(&c.fetched_at, &t.sell) || settle_at(&t.sell).map_or(true, |line| now < line) => {
                let settled = settled_fetch(&c.fetched_at, &t.sell);
                // 只读缓存时,没定稿的(盘中取的)不用,和复查清单一致
                if !fetch_online && !settled { Plan::Uncached } else { Plan::Ready(c.rows, c.fetched_at, true) }
            }
            _ if fetch_online => Plan::Fetch,
            _ => Plan::Uncached,
        };
        plans.push((t, plan));
    }
    let uncached = plans.iter().filter(|(_, p)| matches!(p, Plan::Uncached)).count();
    let fetch_ref = &fetch;
    let results: Vec<(Input, std::result::Result<(Vec<Hl>, String, bool), String>)> = stream::iter(plans.into_iter().filter(|(_, p)| !matches!(p, Plan::Uncached)))
        .map(|(t, plan)| async move {
            match plan {
                Plan::Ready(rows, at, cached) => (t, Ok((rows, at, cached))),
                _ => match fetch_ref(t.clone()).await {
                    Ok(rows) => (t, Ok((rows, now.to_rfc3339(), false))),
                    Err(e) => (t, Err(e.to_string().chars().take(80).collect())),
                },
            }
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;
    let (mut items, mut failures, mut split) = (Vec::new(), Vec::new(), 0usize);
    let (mut intraday, mut cached_n) = (0usize, 0usize);
    // 保持输入顺序(稳定):按 key 对回 recent 里的顺序
    let order: HashMap<String, usize> = recent.iter().enumerate().map(|(i, r)| (format!("mobile:{}", r.e.id), i)).collect();
    let mut results = results;
    results.sort_by_key(|(t, _)| order.get(&t.key).copied().unwrap_or(usize::MAX));
    for (t, got) in results {
        match got {
            Err(why) => failures.push(failed_value(&t, &why)),
            Ok((rows, fetched_at, from_cache)) => {
                if !from_cache && !rows.is_empty() {
                    // 取到就永久缓存;写失败不影响这次结果,下次会重取
                    let _ = cache_put(&conn, &t, &rows, &fetched_at);
                }
                match compute_excursion(&t, &rows) {
                    Computed::Item(it) => {
                        if from_cache {
                            cached_n += 1;
                        }
                        if !settled_fetch(&fetched_at, &t.sell) {
                            intraday += 1;
                        }
                        items.push(it);
                    }
                    Computed::Split => split += 1,
                    Computed::Empty => failures.push(failed_value(&t, "这段的日线行情是空的")),
                }
            }
        }
    }
    failures.sort_by(|a, b| b["date"].as_str().cmp(&a["date"].as_str()).then(a["key"].as_str().cmp(&b["key"].as_str())));
    let failed = failures.len();
    let not_counted: Vec<String> = [
        (not_closed > 0).then(|| format!("{not_closed} 笔还没卖光或没有成交明细")),
        (split > 0).then(|| format!("{split} 笔期间疑似送转(不复权行情和成本不可比)")),
        (bj > 0).then(|| format!("{bj} 笔是北交所(腾讯 K 线取不到)")),
        (failed > 0).then(|| format!("{failed} 笔行情没取到")),
        (uncached > 0).then(|| format!("{uncached} 笔还没取过行情")),
    ]
    .into_iter()
    .flatten()
    .collect();
    let base = json!({
        "scanned": recent.len(),
        "counts": {"not_closed": not_closed, "split": split, "bj": bj, "failed": failed, "uncached": uncached},
        "cached": cached_n,
        "fetch_online": fetch_online,
        "failures": failures.iter().take(30).collect::<Vec<_>>(),
        "intraday_note": if intraday > 0 { json!(format!("{intraday} 笔是今天卖的、还没收盘,高低点按盘中价算,收盘后再看会更新。")) } else { Value::Null },
        "not_counted_note": if not_counted.is_empty() { Value::Null } else { json!(not_counted.join(";")) },
    });
    let mut out = base;
    if items.is_empty() {
        out["available"] = json!(false);
        out["reason"] = json!(if uncached > 0 && !fetch_online { "行情还没取过 —— 点「取行情并计算」,取到的永久存本机" } else { "还没有能算最大浮盈浮亏的交易(要卖光了、有成交明细、能取到行情)" });
        return Ok(out);
    }
    let mut s = summary(&items);
    s["available"] = json!(true);
    out.as_object_mut().unwrap().append(s.as_object_mut().unwrap());
    Ok(out)
}

/// `GET /mobile/journal/excursion?mode=cache|fetch`
pub async fn view(client: &Client, db: &Path, path: &str) -> std::result::Result<Value, crate::discovery::DiscoveryError> {
    let fetch_online = match crate::discovery::parameter(path, "mode").as_deref() {
        None | Some("cache") => false,
        Some("fetch") => true,
        Some(_) => return Err(crate::discovery::DiscoveryError::BadRequest("mode 只能是 cache 或 fetch".into())),
    };
    let a = journal_risk::analysis(db).map_err(crate::discovery::DiscoveryError::Upstream)?;
    let mut rows = a.rows;
    journal_risk::va_order(&mut rows);
    let mut out = compute_with(db, &rows, fetch_online, Utc::now(), |t| async move { fetch_tencent(client, &t).await })
        .await
        .map_err(crate::discovery::DiscoveryError::Upstream)?;
    out["source"] = json!("腾讯不复权日 K,取到永久存本机");
    out["generated_at"] = json!(Utc::now().to_rfc3339());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal_risk::{Entry, Fill};
    use crate::journal_settings::Fees;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const ZERO: Fees = Fees { commission_rate: 0.0, commission_min: 0.0, stamp_tax_rate: 0.0, transfer_fee_rate: 0.0 };

    fn input(buy: &str, sell: &str, realized: f64) -> Input {
        Input { key: "mobile:1".into(), symbol: "600001".into(), name: "样本".into(), cost: 10.0, buy: buy.into(), sell: sell.into(), realized_pct: realized }
    }

    fn hl(date: &str, high: f64, low: f64, close: f64) -> Hl {
        Hl { date: date.into(), high, low, close }
    }

    fn item(t: &Input, rows: &[Hl]) -> Item {
        match compute_excursion(t, rows) {
            Computed::Item(i) => i,
            _ => panic!("应能算出"),
        }
    }

    #[test]
    fn mfe_mae_capture_and_certain_ranges_follow_the_desktop_rules() {
        // 成本 10,买 10-12、卖 10-15,实际 +2%。中间完整交易日 10-13、10-14
        let t = input("2026-10-12", "2026-10-15", 2.0);
        let rows = [hl("2026-10-12", 10.2, 9.6, 10.0), hl("2026-10-13", 11.5, 10.1, 11.0), hl("2026-10-14", 11.0, 10.3, 10.5), hl("2026-10-15", 10.8, 10.1, 10.2)];
        let it = item(&t, &rows);
        assert_eq!((it.mfe_pct, it.mae_pct), (15.0, -4.0));
        assert_eq!((it.mfe_certain, it.mae_certain), (Some(15.0), Some(1.0)), "中间完整交易日的高低点");
        assert!(it.certain_note.as_deref().unwrap().contains("从未浮亏(最差时仍 +1.00%)"), "确定的最差为正 = 那几天没浮亏过");
        assert_eq!((it.give_back_pct, it.capture_rate, it.same_day, it.bars, it.bars_inner), (13.0, Some(0.133), false, 4, 2));
        assert_eq!(it.capture_note, Some("几乎没吃到这段行情"));
        assert_eq!(it.precision, "买卖日两头存疑,中间完整交易日可靠");
        // 同日进出:整段只给上界
        let same = item(&input("2026-10-12", "2026-10-12", 1.0), &[hl("2026-10-12", 10.6, 9.9, 10.1)]);
        assert_eq!((same.same_day, same.mfe_certain, same.mae_certain, same.certain_note.is_none()), (true, None, None, true));
        assert!(same.precision.starts_with("上界(同日进出"));
        // 相邻两日没有完整中间日
        let adj = item(&input("2026-10-12", "2026-10-13", 1.0), &[hl("2026-10-12", 10.6, 9.9, 10.1), hl("2026-10-13", 10.4, 9.8, 10.1)]);
        assert!(adj.precision.starts_with("上界(相邻两日"));
        // 最高没到 3%:没有讨论价值,不给捕获率;从没涨过却亏着走
        let flat = item(&input("2026-10-12", "2026-10-13", -4.0), &[hl("2026-10-12", 10.2, 9.5, 9.7), hl("2026-10-13", 9.9, 9.5, 9.6)]);
        assert_eq!((flat.capture_rate, flat.capture_note), (None, None));
        // 涨过却亏着走:捕获率是负数,要给一句比「没吃到」更差的话
        let lost = item(&input("2026-10-12", "2026-10-13", -2.0), &[hl("2026-10-12", 11.0, 9.7, 10.0), hl("2026-10-13", 10.5, 9.7, 9.8)]);
        assert_eq!(lost.capture_rate, Some(-0.2));
        assert!(lost.capture_note.unwrap().contains("更差的一档"));
        assert!(matches!(compute_excursion(&t, &[]), Computed::Empty));
    }

    #[test]
    fn a_big_one_day_close_drop_inside_the_holding_period_is_treated_as_a_split() {
        let t = input("2026-10-12", "2026-10-15", -10.0);
        let rows = [hl("2026-10-12", 10.2, 9.8, 10.0), hl("2026-10-13", 10.1, 4.9, 5.0), hl("2026-10-15", 5.2, 4.9, 5.1)];
        assert!(matches!(compute_excursion(&t, &rows), Computed::Split), "10 送 10 之类除权:不复权价格腰斩会被读成深度浮亏");
        let limit_down = [hl("2026-10-12", 10.2, 9.8, 10.0), hl("2026-10-13", 9.0, 8.0, 8.0), hl("2026-10-15", 8.1, 7.9, 8.0)];
        assert!(matches!(compute_excursion(&t, &limit_down), Computed::Item(_)), "20% 跌停(0.8)不算");
    }

    #[test]
    fn summary_uses_medians_and_flags_bias_and_negative_capture() {
        let mk = |d: &str, realized: f64, hi: f64, lo: f64| item(&input(d, d, realized), &[hl(d, hi, lo, 10.0)]);
        let items = vec![mk("2026-10-01", 5.0, 11.0, 9.9), mk("2026-10-02", -2.0, 11.0, 9.5), mk("2026-10-03", 1.0, 10.1, 9.4), mk("2026-10-04", 3.0, 10.8, 9.9), mk("2026-10-05", -3.0, 10.2, 9.2), mk("2026-10-06", 2.0, 10.6, 9.3)];
        let s = summary(&items);
        assert_eq!((s["trades"].clone(), s["enough_samples"].clone(), s["same_day_count"].clone()), (json!(6), json!(true), json!(6)));
        // 最大浮盈 ≥3% 的:10-01(10%)实际 5 → 0.5;10-02(10%)实际 -2 → -0.2;10-04(8%)实际 3 → 0.375;10-06(6%)实际 2 → 0.333
        assert_eq!(s["capture_samples"], 4);
        assert_eq!(s["median_capture_rate"], r3(median(&[0.5, -0.2, 0.375, 0.333])));
        assert_eq!(s["lost_with_move_count"], 1);
        assert!(s["capture_note"].as_str().unwrap().contains("中位数可能是负的"));
        // 赚钱收场但中途最多浮亏 ≤ -5%:10-03(-6%,实际 +1)、10-06(-7%,实际 +2);亏钱且 mfe < 3%:10-05(mfe 2%)
        assert_eq!((s["endured_count"].clone(), s["bad_entry_count"].clone()), (json!(2), json!(1)));
        assert_eq!(s["items"][0]["date"], "2026-10-06", "入场日新的在前");
        assert!(s["bias_note"].as_str().unwrap().contains("6 笔是同日进出") && s["bias_note"].as_str().unwrap().contains("系统性低估"));
        let few = summary(&items[..3]);
        assert_eq!(few["enough_samples"], false);
    }

    #[test]
    fn settled_fetch_needs_a_fetch_after_the_sell_day_close() {
        assert!(!settled_fetch("2026-10-15T07:29:59+00:00", "2026-10-15"), "15:29 北京时间取的还是盘中价");
        assert!(settled_fetch("2026-10-15T07:30:00Z", "2026-10-15"));
        assert!(settled_fetch("2026-10-16T01:00:00.123456+00:00", "2026-10-15"));
        assert!(!settled_fetch("garbage", "2026-10-15"));
    }

    #[test]
    fn tencent_rows_are_unadjusted_day_bars_inside_the_window_only() {
        let root = json!({"code": 0, "data": {"sh600001": {"day": [
            ["2026-10-09", "9.80", "9.90", "10.00", "9.70", "1000"], ["2026-10-12", "10.00", "10.40", "10.50", "9.90", "1200"],
            ["2026-10-13", "10.40", "10.20", "10.60", "10.10", "900"], ["2026-10-14", "10.20", "10.30", "10.35", "10.00", "800"],
            ["2026-10-13", "bad", "x", "x", "x", "1"]]}}});
        let rows = rows_from_tencent(&root, "600001", "2026-10-12", "2026-10-13").unwrap();
        assert_eq!(rows, vec![hl("2026-10-12", 10.5, 9.9, 10.4), hl("2026-10-13", 10.6, 10.1, 10.2)], "只取买入日~卖出日,坏行丢弃不补值");
        assert!(rows_from_tencent(&json!({"code": 0, "data": {"sh600001": {}}}), "600001", "a", "b").is_err());
    }

    fn db(label: &str) -> (crate::AppCore, std::path::PathBuf) {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-jexc-{label}-{nonce}-{}", std::process::id()));
        (crate::AppCore::new(dir.clone()).unwrap(), dir)
    }

    fn closed_row(id: i64, symbol: &str, buy: &str, sell: &str, bp: f64, sp: f64) -> Row {
        let e = Entry {
            id, kind: "episode".into(), symbol: symbol.into(), name: format!("N{id}"), date: buy.into(), created_at: format!("{buy}T09:00:00Z"),
            fills: vec![Fill { side: "buy".into(), date: buy.into(), price: bp, shares: 100.0, fee: Some(0.0) }, Fill { side: "sell".into(), date: sell.into(), price: sp, shares: 100.0, fee: Some(0.0) }],
            ..Entry::default()
        };
        journal_risk::settle(e, &ZERO, id as usize)
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[tokio::test]
    async fn bars_are_fetched_once_cached_forever_and_refetched_only_if_cached_before_the_close() {
        let (app, dir) = db("cache");
        let path = app.db_path().to_path_buf();
        let rows = vec![
            closed_row(1, "600001", "2026-10-12", "2026-10-14", 10.0, 10.2),
            closed_row(2, "600002", "2026-10-12", "2026-10-13", 10.0, 9.8),
            closed_row(3, "830001", "2026-10-12", "2026-10-13", 10.0, 10.0),
            row_open(4),
        ];
        let calls = AtomicUsize::new(0);
        let fake = |t: Input| {
            calls.fetch_add(1, Ordering::SeqCst);
            async move { Ok(vec![hl(&t.buy, 10.5, 9.8, 10.0), hl(&t.sell, 10.4, 9.7, 10.1)]) }
        };
        let now = at("2026-10-16T02:00:00Z");
        // 只读缓存:什么都没取过 → 一次网络都不发,说明还没取过
        let cold = compute_with(&path, &rows, false, now, &fake).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!((cold["available"].clone(), cold["counts"]["uncached"].clone(), cold["counts"]["bj"].clone(), cold["counts"]["not_closed"].clone()), (json!(false), json!(2), json!(1), json!(1)));
        assert!(cold["reason"].as_str().unwrap().contains("取行情并计算"));
        // 取:两笔各一次
        let first = compute_with(&path, &rows, true, now, &fake).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!((first["available"].clone(), first["trades"].clone(), first["cached"].clone()), (json!(true), json!(2), json!(0)));
        // 再取:全部来自缓存,不再发请求(永久缓存)
        let again = compute_with(&path, &rows, true, now, &fake).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(again["cached"], 2);
        // 只读缓存也能用了
        assert_eq!(compute_with(&path, &rows, false, now, &fake).await.unwrap()["trades"], 2);
        let (map, skipped) = cached_excursions(&path, &rows).unwrap();
        assert_eq!((map.len(), skipped), (2, 0));
        // 取失败:单笔记原因,不影响别的
        let (app2, dir2) = db("fail");
        let failing = |t: Input| async move { if t.symbol == "600001" { Err(anyhow!("网络断了")) } else { Ok(vec![hl(&t.buy, 10.5, 9.8, 10.0)]) } };
        let partial = compute_with(app2.db_path(), &rows, true, now, failing).await.unwrap();
        assert_eq!((partial["trades"].clone(), partial["counts"]["failed"].clone(), partial["failures"][0]["why"].clone()), (json!(1), json!(1), json!("网络断了")));
        drop(app2);
        let _ = std::fs::remove_dir_all(dir2);

        // 盘中取的(卖出当天 14:00 北京时间)不算定稿:当天不重取,收盘后再打开重取一次
        let (app3, dir3) = db("intraday");
        let today_rows = vec![closed_row(1, "600001", "2026-10-15", "2026-10-15", 10.0, 10.2)];
        let n = AtomicUsize::new(0);
        let counting = |t: Input| {
            n.fetch_add(1, Ordering::SeqCst);
            async move { Ok(vec![hl(&t.buy, 10.5, 9.8, 10.0)]) }
        };
        let noon = at("2026-10-15T06:00:00Z");
        let live = compute_with(app3.db_path(), &today_rows, true, noon, &counting).await.unwrap();
        assert!(live["intraday_note"].as_str().unwrap().contains("还没收盘"));
        compute_with(app3.db_path(), &today_rows, true, at("2026-10-15T06:30:00Z"), &counting).await.unwrap();
        assert_eq!(n.load(Ordering::SeqCst), 1, "收盘前再次打开不重取");
        assert_eq!(compute_with(app3.db_path(), &today_rows, false, noon, &counting).await.unwrap()["available"], false, "只读缓存不用盘中取的");
        let closed = compute_with(app3.db_path(), &today_rows, true, at("2026-10-15T08:00:00Z"), &counting).await.unwrap();
        assert_eq!(n.load(Ordering::SeqCst), 2, "收盘后重取一次");
        assert_eq!(closed["intraday_note"], Value::Null);
        compute_with(app3.db_path(), &today_rows, true, at("2026-10-16T08:00:00Z"), &counting).await.unwrap();
        assert_eq!(n.load(Ordering::SeqCst), 2, "定稿后永久缓存");
        drop(app3);
        drop(app);
        let _ = std::fs::remove_dir_all(dir3);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn row_open(id: i64) -> Row {
        let e = Entry { id, kind: "episode".into(), symbol: "600009".into(), date: "2026-10-14".into(), fills: vec![Fill { side: "buy".into(), date: "2026-10-14".into(), price: 10.0, shares: 100.0, fee: Some(0.0) }], ..Entry::default() };
        journal_risk::settle(e, &ZERO, id as usize)
    }

    #[tokio::test]
    #[ignore]
    async fn live_tencent_probe() {
        let client = Client::builder().timeout(Duration::from_secs(15)).build().unwrap();
        let t = input("2026-09-21", "2026-10-16", 0.0);
        let t = Input { symbol: "600519".into(), ..t };
        let rows = fetch_tencent(&client, &t).await.unwrap();
        println!("{} bars {:?} .. {:?}", rows.len(), rows.first(), rows.last());
        assert!(rows.len() >= 10 && rows.iter().all(|r| r.date.as_str() >= "2026-09-21" && r.date.as_str() <= "2026-10-16"));
    }

    /// 生成前端冻结夹具(全部是合成数据,不含任何真实成交与真实行情):
    /// VIBE_JOURNAL_EXCURSION_FIXTURE=路径 cargo test --locked journal_excursion::tests::dump_synthetic_fixture -- --ignored
    #[tokio::test]
    #[ignore]
    async fn dump_synthetic_fixture() {
        use chrono::Datelike;
        let (app, dir) = db("fixture");
        let path = app.db_path().to_path_buf();
        // (代码, 名称, 买入日, 卖出日, 买价, 卖价, 股数, 是否按计划, 最大浮盈 %, 最大浮亏 %)
        let trades: [(&str, &str, &str, &str, f64, f64, f64, Option<bool>, f64, f64); 12] = [
            ("600101", "样本A", "2026-09-21", "2026-09-24", 10.0, 10.6, 1000.0, Some(true), 12.0, 1.0),
            ("600102", "样本B", "2026-09-22", "2026-09-22", 20.0, 19.2, 500.0, Some(false), 1.5, 5.0),
            ("600103", "样本C", "2026-09-23", "2026-09-30", 8.0, 8.4, 2000.0, Some(true), 6.0, 9.5),
            ("600104", "样本D", "2026-09-25", "2026-09-28", 15.0, 14.4, 800.0, None, 8.0, 5.0),
            ("600105", "样本E", "2026-09-28", "2026-09-29", 12.0, 12.5, 1500.0, Some(true), 4.5, 2.0),
            ("600106", "样本F", "2026-09-29", "2026-10-09", 30.0, 30.6, 300.0, Some(true), 10.0, 3.0),
            ("600107", "样本G", "2026-10-09", "2026-10-12", 9.0, 8.5, 2500.0, Some(false), 2.0, 7.0),
            ("600108", "样本H", "2026-10-09", "2026-10-12", 18.0, 18.9, 1000.0, Some(true), 6.0, 2.0),
            ("600109", "样本I", "2026-10-12", "2026-10-13", 25.0, 25.0, 400.0, None, 3.5, 1.0),
            ("600110", "样本J", "2026-10-13", "2026-10-14", 11.0, 11.5, 1200.0, Some(true), 5.0, 1.5),
            ("600111", "样本K", "2026-10-14", "2026-10-15", 14.0, 13.0, 1400.0, Some(false), 1.0, 8.0),
            ("600112", "样本L", "2026-10-15", "2026-10-16", 6.0, 6.2, 8000.0, Some(true), 4.0, 1.0),
        ];
        let mut bars: std::collections::HashMap<String, Vec<Hl>> = std::collections::HashMap::new();
        for (code, name, buy, sell, bp, sp, shares, planned, peak, trough) in trades {
            let mut body = json!({"action": "record", "symbol": code, "name": name, "playbook": "打板",
                "fills": [{"type": "buy", "date": buy, "price": bp, "shares": shares, "fee": 0}, {"type": "sell", "date": sell, "price": sp, "shares": shares, "fee": 0}]});
            if let Some(p) = planned {
                body["as_planned"] = json!(p);
            }
            crate::journal::act(&path, &body).unwrap();
            // 合成日线:买入价到卖出价线性过渡,第二个交易日压到最大浮亏,中间那天冲到最大浮盈
            let (mut day, end) = (NaiveDate::parse_from_str(buy, "%Y-%m-%d").unwrap(), NaiveDate::parse_from_str(sell, "%Y-%m-%d").unwrap());
            let mut days = Vec::new();
            while day <= end {
                if day.weekday().number_from_monday() <= 5 {
                    days.push(day.format("%Y-%m-%d").to_string());
                }
                day += chrono::Duration::days(1);
            }
            let n = days.len();
            let rows: Vec<Hl> = days
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    let close = bp + (sp - bp) * (i as f64 + 1.0) / n as f64;
                    let (mut high, mut low) = (close.max(bp) * 1.004, close.min(bp) * 0.996);
                    if i == n / 2 {
                        high = high.max(bp * (1.0 + peak / 100.0));
                    }
                    if i == 1.min(n - 1) {
                        low = low.min(bp * (1.0 - trough / 100.0));
                    }
                    if n == 1 {
                        high = high.max(bp * (1.0 + peak / 100.0));
                        low = low.min(bp * (1.0 - trough / 100.0));
                    }
                    Hl { date: d.clone(), high, low, close }
                })
                .collect();
            bars.insert(code.to_string(), rows);
        }
        // 没卖完且没写止损的一笔、一笔北交所、一笔补记
        crate::journal::act(&path, &json!({"action": "record", "symbol": "600113", "name": "样本M", "playbook": "低吸", "as_planned": true,
            "fills": [{"type": "buy", "date": "2026-10-16", "price": 9.5, "shares": 3000}]})).unwrap();
        crate::journal::act(&path, &json!({"action": "record", "symbol": "830001", "name": "北交样本", "fills": [{"type": "buy", "date": "2026-10-13", "price": 5.0, "shares": 1000, "fee": 0}, {"type": "sell", "date": "2026-10-14", "price": 5.2, "shares": 1000, "fee": 0}]})).unwrap();
        crate::journal::act(&path, &json!({"action": "manual", "symbol": "600114", "name": "样本补记", "date": "2026-09-18", "pnl_pct": 3.2})).unwrap();
        let a = journal_risk::analysis(&path).unwrap();
        let mut rows = a.rows;
        journal_risk::va_order(&mut rows);
        let now = at("2026-10-17T02:00:00Z");
        // 先只读缓存(什么都没取过),再「取行情」(用合成日线),再用取到的缓存给复查清单
        let cold = compute_with(&path, &rows, false, now, |_t: Input| async { Err(anyhow!("不该联网")) }).await.unwrap();
        let bars_ref = &bars;
        let fetched = compute_with(&path, &rows, true, now, |t: Input| async move { Ok(bars_ref[&t.symbol].clone()) }).await.unwrap();
        let inbox = crate::journal_inbox::view(&path).unwrap();
        let mut inbox = inbox;
        inbox["generated_at"] = json!("2026-10-17T02:00:00Z");
        let mut fetched = fetched;
        fetched["generated_at"] = json!("2026-10-17T02:00:00Z");
        fetched["source"] = json!("腾讯不复权日 K,取到永久存本机");
        let mut cold = cold;
        cold["source"] = json!("腾讯不复权日 K,取到永久存本机");
        if let Ok(out) = std::env::var("VIBE_JOURNAL_EXCURSION_FIXTURE") {
            std::fs::write(out, serde_json::to_string_pretty(&json!({"cold": cold, "excursion": fetched, "inbox": inbox})).unwrap()).unwrap();
        }
        println!("trades {} median_capture {} inbox {} flags {}", fetched["trades"], fetched["median_capture_rate"], inbox["count"], inbox["by_flag"]);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
