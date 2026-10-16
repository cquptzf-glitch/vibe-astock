//! 情绪复盘:晋级率、赚钱效应、连板溢价、梯队断层、封板质量、亏钱效应、昨日强势股反馈、
//! 行业题材结构、关键事件、分涨跌幅制度、连板天梯、首板名单与情绪周期。
//!
//! 口径移植自数据层 shortline_emotion.py / shortline_facts.py / shortline_boards.py:
//! 所有读数只用东财涨停 / 炸板 / 跌停 / 昨日涨停四张池子和前一交易日涨停池,任意已收盘交易日可算。
//! 与桌面版的差异如实标注:题材按东财所属行业归(桌面版另用同花顺涨停原因),
//! 亏钱效应与反馈矩阵只用定稿记录(昨日炸板股不在其中),不用实时行情兜底。
use crate::discovery::{self, DiscoveryError};
use anyhow::{anyhow, Result};
use chrono::{FixedOffset, NaiveDate, Timelike, Utc};
use futures_util::future::join_all;
use reqwest::Client;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

const ZTB_UT: &str = "7eea3edcaed734bea9cbfc24409ed989";
const ST_10PCT_FROM: &str = "2026-07-06";
const CHINEXT_20PCT_FROM: &str = "2020-08-24";
const RESULT_ORDER: [&str; 5] = ["晋级涨停", "收红", "小跌", "跌超5%", "跌停"];
const SLOTS: [(&str, &str); 5] = [
    ("09:25-09:35", "093500"),
    ("09:35-10:00", "100000"),
    ("10:00-11:30", "113000"),
    ("11:30-14:00", "140000"),
    ("14:00-15:00", "150500"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PoolKind {
    LimitUp,
    Broken,
    LimitDown,
    Yesterday,
}

impl PoolKind {
    fn api(self) -> (&'static str, &'static str) {
        match self {
            Self::LimitUp => ("getTopicZTPool", "fbt:asc"),
            Self::Broken => ("getTopicZBPool", "fbt:asc"),
            Self::LimitDown => ("getTopicDTPool", "fund:asc"),
            Self::Yesterday => ("getYesterdayZTPool", "zs:desc"),
        }
    }

    // 缺这些列就整池不用:缺开板次数会被算成「一次没炸过」,缺连板数会被算成首板。
    fn required(self) -> &'static [&'static str] {
        match self {
            Self::LimitUp => &["zbc", "lbc"],
            Self::Broken => &["zbc"],
            Self::LimitDown => &[],
            Self::Yesterday => &["ylbc"],
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::LimitUp => "涨停池",
            Self::Broken => "炸板池",
            Self::LimitDown => "跌停池",
            Self::Yesterday => "昨日涨停表现",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Row {
    pub code: String,
    pub name: String,
    pub board: &'static str,
    pub ret: Option<f64>,
    pub close: Option<f64>,
    pub turnover: Option<f64>,
    pub sector: String,
    pub amount: Option<f64>,
    pub float_cap: Option<f64>,
    pub seal_fund: Option<f64>,
    /// HHMMSS;空串 = 没有
    pub first_seal: String,
    pub last_seal: String,
    pub broken_times: i64,
    /// 东财连板数;非涨停池为 0
    pub boards: i64,
    pub stat: Option<(i64, i64)>,
    pub limit_price: Option<f64>,
    pub amplitude: Option<f64>,
    pub consec_dt: i64,
    pub open_times: i64,
    pub prev_boards: i64,
}

/// 一次复盘要用的全部池子。只有当天涨停池是必需的,其余取不到各自降级。
pub(crate) struct Pools {
    pub zt: Vec<Row>,
    pub zb: Result<Vec<Row>, String>,
    pub dt: Result<Vec<Row>, String>,
    /// (有当日涨跌幅的行, 缺涨跌幅的行)
    pub yzt: Result<(Vec<Row>, Vec<Row>), String>,
    pub prev_zt: Result<Vec<Row>, String>,
}

// ---------------------------------------------------------------- 入口

pub async fn emotion(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let requested = date_parameter(path)?;
    let calendar = trade_dates(client).await?;
    let (date, prev) = review_day(&calendar, requested.as_deref(), now_china())?;
    let settled = is_settled(&date, &calendar, now_china());
    let compact = date.replace('-', "");
    let prev_compact = prev.as_ref().map(|value| value.replace('-', ""));
    let (zt, zb, dt, yzt, prev_zt) = futures_util::join!(
        fetch_pool(client, PoolKind::LimitUp, &compact),
        fetch_pool(client, PoolKind::Broken, &compact),
        fetch_pool(client, PoolKind::LimitDown, &compact),
        fetch_pool(client, PoolKind::Yesterday, &compact),
        async {
            match &prev_compact {
                Some(day) => fetch_pool(client, PoolKind::LimitUp, day).await,
                None => Err(anyhow!("取不到前一交易日")),
            }
        },
    );
    let zt = match zt {
        Ok(rows) if !rows.is_empty() => rows,
        Ok(_) => {
            return Ok(json!({"status": "failed", "date": date, "message": format!("{date} 涨停池为空(数据还没出或上游未提供)")}));
        }
        Err(error) => {
            return Ok(json!({"status": "failed", "date": date, "message": error_message(PoolKind::LimitUp.label(), &error)}));
        }
    };
    let mut errors = Vec::new();
    let keep = |kind: PoolKind, result: Result<Vec<Row>>, errors: &mut Vec<Value>| match result {
        Ok(rows) => Ok(rows),
        Err(error) => {
            let message = error_message(kind.label(), &error);
            errors.push(json!({"source": kind.label(), "message": message}));
            Err(message)
        }
    };
    let zb = keep(PoolKind::Broken, zb, &mut errors);
    let dt = keep(PoolKind::LimitDown, dt, &mut errors);
    let yzt = keep(PoolKind::Yesterday, yzt, &mut errors).and_then(|rows| {
        let (with_ret, without): (Vec<Row>, Vec<Row>) = rows.into_iter().partition(|row| row.ret.is_some());
        if with_ret.is_empty() {
            Err("昨日涨停表现为空".to_string())
        } else {
            Ok((with_ret, without))
        }
    });
    let prev_zt = match prev_zt {
        Ok(rows) if !rows.is_empty() => Ok(rows),
        Ok(_) => {
            let message = "前一交易日涨停池为空".to_string();
            errors.push(json!({"source": "前一交易日涨停池", "message": message}));
            Err(message)
        }
        Err(error) => {
            let message = error_message("前一交易日涨停池", &error);
            errors.push(json!({"source": "前一交易日涨停池", "message": message}));
            Err(message)
        }
    };
    let pools = Pools { zt, zb, dt, yzt, prev_zt };
    let mut body = build(&date, prev.as_deref(), settled, &pools);
    body["status"] = json!(if errors.is_empty() { "ok" } else { "partial" });
    body["errors"] = Value::Array(errors);
    body["source"] = json!("eastmoney");
    body["fetched_at"] = json!(Utc::now().to_rfc3339());
    Ok(body)
}

pub async fn cycle(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let requested = date_parameter(path)?;
    let lookback = discovery::integer(path, "days", 10, 3, 30)?;
    let now = now_china();
    let calendar = trade_dates(client).await?;
    let (date, _) = review_day(&calendar, requested.as_deref(), now)?;
    let today = now.0.format("%Y-%m-%d").to_string();
    let mut window: Vec<String> = calendar.iter().filter(|day| day.as_str() <= date.as_str()).cloned().collect();
    if window.last().is_some_and(|day| *day == today && !is_settled(day, &calendar, now)) {
        window.pop();
    }
    let start = window.len().saturating_sub(lookback);
    let window = window[start..].to_vec();
    let fetches = window.iter().map(|day| {
        let compact = day.replace('-', "");
        async move {
            let (zt, zb) = futures_util::join!(
                fetch_pool(client, PoolKind::LimitUp, &compact),
                fetch_pool(client, PoolKind::Broken, &compact)
            );
            (day.clone(), zt.ok(), zb.ok())
        }
    });
    let days: Vec<(String, Option<Vec<Row>>, Option<Vec<Row>>)> = join_all(fetches).await;
    let summaries: Vec<Value> = days
        .iter()
        .filter_map(|(day, zt, zb)| day_summary(day, zt.as_deref()?, zb.as_deref()?))
        .collect();
    Ok(json!({
        "status": "ok",
        "date": date,
        "settled": is_settled(&date, &calendar, now),
        "cycle": cycle_position(summaries, lookback, window.len()),
        "source": "eastmoney",
        "fetched_at": Utc::now().to_rfc3339(),
    }))
}

// ---------------------------------------------------------------- 交易日与取数

pub(crate) type ChinaNow = (NaiveDate, u32);

pub(crate) fn now_china() -> ChinaNow {
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(8 * 3600).expect("valid offset"));
    (now.date_naive(), now.hour() * 100 + now.minute())
}

pub(crate) fn date_parameter(path: &str) -> Result<Option<String>, DiscoveryError> {
    let Some(raw) = discovery::parameter(path, "date").filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let compact = raw.trim().replace('-', "");
    if compact.len() != 8 || !compact.chars().all(|c| c.is_ascii_digit()) {
        return Err(DiscoveryError::BadRequest("date 必须是 YYYY-MM-DD 或 YYYYMMDD".into()));
    }
    let date = NaiveDate::parse_from_str(&compact, "%Y%m%d")
        .map_err(|_| DiscoveryError::BadRequest("date 不是有效日期".into()))?;
    Ok(Some(date.format("%Y-%m-%d").to_string()))
}

/// 上证指数日线的日期就是交易日历(含盘中的今天)。一次取约一年,足够覆盖东财池子的保留期。
/// 东财失败时改用腾讯同一指数日线,两边都失败才报错。
pub(crate) async fn trade_dates(client: &Client) -> Result<Vec<String>> {
    match eastmoney_trade_dates(client).await {
        Ok(dates) => Ok(dates),
        Err(primary) => {
            let root: Value = client
                .get("https://web.ifzq.gtimg.cn/appstock/app/fqkline/get")
                .query(&[("param", "sh000001,day,,,320,")])
                .timeout(Duration::from_secs(12))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            parse_tencent_dates(&root).map_err(|fallback| anyhow!("交易日历取数失败:{primary};{fallback}"))
        }
    }
}

pub(crate) fn parse_tencent_dates(root: &Value) -> Result<Vec<String>> {
    let rows = root["data"]["sh000001"]["day"].as_array().ok_or_else(|| anyhow!("腾讯日线缺少 day"))?;
    let lines: Vec<Value> = rows.iter().filter_map(|row| row.get(0).cloned()).collect();
    parse_trade_dates(&json!({"data": {"klines": lines}}))
}

async fn eastmoney_trade_dates(client: &Client) -> Result<Vec<String>> {
    let root: Value = client
        .get("https://push2his.eastmoney.com/api/qt/stock/kline/get")
        .query(&[
            ("secid", "1.000001"),
            ("klt", "101"),
            ("fqt", "0"),
            ("lmt", "250"),
            ("end", "20500101"),
            ("fields1", "f1"),
            ("fields2", "f51"),
        ])
        .header("Referer", "https://quote.eastmoney.com/")
        .timeout(Duration::from_secs(12))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    parse_trade_dates(&root)
}

pub(crate) fn parse_trade_dates(root: &Value) -> Result<Vec<String>> {
    let lines = root["data"]["klines"].as_array().ok_or_else(|| anyhow!("交易日历缺少 klines"))?;
    let mut dates: Vec<String> = lines
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|line| line.split(',').next())
        .filter(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").is_ok())
        .map(str::to_string)
        .collect();
    dates.sort();
    dates.dedup();
    if dates.is_empty() {
        return Err(anyhow!("交易日历为空"));
    }
    Ok(dates)
}

pub(crate) fn is_settled(day: &str, calendar: &[String], now: ChinaNow) -> bool {
    let today = now.0.format("%Y-%m-%d").to_string();
    match day.cmp(today.as_str()) {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Equal => now.1 >= 1505 && calendar.iter().any(|d| d == day),
    }
}

/// 复盘日:不给 = 最近一个已收盘交易日(盘中不拿半天的数冒充复盘);给了就校验。
/// 未来日期和非交易日直接拒绝,上游对它们返回空池,会被读成「涨停 0 家」。
pub(crate) fn review_day(
    calendar: &[String],
    requested: Option<&str>,
    now: ChinaNow,
) -> Result<(String, Option<String>), DiscoveryError> {
    let today = now.0.format("%Y-%m-%d").to_string();
    let date = match requested {
        None => calendar
            .iter()
            .rev()
            .find(|day| day.as_str() < today.as_str() || is_settled(day, calendar, now))
            .cloned()
            .ok_or_else(|| DiscoveryError::BadRequest("交易日历里找不到最近的已收盘交易日".into()))?,
        Some(day) => {
            if day > today.as_str() {
                return Err(DiscoveryError::BadRequest(format!("{day} 还没到")));
            }
            if day == today && now.1 < 930 && !calendar.iter().any(|d| d == day) {
                return Err(DiscoveryError::BadRequest("今天还没开盘,先看上一交易日".into()));
            }
            if calendar.first().is_some_and(|first| day < first.as_str()) {
                return Err(DiscoveryError::BadRequest(format!("{day} 早于可查范围(约一年)")));
            }
            if !calendar.iter().any(|d| d == day) {
                return Err(DiscoveryError::BadRequest(format!("{day} 不是交易日")));
            }
            day.to_string()
        }
    };
    let prev = calendar.iter().rev().find(|day| day.as_str() < date.as_str()).cloned();
    Ok((date, prev))
}

pub(crate) async fn fetch_pool(client: &Client, kind: PoolKind, compact_date: &str) -> Result<Vec<Row>> {
    let (api, sort) = kind.api();
    let root: Value = client
        .get(format!("https://push2ex.eastmoney.com/{api}"))
        .query(&[
            ("ut", ZTB_UT),
            ("dpt", "wz.ztzt"),
            ("Pageindex", "0"),
            ("pagesize", "10000"),
            ("sort", sort),
            ("date", compact_date),
        ])
        .header("Referer", "https://quote.eastmoney.com/")
        .timeout(Duration::from_secs(12))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    parse_pool(&root, kind, compact_date)
}

pub(crate) fn error_message(label: &str, error: &anyhow::Error) -> String {
    if error.downcast_ref::<reqwest::Error>().is_some() {
        let failure = discovery::failure(label, error);
        format!("{label}:{}", failure["message"].as_str().unwrap_or("数据源暂不可用"))
    } else {
        // 本模块自己的错误串只含池名与日期,不含用户数据。
        format!("{label}:{error}")
    }
}

// ---------------------------------------------------------------- 解析

fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(v) => Some(v.trim().to_string()),
        Value::Number(v) => Some(v.to_string()),
        _ => None,
    }
}

fn int(value: Option<&Value>) -> Option<i64> {
    discovery::number(value).map(|v| v as i64)
}

fn hhmmss(value: Option<&Value>) -> String {
    let raw = text(value).unwrap_or_default().replace(':', "");
    if raw.is_empty() || !raw.chars().all(|c| c.is_ascii_digit()) || raw.trim_start_matches('0').is_empty() {
        return String::new();
    }
    format!("{raw:0>6}")
}

fn price(value: Option<&Value>) -> Option<f64> {
    discovery::number(value).map(|v| v / 1000.0)
}

fn is_st(name: &str) -> bool {
    name.replace('Ｓ', "S").replace('Ｔ', "T").to_uppercase().contains("ST")
}

pub(crate) fn board_of(code: &str, name: &str) -> &'static str {
    let st = is_st(name);
    if ["43", "83", "87", "88", "92"].iter().any(|p| code.starts_with(p)) {
        "北交所"
    } else if ["30", "688", "689"].iter().any(|p| code.starts_with(p)) {
        if st { "20cm(ST)" } else { "20cm" }
    } else if st {
        "主板ST"
    } else {
        "10cm"
    }
}

