//! 盯盘 · 实时动态(移植自 orchestrator/src/finance/shortline/watch.ts)。
//!
//! 页面开着时由页面带动:每次 poll 取一轮行情、和前几帧比、发现异动就记进本机 SQLite;页面关了就不取。
//! 判断状态放在 AppCore 里(进程内一份),同时几个页面在问也只算一份、不会重复报。
//!
//! - 名单:持仓、自选、昨日梯队(上一交易日涨停池)、昨日成交额前十每次都取;500 亿以上大票每 30 秒取一次。
//! - 规则(桌面版同口径):3 分钟内涨跌 ≥ 1.5% 报急拉 / 急跌(至少攒够 1 分钟才判);碰到涨停价报封板,离开涨停价报开板;
//!   同一只同一类 5 分钟内不重复。只在连续竞价时段、且行情时间是今天(上午 ≥ 09:30、下午 ≥ 13:00)时判断。
//! - 只列客观读数,不排序打分、不给买卖倾向。持仓只给现价、涨跌、浮动盈亏 % 和止损/目标状态,不给股数和成本。
//!
//! 和桌面版的差别:昨日成交额前十桌面版用通达信盘后包(任意交易日);手机上不下载整包,改为在「当天收盘后到下一场开盘前」
//! 从新浪成交额榜记下一份存本机(hs_a 节点,不含北交所),没存下的那天写明原因、不拿别的日子顶替。

use crate::{
    discovery::{self, DiscoveryError},
    emotion::{self, PoolKind},
    journal_review, monitor, storage,
    symbol::normalize_stock_symbol,
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Datelike, FixedOffset, Timelike, Utc, Weekday};
use encoding_rs::GBK;
use futures_util::{lock::Mutex, stream, StreamExt};
use reqwest::Client;
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::Path,
    time::Duration,
};

pub(crate) const POLL_MS: i64 = 3_000;
const BIGCAP_EVERY_MS: i64 = 30_000;
pub(crate) const SURGE_WINDOW_MS: i64 = 180_000;
pub(crate) const SURGE_MIN_SPAN_MS: i64 = 60_000;
pub(crate) const SURGE_PCT: f64 = 1.5;
pub(crate) const COOLDOWN_MS: i64 = 300_000;
const FRAME_KEEP: usize = 120;
const ALERTS_KEEP: usize = 500;
/// 名单(梯队、成交额前十、大票、持仓自选)多久重读一次
const LISTS_EVERY_MS: i64 = 120_000;
/// 大票名单(新浪市值榜翻页)一小时内复用
const BIGCAP_LIST_EVERY_MS: i64 = 3_600_000;
const BIGCAP_FLOOR_YI: f64 = 500.0;
const QUOTE_BATCH: usize = 50;
/// 异动记录只留最近这么多天(公开行情事件,不进备份)
const ALERTS_RETENTION_DAYS: i64 = 30;
const SINA_LIST: &str = "https://vip.stock.finance.sina.com.cn/quotes_service/api/json_v2.php/Market_Center.getHQNodeData";
const INDEX_SYMBOL: &str = "sh000001";
const SOURCE: &str = "腾讯实时行情;名单:持仓与自选(本机)、昨日梯队(东财上一交易日涨停池)、昨日成交额前十(新浪成交额榜,收盘后记下存本机,不含北交所)、500 亿以上(新浪市值榜)";

// ---------------------------------------------------------------- 时间与时段

fn china() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("valid China timezone offset")
}

#[derive(Clone, Debug)]
pub(crate) struct Clock {
    pub ms: i64,
    pub day: String,
    pub hms: String,
    /// HHMM
    pub hm: u32,
    pub weekend: bool,
}

impl Clock {
    pub(crate) fn now() -> Self {
        Self::at(Utc::now().with_timezone(&china()))
    }

    pub(crate) fn at(t: DateTime<FixedOffset>) -> Self {
        let t = t.with_timezone(&china());
        Self {
            ms: t.timestamp_millis(),
            day: t.format("%Y-%m-%d").to_string(),
            hms: t.format("%H:%M:%S").to_string(),
            hm: t.hour() * 100 + t.minute(),
            weekend: matches!(t.weekday(), Weekday::Sat | Weekday::Sun),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Phase {
    Weekend,
    PreOpen,
    Auction,
    Morning,
    Lunch,
    Afternoon,
    ClosingAuction,
    Closed,
}

impl Phase {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Weekend => "weekend",
            Self::PreOpen => "pre_open",
            Self::Auction => "auction",
            Self::Morning => "morning",
            Self::Lunch => "lunch",
            Self::Afternoon => "afternoon",
            Self::ClosingAuction => "closing_auction",
            Self::Closed => "closed",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Weekend => "周末休市",
            Self::PreOpen => "开盘前",
            Self::Auction => "集合竞价",
            Self::Morning => "上午连续竞价",
            Self::Lunch => "午间休市",
            Self::Afternoon => "下午连续竞价",
            Self::ClosingAuction => "收盘集合竞价",
            Self::Closed => "已收盘",
        }
    }

    pub(crate) fn continuous(self) -> bool {
        matches!(self, Self::Morning | Self::Afternoon)
    }
}

/// 按北京时间钟点分时段(节假日另靠交易日历和行情时间判断)
pub(crate) fn phase_of(clock: &Clock) -> Phase {
    if clock.weekend {
        return Phase::Weekend;
    }
    match clock.hm {
        hm if hm < 915 => Phase::PreOpen,
        hm if hm < 930 => Phase::Auction,
        hm if hm < 1130 => Phase::Morning,
        hm if hm < 1300 => Phase::Lunch,
        hm if hm < 1457 => Phase::Afternoon,
        hm if hm < 1500 => Phase::ClosingAuction,
        _ => Phase::Closed,
    }
}

/// 下一次请求隔多久:连续竞价 3 秒,竞价 / 午休 15 秒,其余 60 秒(页面切走时页面自己停)
pub(crate) fn poll_ms_of(phase: Phase, trading_day: Option<bool>) -> i64 {
    if trading_day == Some(false) {
        60_000
    } else if phase.continuous() {
        POLL_MS
    } else if matches!(phase, Phase::Auction | Phase::Lunch | Phase::ClosingAuction) {
        15_000
    } else {
        60_000
    }
}

/// 今天开不开市:周末不开;交易日历(上证日线)里有今天、或上证指数行情时间是今天 = 开;
/// 09:35 以后取到的上证指数行情还不是今天 = 节假日。其余说不准(None)。
pub(crate) fn trading_day_of(clock: &Clock, calendar: Option<&[String]>, index: Option<&IndexSeen>) -> Option<bool> {
    if clock.weekend {
        return Some(false);
    }
    if calendar.is_some_and(|c| c.iter().any(|d| *d == clock.day)) {
        return Some(true);
    }
    let index = index.filter(|i| i.seen_day == clock.day)?;
    if index.quote_day.as_deref() == Some(clock.day.as_str()) {
        return Some(true);
    }
    (index.seen_hm >= 935 && index.quote_day.is_some()).then_some(false)
}

/// 盯的是哪一场:今天开市 = 今天;今天休市 = 最近一场;开盘前说不准时按今天准备(交易日历要到开盘才有今天)。
pub(crate) fn quote_day_of(clock: &Clock, trading: Option<bool>, calendar: Option<&[String]>) -> Option<String> {
    match trading {
        Some(true) => Some(clock.day.clone()),
        Some(false) => calendar?.iter().rev().find(|d| d.as_str() < clock.day.as_str()).cloned(),
        None => (calendar.is_some() && clock.hm < 930).then(|| clock.day.clone()),
    }
}

fn previous_session(calendar: &[String], day: &str) -> Option<String> {
    calendar.iter().rev().find(|d| d.as_str() < day).cloned()
}

// ---------------------------------------------------------------- 异动判断(纯计算)

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct Alert {
    pub ts: String,
    pub code: String,
    pub name: String,
    pub kind: String,
    pub change_pct: Option<f64>,
    pub price: Option<f64>,
    pub pct: Option<f64>,
    pub msg: String,
    pub sources: Vec<String>,
}

pub(crate) const KINDS: [&str; 4] = ["急拉", "急跌", "封板", "开板"];

#[derive(Default)]
pub(crate) struct Detect {
    frames: HashMap<String, VecDeque<(i64, f64)>>,
    limit: HashMap<String, bool>,
    cooldown: HashMap<String, i64>,
}

impl Detect {
    /// 竞价、午休、收盘后的报价不进连续竞价的比较基准:下一段重新攒(冷却照旧)
    fn pause(&mut self) {
        self.frames.clear();
        self.limit.clear();
    }
}

pub(crate) struct Tick<'a> {
    pub name: &'a str,
    pub price: f64,
    pub pct: Option<f64>,
    pub limit_up: Option<f64>,
}