pub(crate) fn limit_pct(code: &str, name: &str, day: &str) -> f64 {
    match board_of(code, name) {
        "北交所" => 30.0,
        "主板ST" => if day < ST_10PCT_FROM { 5.0 } else { 10.0 },
        "20cm" | "20cm(ST)" if code.starts_with("30") && day < CHINEXT_20PCT_FROM => 10.0,
        "20cm" | "20cm(ST)" => 20.0,
        _ => 10.0,
    }
}

fn is_limit_down(row: &Row, day: &str) -> bool {
    row.ret.is_some_and(|ret| ret <= -(limit_pct(&row.code, &row.name, day) - 0.3))
}

fn is_limit_up(row: &Row, day: &str) -> Option<bool> {
    if let (Some(close), Some(limit)) = (row.close, row.limit_price) {
        if limit > 0.0 {
            return Some((close * 100.0).round() == (limit * 100.0).round());
        }
    }
    row.ret.map(|ret| ret >= limit_pct(&row.code, &row.name, day) - 0.3)
}

pub(crate) fn parse_pool(root: &Value, kind: PoolKind, requested: &str) -> Result<Vec<Row>> {
    if root.get("rc").and_then(Value::as_i64).is_some_and(|rc| rc != 0) {
        return Err(anyhow!("上游返回业务错误"));
    }
    let data = match root.get("data") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Object(data)) => data,
        _ => return Err(anyhow!("响应格式错误")),
    };
    if let Some(qdate) = text(data.get("qdate")).filter(|q| q.len() == 8) {
        if qdate.as_str() < requested {
            return Err(anyhow!("上游还没有 {requested} 的数据(最新 {qdate})"));
        }
    }
    let raw = match data.get("pool") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(rows)) => rows,
        _ => return Err(anyhow!("pool 格式错误")),
    };
    let mut rows = Vec::with_capacity(raw.len());
    let mut missing: BTreeMap<&str, usize> = BTreeMap::new();
    for item in raw {
        let Some(obj) = item.as_object() else { continue };
        let code = format!("{:0>6}", text(obj.get("c")).unwrap_or_default());
        if code.trim_start_matches('0').is_empty() || code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        for field in kind.required() {
            let absent = match obj.get(*field) {
                None | Some(Value::Null) => true,
                Some(Value::String(v)) => v.trim().is_empty() || v.trim() == "-",
                _ => false,
            };
            if absent {
                *missing.entry(field).or_default() += 1;
            }
        }
        let name = text(obj.get("n")).unwrap_or_default();
        let stat = obj.get("zttj").and_then(Value::as_object).and_then(|z| Some((int(z.get("days"))?, int(z.get("ct"))?)));
        let one = |v: Option<i64>| v.filter(|n| *n != 0).unwrap_or(1);
        let mut row = Row {
            board: board_of(&code, &name),
            ret: discovery::number(obj.get("zdp")),
            close: price(obj.get("p")),
            turnover: discovery::number(obj.get("hs")),
            sector: text(obj.get("hybk")).unwrap_or_default(),
            stat,
            code,
            name,
            ..Row::default()
        };
        match kind {
            PoolKind::LimitUp => {
                row.amount = discovery::number(obj.get("amount"));
                row.float_cap = discovery::number(obj.get("ltsz"));
                row.seal_fund = discovery::number(obj.get("fund"));
                row.first_seal = hhmmss(obj.get("fbt"));
                row.last_seal = hhmmss(obj.get("lbt"));
                row.broken_times = int(obj.get("zbc")).unwrap_or(0);
                row.boards = one(int(obj.get("lbc")));
            }
            PoolKind::Broken => {
                row.amount = discovery::number(obj.get("amount"));
                row.limit_price = price(obj.get("ztp"));
                row.first_seal = hhmmss(obj.get("fbt"));
                row.broken_times = int(obj.get("zbc")).unwrap_or(0);
                row.amplitude = discovery::number(obj.get("zf"));
            }
            PoolKind::LimitDown => {
                row.amount = discovery::number(obj.get("amount"));
                row.seal_fund = discovery::number(obj.get("fund"));
                row.last_seal = hhmmss(obj.get("lbt"));
                row.consec_dt = one(int(obj.get("days")));
                row.open_times = int(obj.get("oc")).unwrap_or(0);
            }
            PoolKind::Yesterday => {
                row.prev_boards = one(int(obj.get("ylbc")));
                // 定稿记录里的封板时间是前一天的最后封板时间。
                row.last_seal = hhmmss(obj.get("yfbt"));
                row.limit_price = price(obj.get("ztp"));
            }
        }
        rows.push(row);
    }
    if let Some((field, count)) = missing.iter().next() {
        return Err(anyhow!("{count} 行缺「{field}」,上游格式可能变了,不按默认值补"));
    }
    Ok(rows)
}