fn signed1(v: f64) -> String {
    format!("{}{:.1}%", if v > 0.0 { "+" } else { "" }, v)
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// 这一只这一刻的报价进来,和之前几帧比,返回新异动(已过冷却)
pub(crate) fn detect(st: &mut Detect, code: &str, q: &Tick, t: i64, ts: &str, sources: &[String]) -> Vec<Alert> {
    let frames = st.frames.entry(code.to_string()).or_default();
    frames.push_back((t, q.price));
    while frames.len() > FRAME_KEEP {
        frames.pop_front();
    }
    let today = q.pct.map(|p| format!(",今日 {}", signed1(p))).unwrap_or_default();
    let mut events: Vec<(&str, String, Option<f64>)> = Vec::new();
    // 规则①:窗口里最早的一帧和现在比;至少攒够 1 分钟才判
    if let Some(&(t0, p0)) = frames.iter().find(|(ft, _)| t - ft <= SURGE_WINDOW_MS) {
        if p0 > 0.0 && t - t0 >= SURGE_MIN_SPAN_MS {
            let delta = (q.price / p0 - 1.0) * 100.0;
            let mins = ((t - t0) as f64 / 60_000.0).round() as i64;
            let body = format!("{mins} 分钟 {}(现价 {}{today})", signed1(delta), q.price);
            if delta >= SURGE_PCT - 1e-9 {
                events.push(("急拉", body, Some(delta)));
            } else if delta <= -SURGE_PCT + 1e-9 {
                events.push(("急跌", body, Some(delta)));
            }
        }
    }
    // 规则②:涨停价来自行情本身(含创业板 / 科创板 20%、北交所 30%);第一次见到只记状态不报
    if let Some(limit_up) = q.limit_up.filter(|v| *v > 0.0) {
        let at = q.price >= limit_up - 1e-6;
        if let Some(was) = st.limit.get(code).copied() {
            if at && !was {
                events.push(("封板", format!("触及涨停 {limit_up}"), None));
            } else if !at && was {
                events.push(("开板", format!("涨停打开(现价 {}{today})", q.price), None));
            }
        }
        st.limit.insert(code.to_string(), at);
    }
    let mut out = Vec::new();
    for (kind, msg, change) in events {
        let key = format!("{code}|{kind}");
        if st.cooldown.get(&key).is_some_and(|last| t - last < COOLDOWN_MS) {
            continue;
        }
        st.cooldown.insert(key, t);
        out.push(Alert {
            ts: ts.to_string(),
            code: code.to_string(),
            name: q.name.to_string(),
            kind: kind.to_string(),
            change_pct: change.map(r2),
            price: Some(q.price),
            pct: q.pct,
            msg,
            sources: sources.to_vec(),
        });
    }
    out
}

// ---------------------------------------------------------------- 行情

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct WQuote {
    pub name: String,
    pub price: Option<f64>,
    pub pct: Option<f64>,
    pub amount_yuan: Option<f64>,
    pub limit_up: Option<f64>,
    pub limit_down: Option<f64>,
    /// YYYYMMDDHHMMSS;取不到为空串
    pub quote_time: String,
}

impl WQuote {
    fn is_day(&self, day: &str) -> bool {
        self.quote_time.len() == 14 && self.quote_time.starts_with(&day.replace('-', ""))
    }
}

fn field_number(fields: &[&str], index: usize) -> Option<f64> {
    let text = fields.get(index)?.trim();
    if text.is_empty() || text == "-" {
        return None;
    }
    text.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// 腾讯批量行情(qt.gtimg.cn,GBK 已解码)→ 腾讯代码(sh600519)→ 读数。价格 ≤ 0(停牌)当没有。
pub(crate) fn parse_tencent(text: &str) -> HashMap<String, WQuote> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let Some((lhs, rhs)) = line.trim().split_once('=') else { continue };
        let Some(symbol) = lhs.trim().strip_prefix("v_") else { continue };
        let payload = rhs.trim().trim_end_matches(';').trim_matches('"');
        let fields: Vec<&str> = payload.split('~').collect();
        if fields.len() < 49 {
            continue;
        }
        let time = fields[30].trim();
        out.insert(
            symbol.to_ascii_lowercase(),
            WQuote {
                name: fields[1].trim().to_string(),
                price: field_number(&fields, 3).filter(|p| *p > 0.0),
                pct: field_number(&fields, 32),
                amount_yuan: field_number(&fields, 37).map(|v| v * 10_000.0),
                limit_up: field_number(&fields, 47).filter(|p| *p > 0.0),
                limit_down: field_number(&fields, 48).filter(|p| *p > 0.0),
                quote_time: if time.len() == 14 && time.bytes().all(|b| b.is_ascii_digit()) { time.to_string() } else { String::new() },
            },
        );
    }
    out
}

async fn tencent_batch(client: &Client, symbols: &[String]) -> Result<HashMap<String, WQuote>> {
    let bytes = client
        .get("https://qt.gtimg.cn")
        .query(&[("q", symbols.join(","))])
        .header("Referer", "https://gu.qq.com/")
        .timeout(Duration::from_secs(8))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let (text, _, _) = GBK.decode(&bytes);
    let rows = parse_tencent(&text);
    if rows.is_empty() {
        return Err(anyhow!("腾讯批量行情没有返回可用数据"));
    }
    Ok(rows)
}

/// 分批(每批 50 只、同时 3 批)取行情;返回 6 位代码 → 读数,和没取到的批数
async fn fetch_quotes(client: &Client, codes: &[String]) -> (HashMap<String, WQuote>, usize, usize) {
    let mut by_symbol: HashMap<String, String> = HashMap::new();
    for code in codes {
        if let Ok(key) = normalize_stock_symbol(code) {
            by_symbol.insert(key.tencent.to_ascii_lowercase(), code.clone());
        }
    }
    let symbols: Vec<String> = by_symbol.keys().cloned().collect();
    let batches: Vec<Vec<String>> = symbols.chunks(QUOTE_BATCH).map(<[String]>::to_vec).collect();
    let total = batches.len();
    let results: Vec<Result<HashMap<String, WQuote>>> = stream::iter(batches)
        .map(|batch| async move { tencent_batch(client, &batch).await })
        .buffer_unordered(3)
        .collect()
        .await;
    let mut out = HashMap::new();
    let mut failed = 0;
    for result in results {
        match result {
            Ok(rows) => {
                for (symbol, quote) in rows {
                    if let Some(code) = by_symbol.get(&symbol) {
                        out.insert(code.clone(), quote);
                    }
                }
            }
            Err(_) => failed += 1,
        }
    }
    (out, failed, total)
}

/// 这笔报价是不是今天连续竞价里的(上午 ≥ 09:30、下午 ≥ 13:00):竞价、午休、节前的报价不进比较
pub(crate) fn in_session_quote(q: &WQuote, day: &str, phase: Phase) -> bool {
    if !q.is_day(day) {
        return false;
    }
    let hm = &q.quote_time[8..12];
    hm >= if phase == Phase::Afternoon { "1300" } else { "0930" }
}

// ---------------------------------------------------------------- 新浪列表(成交额榜、市值榜)

pub(crate) fn parse_sina_list(bytes: &[u8]) -> Result<Vec<Value>> {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => GBK.decode(bytes).0.into_owned(),
    };
    let (Some(start), Some(end)) = (text.find('['), text.rfind(']')) else {
        return Err(anyhow!("新浪列表响应格式错误"));
    };
    match serde_json::from_str::<Value>(&text[start..=end])? {
        Value::Array(rows) => Ok(rows),
        _ => Err(anyhow!("新浪列表不是数组")),
    }
}

async fn sina_list(client: &Client, sort: &str, page: usize, num: usize) -> Result<Vec<Value>> {
    let bytes = client
        .get(SINA_LIST)
        .query(&[
            ("page", page.to_string()),
            ("num", num.to_string()),
            ("sort", sort.to_string()),
            ("asc", "0".to_string()),
            ("node", "hs_a".to_string()),
            ("symbol", String::new()),
            ("_s_r_a", "page".to_string()),
        ])
        .header("Referer", "https://finance.sina.com.cn/")
        .timeout(Duration::from_secs(12))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    parse_sina_list(&bytes)
}

/// 沪深京 A 股号段(按交易所区分:sh000001 是上证指数,sz000001 是平安银行;B 股 900 / 200 不算)
pub(crate) fn is_a_share(symbol: &str) -> bool {
    let symbol = symbol.to_ascii_lowercase();
    let (market, code) = symbol.split_at(symbol.len().saturating_sub(6));
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let segs: &[&str] = match market {
        "sh" => &["600", "601", "603", "605", "688", "689"],
        "sz" => &["000", "001", "002", "003", "300", "301", "302"],
        "bj" => &["920", "43", "83", "87"],
        _ => return false,
    };
    segs.iter().any(|s| code.starts_with(s))
}