// ---------------------------------------------------------------- 计算

pub(crate) fn r3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

pub(crate) fn r2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn ratio(part: usize, total: usize) -> Value {
    if total == 0 { Value::Null } else { json!(r3(part as f64 / total as f64)) }
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

pub(crate) fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 { (sorted[mid - 1] + sorted[mid]) / 2.0 } else { sorted[mid] }
}

fn display_time(raw: &str) -> Value {
    if raw.len() == 6 {
        json!(format!("{}:{}:{}", &raw[0..2], &raw[2..4], &raw[4..6]))
    } else {
        Value::Null
    }
}

pub(crate) fn board_label(boards: i64, stat: Option<(i64, i64)>) -> String {
    match stat {
        Some((days, count)) if count > 0 && days > count => format!("{days}天{count}板"),
        Some((_, count)) if count > 0 => format!("{count}板"),
        _ => format!("{}板", boards.max(1)),
    }
}

fn unavailable(reason: impl Into<String>) -> Value {
    json!({"available": false, "reason": reason.into()})
}

fn coverage(sample: usize, missing: usize) -> Map<String, Value> {
    let total = sample + missing;
    let mut out = Map::new();
    out.insert("sample".into(), json!(sample));
    out.insert("expected_sample".into(), json!(total));
    out.insert("coverage_rate".into(), ratio(sample, total));
    out.insert("partial".into(), json!(missing > 0));
    out
}

fn with(mut base: Map<String, Value>, extra: Value) -> Value {
    if let Value::Object(extra) = extra {
        base.extend(extra);
    }
    Value::Object(base)
}

pub(crate) fn build(date: &str, prev: Option<&str>, settled: bool, pools: &Pools) -> Value {
    let zt = &pools.zt;
    let today_codes: HashSet<&str> = zt.iter().map(|row| row.code.as_str()).collect();
    let prev_zt = pools.prev_zt.as_ref().ok();
    // 昨日板数:前一交易日涨停池为准;东财偶有昨日涨停池里有、前一日涨停池漏掉的票,用昨日涨停池的「昨日连板」补上。
    // 晋级率仍只按前一交易日涨停池算,与桌面版口径一致。
    let mut prev_boards: HashMap<&str, i64> = prev_zt
        .map(|rows| rows.iter().map(|row| (row.code.as_str(), row.boards)).collect())
        .unwrap_or_default();
    if let Ok((rows, missing)) = &pools.yzt {
        for row in rows.iter().chain(missing) {
            prev_boards.entry(row.code.as_str()).or_insert(row.prev_boards);
        }
    }
    let prev_known = prev_zt.is_some() || pools.yzt.is_ok();
    let highest = zt.iter().map(|row| row.boards).max().unwrap_or(0);
    let consec = zt.iter().filter(|row| row.boards >= 2).count();
    let zb_len = pools.zb.as_ref().ok().map(Vec::len);
    let dt_len = pools.dt.as_ref().ok().map(Vec::len);
    let broken_rate = zb_len.map(|zb| r3(zb as f64 / (zb + zt.len()) as f64));
    let prev_summary = prev_zt.map(|rows| {
        json!({
            "limit_up": rows.len(),
            "highest": rows.iter().map(|row| row.boards).max().unwrap_or(0),
            "consec_count": rows.iter().filter(|row| row.boards >= 2).count(),
        })
    });
    let limit_up_rows: Vec<Value> = zt
        .iter()
        .map(|row| {
            let mut value = stock_value(row);
            value["prev_boards"] = prev_value(&prev_boards, prev_known, &row.code);
            value
        })
        .collect();
    json!({
        "date": date,
        "prev_date": prev,
        "settled": settled,
        "summary": {
            "limit_up": zt.len(),
            "broken": zb_len,
            "limit_down": dt_len,
            "highest": highest,
            "consec_count": consec,
            "first_count": zt.len() - consec,
            "broken_rate": broken_rate,
            "seal_rate": broken_rate.map(|rate| r3(1.0 - rate)),
        },
        "prev_summary": prev_summary,
        "money_effect": money_effect(pools, &today_codes, date, prev),
        "promotion": promotion(prev_zt, &today_codes, zt.len(), prev, &pools.prev_zt),
        "consec_premium": consec_premium(pools, prev),
        "ladder_gap": ladder_gap(zt),
        "seal_quality": seal_quality(zt, pools.zb.as_ref().ok()),
        "loss_effect": loss_effect(pools, date, prev),
        "feedback": feedback(pools, &today_codes, date, prev),
        "themes": themes(zt, pools.zb.as_ref().ok(), prev_zt),
        "events": events(zt, pools.zb.as_ref().ok(), pools.dt.as_ref().ok(), &prev_boards, prev_known, prev),
        "by_board": by_board(zt, pools.zb.as_ref().ok(), pools.dt.as_ref().ok(), prev_zt, date, prev),
        "ladder": ladder(pools, &today_codes, prev),
        "first_board": first_board(zt),
        "limit_up_rows": limit_up_rows,
        "broken_rows": pool_rows(&pools.zb, &prev_boards, prev_known),
        "limit_down_rows": pool_rows(&pools.dt, &prev_boards, prev_known),
        "source_note": "题材按东财所属行业归;桌面版另用同花顺涨停原因。昨日炸板股不在定稿记录中,不计入反馈矩阵和修复读数。",
    })
}