fn text_of(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(v)) => v.trim().to_string(),
        Some(Value::Number(v)) => v.to_string(),
        _ => String::new(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TurnoverRow {
    pub code: String,
    pub name: String,
    pub rank: usize,
    pub amount: f64,
}

/// 新浪成交额榜 → A 股前 N(成交额为元)
pub(crate) fn turnover_from_sina(rows: &[Value], top: usize) -> Vec<TurnoverRow> {
    let mut out: Vec<TurnoverRow> = Vec::new();
    for row in rows {
        let symbol = text_of(row.get("symbol"));
        let Some(amount) = discovery::number(row.get("amount")).filter(|v| *v > 0.0) else { continue };
        if !is_a_share(&symbol) {
            continue;
        }
        let code = symbol[symbol.len() - 6..].to_string();
        if out.iter().any(|r| r.code == code) {
            continue;
        }
        let name = text_of(row.get("name"));
        out.push(TurnoverRow { name: if name.is_empty() { code.clone() } else { name }, code, rank: 0, amount });
    }
    out.sort_by(|a, b| b.amount.partial_cmp(&a.amount).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(top);
    for (i, row) in out.iter_mut().enumerate() {
        row.rank = i + 1;
    }
    out
}

/// 新浪行情此刻属于哪个交易日、收盘没有:交易日 09:15 竞价开始后算今天(15:05 后算收盘),否则是上一个交易日。
pub(crate) fn sina_data_day(clock: &Clock, trading_today: Option<bool>, calendar: &[String]) -> Option<(String, bool)> {
    if clock.hm < 915 || trading_today == Some(false) {
        return previous_session(calendar, &clock.day).map(|d| (d, true));
    }
    match trading_today {
        Some(true) => Some((clock.day.clone(), clock.hm >= 1505)),
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct Bigcaps {
    at: i64,
    as_of: String,
    stocks: Vec<(String, String)>,
}

async fn load_bigcaps(client: &Client, clock: &Clock) -> Result<Bigcaps> {
    let floor = BIGCAP_FLOOR_YI * 1e8;
    let mut stocks: Vec<(String, String)> = Vec::new();
    let mut seen = HashSet::new();
    for page in 1..=10 {
        let rows = sina_list(client, "mktcap", page, 100).await?;
        if rows.is_empty() {
            // 翻过最后一页新浪回空列表、不报错;还没翻到门槛就空了 = 名单不全,不能当完整的交出去
            return Err(anyhow!("新浪市值榜第 {page} 页是空的,还没翻到 500 亿"));
        }
        let mut below = false;
        for row in &rows {
            let symbol = text_of(row.get("symbol"));
            let Some(cap_wan) = discovery::number(row.get("mktcap")) else { continue };
            if cap_wan * 1e4 < floor {
                below = true;
                break;
            }
            if !is_a_share(&symbol) {
                continue;
            }
            let code = symbol[symbol.len() - 6..].to_string();
            if seen.insert(code.clone()) {
                let name = text_of(row.get("name"));
                stocks.push((code.clone(), if name.is_empty() { code } else { name }));
            }
        }
        if below {
            if stocks.is_empty() {
                return Err(anyhow!("新浪市值榜一只都没取到"));
            }
            return Ok(Bigcaps { at: clock.ms, as_of: format!("{} {}", clock.day, &clock.hms[..5]), stocks });
        }
    }
    Err(anyhow!("翻了 10 页还没到 500 亿"))
}

// ---------------------------------------------------------------- 昨日梯队

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LadderRow {
    pub code: String,
    pub name: String,
    pub boards: i64,
    pub label: String,
    pub sector: String,
}

/// 上一交易日涨停池 → 梯队(原样固定,今天没接上的、没取到行情的都留着)。空池 / 异常行整份不用,不当成 0 只。
pub(crate) fn ladder_from_pool(rows: &[emotion::Row], sample_day: &str) -> Result<Vec<LadderRow>, String> {
    if rows.is_empty() {
        return Err(format!("{sample_day} 涨停池是空的(东财这几张池只留约一个月,可能超出了),不当成 0 只"));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        if row.code.len() != 6 || !row.code.bytes().all(|b| b.is_ascii_digit()) || row.boards < 1 || !seen.insert(row.code.clone()) {
            return Err(format!("{sample_day} 涨停池有异常行({}),不交一份不完整的名单", row.code));
        }
        out.push(LadderRow {
            code: row.code.clone(),
            name: if row.name.is_empty() { row.code.clone() } else { row.name.clone() },
            boards: row.boards,
            label: emotion::board_label(row.boards, row.stat),
            sector: row.sector.clone(),
        });
    }
    out.sort_by(|a, b| b.boards.cmp(&a.boards).then_with(|| a.code.cmp(&b.code)));
    Ok(out)
}

// ---------------------------------------------------------------- 名单

#[derive(Clone, Debug, Default)]
pub(crate) struct Holding {
    pub code: String,
    pub name: String,
    pub shares: f64,
    /// 持有成本合计;有一笔没有成本就算不出比例
    pub cost: Option<f64>,
    pub stop: Option<f64>,
    pub target: Option<f64>,
}

/// 交易日志推导的未平仓持仓,同一只合在一起:股数、成本相加;止损取最高的、目标取最低的(先触发的那条)
pub(crate) fn holdings_from(positions: &[Value]) -> Vec<Holding> {
    let mut out: Vec<Holding> = Vec::new();
    for p in positions {
        let Some(key) = p.get("symbol").and_then(Value::as_str).and_then(|s| normalize_stock_symbol(s).ok()) else { continue };
        let shares = p.get("open_shares").and_then(Value::as_f64).unwrap_or(0.0);
        if shares <= 0.0 {
            continue;
        }
        let cost = p.get("open_cost").and_then(Value::as_f64);
        let stop = p.get("planned_stop").and_then(Value::as_f64);
        let target = p.get("planned_target").and_then(Value::as_f64);
        let idx = match out.iter().position(|h| h.code == key.digits) {
            Some(i) => i,
            None => {
                out.push(Holding {
                    code: key.digits.clone(),
                    name: p.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                    cost: Some(0.0),
                    ..Holding::default()
                });
                out.len() - 1
            }
        };
        let h = &mut out[idx];
        h.shares += shares;
        h.cost = h.cost.zip(cost).map(|(a, b)| a + b);
        h.stop = match (h.stop, stop) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        h.target = match (h.target, target) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }
    out
}

/// 浮动盈亏 %:总成本 ≤ 0(分红送转吃够了)算不出比例,不给
pub(crate) fn pnl_pct(price: Option<f64>, h: &Holding) -> Option<f64> {
    let price = price?;
    let cost = h.cost.filter(|c| *c > 0.0)?;
    (h.shares > 0.0).then(|| r2((price * h.shares - cost) / cost * 100.0))
}

#[derive(Clone, Debug)]
struct Turnover {
    date: String,
    stocks: Vec<TurnoverRow>,
}

#[derive(Clone, Debug, Default)]
struct Lists {
    at: i64,
    quote_day: Option<String>,
    holdings: Vec<Holding>,
    watch: Vec<(String, String)>,
    ladder: Option<(String, Vec<LadderRow>)>,
    turnover: Option<Turnover>,
    notes: Vec<String>,
}

/// 每只票为什么在名单里(异动流里显示「持仓 · 昨日3板」)
fn sources_of(lists: &Lists, bigcaps: Option<&Bigcaps>) -> Vec<(String, Vec<String>)> {
    let mut order: Vec<(String, Vec<String>)> = Vec::new();
    let mut add = |code: &str, tag: String| match order.iter_mut().find(|(c, _)| c == code) {
        Some((_, tags)) => {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        None => order.push((code.to_string(), vec![tag])),
    };
    for h in &lists.holdings {
        add(&h.code, "持仓".into());
    }
    for (code, _) in &lists.watch {
        add(code, "自选".into());
    }
    for s in lists.ladder.iter().flat_map(|(_, rows)| rows) {
        add(&s.code, format!("昨日{}板", s.boards));
    }
    for t in lists.turnover.iter().flat_map(|t| &t.stocks) {
        add(&t.code, "昨日成交额前十".into());
    }
    for (code, _) in bigcaps.map(|b| b.stocks.as_slice()).unwrap_or_default() {
        add(code, "大票".into());
    }
    order
}

// ---------------------------------------------------------------- 存取

fn store_alerts(db: &Path, day: &str, alerts: &[Alert]) -> Result<()> {
    if alerts.is_empty() {
        return Ok(());
    }
    let mut conn = Connection::open(db)?;
    let tx = conn.transaction()?;
    for a in alerts {
        tx.execute(
            "INSERT INTO mobile_watch_alerts(day, ts, code, name, kind, change_pct, price, pct, msg, sources)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![day, a.ts, a.code, a.name, a.kind, a.change_pct, a.price, a.pct, a.msg, serde_json::to_string(&a.sources)?],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// 某天记下的异动(新的在前)。坏行跳过
pub(crate) fn load_alerts(db: &Path, day: &str) -> Result<Vec<Alert>> {
    let conn = Connection::open(db)?;
    let mut stmt = conn.prepare(
        "SELECT ts, code, name, kind, change_pct, price, pct, msg, sources FROM mobile_watch_alerts
         WHERE day = ?1 ORDER BY id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![day, ALERTS_KEEP as i64], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<f64>>(4)?,
            r.get::<_, Option<f64>>(5)?,
            r.get::<_, Option<f64>>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, String>(8)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows.flatten() {
        let (ts, code, name, kind, change_pct, price, pct, msg, sources) = row;
        let Ok(sources) = serde_json::from_str::<Vec<String>>(&sources) else { continue };
        if code.len() != 6 || !KINDS.contains(&kind.as_str()) {
            continue;
        }
        out.push(Alert { ts, code, name, kind, change_pct, price, pct, msg, sources });
    }
    Ok(out)
}

fn prune_alerts(db: &Path, clock: &Clock) -> Result<()> {
    let today = chrono::NaiveDate::parse_from_str(&clock.day, "%Y-%m-%d")?;
    let floor = (today - chrono::Duration::days(ALERTS_RETENTION_DAYS)).format("%Y-%m-%d").to_string();
    Connection::open(db)?.execute("DELETE FROM mobile_watch_alerts WHERE day < ?1", [floor])?;
    Ok(())
}

fn load_turnover(db: &Path, day: &str) -> Result<Option<Turnover>> {
    let conn = Connection::open(db)?;
    let payload: Option<String> = conn
        .query_row("SELECT payload FROM mobile_watch_turnover WHERE date = ?1", [day], |r| r.get(0))
        .ok();
    let Some(payload) = payload else { return Ok(None) };
    let value: Value = serde_json::from_str(&payload)?;
    let stocks = value
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    Some(TurnoverRow {
                        code: r.get("code")?.as_str()?.to_string(),
                        name: r.get("name")?.as_str()?.to_string(),
                        rank: r.get("rank")?.as_u64()? as usize,
                        amount: r.get("amount")?.as_f64()?,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok((!stocks.is_empty()).then(|| Turnover { date: day.to_string(), stocks }))
}

fn store_turnover(db: &Path, day: &str, rows: &[TurnoverRow]) -> Result<()> {
    let payload = Value::Array(rows.iter().map(|r| json!({"code": r.code, "name": r.name, "rank": r.rank, "amount": r.amount})).collect());
    Connection::open(db)?.execute(
        "INSERT INTO mobile_watch_turnover(date, payload, fetched_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(date) DO UPDATE SET payload = excluded.payload, fetched_at = excluded.fetched_at",
        params![day, payload.to_string(), Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- 状态与轮询

#[derive(Clone, Debug)]
pub(crate) struct IndexSeen {
    seen_day: String,
    seen_hm: u32,
    at: i64,
    quote_day: Option<String>,
}

#[derive(Default)]
pub struct WatchState {
    day: String,
    detect: Detect,
    alerts: Vec<Alert>,
    quotes: HashMap<String, WQuote>,
    focus_at: i64,
    big_at: i64,
    big_refreshed: Option<String>,
    calendar: Option<Vec<String>>,
    calendar_at: i64,
    calendar_ok: bool,
    index: Option<IndexSeen>,
    lists: Option<Lists>,
    ladder_cache: Option<(String, i64, Result<Vec<LadderRow>, String>)>,
    bigcaps: Option<Bigcaps>,
    bigcaps_tried: i64,
    bigcaps_error: Option<String>,
    last: Option<(i64, Value)>,
}

pub type SharedWatch = Mutex<WatchState>;

/// 盯盘页每隔几秒调一次。同时开着几个页面时,2.5 秒内的请求共用上一次结果,在途请求不重复发(锁住排队)。
pub async fn poll(client: &Client, db: &Path, state: &SharedWatch) -> Result<Value, DiscoveryError> {
    let clock = Clock::now();
    let mut st = state.lock().await;
    if st.day != clock.day {
        let alerts = load_alerts(db, &clock.day).map_err(DiscoveryError::Upstream)?;
        let _ = prune_alerts(db, &clock);
        let calendar = st.calendar.take();
        let (calendar_at, calendar_ok) = (st.calendar_at, st.calendar_ok);
        *st = WatchState { day: clock.day.clone(), alerts, calendar, calendar_at, calendar_ok, ..WatchState::default() };
    }
    if let Some((at, view)) = &st.last {
        if clock.ms - at < POLL_MS - 500 {
            return Ok(view.clone());
        }
    }
    let view = poll_once(client, db, &mut st, &clock).await;
    st.last = Some((Clock::now().ms, view.clone()));
    Ok(view)
}

async fn refresh_calendar(client: &Client, st: &mut WatchState, clock: &Clock) {
    let ttl = if st.calendar_ok { 30 * 60_000 } else { 60_000 };
    if st.calendar_at != 0 && clock.ms - st.calendar_at < ttl {
        return;
    }
    st.calendar_at = clock.ms;
    match emotion::trade_dates(client).await {
        Ok(dates) => {
            st.calendar = Some(dates);
            st.calendar_ok = true;
        }
        Err(_) => st.calendar_ok = false,
    }
}

/// 交易日历里还没有今天的工作日,用上证指数行情时间补判(一分钟最多问一次)
async fn refresh_index(client: &Client, st: &mut WatchState, clock: &Clock) {
    if clock.weekend || clock.hm < 915 || st.calendar.as_ref().is_some_and(|c| c.iter().any(|d| *d == clock.day)) {
        return;
    }
    if st.index.as_ref().is_some_and(|i| i.seen_day == clock.day && clock.ms - i.at < 60_000) {
        return;
    }
    if let Ok(rows) = tencent_batch(client, &[INDEX_SYMBOL.to_string()]).await {
        let quote_day = rows.get(INDEX_SYMBOL).filter(|q| q.quote_time.len() == 14).map(|q| {
            let t = &q.quote_time;
            format!("{}-{}-{}", &t[0..4], &t[4..6], &t[6..8])
        });
        st.index = Some(IndexSeen { seen_day: clock.day.clone(), seen_hm: clock.hm, at: clock.ms, quote_day });
    }
}

async fn load_lists(client: &Client, db: &Path, st: &mut WatchState, clock: &Clock, trading: Option<bool>) -> Lists {
    let mut notes = Vec::new();
    let mut holdings = Vec::new();
    let mut watch = Vec::new();
    match journal_review::view(db) {
        Ok(review) => holdings = holdings_from(review.get("positions").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()),
        Err(_) => notes.push("持仓没读出来(交易日志有问题,去「交易日志」看看)".to_string()),
    }
    match storage::list(db) {
        Ok(list) => {
            for item in list.get("items").and_then(Value::as_array).into_iter().flatten() {
                let Some(key) = item.get("symbol").and_then(Value::as_str).and_then(|s| normalize_stock_symbol(s).ok()) else { continue };
                if !watch.iter().any(|(c, _): &(String, String)| *c == key.digits) {
                    watch.push((key.digits.clone(), item.get("name").and_then(Value::as_str).unwrap_or("").to_string()));
                }
            }
        }
        Err(_) => notes.push("自选没读出来".to_string()),
    }

    let calendar = st.calendar.clone();
    let quote_day = quote_day_of(clock, trading, calendar.as_deref());
    let sample_day = match (&calendar, &quote_day) {
        (Some(cal), Some(day)) => previous_session(cal, day),
        _ => None,
    };
    if quote_day.is_none() {
        notes.push(if calendar.is_none() {
            "交易日历没取到,昨日梯队和昨日成交额前十先不列".to_string()
        } else {
            "今天开不开市还说不准,昨日梯队和昨日成交额前十先不列".to_string()
        });
    } else if sample_day.is_none() {
        notes.push("交易日历里找不到上一交易日,昨日梯队先不列".to_string());
    }

    // 昨日梯队:历史名单,取到就整天复用;没取到一分钟后再试
    let mut ladder = None;
    if let Some(sample) = &sample_day {
        let cached = st.ladder_cache.as_ref().filter(|(d, at, r)| d == sample && (r.is_ok() || clock.ms - at < 60_000));
        let result = match cached {
            Some((_, _, r)) => r.clone(),
            None => {
                let r = match emotion::fetch_pool(client, PoolKind::LimitUp, &sample.replace('-', "")).await {
                    Ok(rows) => ladder_from_pool(&rows, sample),
                    Err(e) => Err(format!("{}(不改用其他日期的名单)", emotion::error_message(&format!("{sample} 涨停池"), &e))),
                };
                st.ladder_cache = Some((sample.clone(), clock.ms, r.clone()));
                r
            }
        };
        match result {
            Ok(rows) => ladder = Some((sample.clone(), rows)),
            Err(reason) => notes.push(format!("昨日梯队没取到:{reason}")),
        }
    }

    // 昨日成交额前十:本机记下的那份;新浪榜此刻正好是那天的就现取现存
    let mut turnover = None;
    if let (Some(sample), Some(cal)) = (&sample_day, &calendar) {
        let data_day = sina_data_day(clock, trading, cal);
        turnover = load_turnover(db, sample).ok().flatten();
        if turnover.is_none() {
            if data_day.as_ref().is_some_and(|(d, settled)| d == sample && *settled) {
                match sina_list(client, "amount", 1, 40).await.map(|rows| turnover_from_sina(&rows, 10)) {
                    Ok(rows) if !rows.is_empty() => {
                        let _ = store_turnover(db, sample, &rows);
                        turnover = Some(Turnover { date: sample.clone(), stocks: rows });
                    }
                    _ => notes.push(format!("昨日成交额前十({sample})没取到,稍后再试")),
                }
            } else {
                notes.push(format!(
                    "昨日成交额前十({sample})没存下:手机上只能在当天收盘后到下一场开盘前从成交额榜记下这份名单;今天收盘后开着盯盘会自动存"
                ));
            }
        }
        // 收盘后顺手把今天的存下,明天盯盘就有
        if let Some((day, true)) = data_day.filter(|(d, _)| d != sample) {
            if load_turnover(db, &day).ok().flatten().is_none() {
                if let Ok(rows) = sina_list(client, "amount", 1, 40).await.map(|rows| turnover_from_sina(&rows, 10)) {
                    if !rows.is_empty() {
                        let _ = store_turnover(db, &day, &rows);
                    }
                }
            }
        }
    }

    // 大票名单:一小时内复用;没取到两分钟后随名单一起再试
    let stale = st.bigcaps.as_ref().map_or(true, |b| clock.ms - b.at >= BIGCAP_LIST_EVERY_MS);
    if stale && clock.ms - st.bigcaps_tried >= LISTS_EVERY_MS - 1_000 {
        st.bigcaps_tried = clock.ms;
        match load_bigcaps(client, clock).await {
            Ok(b) => {
                st.bigcaps = Some(b);
                st.bigcaps_error = None;
            }
            Err(e) => st.bigcaps_error = Some(emotion::error_message("500 亿以上大票名单", &e)),
        }
    }
    if st.bigcaps.is_none() {
        if let Some(e) = &st.bigcaps_error {
            notes.push(format!("{e},稍后自动重试"));
        }
    }

    Lists { at: clock.ms, quote_day, holdings, watch, ladder, turnover, notes }
}

fn row_view(st: &WatchState, code: &str, name: &str, day: &str) -> Value {
    let q = st.quotes.get(code);
    json!({
        "code": code,
        "name": q.map(|q| q.name.as_str()).filter(|n| !n.is_empty()).or(Some(name).filter(|n| !n.is_empty())).unwrap_or(code),
        "price": q.and_then(|q| q.price),
        "pct": q.and_then(|q| q.pct),
        "amount_yuan": q.and_then(|q| q.amount_yuan),
        "quote_time": q.map(|q| q.quote_time.as_str()).filter(|t| !t.is_empty()),
        "today": q.is_some_and(|q| q.is_day(day)),
    })
}

async fn poll_once(client: &Client, db: &Path, st: &mut WatchState, clock: &Clock) -> Value {
    let phase = phase_of(clock);
    refresh_calendar(client, st, clock).await;
    refresh_index(client, st, clock).await;
    let trading = trading_day_of(clock, st.calendar.as_deref(), st.index.as_ref());
    let live = phase.continuous() && trading != Some(false);

    let reload = st.lists.as_ref().map_or(true, |l| {
        clock.ms - l.at >= LISTS_EVERY_MS || l.quote_day != quote_day_of(clock, trading, st.calendar.as_deref())
    });
    if reload {
        let lists = load_lists(client, db, st, clock, trading).await;
        st.lists = Some(lists);
    }
    let lists = st.lists.clone().unwrap_or_default();
    let tags = sources_of(&lists, st.bigcaps.as_ref());
    let big_set: HashSet<&str> = st.bigcaps.iter().flat_map(|b| b.stocks.iter().map(|(c, _)| c.as_str())).collect();
    // 大票只在名单里挂「大票」一个标签的 30 秒取一次;同时是持仓 / 自选 / 梯队的,跟着重点名单每次取
    let focus: Vec<String> = tags.iter().filter(|(c, t)| !big_set.contains(c.as_str()) || t.len() > 1).map(|(c, _)| c.clone()).collect();
    let focus_set: HashSet<&str> = focus.iter().map(String::as_str).collect();
    let big: Vec<String> = big_set.iter().filter(|c| !focus_set.contains(**c)).map(|c| c.to_string()).collect();
    let mut notes = lists.notes.clone();

    // 连续竞价里现取;其余时段行情不怎么变,拉长间隔
    let focus_due = clock.ms - st.focus_at >= if live { POLL_MS - 500 } else { 60_000 };
    let big_due = clock.ms - st.big_at >= if live { BIGCAP_EVERY_MS } else { 5 * 60_000 };
    let mut want: Vec<String> = Vec::new();
    if focus_due {
        want.extend(focus.iter().cloned());
    }
    if big_due {
        want.extend(big.iter().cloned());
    }
    if !want.is_empty() {
        let (quotes, failed, batches) = fetch_quotes(client, &want).await;
        // 这一轮问了却没返回的,旧价不留着当现价
        for code in &want {
            st.quotes.remove(code);
        }
        if focus_due {
            st.focus_at = clock.ms;
        }
        if big_due {
            st.big_at = clock.ms;
            st.big_refreshed = Some(Utc::now().to_rfc3339());
        }
        if failed > 0 {
            notes.push(format!("行情这一轮分 {batches} 批取,{failed} 批没取到,稍后自动重试"));
        }
        if live {
            let mut fresh = Vec::new();
            for code in &want {
                let Some(q) = quotes.get(code) else { continue };
                let Some(price) = q.price else { continue };
                if !in_session_quote(q, &clock.day, phase) {
                    continue;
                }
                let sources = tags.iter().find(|(c, _)| c == code).map(|(_, t)| t.as_slice()).unwrap_or_default();
                let name = if q.name.is_empty() { code.as_str() } else { q.name.as_str() };
                fresh.extend(detect(&mut st.detect, code, &Tick { name, price, pct: q.pct, limit_up: q.limit_up }, clock.ms, &clock.hms, sources));
            }
            if !fresh.is_empty() {
                if store_alerts(db, &clock.day, &fresh).is_err() {
                    notes.push("异动没存上(本机存储有问题),页面上照常显示".to_string());
                }
                fresh.reverse();
                fresh.append(&mut st.alerts);
                fresh.truncate(ALERTS_KEEP);
                st.alerts = fresh;
            }
        }
        st.quotes.extend(quotes);
    }
    if !live {
        st.detect.pause();
    }

    // 封没封只在连续竞价和收盘后、行情时间是今天时说;竞价 / 午休里的价位碰到涨停价不算
    let limit_known = matches!(phase, Phase::Morning | Phase::Afternoon | Phase::Closed) && trading != Some(false);
    let ladder = lists.ladder.as_ref().map(|(sample, rows)| {
        let views: Vec<Value> = rows
            .iter()
            .map(|s| {
                let mut row = row_view(st, &s.code, &s.name, &clock.day);
                let q = st.quotes.get(&s.code);
                let at = (limit_known && q.is_some_and(|q| q.is_day(&clock.day)))
                    .then(|| q.and_then(|q| q.price.zip(q.limit_up)).map(|(p, l)| p >= l - 1e-6))
                    .flatten();
                row["boards"] = json!(s.boards);
                row["label"] = json!(s.label);
                row["sector"] = json!(s.sector);
                row["at_limit"] = json!(at);
                row
            })
            .collect();
        json!({
            "sample_date": sample,
            "quote_date": lists.quote_day,
            "highest": rows.iter().map(|r| r.boards).max().unwrap_or(0),
            "rows": views,
        })
    });
    let bigcap = st.bigcaps.as_ref().map(|b| {
        let mut rows: Vec<Value> = b.stocks.iter().map(|(c, n)| row_view(st, c, n, &clock.day)).filter(|r| r["pct"].is_number()).collect();
        rows.sort_by(|a, b| b["pct"].as_f64().partial_cmp(&a["pct"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
        let top: Vec<Value> = rows.iter().take(10).cloned().collect();
        let bottom: Vec<Value> = rows.iter().rev().take(5).cloned().collect();
        json!({ "total": b.stocks.len(), "date": lists.quote_day, "as_of": b.as_of, "refreshed_at": st.big_refreshed, "top": top, "bottom": bottom })
    });
    if trading.is_none() && phase != Phase::Weekend {
        notes.push("今天开不开市还说不准:以行情时间为准".to_string());
    }
    let holdings: Vec<Value> = lists
        .holdings
        .iter()
        .map(|h| {
            let mut row = row_view(st, &h.code, &h.name, &clock.day);
            let q = st.quotes.get(&h.code);
            let price = q.and_then(|q| q.price);
            row["pnl_pct"] = json!(pnl_pct(price, h));
            row["risk_state"] = json!(monitor::risk_state(price, h.stop, h.target, q.is_some_and(|q| q.is_day(&clock.day))));
            row["limit_up"] = json!(q.and_then(|q| q.limit_up));
            row["limit_down"] = json!(q.and_then(|q| q.limit_down));
            row
        })
        .collect();
    let watchlist: Vec<Value> = lists
        .watch
        .iter()
        .map(|(c, n)| {
            let mut row = row_view(st, c, n, &clock.day);
            let q = st.quotes.get(c);
            row["limit_up"] = json!(q.and_then(|q| q.limit_up));
            row["limit_down"] = json!(q.and_then(|q| q.limit_down));
            row
        })
        .collect();
    let phase_label = if trading == Some(false) && phase != Phase::Weekend { "今天休市" } else { phase.label() };

    json!({
        "date": clock.day,
        "clock": clock.hms,
        "phase": phase.key(),
        "phase_label": phase_label,
        "trading_day": trading,
        "detecting": live,
        "poll_ms": poll_ms_of(phase, trading),
        "quote_day": lists.quote_day,
        "holdings": holdings,
        "watchlist": watchlist,
        "ladder": ladder,
        "turnover": lists.turnover.as_ref().map(|t| json!({
            "date": t.date,
            "rows": t.stocks.iter().map(|s| {
                let mut row = row_view(st, &s.code, &s.name, &clock.day);
                row["rank"] = json!(s.rank);
                row["prev_amount_yuan"] = json!(s.amount);
                row
            }).collect::<Vec<_>>(),
        })),
        "bigcap": bigcap,
        "alerts": st.alerts,
        "notes": notes,
        "rules": { "surge_pct": SURGE_PCT, "window_min": SURGE_WINDOW_MS / 60_000, "cooldown_min": COOLDOWN_MS / 60_000 },
        "source": SOURCE,
    })
}

/// 某天记下的异动(复盘时回看):`?date=YYYY-MM-DD`,默认今天
pub fn alerts(db: &Path, path: &str) -> Result<Value, DiscoveryError> {
    let clock = Clock::now();
    let date = emotion::date_parameter(path)?.unwrap_or_else(|| clock.day.clone());
    if date > clock.day {
        return Err(DiscoveryError::BadRequest(format!("{date} 还没到")));
    }
    Ok(json!({ "date": date, "alerts": load_alerts(db, &date).map_err(DiscoveryError::Upstream)? }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn clock(day: &str, hms: &str) -> Clock {
        let t = china().from_local_datetime(&chrono::NaiveDateTime::parse_from_str(&format!("{day} {hms}"), "%Y-%m-%d %H:%M:%S").unwrap()).unwrap();
        Clock::at(t)
    }

    fn tick(price: f64, limit_up: Option<f64>) -> Tick<'static> {
        Tick { name: "测试", price, pct: Some(1.0), limit_up }
    }

    #[test]
    fn phases_follow_beijing_clock() {
        assert_eq!(phase_of(&clock("2026-10-17", "10:00:00")), Phase::Weekend);
        assert_eq!(phase_of(&clock("2026-10-16", "09:14:59")), Phase::PreOpen);
        assert_eq!(phase_of(&clock("2026-10-16", "09:15:00")), Phase::Auction);
        assert_eq!(phase_of(&clock("2026-10-16", "09:30:00")), Phase::Morning);
        assert_eq!(phase_of(&clock("2026-10-16", "11:30:00")), Phase::Lunch);
        assert_eq!(phase_of(&clock("2026-10-16", "13:00:00")), Phase::Afternoon);
        assert_eq!(phase_of(&clock("2026-10-16", "14:57:00")), Phase::ClosingAuction);
        assert_eq!(phase_of(&clock("2026-10-16", "15:00:00")), Phase::Closed);
        assert_eq!(poll_ms_of(Phase::Morning, Some(true)), 3_000);
        assert_eq!(poll_ms_of(Phase::Morning, Some(false)), 60_000);
        assert_eq!(poll_ms_of(Phase::Lunch, None), 15_000);
        assert_eq!(poll_ms_of(Phase::Closed, Some(true)), 60_000);
    }

    #[test]
    fn surge_needs_a_minute_of_frames_and_respects_cooldown() {
        let mut st = Detect::default();
        let src = vec!["自选".to_string()];
        assert!(detect(&mut st, "600519", &tick(10.0, None), 0, "10:00:00", &src).is_empty());
        // 不到 1 分钟,涨再多也不判
        assert!(detect(&mut st, "600519", &tick(10.5, None), 30_000, "10:00:30", &src).is_empty());
        let a = detect(&mut st, "600519", &tick(10.15, None), 60_000, "10:01:00", &src);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].kind, "急拉");
        assert_eq!(a[0].change_pct, Some(1.5));
        assert_eq!(a[0].msg, "1 分钟 +1.5%(现价 10.15,今日 +1.0%)");
        assert_eq!(a[0].sources, src);
        // 5 分钟内同一只同一类不再报
        assert!(detect(&mut st, "600519", &tick(10.4, None), 120_000, "10:02:00", &src).is_empty());
        // 窗口外的旧帧不比:4 分钟后和 3 分钟内最早一帧比
        let b = detect(&mut st, "600519", &tick(9.9, None), 240_000, "10:04:00", &src);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].kind, "急跌");
        assert!(b[0].change_pct.unwrap() <= -1.5);
    }

    #[test]
    fn limit_events_skip_the_first_sighting() {
        let mut st = Detect::default();
        assert!(detect(&mut st, "300750", &tick(11.0, Some(11.0)), 0, "10:00:00", &[]).is_empty());
        let open = detect(&mut st, "300750", &tick(10.9, Some(11.0)), 3_000, "10:00:03", &[]);
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].kind, "开板");
        let seal = detect(&mut st, "300750", &tick(11.0, Some(11.0)), 6_000, "10:00:06", &[]);
        assert_eq!(seal[0].kind, "封板");
        assert_eq!(seal[0].msg, "触及涨停 11");
        assert_eq!(seal[0].change_pct, None);
        // 冷却 5 分钟:再开再封不重复报
        assert!(detect(&mut st, "300750", &tick(10.9, Some(11.0)), 9_000, "10:00:09", &[]).is_empty());
        st.pause();
        assert!(detect(&mut st, "300750", &tick(11.0, Some(11.0)), 400_000, "13:00:00", &[]).is_empty(), "午休后重新攒,第一次见到不报");
    }

    #[test]
    fn tencent_parser_keeps_quote_time_and_drops_suspended_prices() {
        let mut fields = vec!["0"; 50];
        fields[1] = "贵州茅台";
        fields[2] = "600519";
        fields[3] = "1500.5";
        fields[30] = "20261016100003";
        fields[32] = "0.67";
        fields[37] = "123456";
        fields[47] = "1650.0";
        fields[48] = "1350.0";
        let mut halted = fields.clone();
        halted[2] = "000001";
        halted[3] = "0.00";
        let text = format!("v_sh600519=\"{}\";\nv_sz000001=\"{}\";\n", fields.join("~"), halted.join("~"));
        let rows = parse_tencent(&text);
        let q = &rows["sh600519"];
        assert_eq!(q.price, Some(1500.5));
        assert_eq!(q.amount_yuan, Some(1_234_560_000.0));
        assert_eq!(q.quote_time, "20261016100003");
        assert!(q.is_day("2026-10-16"));
        assert!(in_session_quote(q, "2026-10-16", Phase::Morning));
        assert!(!in_session_quote(q, "2026-10-16", Phase::Afternoon), "下午只认 13:00 以后的报价");
        assert!(!in_session_quote(q, "2026-10-17", Phase::Morning));
        assert_eq!(rows["sz000001"].price, None);
    }

    #[test]
    fn trading_day_and_quote_day() {
        let cal: Vec<String> = ["2026-10-14", "2026-10-15", "2026-10-16"].iter().map(|s| s.to_string()).collect();
        let fri = clock("2026-10-16", "10:00:00");
        assert_eq!(trading_day_of(&fri, Some(&cal), None), Some(true));
        assert_eq!(quote_day_of(&fri, Some(true), Some(&cal)).as_deref(), Some("2026-10-16"));
        assert_eq!(previous_session(&cal, "2026-10-16").as_deref(), Some("2026-10-15"));
        let sat = clock("2026-10-17", "10:00:00");
        assert_eq!(trading_day_of(&sat, Some(&cal), None), Some(false));
        assert_eq!(quote_day_of(&sat, Some(false), Some(&cal)).as_deref(), Some("2026-10-16"));
        // 周一开盘前:日历还没有今天,按今天准备名单
        let mon_pre = clock("2026-10-19", "08:50:00");
        assert_eq!(trading_day_of(&mon_pre, Some(&cal), None), None);
        assert_eq!(quote_day_of(&mon_pre, None, Some(&cal)).as_deref(), Some("2026-10-19"));
        assert_eq!(previous_session(&cal, "2026-10-19").as_deref(), Some("2026-10-16"));
        // 节假日:09:35 后上证指数行情还是旧日期
        let holiday = clock("2026-10-19", "10:00:00");
        let stale = IndexSeen { seen_day: "2026-10-19".into(), seen_hm: 1000, at: 0, quote_day: Some("2026-10-16".into()) };
        assert_eq!(trading_day_of(&holiday, Some(&cal), Some(&stale)), Some(false));
        let early = IndexSeen { seen_hm: 920, ..stale.clone() };
        assert_eq!(trading_day_of(&holiday, Some(&cal), Some(&early)), None);
        let fresh = IndexSeen { quote_day: Some("2026-10-19".into()), ..stale };
        assert_eq!(trading_day_of(&holiday, Some(&cal), Some(&fresh)), Some(true));
        assert_eq!(quote_day_of(&holiday, None, None), None);
    }

    #[test]
    fn sina_day_only_counts_settled_sessions() {
        let cal: Vec<String> = ["2026-10-15", "2026-10-16"].iter().map(|s| s.to_string()).collect();
        assert_eq!(sina_data_day(&clock("2026-10-16", "15:10:00"), Some(true), &cal), Some(("2026-10-16".into(), true)));
        assert_eq!(sina_data_day(&clock("2026-10-16", "14:00:00"), Some(true), &cal), Some(("2026-10-16".into(), false)));
        assert_eq!(sina_data_day(&clock("2026-10-16", "09:00:00"), None, &cal), Some(("2026-10-15".into(), true)));
        assert_eq!(sina_data_day(&clock("2026-10-17", "11:00:00"), Some(false), &cal), Some(("2026-10-16".into(), true)));
        assert_eq!(sina_data_day(&clock("2026-10-19", "10:00:00"), None, &cal), None);
    }

    #[test]
    fn sina_lists_map_to_a_share_rows() {
        let body = r#"[{"symbol":"sh600519","code":"600519","name":"贵州茅台","amount":5000000000,"mktcap":180000000},
            {"symbol":"sh900901","code":"900901","name":"B","amount":9000000000},
            {"symbol":"sz300750","code":"300750","name":"宁德时代","amount":"7000000000"},
            {"symbol":"bj920001","code":"920001","name":"BJ","amount":0}]"#.as_bytes();
        let rows = parse_sina_list(body).unwrap();
        let top = turnover_from_sina(&rows, 10);
        assert_eq!(top.iter().map(|r| (r.code.as_str(), r.rank)).collect::<Vec<_>>(), vec![("300750", 1), ("600519", 2)]);
        assert_eq!(top[1].name, "贵州茅台");
        assert!(is_a_share("bj920001") && is_a_share("sz301001") && !is_a_share("sh000001") && !is_a_share("sz200011"));
        assert!(parse_sina_list(b"null").is_err());
    }

    #[test]
    fn ladder_rejects_empty_or_bad_pools() {
        let row = |code: &str, boards: i64| emotion::Row { code: code.into(), name: format!("N{code}"), boards, sector: "汽车".into(), ..emotion::Row::default() };
        assert!(ladder_from_pool(&[], "2026-10-15").unwrap_err().contains("不当成 0 只"));
        assert!(ladder_from_pool(&[row("600001", 1), row("600001", 2)], "2026-10-15").is_err());
        let rows = ladder_from_pool(&[row("600002", 1), row("000003", 3), row("600001", 1)], "2026-10-15").unwrap();
        assert_eq!(rows.iter().map(|r| r.code.as_str()).collect::<Vec<_>>(), vec!["000003", "600001", "600002"]);
        assert_eq!(rows[0].label, "3板");
    }

    #[test]
    fn holdings_merge_entries_and_compute_pnl() {
        let positions = vec![
            json!({"symbol": "600519", "name": "贵州茅台", "open_shares": 100.0, "open_cost": 150000.0, "planned_stop": 1400.0, "planned_target": 1800.0}),
            json!({"symbol": "SH600519", "name": "贵州茅台", "open_shares": 100.0, "open_cost": 160000.0, "planned_stop": 1450.0}),
            json!({"symbol": "000001", "name": "平安银行", "open_shares": 0.0, "open_cost": 0.0}),
        ];
        let h = holdings_from(&positions);
        assert_eq!(h.len(), 1);
        assert_eq!((h[0].shares, h[0].cost, h[0].stop, h[0].target), (200.0, Some(310000.0), Some(1450.0), Some(1800.0)));
        assert_eq!(pnl_pct(Some(1705.0), &h[0]), Some(10.0));
        assert_eq!(pnl_pct(None, &h[0]), None);
        assert_eq!(pnl_pct(Some(10.0), &Holding { shares: 1.0, cost: Some(0.0), ..Holding::default() }), None);
    }

    #[test]
    fn sources_tag_every_list() {
        let lists = Lists {
            holdings: vec![Holding { code: "600519".into(), ..Holding::default() }],
            watch: vec![("600519".into(), String::new()), ("000001".into(), String::new())],
            ladder: Some(("2026-10-15".into(), vec![LadderRow { code: "000001".into(), name: String::new(), boards: 2, label: "2板".into(), sector: String::new() }])),
            ..Lists::default()
        };
        let big = Bigcaps { at: 0, as_of: String::new(), stocks: vec![("600519".into(), String::new()), ("601318".into(), String::new())] };
        let tags = sources_of(&lists, Some(&big));
        assert_eq!(tags[0], ("600519".to_string(), vec!["持仓".to_string(), "自选".into(), "大票".into()]));
        assert_eq!(tags[1].1, vec!["自选".to_string(), "昨日2板".into()]);
        assert_eq!(tags[2], ("601318".to_string(), vec!["大票".to_string()]));
    }

    #[test]
    fn alerts_round_trip_through_sqlite() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-watch-{nonce}-{}", std::process::id()));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let db = app.db_path().to_path_buf();
        let a = Alert { ts: "10:00:00".into(), code: "600519".into(), name: "贵州茅台".into(), kind: "急拉".into(), change_pct: Some(1.6), price: Some(10.0), pct: Some(2.0), msg: "m".into(), sources: vec!["自选".into()] };
        let b = Alert { ts: "10:01:00".into(), kind: "封板".into(), change_pct: None, ..a.clone() };
        store_alerts(&db, "2026-10-16", &[a.clone(), b.clone()]).unwrap();
        assert_eq!(load_alerts(&db, "2026-10-16").unwrap(), vec![b, a]);
        assert!(load_alerts(&db, "2026-10-15").unwrap().is_empty());
        let rows = vec![TurnoverRow { code: "300750".into(), name: "宁德时代".into(), rank: 1, amount: 7e9 }];
        store_turnover(&db, "2026-10-15", &rows).unwrap();
        assert_eq!(load_turnover(&db, "2026-10-15").unwrap().unwrap().stocks, rows);
        assert!(load_turnover(&db, "2026-10-16").unwrap().is_none());
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 对真实上游跑一轮:cargo test --locked watch::tests::live_watch_probe -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_watch_probe() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-watch-live-{nonce}"));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let started = std::time::Instant::now();
        let response = app.handle(crate::MobileApiRequest { path: "/mobile/watch/poll".into(), method: "POST".into(), body: json!({}) }).await;
        let v = &response.body;
        println!("status {} in {:?}", response.status, started.elapsed());
        println!("phase {} trading {} quote_day {} notes {}", v["phase_label"], v["trading_day"], v["quote_day"], v["notes"]);
        println!(
            "ladder {} rows {} highest {}; bigcap total {} top {}; turnover {}",
            v["ladder"]["sample_date"], v["ladder"]["rows"].as_array().map_or(0, Vec::len), v["ladder"]["highest"],
            v["bigcap"]["total"], v["bigcap"]["top"].as_array().map_or(0, Vec::len), v["turnover"]["date"]
        );
        if let Some(r) = v["ladder"]["rows"].get(0) {
            println!("first ladder row {r}");
        }
        if let Ok(out) = std::env::var("VIBE_WATCH_FIXTURE") {
            std::fs::write(out, serde_json::to_string_pretty(v).unwrap()).unwrap();
        }
        if let Some(day) = v["quote_day"].as_str() {
            let stored = load_turnover(app.db_path(), day).unwrap();
            println!("turnover stored for {day}: {:?}", stored.map(|t| t.stocks.into_iter().map(|r| format!("{} {} {:.1}亿", r.rank, r.name, r.amount / 1e8)).collect::<Vec<_>>()));
        }
        assert_eq!(response.status, 200);
        assert!(v["ladder"]["rows"].as_array().is_some_and(|r| !r.is_empty()), "ladder should load");
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