fn prev_value(prev_boards: &HashMap<&str, i64>, prev_ok: bool, code: &str) -> Value {
    match prev_boards.get(code) {
        Some(boards) => json!(boards),
        None if prev_ok => json!(0),
        None => Value::Null,
    }
}

fn stock_value(row: &Row) -> Value {
    let label = board_label(row.boards, row.stat);
    json!({
        "code": row.code,
        "name": row.name,
        "board": row.board,
        "boards": row.boards,
        "label": label,
        "rebound": row.boards > 0 && label != format!("{}板", row.boards),
        "ret": row.ret.map(r2),
        "close": row.close,
        "amount": row.amount,
        "float_cap": row.float_cap,
        "turnover": row.turnover,
        "seal_fund": row.seal_fund,
        "first_seal": display_time(&row.first_seal),
        "last_seal": display_time(&row.last_seal),
        "broken_times": row.broken_times,
        "sector": row.sector,
    })
}

fn pool_rows(pool: &Result<Vec<Row>, String>, prev_boards: &HashMap<&str, i64>, prev_ok: bool) -> Value {
    match pool {
        Err(reason) => json!({"available": false, "reason": reason, "rows": []}),
        Ok(rows) => json!({
            "available": true,
            "rows": rows.iter().map(|row| json!({
                "code": row.code,
                "name": row.name,
                "board": row.board,
                "ret": row.ret.map(r2),
                "close": row.close,
                "amount": row.amount,
                "turnover": row.turnover,
                "limit_price": row.limit_price,
                "amplitude": row.amplitude,
                "seal_fund": row.seal_fund,
                "first_seal": display_time(&row.first_seal),
                "last_seal": display_time(&row.last_seal),
                "broken_times": row.broken_times,
                "consec_dt": row.consec_dt,
                "open_times": row.open_times,
                "sector": row.sector,
                "prev_boards": prev_value(prev_boards, prev_ok, &row.code),
            })).collect::<Vec<_>>(),
        }),
    }
}

fn promotion(
    prev_zt: Option<&Vec<Row>>,
    today_codes: &HashSet<&str>,
    today_count: usize,
    prev: Option<&str>,
    prev_result: &Result<Vec<Row>, String>,
) -> Value {
    let Some(prev_day) = prev else { return unavailable("取不到前一交易日") };
    let Some(prev_zt) = prev_zt else {
        return unavailable(prev_result.as_ref().err().cloned().unwrap_or_else(|| format!("{prev_day} 涨停池取数失败")));
    };
    let mut buckets: [(usize, usize); 3] = [(0, 0); 3];
    for row in prev_zt {
        let index = match row.boards { 1 => 0, 2 => 1, _ => 2 };
        buckets[index].0 += 1;
        if today_codes.contains(row.code.as_str()) {
            buckets[index].1 += 1;
        }
    }
    let tier = |(base, promoted): (usize, usize)| json!({"base": base, "promoted": promoted, "rate": ratio(promoted, base)});
    let base: usize = buckets.iter().map(|b| b.0).sum();
    let promoted: usize = buckets.iter().map(|b| b.1).sum();
    json!({
        "available": true,
        "prev_date": prev_day,
        "tiers": {"1进2": tier(buckets[0]), "2进3": tier(buckets[1]), "3板以上晋级": tier(buckets[2])},
        "overall": tier((base, promoted)),
        "limit_up_count": today_count,
        "prev_limit_up_count": prev_zt.len(),
    })
}

fn money_effect(pools: &Pools, today_codes: &HashSet<&str>, date: &str, prev: Option<&str>) -> Value {
    let (rows, missing) = match &pools.yzt {
        Ok(value) => value,
        Err(reason) => return unavailable(reason.clone()),
    };
    let values: Vec<f64> = rows.iter().filter_map(|row| row.ret).collect();
    let again = rows.iter().filter(|row| today_codes.contains(row.code.as_str()) || (today_codes.is_empty() && is_limit_up(row, date) == Some(true))).count();
    with(coverage(values.len(), missing.len()), json!({
        "available": true,
        "prev_date": prev,
        "source": "settled",
        "avg": r2(mean(&values)),
        "median": r2(median(&values)),
        "positive_rate": ratio(values.iter().filter(|v| **v > 0.0).count(), values.len()),
        "limit_up_again_rate": ratio(again, values.len()),
    }))
}

fn consec_premium(pools: &Pools, prev: Option<&str>) -> Value {
    let (rows, missing) = match &pools.yzt {
        Ok(value) => value,
        Err(reason) => return unavailable(reason.clone()),
    };
    let values: Vec<f64> = rows.iter().filter(|row| row.prev_boards >= 2).filter_map(|row| row.ret).collect();
    if values.is_empty() {
        return unavailable(format!("{} 无 2 板以上个股", prev.unwrap_or("前一交易日")));
    }
    let miss = missing.iter().filter(|row| row.prev_boards >= 2).count();
    with(coverage(values.len(), miss), json!({
        "available": true,
        "prev_date": prev,
        "source": "settled",
        "avg": r2(mean(&values)),
        "median": r2(median(&values)),
        "positive_rate": ratio(values.iter().filter(|v| **v > 0.0).count(), values.len()),
    }))
}

fn ladder_gap(zt: &[Row]) -> Value {
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    for row in zt.iter().filter(|row| row.boards >= 2) {
        *counts.entry(row.boards).or_default() += 1;
    }
    let Some(&highest) = counts.keys().next_back() else {
        return json!({"available": true, "highest": zt.iter().map(|r| r.boards).max().unwrap_or(0), "tiers": [], "gaps": [], "continuous": true, "note": "无 2 板以上个股,梯队无从谈起"});
    };
    let gaps: Vec<i64> = (2..highest).filter(|b| !counts.contains_key(b)).collect();
    let note = if gaps.is_empty() {
        "全市场板位结构连续(跨题材,不代表同一题材内部有梯队)".to_string()
    } else {
        format!(
            "板位缺档 {} → 最高标下方断层;是否有同题材承接需另查",
            gaps.iter().map(|g| format!("{g}板")).collect::<Vec<_>>().join("、")
        )
    };
    json!({
        "available": true,
        "highest": highest,
        "tiers": counts.iter().rev().map(|(boards, count)| json!({"boards": boards, "count": count})).collect::<Vec<_>>(),
        "gaps": gaps,
        "continuous": gaps.is_empty(),
        "note": note,
    })
}

fn seal_quality(zt: &[Row], zb: Option<&Vec<Row>>) -> Value {
    let total = zt.len();
    let never = zt.iter().filter(|row| row.broken_times == 0).count();
    let reopened = total - never;
    let seconds = zt.iter().filter(|row| !row.first_seal.is_empty() && row.first_seal.as_str() <= "093500").count();
    let late = zt.iter().filter(|row| !row.last_seal.is_empty() && row.last_seal.as_str() >= "143000").count();
    let broken_times: i64 = zt.iter().map(|row| row.broken_times).sum();
    json!({
        "available": true,
        "total": total,
        "never_broken": never,
        "never_broken_rate": ratio(never, total),
        "opening_seconds": seconds,
        "late_seal": late,
        "reopened": reopened,
        "avg_broken_times": r2(broken_times as f64 / total.max(1) as f64),
        "broken_pool": zb.map(Vec::len),
        "broken_rate": zb.map(|zb| r3(zb.len() as f64 / (zb.len() + total) as f64)),
        "ever_opened_rate": ratio(reopened, total),
        "note": if zb.is_none() { "炸板池未取得,炸板率未算" } else { "炸板率 = 炸板未回封 ÷(涨停 + 炸板未回封)" },
    })
}

fn loss_effect(pools: &Pools, date: &str, prev: Option<&str>) -> Value {
    let (rows, missing) = match &pools.yzt {
        Ok(value) => value,
        Err(reason) => return unavailable(reason.clone()),
    };
    let values: Vec<f64> = rows.iter().filter_map(|row| row.ret).collect();
    let deep5 = values.iter().filter(|v| **v <= -5.0).count();
    let worst = values.iter().copied().fold(f64::INFINITY, f64::min);
    with(coverage(values.len(), missing.len()), json!({
        "available": true,
        "prev_date": prev,
        "source": "settled",
        "deep_loss_5_count": deep5,
        "deep_loss_5_rate": ratio(deep5, values.len()),
        "deep_loss_7_count": values.iter().filter(|v| **v <= -7.0).count(),
        "limit_down_count": rows.iter().filter(|row| is_limit_down(row, date)).count(),
        "worst": r2(worst),
        "market_limit_down": pools.dt.as_ref().ok().map(Vec::len),
        "prev_broken_recovery": Value::Null,
        "note": if pools.dt.is_ok() {
            "昨日强势股由定稿记录算;全市场跌停取复盘日东财跌停池。昨日炸板股修复未覆盖。"
        } else {
            "昨日强势股由定稿记录算;复盘日跌停池未取得。昨日炸板股修复未覆盖。"
        },
    }))
}

fn tier_name(boards: i64) -> String {
    if boards <= 1 { "首板".into() } else { format!("{boards}板") }
}

fn feedback(pools: &Pools, today_codes: &HashSet<&str>, date: &str, prev: Option<&str>) -> Value {
    let (rows, missing) = match &pools.yzt {
        Ok(value) => value,
        Err(reason) => return unavailable(reason.clone()),
    };
    let mut matrix: BTreeMap<i64, [usize; 5]> = BTreeMap::new();
    let mut details = Vec::new();
    for row in rows {
        let Some(ret) = row.ret else { continue };
        let result = if today_codes.contains(row.code.as_str()) {
            0
        } else if is_limit_down(row, date) {
            4
        } else if ret <= -5.0 {
            3
        } else if ret > 0.0 {
            1
        } else {
            2
        };
        matrix.entry(row.prev_boards.max(1)).or_insert([0; 5])[result] += 1;
        details.push((ret, json!({
            "code": row.code, "name": row.name, "board": row.prev_boards, "prev_tier": tier_name(row.prev_boards),
            "ret": r2(ret), "result": RESULT_ORDER[result], "sector": row.sector,
        })));
    }
    details.sort_by(|a, b| a.0.total_cmp(&b.0));
    let tiers: Vec<Value> = matrix
        .iter()
        .rev()
        .map(|(boards, cells)| {
            let total: usize = cells.iter().sum();
            let mut cell = Map::new();
            cell.insert("tier".into(), json!(tier_name(*boards)));
            cell.insert("boards".into(), json!(boards));
            for (index, key) in RESULT_ORDER.iter().enumerate() {
                cell.insert((*key).into(), json!(cells[index]));
            }
            cell.insert("合计".into(), json!(total));
            cell.insert("晋级率".into(), ratio(cells[0], total));
            Value::Object(cell)
        })
        .collect();
    with(coverage(details.len(), missing.len()), json!({
        "available": true,
        "prev_date": prev,
        "source": "settled",
        "result_order": RESULT_ORDER,
        "rows": tiers,
        "details": details.into_iter().map(|(_, value)| value).collect::<Vec<_>>(),
        "note": "涨跌幅取昨日涨停股当日定稿记录;晋级以复盘日东财涨停池成员为准。记录不含昨日炸板股。",
    }))
}

fn themes(zt: &[Row], zb: Option<&Vec<Row>>, prev_zt: Option<&Vec<Row>>) -> Value {
    struct Group<'a> {
        limit_up: usize,
        first: usize,
        consec: usize,
        highest: i64,
        broken: usize,
        seals: Vec<&'a str>,
        names: Vec<&'a Row>,
        prev: usize,
        continued: usize,
    }
    let sector = |row: &Row| if row.sector.trim().is_empty() { "未分类".to_string() } else { row.sector.trim().to_string() };
    let today: HashSet<&str> = zt.iter().map(|row| row.code.as_str()).collect();
    let mut groups: HashMap<String, Group> = HashMap::new();
    for row in zt {
        let group = groups.entry(sector(row)).or_insert_with(|| Group {
            limit_up: 0, first: 0, consec: 0, highest: 0, broken: 0, seals: vec![], names: vec![], prev: 0, continued: 0,
        });
        group.limit_up += 1;
        group.highest = group.highest.max(row.boards);
        if row.boards <= 1 { group.first += 1 } else { group.consec += 1 }
        if !row.first_seal.is_empty() {
            group.seals.push(&row.first_seal);
        }
        group.names.push(row);
    }
    for row in zb.into_iter().flatten() {
        if let Some(group) = groups.get_mut(&sector(row)) {
            group.broken += 1;
        }
    }
    let mut prev_only: BTreeMap<String, usize> = BTreeMap::new();
    for row in prev_zt.into_iter().flatten() {
        match groups.get_mut(&sector(row)) {
            Some(group) => {
                group.prev += 1;
                if today.contains(row.code.as_str()) {
                    group.continued += 1;
                }
            }
            None => *prev_only.entry(sector(row)).or_default() += 1,
        }
    }
    let mut rows: Vec<(String, Group)> = groups.into_iter().collect();
    rows.sort_by(|a, b| b.1.limit_up.cmp(&a.1.limit_up).then(b.1.highest.cmp(&a.1.highest)).then(a.0.cmp(&b.0)));
    let themes: Vec<Value> = rows
        .iter_mut()
        .map(|(name, group)| {
            group.seals.sort();
            group.names.sort_by(|a, b| b.boards.cmp(&a.boards).then(a.first_seal.cmp(&b.first_seal)));
            let attempts = group.limit_up + group.broken;
            json!({
                "sector": name,
                "limit_up": group.limit_up,
                "first_boards": group.first,
                "consec_boards": group.consec,
                "highest": group.highest,
                "broken": group.broken,
                "broken_rate": if zb.is_some() { ratio(group.broken, attempts) } else { Value::Null },
                "first_seal": group.seals.first().map(|t| display_time(t)).unwrap_or(Value::Null),
                "last_seal": group.seals.last().map(|t| display_time(t)).unwrap_or(Value::Null),
                "prev_limit_up": prev_zt.map(|_| group.prev),
                "continued": prev_zt.map(|_| group.continued),
                "continuation_rate": if prev_zt.is_some() { ratio(group.continued, group.prev) } else { Value::Null },
                "names": group.names.iter().map(|row| json!({
                    "code": row.code, "name": row.name, "boards": row.boards, "label": board_label(row.boards, row.stat),
                    "first_seal": display_time(&row.first_seal), "broken_times": row.broken_times,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let mut times: Vec<&str> = zt.iter().filter(|r| !r.first_seal.is_empty()).map(|r| r.first_seal.as_str()).collect();
    times.sort();
    let mut low = "000000";
    let timeline: Vec<Value> = SLOTS
        .iter()
        .map(|(label, high)| {
            let count = times.iter().filter(|t| **t > low && **t <= *high).count();
            low = high;
            json!({"slot": label, "count": count})
        })
        .collect();
    json!({
        "available": true,
        "themes": themes,
        "theme_count": rows.len(),
        "concentration": rows.first().map(|(_, g)| json!(r3(g.limit_up as f64 / zt.len() as f64))).unwrap_or(Value::Null),
        "timeline": timeline,
        "faded": prev_only.into_iter().map(|(sector, count)| json!({"sector": sector, "prev_limit_up": count})).collect::<Vec<_>>(),
        "prev_available": prev_zt.is_some(),
    })
}

fn events(zt: &[Row], zb: Option<&Vec<Row>>, dt: Option<&Vec<Row>>, prev_boards: &HashMap<&str, i64>, prev_ok: bool, prev: Option<&str>) -> Value {
    let unknown = "(昨日板数未取得)";
    let mut list: Vec<Value> = Vec::new();
    let mut add = |tag: &str, row: &Row, note: String| {
        list.push(json!({
            "tag": tag, "code": row.code, "name": row.name, "board": row.board, "boards": row.boards,
            "sector": row.sector, "ret": row.ret.map(r2), "first_seal": display_time(&row.first_seal),
            "last_seal": display_time(&row.last_seal), "broken_times": row.broken_times, "turnover": row.turnover,
            "note": note, "prev_boards": prev_boards.get(row.code.as_str()),
        }));
    };
    if let Some(top) = zt.iter().map(|row| row.boards).max() {
        for row in zt.iter().filter(|row| row.boards == top) {
            add("最高标", row, format!("今日最高 {top} 板"));
        }
        let mut by_broken: Vec<&Row> = zt.iter().collect();
        by_broken.sort_by(|a, b| b.broken_times.cmp(&a.broken_times));
        for row in by_broken.into_iter().take(3).filter(|row| row.broken_times >= 2) {
            add("反复开板", row, format!("炸板 {} 次后回封", row.broken_times));
        }
        let mut by_seal: Vec<&Row> = zt.iter().filter(|row| !row.first_seal.is_empty()).collect();
        by_seal.sort_by(|a, b| a.first_seal.cmp(&b.first_seal));
        let mut seen = HashSet::new();
        for row in by_seal {
            let sector = row.sector.trim();
            if !sector.is_empty() && seen.insert(sector.to_string()) {
                add("题材首封", row, format!("{sector} 当日最早封板"));
            }
            if seen.len() >= 4 {
                break;
            }
        }
    }
    let mut broken: Vec<&Row> = zb.into_iter().flatten().collect();
    broken.sort_by(|a, b| {
        let pa = prev_boards.get(a.code.as_str()).copied().unwrap_or(0);
        let pb = prev_boards.get(b.code.as_str()).copied().unwrap_or(0);
        pb.cmp(&pa).then(b.broken_times.cmp(&a.broken_times))
    });
    for row in broken.into_iter().take(6) {
        match prev_boards.get(row.code.as_str()).copied() {
            Some(pb) if pb >= 3 => add("高位断板", row, format!("昨日 {pb} 板,今日炸板未回封(炸 {} 次)", row.broken_times)),
            Some(pb) if pb >= 2 => add("连板断板", row, format!("昨日 {pb} 板,今日炸板未回封")),
            Some(pb) => add("炸板", row, format!("炸板 {} 次未回封,昨日 {pb} 板", row.broken_times)),
            None => add("炸板", row, format!("炸板 {} 次未回封{}", row.broken_times, if prev_ok { "(昨日未涨停)" } else { unknown })),
        }
    }
    let mut down: Vec<&Row> = dt.into_iter().flatten().collect();
    down.sort_by(|a, b| {
        let pa = prev_boards.get(a.code.as_str()).copied().unwrap_or(0);
        let pb = prev_boards.get(b.code.as_str()).copied().unwrap_or(0);
        pb.cmp(&pa).then(b.consec_dt.cmp(&a.consec_dt))
    });
    for row in down.into_iter().take(5) {
        let pb = prev_boards.get(row.code.as_str()).copied();
        let suffix = match pb {
            Some(pb) => format!("|昨日 {pb} 板"),
            None if prev_ok => String::new(),
            None => unknown.to_string(),
        };
        add(if pb.is_some_and(|pb| pb >= 2) { "高标跌停" } else { "跌停" }, row, format!("连续跌停 {} 天{suffix}", row.consec_dt.max(1)));
    }
    let loss = ["高位断板", "连板断板", "炸板", "高标跌停", "跌停"];
    let gain = ["最高标", "题材首封"];
    let pick = |filter: &dyn Fn(&str) -> bool| list.iter().filter(|e| filter(e["tag"].as_str().unwrap_or(""))).cloned().collect::<Vec<_>>();
    json!({
        "available": true,
        "count": list.len(),
        "prev_date": prev,
        "prev_available": prev_ok,
        "loss_events": pick(&|tag| loss.contains(&tag)),
        "gain_events": pick(&|tag| gain.contains(&tag)),
        "split_events": pick(&|tag| !loss.contains(&tag) && !gain.contains(&tag)),
        "zb_available": zb.is_some(),
        "dt_available": dt.is_some(),
    })
}

fn by_board(zt: &[Row], zb: Option<&Vec<Row>>, dt: Option<&Vec<Row>>, prev_zt: Option<&Vec<Row>>, date: &str, prev: Option<&str>) -> Value {
    #[derive(Default)]
    struct Stat { limit_up: usize, highest: i64, broken: usize, limit_down: usize, prev_base: usize, promoted: usize }
    let today: HashSet<&str> = zt.iter().map(|row| row.code.as_str()).collect();
    let mut out: BTreeMap<&str, Stat> = BTreeMap::new();
    for row in zt {
        let stat = out.entry(row.board).or_default();
        stat.limit_up += 1;
        stat.highest = stat.highest.max(row.boards);
    }
    // 炸板 / 跌停按行累加到各自制度:当天没有涨停的制度也要算上。
    for row in zb.into_iter().flatten() {
        out.entry(row.board).or_default().broken += 1;
    }
    for row in dt.into_iter().flatten() {
        out.entry(row.board).or_default().limit_down += 1;
    }
    for row in prev_zt.into_iter().flatten() {
        let stat = out.entry(row.board).or_default();
        stat.prev_base += 1;
        if today.contains(row.code.as_str()) {
            stat.promoted += 1;
        }
    }
    let mut note = if date >= ST_10PCT_FROM {
        "按 2026-07-06 起施行的交易规则:主板 ST 涨跌幅 10%".to_string()
    } else {
        "主板 ST 当时涨跌幅 5%".to_string()
    };
    if prev_zt.is_none() {
        note.push_str(";前一交易日涨停池未取得,晋级未算");
    }
    if zb.is_none() || dt.is_none() {
        note.push_str(";炸板或跌停池未取得,对应列不完整");
    }
    json!({
        "available": true,
        "prev_date": prev,
        "prev_available": prev_zt.is_some(),
        "boards": out.iter().map(|(board, s)| json!({
            "board": board, "limit_up": s.limit_up, "highest": s.highest,
            "broken": zb.map(|_| s.broken), "limit_down": dt.map(|_| s.limit_down),
            "prev_base": prev_zt.map(|_| s.prev_base), "promoted": prev_zt.map(|_| s.promoted),
            "promotion_rate": if prev_zt.is_some() { ratio(s.promoted, s.prev_base) } else { Value::Null },
        })).collect::<Vec<_>>(),
        "note": note,
    })
}

fn ladder(pools: &Pools, today_codes: &HashSet<&str>, prev: Option<&str>) -> Value {
    #[derive(Default)]
    struct Level { stocks: Vec<Value>, fell: Vec<(Option<f64>, Value)>, base: Option<usize>, promoted: Option<usize> }
    let zt = &pools.zt;
    let mut levels: BTreeMap<i64, Level> = BTreeMap::new();
    for row in zt {
        levels.entry(row.boards.max(1)).or_default().stocks.push(stock_value(row));
    }
    let prev_note = match (&pools.prev_zt, prev) {
        (Ok(_), _) => String::new(),
        (Err(_), Some(day)) => format!("前一交易日 {day} 的涨停池没取到:没接上的票和晋级数没法算"),
        (Err(_), None) => "取不到前一交易日".into(),
    };
    if let Ok(prev_zt) = &pools.prev_zt {
        let settled: HashMap<&str, f64> = pools.yzt.as_ref().ok().into_iter()
            .flat_map(|(rows, _)| rows.iter())
            .filter_map(|row| Some((row.code.as_str(), row.ret?)))
            .collect();
        let zb: HashMap<&str, &Row> = pools.zb.as_ref().ok().into_iter().flatten().map(|r| (r.code.as_str(), r)).collect();
        let dt: HashMap<&str, &Row> = pools.dt.as_ref().ok().into_iter().flatten().map(|r| (r.code.as_str(), r)).collect();
        for row in prev_zt {
            let level = levels.entry(row.boards + 1).or_default();
            level.base = Some(level.base.unwrap_or(0) + 1);
            if today_codes.contains(row.code.as_str()) {
                level.promoted = Some(level.promoted.unwrap_or(0) + 1);
                continue;
            }
            level.promoted = Some(level.promoted.unwrap_or(0));
            let mut ret = settled.get(row.code.as_str()).copied();
            let outcome = if let Some(hit) = dt.get(row.code.as_str()) {
                ret = ret.or(hit.ret);
                "跌停"
            } else if let Some(hit) = zb.get(row.code.as_str()) {
                ret = ret.or(hit.ret);
                "炸板"
            } else if ret.is_some() {
                "没涨停"
            } else {
                "没取到"
            };
            level.fell.push((ret, json!({
                "code": row.code, "name": row.name, "prev_boards": row.boards,
                "prev_label": board_label(row.boards, row.stat), "outcome": outcome, "ret": ret.map(r2), "sector": row.sector,
            })));
        }
    }
    let rows: Vec<Value> = levels
        .into_iter()
        .rev()
        .map(|(boards, mut level)| {
            level.fell.sort_by(|a, b| match (a.0, b.0) {
                (Some(x), Some(y)) => y.total_cmp(&x),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            });
            let rate = match (level.base, level.promoted) {
                (Some(base), Some(promoted)) if base > 0 => ratio(promoted, base),
                _ => Value::Null,
            };
            json!({
                "boards": boards, "stocks": level.stocks, "base": level.base, "promoted": level.promoted, "rate": rate,
                "fell": level.fell.into_iter().map(|(_, value)| value).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "available": true,
        "levels": rows,
        "total_limit_up": zt.len(),
        "highest": zt.iter().map(|row| row.boards).max().unwrap_or(0),
        "consec_count": zt.iter().filter(|row| row.boards >= 2).count(),
        "prev_available": pools.prev_zt.is_ok(),
        "prev_note": prev_note,
    })
}

fn first_board(zt: &[Row]) -> Value {
    let mut stocks: Vec<&Row> = zt.iter().filter(|row| row.boards <= 1).collect();
    stocks.sort_by(|a, b| {
        let key = |row: &Row| if row.first_seal.is_empty() { "999999".to_string() } else { row.first_seal.clone() };
        key(a).cmp(&key(b))
    });
    let values: Vec<Value> = stocks.iter().map(|row| stock_value(row)).collect();
    let rebound = values.iter().filter(|value| value["rebound"] == true).count();
    json!({
        "available": true,
        "total_limit_up": zt.len(),
        "first_count": values.len(),
        "rebound_count": rebound,
        "stocks": values,
    })
}

// ---------------------------------------------------------------- 情绪周期

fn day_summary(day: &str, zt: &[Row], zb: &[Row]) -> Option<Value> {
    // 炸板池取不到就整天不要:按 0 家算会把炸板率写成 0%。
    if zt.is_empty() {
        return None;
    }
    Some(json!({
        "date": day,
        "limit_up": zt.len(),
        "highest_consec": zt.iter().map(|row| row.boards.max(1)).max().unwrap_or(1),
        "broken_rate": zb.len() as f64 / (zb.len() + zt.len()) as f64,
    }))
}

fn minmax(values: &[f64]) -> Vec<f64> {
    let low = values.iter().copied().fold(f64::INFINITY, f64::min);
    let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if (high - low).abs() < f64::EPSILON {
        vec![0.5; values.len()]
    } else {
        values.iter().map(|v| (v - low) / (high - low)).collect()
    }
}

fn recent_trend(scores: &[f64]) -> &'static str {
    const EPS: f64 = 0.03;
    if scores.len() < 3 {
        return "样本不足";
    }
    let n = scores.len();
    let (d1, d2) = (scores[n - 1] - scores[n - 2], scores[n - 2] - scores[n - 3]);
    if d1 > EPS && d2 > EPS {
        "连续两日走强"
    } else if d1 < -EPS && d2 < -EPS {
        "连续两日转弱"
    } else if d1 > EPS {
        "今日走强"
    } else if d1 < -EPS {
        "今日转弱"
    } else {
        "基本走平"
    }
}

/// 分数 = 涨停家数、最高连板、(1 − 炸板率)各自在窗口内归一化后取平均 —— 是相对读数,没有绝对含义。
pub(crate) fn cycle_position(mut series: Vec<Value>, requested: usize, calendar_days: usize) -> Value {
    if calendar_days < 3 {
        return unavailable("可用交易日不足 3 天,无法定位周期");
    }
    if series.len() < 3 {
        return unavailable(format!("涨停池可用天数不足({}/3)", series.len()));
    }
    let pick = |key: &str| series.iter().map(|s| s[key].as_f64().unwrap_or(0.0)).collect::<Vec<_>>();
    let (zt, hc, br) = (minmax(&pick("limit_up")), minmax(&pick("highest_consec")), minmax(&pick("broken_rate")));
    for (index, day) in series.iter_mut().enumerate() {
        day["score"] = json!(r3((zt[index] + hc[index] + (1.0 - br[index])) / 3.0));
        day["broken_rate"] = json!(r3(day["broken_rate"].as_f64().unwrap_or(0.0)));
    }
    let scores: Vec<f64> = series.iter().map(|s| s["score"].as_f64().unwrap_or(0.0)).collect();
    let (trough_index, trough) = scores
        .iter()
        .enumerate()
        .fold((0, f64::INFINITY), |best, (i, v)| if *v < best.1 { (i, *v) } else { best });
    let top = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let current = *scores.last().unwrap_or(&0.0);
    json!({
        "available": true,
        "window": series.len(),
        "requested_window": requested,
        "trough_date": series[trough_index]["date"],
        "trough_score": trough,
        "current_score": current,
        "current_date": series.last().map(|s| s["date"].clone()).unwrap_or(Value::Null),
        "day_n": series.len() - trough_index,
        "rising": current > trough,
        "trend": recent_trend(&scores),
        "pctile": if top > trough { json!(r3((current - trough) / (top - trough))) } else { Value::Null },
        "series": series,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zt(code: &str, name: &str, boards: i64, first: &str, last: &str, broken: i64, sector: &str) -> Row {
        Row {
            code: code.into(), name: name.into(), board: board_of(code, name), ret: Some(10.0), close: Some(10.0),
            sector: sector.into(), first_seal: first.into(), last_seal: last.into(), broken_times: broken,
            boards, ..Row::default()
        }
    }

    fn yzt(code: &str, name: &str, prev_boards: i64, ret: Option<f64>, sector: &str) -> Row {
        Row { code: code.into(), name: name.into(), board: board_of(code, name), ret, prev_boards, sector: sector.into(), ..Row::default() }
    }

    fn fixture() -> Pools {
        Pools {
            zt: vec![
                zt("600001", "高标A", 4, "092500", "092500", 0, "机器人"),
                zt("600002", "二板B", 2, "093200", "093200", 0, "机器人"),
                zt("300003", "首板C", 1, "101500", "145000", 3, "算力"),
                zt("000004", "首板D", 1, "140500", "140500", 1, "机器人"),
            ],
            zb: Ok(vec![Row { code: "600005".into(), name: "断板E".into(), board: "10cm", ret: Some(3.0), broken_times: 2, sector: "算力".into(), ..Row::default() }]),
            dt: Ok(vec![Row { code: "600006".into(), name: "跌停F".into(), board: "10cm", ret: Some(-10.0), consec_dt: 1, sector: "机器人".into(), ..Row::default() }]),
            yzt: Ok((
                vec![
                    yzt("600001", "高标A", 3, Some(10.0), "机器人"),
                    yzt("600002", "二板B", 1, Some(10.0), "机器人"),
                    yzt("600005", "断板E", 2, Some(3.0), "算力"),
                    yzt("600006", "跌停F", 1, Some(-10.0), "机器人"),
                    yzt("600007", "小跌G", 1, Some(-2.0), "汽车"),
                ],
                vec![yzt("600008", "缺价H", 1, None, "汽车")],
            )),
            prev_zt: Ok(vec![
                zt("600001", "高标A", 3, "", "", 0, "机器人"),
                zt("600002", "二板B", 1, "", "", 0, "机器人"),
                zt("600005", "断板E", 2, "", "", 0, "算力"),
                zt("600006", "跌停F", 1, "", "", 0, "机器人"),
                zt("600007", "小跌G", 1, "", "", 0, "汽车"),
                zt("600008", "缺价H", 1, "", "", 0, "汽车"),
            ]),
        }
    }

    #[test]
    fn promotion_tiers_follow_previous_limit_up_membership() {
        let value = build("2026-10-16", Some("2026-10-15"), true, &fixture());
        let tiers = &value["promotion"]["tiers"];
        assert_eq!(tiers["1进2"], json!({"base": 4, "promoted": 1, "rate": 0.25}));
        assert_eq!(tiers["2进3"], json!({"base": 1, "promoted": 0, "rate": 0.0}));
        assert_eq!(tiers["3板以上晋级"], json!({"base": 1, "promoted": 1, "rate": 1.0}));
        assert_eq!(value["promotion"]["overall"]["rate"], json!(0.333));
    }

    #[test]
    fn money_effect_reports_coverage_instead_of_silently_dropping_rows() {
        let value = build("2026-10-16", Some("2026-10-15"), true, &fixture());
        let me = &value["money_effect"];
        assert_eq!(me["sample"], 5);
        assert_eq!(me["expected_sample"], 6);
        assert_eq!(me["partial"], true);
        assert_eq!(me["median"], json!(3.0));
        assert_eq!(me["limit_up_again_rate"], json!(0.4));
        assert_eq!(value["consec_premium"]["sample"], 2);
        assert_eq!(value["consec_premium"]["avg"], json!(6.5));
    }

    #[test]
    fn ladder_marks_gaps_fell_outcomes_and_promotion_per_level() {
        let value = build("2026-10-16", Some("2026-10-15"), true, &fixture());
        assert_eq!(value["ladder_gap"]["gaps"], json!([3]));
        let levels = value["ladder"]["levels"].as_array().unwrap();
        let three = levels.iter().find(|l| l["boards"] == 3).unwrap();
        assert_eq!(three["stocks"].as_array().unwrap().len(), 0);
        assert_eq!(three["fell"][0]["outcome"], "炸板");
        let two = levels.iter().find(|l| l["boards"] == 2).unwrap();
        assert_eq!((two["base"].as_u64(), two["promoted"].as_u64()), (Some(4), Some(1)));
        let outcomes: Vec<&str> = two["fell"].as_array().unwrap().iter().map(|f| f["outcome"].as_str().unwrap()).collect();
        assert_eq!(outcomes, vec!["没涨停", "跌停", "没取到"]);
    }

    #[test]
    fn seal_quality_themes_and_board_stats_use_pool_definitions() {
        let value = build("2026-10-16", Some("2026-10-15"), true, &fixture());
        let seal = &value["seal_quality"];
        assert_eq!((seal["never_broken"].as_u64(), seal["opening_seconds"].as_u64(), seal["late_seal"].as_u64()), (Some(2), Some(2), Some(1)));
        assert_eq!(seal["broken_rate"], json!(0.2));
        let themes = value["themes"]["themes"].as_array().unwrap();
        assert_eq!(themes[0]["sector"], "机器人");
        assert_eq!(themes[0]["limit_up"], 3);
        assert_eq!(themes[0]["prev_limit_up"], 3);
        assert_eq!(themes[0]["continued"], 2);
        assert_eq!(value["themes"]["faded"][0]["sector"], "汽车");
        let boards = value["by_board"]["boards"].as_array().unwrap();
        let ten = boards.iter().find(|b| b["board"] == "10cm").unwrap();
        assert_eq!((ten["broken"].as_u64(), ten["limit_down"].as_u64()), (Some(1), Some(1)));
        assert_eq!(value["first_board"]["first_count"], 2);
        assert_eq!(value["first_board"]["stocks"][0]["first_seal"], "10:15:00");
    }

    #[test]
    fn feedback_matrix_sorts_details_worst_first() {
        let value = build("2026-10-16", Some("2026-10-15"), true, &fixture());
        let feedback = &value["feedback"];
        assert_eq!(feedback["details"][0]["result"], "跌停");
        let first = feedback["rows"].as_array().unwrap().iter().find(|r| r["tier"] == "首板").unwrap();
        assert_eq!((first["晋级涨停"].as_u64(), first["跌停"].as_u64(), first["小跌"].as_u64()), (Some(1), Some(1), Some(1)));
        assert_eq!(value["loss_effect"]["limit_down_count"], 1);
        assert_eq!(value["loss_effect"]["market_limit_down"], 1);
    }

    #[test]
    fn missing_secondary_pools_degrade_without_inventing_zero() {
        let mut pools = fixture();
        pools.zb = Err("炸板池:数据源请求超时".into());
        pools.prev_zt = Err("前一交易日涨停池:数据源请求超时".into());
        let value = build("2026-10-16", Some("2026-10-15"), true, &pools);
        assert!(value["summary"]["broken"].is_null());
        assert!(value["seal_quality"]["broken_rate"].is_null());
        assert_eq!(value["promotion"]["available"], false);
        assert_eq!(value["ladder"]["prev_available"], false);
        // 前一日涨停池没取到时,昨日板数由昨日涨停池补;两者都缺才留空。
        assert_eq!(value["limit_up_rows"][0]["prev_boards"], 3);
        pools.yzt = Err("昨日涨停表现:数据源请求超时".into());
        let value = build("2026-10-16", Some("2026-10-15"), true, &pools);
        assert!(value["limit_up_rows"][0]["prev_boards"].is_null());
        assert_eq!(value["money_effect"]["available"], false);
        assert_eq!(value["broken_rows"]["available"], false);
    }

    #[test]
    fn pool_parser_rejects_missing_required_columns_and_unpublished_dates() {
        let ok = json!({"rc": 0, "data": {"qdate": 20261016, "pool": [
            {"c": "1", "n": "ＳＴ测试", "zdp": 4.99, "p": 5200, "lbc": 2, "zbc": 0, "fbt": 92500, "lbt": 93000, "hybk": "化工", "zttj": {"days": 3, "ct": 2}}
        ]}});
        let rows = parse_pool(&ok, PoolKind::LimitUp, "20261016").unwrap();
        assert_eq!(rows[0].code, "000001");
        assert_eq!(rows[0].board, "主板ST");
        assert_eq!(rows[0].first_seal, "092500");
        assert_eq!(board_label(rows[0].boards, rows[0].stat), "3天2板");
        let missing = json!({"rc": 0, "data": {"pool": [{"c": "600001", "n": "缺列", "lbc": 1}]}});
        assert!(parse_pool(&missing, PoolKind::LimitUp, "20261016").is_err());
        assert!(parse_pool(&ok, PoolKind::LimitUp, "20261019").is_err());
        assert!(parse_pool(&json!({"rc": 0, "data": null}), PoolKind::Broken, "20261016").unwrap().is_empty());
    }

    #[test]
    fn limit_rules_follow_board_and_regime_dates() {
        assert_eq!(limit_pct("600001", "*ST甲", "2026-07-03"), 5.0);
        assert_eq!(limit_pct("600001", "*ST甲", "2026-07-06"), 10.0);
        assert_eq!(limit_pct("300001", "乙", "2020-08-21"), 10.0);
        assert_eq!(limit_pct("688001", "丙", "2026-10-16"), 20.0);
        assert_eq!(limit_pct("920001", "丁", "2026-10-16"), 30.0);
    }

    #[test]
    fn review_day_defaults_to_latest_settled_session_and_rejects_bad_days() {
        let calendar: Vec<String> = ["2026-10-14", "2026-10-15", "2026-10-16"].iter().map(|s| s.to_string()).collect();
        let day = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        let (date, prev) = review_day(&calendar, None, (day, 1100)).unwrap();
        assert_eq!((date.as_str(), prev.as_deref()), ("2026-10-15", Some("2026-10-14")));
        let (date, _) = review_day(&calendar, None, (day, 1510)).unwrap();
        assert_eq!(date, "2026-10-16");
        assert!(review_day(&calendar, Some("2026-10-17"), (day, 1510)).is_err());
        assert!(review_day(&calendar, Some("2026-10-11"), (day, 1510)).is_err());
        assert!(!is_settled("2026-10-16", &calendar, (day, 1100)));
    }

    #[test]
    fn cycle_scores_are_relative_and_trend_reads_the_tail() {
        let series: Vec<Value> = [(40, 3, 0.4), (20, 2, 0.5), (60, 5, 0.2), (80, 6, 0.1)]
            .iter()
            .enumerate()
            .map(|(i, (zt, hc, br))| json!({"date": format!("2026-10-1{i}"), "limit_up": zt, "highest_consec": hc, "broken_rate": br}))
            .collect();
        let value = cycle_position(series, 10, 4);
        assert_eq!(value["trough_date"], "2026-10-11");
        assert_eq!(value["day_n"], 3);
        assert_eq!(value["trend"], "连续两日走强");
        assert_eq!(value["pctile"], json!(1.0));
        assert_eq!(cycle_position(vec![], 10, 2)["available"], false);
    }

    /// 手动探测真实上游:cargo test --locked emotion::tests::live -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_emotion_and_cycle_probe() {
        let client = Client::builder().timeout(Duration::from_secs(20)).user_agent("Mozilla/5.0 VibeAStock-Mobile/test").build().unwrap();
        let value = emotion(&client, "/mobile/shortline/emotion").await.unwrap();
        println!("{} {} {} errors={}", value["status"], value["date"], value["summary"], value["errors"]);
        println!("promotion={} money={}", value["promotion"]["overall"], value["money_effect"]["median"]);
        println!("ladder levels={} themes={}", value["ladder"]["levels"].as_array().map_or(0, Vec::len), value["themes"]["theme_count"]);
        let cycle = cycle(&client, "/mobile/shortline/cycle?days=10").await.unwrap();
        println!("cycle={} {}", cycle["cycle"]["window"], cycle["cycle"]["trend"]);
        if let Ok(dir) = std::env::var("VIBE_EMOTION_DUMP") {
            std::fs::write(format!("{dir}/emotion.json"), value.to_string()).unwrap();
            std::fs::write(format!("{dir}/cycle.json"), cycle.to_string()).unwrap();
        }
        assert_eq!(value["status"], "ok");
    }

    #[test]
    fn trade_dates_parse_index_kline_dates() {
        let root = json!({"data": {"klines": ["2026-10-15,1", "2026-10-16,2", "bad"]}});
        assert_eq!(parse_trade_dates(&root).unwrap(), vec!["2026-10-15", "2026-10-16"]);
        assert!(parse_trade_dates(&json!({"data": {"klines": []}})).is_err());
        let tencent = json!({"data": {"sh000001": {"day": [["2026-10-16", "1"], ["2026-10-15", "1"]]}}});
        assert_eq!(parse_tencent_dates(&tencent).unwrap(), vec!["2026-10-15", "2026-10-16"]);
    }
}
