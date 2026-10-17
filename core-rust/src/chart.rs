//! 个股 K 线图数据:多周期 K 线(日 / 周 / 月 / 60 / 30 / 15 / 5 分钟,前复权或不复权)、
//! 分时与五日分时、带五档的完整快照。
//!
//! K 线先取东财,失败或被限流时改用腾讯;分时与快照先取腾讯(腾讯快照带五档),失败改用东财。
//! 每条 K 线校验日期顺序与 OHLC 关系,异常行剔除并计数,不补值。技术指标在页面上由同一批 K 线计算。
use crate::{
    discovery::{self, DiscoveryError},
    symbol::{normalize_stock_symbol, SymbolKey},
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, FixedOffset, Utc};
use encoding_rs::GBK;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Period {
    Day,
    Week,
    Month,
    M60,
    M30,
    M15,
    M5,
}

impl Period {
    fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "day" => Self::Day,
            "week" => Self::Week,
            "month" => Self::Month,
            "m60" => Self::M60,
            "m30" => Self::M30,
            "m15" => Self::M15,
            "m5" => Self::M5,
            _ => return None,
        })
    }
    fn key(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::M60 => "m60",
            Self::M30 => "m30",
            Self::M15 => "m15",
            Self::M5 => "m5",
        }
    }
    fn klt(self) -> &'static str {
        match self {
            Self::Day => "101",
            Self::Week => "102",
            Self::Month => "103",
            Self::M60 => "60",
            Self::M30 => "30",
            Self::M15 => "15",
            Self::M5 => "5",
        }
    }
    fn is_minute(self) -> bool {
        matches!(self, Self::M60 | Self::M30 | Self::M15 | Self::M5)
    }
}

fn china() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("valid offset")
}

fn symbol_param(path: &str) -> Result<SymbolKey, DiscoveryError> {
    let raw = discovery::parameter(path, "symbol").unwrap_or_default();
    normalize_stock_symbol(&raw).map_err(|e| DiscoveryError::BadRequest(e.to_string()))
}

fn num(v: Option<&str>) -> Option<f64> {
    v?.trim().parse::<f64>().ok().filter(|x| x.is_finite())
}

async fn get_json(client: &Client, url: &str, params: &[(&str, String)]) -> Result<Value> {
    Ok(client
        .get(url)
        .query(params)
        .header("Referer", "https://quote.eastmoney.com/")
        .timeout(Duration::from_secs(10))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

// ---------------------------------------------------------------- K 线

/// 一根 K 线:[时间, 开, 收, 高, 低, 成交量(手), 成交额(元|null), 换手率(%|null)]
pub(crate) type Bar = (String, f64, f64, f64, f64, f64, Option<f64>, Option<f64>);

fn valid(bar: &Bar) -> bool {
    let (_, o, c, h, l, v, _, _) = bar;
    *o > 0.0 && *c > 0.0 && *l > 0.0 && *h >= o.max(*c) && *l <= o.min(*c) && *h >= *l && *v >= 0.0
}

/// 校验并去掉异常行与乱序/重复行;返回保留的行和剔除数。
fn clean(rows: Vec<Bar>) -> (Vec<Bar>, usize) {
    let total = rows.len();
    let mut out: Vec<Bar> = Vec::with_capacity(total);
    for bar in rows {
        if !valid(&bar) || out.last().is_some_and(|last| last.0 >= bar.0) {
            continue;
        }
        out.push(bar);
    }
    let dropped = total - out.len();
    (out, dropped)
}

pub(crate) fn parse_em_kline(root: &Value, key: &SymbolKey) -> Result<(Vec<Bar>, usize, String)> {
    let data = crate::stock_context::provider_data(root, key)?;
    let name = data["name"].as_str().unwrap_or("").to_string();
    let lines = data["klines"].as_array().ok_or_else(|| anyhow!("东财 K 线缺少 klines"))?;
    let rows: Vec<Bar> = lines
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|line| {
            let f: Vec<&str> = line.split(',').collect();
            if f.len() < 7 {
                return None;
            }
            Some((f[0].to_string(), num(f.get(1).copied())?, num(f.get(2).copied())?, num(f.get(3).copied())?, num(f.get(4).copied())?,
                num(f.get(5).copied())?, num(f.get(6).copied()), num(f.get(10).copied())))
        })
        .collect();
    let parsed = rows.len();
    let (rows, dropped) = clean(rows);
    Ok((rows, dropped + (lines.len() - parsed), name))
}

fn tencent_time(raw: &str) -> String {
    // 202610161500 → 2026-10-16 15:00;日期原样
    if raw.len() == 12 && raw.chars().all(|c| c.is_ascii_digit()) {
        format!("{}-{}-{} {}:{}", &raw[0..4], &raw[4..6], &raw[6..8], &raw[8..10], &raw[10..12])
    } else {
        raw.to_string()
    }
}

pub(crate) fn parse_tencent_kline(root: &Value, key: &SymbolKey, field: &str) -> Result<(Vec<Bar>, usize, String)> {
    if root["code"].as_i64().is_some_and(|c| c != 0) {
        return Err(anyhow!("腾讯 K 线返回错误"));
    }
    let item = &root["data"][&key.tencent];
    let lines = item[field].as_array().ok_or_else(|| anyhow!("腾讯 K 线缺少 {field}"))?;
    let name = item["qt"][&key.tencent].get(1).and_then(Value::as_str).unwrap_or("").to_string();
    let rows: Vec<Bar> = lines
        .iter()
        .filter_map(Value::as_array)
        .filter_map(|r| {
            let s = |i: usize| r.get(i).and_then(Value::as_str);
            Some((tencent_time(s(0)?), num(s(1))?, num(s(2))?, num(s(3))?, num(s(4))?, num(s(5))?, None, if field.starts_with('m') { num(s(7)) } else { None }))
        })
        .collect();
    let parsed = rows.len();
    let (rows, dropped) = clean(rows);
    Ok((rows, dropped + (lines.len() - parsed), name))
}

async fn em_kline(client: &Client, key: &SymbolKey, period: Period, qfq: bool, limit: usize) -> Result<(Vec<Bar>, usize, String)> {
    let root = get_json(client, "https://push2his.eastmoney.com/api/qt/stock/kline/get", &[
        ("secid", key.eastmoney.clone()), ("klt", period.klt().into()), ("fqt", if qfq { "1" } else { "0" }.into()),
        ("lmt", limit.to_string()), ("end", "20500101".into()), ("fields1", "f1,f2,f3".into()),
        ("fields2", "f51,f52,f53,f54,f55,f56,f57,f58,f59,f60,f61".into()),
    ]).await?;
    parse_em_kline(&root, key)
}

async fn tencent_kline(client: &Client, key: &SymbolKey, period: Period, qfq: bool, limit: usize) -> Result<(Vec<Bar>, usize, String)> {
    if period.is_minute() {
        let root = get_json(client, "https://ifzq.gtimg.cn/appstock/app/kline/mkline", &[("param", format!("{},{},,{}", key.tencent, period.key(), limit.min(320)))]).await?;
        parse_tencent_kline(&root, key, period.key())
    } else {
        let adjust = if qfq { "qfq" } else { "" };
        let root = get_json(client, "https://web.ifzq.gtimg.cn/appstock/app/fqkline/get", &[("param", format!("{},{},,,{},{adjust}", key.tencent, period.key(), limit))]).await?;
        let field = format!("{adjust}{}", period.key());
        // 腾讯不复权或停牌股有时只给 day 字段
        parse_tencent_kline(&root, key, &field).or_else(|_| parse_tencent_kline(&root, key, period.key()))
    }
}

pub async fn kline(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let key = symbol_param(path)?;
    let period = Period::parse(&discovery::parameter(path, "period").unwrap_or_else(|| "day".into()))
        .ok_or_else(|| DiscoveryError::BadRequest("period 只能是 day / week / month / m60 / m30 / m15 / m5".into()))?;
    let qfq = match discovery::parameter(path, "adjust").as_deref() {
        None | Some("qfq") => true,
        Some("none") => false,
        Some(_) => return Err(DiscoveryError::BadRequest("adjust 只能是 qfq 或 none".into())),
    };
    let limit = discovery::integer(path, "limit", 300, 30, 800)?;
    let (result, source, fallback) = match em_kline(client, &key, period, qfq, limit).await {
        Ok(v) if !v.0.is_empty() => (Ok(v), "eastmoney", None),
        primary => {
            let why = match &primary { Err(e) => crate::emotion::error_message("东财 K 线", e), Ok(_) => "东财 K 线为空".to_string() };
            (tencent_kline(client, &key, period, qfq, limit).await, "tencent", Some(why))
        }
    };
    let (rows, dropped, name) = result.map_err(DiscoveryError::Upstream)?;
    if rows.is_empty() {
        return Ok(json!({"status": "failed", "message": "两个数据源都没有返回 K 线"}));
    }
    let today = Utc::now().with_timezone(&china()).format("%Y-%m-%d").to_string();
    let last = rows.last().map(|r| r.0.clone()).unwrap_or_default();
    Ok(json!({
        "status": if dropped > 0 || fallback.is_some() { "partial" } else { "ok" },
        "symbol": key.digits, "market": key.market.as_str(), "name": name,
        "period": period.key(), "adjust": if qfq { "qfq" } else { "none" },
        "rows": rows.iter().map(|r| json!([r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7])).collect::<Vec<_>>(),
        "columns": ["time", "open", "close", "high", "low", "volume_lots", "amount_yuan", "turnover_pct"],
        "dropped": dropped,
        "source": source,
        "fallback_reason": fallback,
        "includes_today": last.starts_with(&today),
        "note": match (source, key.market.as_str(), period.is_minute()) {
            ("tencent", "BJ", _) => "东财没取到,腾讯对北交所只给很短的历史",
            ("tencent", _, true) => "腾讯分钟 K 线不复权",
            _ => "",
        },
        "fetched_at": Utc::now().to_rfc3339(),
    }))
}

// ---------------------------------------------------------------- 分时 / 五日

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Minute {
    pub time: String,
    pub price: f64,
    pub avg: Option<f64>,
    pub volume: f64,
    pub amount: f64,
}

fn day_view(date: &str, pre_close: Option<f64>, points: &[Minute]) -> Value {
    json!({"date": date, "pre_close": pre_close, "points": points.iter().map(|p| json!([p.time, p.price, p.avg, p.volume, p.amount])).collect::<Vec<_>>()})
}

/// 腾讯分时串 "HHMM 价格 累计量(手) 累计额(元)" → 每分钟量额与均价。15:00 之后(盘后固定价)不画。
pub(crate) fn parse_tencent_minutes(lines: &[Value]) -> Vec<Minute> {
    let mut out = Vec::new();
    let (mut prev_vol, mut prev_amt) = (0.0, 0.0);
    for line in lines.iter().filter_map(Value::as_str) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 4 || f[0].len() != 4 || f[0] > "1500" {
            continue;
        }
        let (Some(price), Some(cum_vol), Some(cum_amt)) = (num(Some(f[1])), num(Some(f[2])), num(Some(f[3]))) else { continue };
        if price <= 0.0 || cum_vol < prev_vol {
            continue;
        }
        out.push(Minute {
            time: format!("{}:{}", &f[0][0..2], &f[0][2..4]),
            price,
            avg: if cum_vol > 0.0 { Some(crate::emotion::r3(cum_amt / (cum_vol * 100.0))) } else { None },
            volume: cum_vol - prev_vol,
            amount: cum_amt - prev_amt,
        });
        prev_vol = cum_vol;
        prev_amt = cum_amt;
    }
    out
}

/// 东财 trends2:"YYYY-MM-DD HH:MM,开,收,高,低,量,额,均价",按日期分组。
pub(crate) fn parse_em_trends(root: &Value, key: &SymbolKey) -> Result<Vec<(String, Option<f64>, Vec<Minute>)>> {
    let data = crate::stock_context::provider_data(root, key)?;
    let lines = data["trends"].as_array().ok_or_else(|| anyhow!("东财分时缺少 trends"))?;
    let mut days: Vec<(String, Option<f64>, Vec<Minute>)> = Vec::new();
    for line in lines.iter().filter_map(Value::as_str) {
        let f: Vec<&str> = line.split(',').collect();
        if f.len() < 8 {
            continue;
        }
        let (date, time) = match f[0].split_once(' ') { Some(x) => x, None => continue };
        let (Some(price), Some(vol), Some(amt)) = (num(Some(f[2])), num(Some(f[5])), num(Some(f[6]))) else { continue };
        if price <= 0.0 || time > "15:00" {
            continue;
        }
        if days.last().map(|d| d.0.as_str()) != Some(date) {
            let pre = days.last().and_then(|d| d.2.last()).map(|p| p.price);
            days.push((date.to_string(), pre, Vec::new()));
        }
        days.last_mut().unwrap().2.push(Minute { time: time.to_string(), price, avg: num(Some(f[7])).filter(|v| *v > 0.0), volume: vol, amount: amt });
    }
    if let Some(first) = days.first_mut() {
        first.1 = data["preClose"].as_f64().filter(|v| *v > 0.0).or(first.1);
    }
    Ok(days)
}

pub async fn intraday(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let key = symbol_param(path)?;
    let days = discovery::integer(path, "days", 1, 1, 5)?;
    if days != 1 && days != 5 {
        return Err(DiscoveryError::BadRequest("days 只能是 1 或 5".into()));
    }
    let tencent: Result<Vec<Value>> = async {
        if days == 1 {
            let root = get_json(client, "https://web.ifzq.gtimg.cn/appstock/app/minute/query", &[("code", key.tencent.clone())]).await?;
            let item = &root["data"][&key.tencent];
            let raw_date = item["data"]["date"].as_str().unwrap_or("");
            let date = if raw_date.len() == 8 { format!("{}-{}-{}", &raw_date[0..4], &raw_date[4..6], &raw_date[6..8]) } else { raw_date.to_string() };
            let pre = item["qt"][&key.tencent].get(4).and_then(Value::as_str).and_then(|s| num(Some(s)));
            let points = parse_tencent_minutes(item["data"]["data"].as_array().map(Vec::as_slice).unwrap_or(&[]));
            if points.is_empty() { return Err(anyhow!("腾讯分时为空")); }
            Ok(vec![day_view(&date, pre, &points)])
        } else {
            let root = get_json(client, "https://web.ifzq.gtimg.cn/appstock/app/day/query", &[("code", key.tencent.clone())]).await?;
            let list = root["data"][&key.tencent]["data"].as_array().cloned().ok_or_else(|| anyhow!("腾讯五日分时缺少数据"))?;
            let mut out: Vec<Value> = list.iter().rev().filter_map(|d| {
                let raw = d["date"].as_str()?;
                let date = if raw.len() == 8 { format!("{}-{}-{}", &raw[0..4], &raw[4..6], &raw[6..8]) } else { raw.to_string() };
                let pre = d["prec"].as_str().and_then(|s| num(Some(s))).or_else(|| d["prec"].as_f64());
                let points = parse_tencent_minutes(d["data"].as_array().map(Vec::as_slice).unwrap_or(&[]));
                (!points.is_empty()).then(|| day_view(&date, pre, &points))
            }).collect();
            if out.is_empty() { return Err(anyhow!("腾讯五日分时为空")); }
            out.truncate(5);
            Ok(out)
        }
    }.await;
    let (list, source, fallback) = match tencent {
        Ok(list) => (list, "tencent", None),
        Err(e) => {
            let why = crate::emotion::error_message("腾讯分时", &e);
            let root = get_json(client, "https://push2his.eastmoney.com/api/qt/stock/trends2/get", &[
                ("secid", key.eastmoney.clone()), ("fields1", "f1,f2,f3,f4,f5,f6,f7,f8,f9,f10,f11,f12,f13".into()),
                ("fields2", "f51,f52,f53,f54,f55,f56,f57,f58".into()), ("ndays", days.to_string()), ("iscr", "0".into()),
            ]).await.map_err(DiscoveryError::Upstream)?;
            let parsed = parse_em_trends(&root, &key).map_err(DiscoveryError::Upstream)?;
            (parsed.iter().map(|(d, p, pts)| day_view(d, *p, pts)).collect(), "eastmoney", Some(why))
        }
    };
    Ok(json!({
        "status": if fallback.is_some() { "partial" } else { "ok" },
        "symbol": key.digits, "market": key.market.as_str(), "days": list,
        "columns": ["time", "price", "avg_price", "volume_lots", "amount_yuan"],
        "source": source, "fallback_reason": fallback, "fetched_at": Utc::now().to_rfc3339(),
    }))
}

// ---------------------------------------------------------------- 快照

pub(crate) fn parse_tencent_snapshot(text: &str, key: &SymbolKey, today: &str) -> Result<Value> {
    let payload = text.split('"').nth(1).ok_or_else(|| anyhow!("腾讯返回无载荷"))?;
    let f: Vec<&str> = payload.split('~').collect();
    if f.len() < 53 || f.get(2).copied() != Some(key.digits.as_str()) || num(f.get(3).copied()).is_none() {
        return Err(anyhow!("腾讯快照字段不足或代码不匹配"));
    }
    let n = |i: usize| num(f.get(i).copied());
    let level = |i: usize| json!({"price": n(i), "volume": n(i + 1)});
    let time = f.get(30).copied().unwrap_or("");
    let date = if time.len() >= 8 { format!("{}-{}-{}", &time[0..4], &time[4..6], &time[6..8]) } else { String::new() };
    Ok(json!({
        "name": f[1], "price": n(3), "prev_close": n(4), "open": n(5), "volume_lots": n(6),
        "outer_lots": n(7), "inner_lots": n(8),
        "bids": [level(9), level(11), level(13), level(15), level(17)],
        "asks": [level(19), level(21), level(23), level(25), level(27)],
        "quote_time": if time.len() >= 12 { format!("{} {}:{}:{}", date, &time[8..10], &time[10..12], time.get(12..14).unwrap_or("00")) } else { time.to_string() },
        "change": n(31), "change_pct": n(32), "high": n(33), "low": n(34),
        "amount_yuan": n(37).map(|v| (v * 10_000.0).round()), "turnover_pct": n(38), "pe_ttm": n(39), "amplitude_pct": n(43),
        "float_mcap_yuan": n(44).map(|v| (v * 100_000_000.0).round()), "total_mcap_yuan": n(45).map(|v| (v * 100_000_000.0).round()),
        "pb": n(46), "limit_up": n(47), "limit_down": n(48), "volume_ratio": n(49), "avg_price": n(51),
        "pe_dynamic": n(52), "pe_static": n(53), "commission_ratio": n(74),
        "is_today": date == today,
    }))
}

pub(crate) fn parse_em_snapshot(root: &Value, key: &SymbolKey, today: &str) -> Result<Value> {
    let d = &root["data"];
    if d["f57"].as_str() != Some(key.digits.as_str()) {
        return Err(anyhow!("东财快照代码不匹配"));
    }
    let n = |k: &str| d[k].as_f64().filter(|v| v.is_finite());
    let ts = d["f86"].as_i64().and_then(|t| DateTime::from_timestamp(t, 0)).map(|t| t.with_timezone(&china()));
    Ok(json!({
        "name": d["f58"], "price": n("f43"), "prev_close": n("f60"), "open": n("f46"), "volume_lots": n("f47"),
        "outer_lots": n("f49"), "inner_lots": n("f161"), "bids": [], "asks": [],
        "quote_time": ts.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()),
        "change": n("f169"), "change_pct": n("f170"), "high": n("f44"), "low": n("f45"), "amount_yuan": n("f48"),
        "turnover_pct": n("f168"), "pe_ttm": Value::Null, "amplitude_pct": n("f171"), "float_mcap_yuan": n("f117"),
        "total_mcap_yuan": n("f116"), "pb": n("f167"), "limit_up": n("f51"), "limit_down": n("f52"),
        "volume_ratio": n("f50"), "avg_price": n("f71"), "pe_dynamic": n("f162"), "pe_static": Value::Null,
        "commission_ratio": n("f191"),
        "is_today": ts.is_some_and(|t| t.format("%Y-%m-%d").to_string() == today),
    }))
}

pub async fn snapshot(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let key = symbol_param(path)?;
    let today = Utc::now().with_timezone(&china()).format("%Y-%m-%d").to_string();
    let tencent: Result<Value> = async {
        let bytes = client.get(format!("https://qt.gtimg.cn/q={}", key.tencent)).timeout(Duration::from_secs(8)).send().await?.error_for_status()?.bytes().await?;
        let (decoded, _, _) = GBK.decode(&bytes);
        parse_tencent_snapshot(&decoded, &key, &today)
    }.await;
    let (mut body, source, fallback) = match tencent {
        Ok(v) => (v, "tencent", None),
        Err(e) => {
            let why = crate::emotion::error_message("腾讯快照", &e);
            let root = get_json(client, "https://push2.eastmoney.com/api/qt/stock/get", &[
                ("secid", key.eastmoney.clone()), ("fltt", "2".into()), ("invt", "2".into()),
                ("fields", "f43,f44,f45,f46,f47,f48,f49,f50,f51,f52,f57,f58,f60,f71,f86,f116,f117,f161,f162,f167,f168,f169,f170,f171,f191".into()),
            ]).await.map_err(DiscoveryError::Upstream)?;
            (parse_em_snapshot(&root, &key, &today).map_err(DiscoveryError::Upstream)?, "eastmoney", Some(why))
        }
    };
    body["status"] = json!(if fallback.is_some() { "partial" } else { "ok" });
    body["symbol"] = json!(key.digits);
    body["market"] = json!(key.market.as_str());
    body["source"] = json!(source);
    body["fallback_reason"] = json!(fallback);
    body["fetched_at"] = json!(Utc::now().to_rfc3339());
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> SymbolKey {
        normalize_stock_symbol("600519").unwrap()
    }

    #[test]
    fn eastmoney_kline_keeps_amount_and_turnover_and_drops_bad_rows() {
        let root = json!({"rc": 0, "data": {"code": "600519", "market": 1, "name": "贵州茅台", "klines": [
            "2026-10-15,1290.00,1312.97,1315.00,1288.00,30000,3930000000.00,2.1,1.78,22.97,0.24",
            "2026-10-16,1311.57,1329.00,1329.88,1301.68,38502,5075528306.00,2.15,1.22,16.03,0.31",
            "2026-10-16,1311.57,1329.00,1329.88,1301.68,38502,5075528306.00,2.15,1.22,16.03,0.31",
            "2026-10-17,10,12,11,9,1,1,0,0,0,0",
        ]}});
        let (rows, dropped, name) = parse_em_kline(&root, &key()).unwrap();
        assert_eq!(name, "贵州茅台");
        assert_eq!(rows.len(), 2);
        assert_eq!(dropped, 2, "duplicate date and high<close are dropped");
        assert_eq!(rows[1].6, Some(5075528306.0));
        assert_eq!(rows[1].7, Some(0.31));
    }

    #[test]
    fn tencent_kline_reads_qfq_week_and_minute_rows() {
        let root = json!({"code": 0, "data": {"sh600519": {
            "qfqweek": [["2026-10-09", "1252.900", "1263.000", "1282.000", "1242.000", "60279.000"], ["2026-10-16", "1262.950", "1329.000", "1329.880", "1261.000", "178081.000"]],
            "m5": [["202610161455", "1323.00", "1329.78", "1329.78", "1322.98", "5038.00", {}, "4.03"], ["202610161500", "1327.53", "1329.00", "1329.69", "1325.76", "2801.00", {}, "2.24"]],
            "qt": {"sh600519": ["1", "贵州茅台", "600519"]}}}});
        let (week, dropped, name) = parse_tencent_kline(&root, &key(), "qfqweek").unwrap();
        assert_eq!((week.len(), dropped, name.as_str()), (2, 0, "贵州茅台"));
        assert_eq!(week[1].2, 1329.0);
        assert_eq!(week[1].6, None);
        let (m5, _, _) = parse_tencent_kline(&root, &key(), "m5").unwrap();
        assert_eq!(m5[1].0, "2026-10-16 15:00");
        assert!(parse_tencent_kline(&root, &key(), "qfqday").is_err());
    }

    #[test]
    fn tencent_minutes_turn_cumulative_into_per_minute_and_average_price() {
        let lines = vec![json!("0930 1311.57 335 43937595.00"), json!("0931 1303.18 1552 203058136.71"), json!("1500 1329.00 38502 5075528306.00"), json!("1530 1329.00 38522 5078219262.62")];
        let pts = parse_tencent_minutes(&lines);
        assert_eq!(pts.len(), 3, "after-hours fixed-price row is not drawn");
        assert_eq!(pts[0].avg, Some(1311.57));
        assert_eq!(pts[1].volume, 1217.0);
        assert_eq!(pts[1].time, "09:31");
        assert!((pts[1].amount - 159120541.71).abs() < 0.01);
    }

    #[test]
    fn eastmoney_trends_split_days_and_chain_previous_close() {
        let root = json!({"rc": 0, "data": {"code": "600519", "market": 1, "preClose": 1286.0, "trends": [
            "2026-10-15 09:30,1290,1290.00,1290,1290,100,12900000,1290.000",
            "2026-10-15 15:00,1312,1312.97,1313,1312,200,26200000,1305.000",
            "2026-10-16 09:30,1311.57,1311.57,1311.57,1311.57,335,43937595.00,1311.570",
        ]}});
        let days = parse_em_trends(&root, &key()).unwrap();
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].1, Some(1286.0));
        assert_eq!(days[1].1, Some(1312.97));
        assert_eq!(days[1].2[0].avg, Some(1311.57));
    }

    #[test]
    fn tencent_snapshot_maps_order_book_limits_and_units() {
        let mut f: Vec<String> = (0..88).map(|_| String::new()).collect();
        let set = |f: &mut Vec<String>, i: usize, v: &str| f[i] = v.to_string();
        for (i, v) in [(0, "1"), (1, "贵州茅台"), (2, "600519"), (3, "1329.00"), (4, "1312.97"), (5, "1311.57"), (6, "38502"), (7, "21136"), (8, "17365"),
            (9, "1328.58"), (10, "1"), (19, "1329.00"), (20, "54"), (30, "20261016161447"), (31, "16.03"), (32, "1.22"), (33, "1329.88"), (34, "1301.68"),
            (37, "507553"), (38, "0.31"), (39, "20.40"), (43, "2.15"), (44, "16613.58"), (45, "16613.58"), (46, "6.61"), (47, "1444.27"), (48, "1181.67"),
            (49, "1.10"), (51, "1318.26"), (52, "18.66"), (53, "20.18"), (74, "-83.10")] {
            set(&mut f, i, v);
        }
        let text = format!("v_sh600519=\"{}\";", f.join("~"));
        let s = parse_tencent_snapshot(&text, &key(), "2026-10-16").unwrap();
        assert_eq!(s["bids"][0], json!({"price": 1328.58, "volume": 1.0}));
        assert_eq!(s["asks"][0], json!({"price": 1329.0, "volume": 54.0}));
        assert_eq!(s["limit_up"], json!(1444.27));
        assert_eq!(s["amount_yuan"], json!(5075530000.0));
        assert_eq!(s["total_mcap_yuan"], json!(1661358000000.0));
        assert_eq!(s["quote_time"], "2026-10-16 16:14:47");
        assert_eq!(s["is_today"], true);
        assert!(parse_tencent_snapshot(&text.replace("600519", "600520"), &key(), "2026-10-16").is_err());
    }

    #[test]
    fn eastmoney_snapshot_fallback_has_no_order_book() {
        let root = json!({"data": {"f43": 1329.0, "f44": 1329.88, "f45": 1301.68, "f46": 1311.57, "f47": 38502, "f48": 5075528306.0, "f50": 1.1,
            "f51": 1444.27, "f52": 1181.67, "f57": "600519", "f58": "贵州茅台", "f60": 1312.97, "f71": 1318.26, "f86": 1792138307, "f170": 1.22}});
        let s = parse_em_snapshot(&root, &key(), "2026-10-16").unwrap();
        assert_eq!(s["bids"], json!([]));
        assert_eq!(s["limit_down"], json!(1181.67));
        assert!(s["quote_time"].as_str().unwrap().starts_with("2026-10-16"));
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// 手动探测真实上游:cargo test --locked chart::live -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_chart_probe() {
        let client = Client::builder().timeout(Duration::from_secs(20)).user_agent("Mozilla/5.0 VibeAStock-Mobile/test").build().unwrap();
        let mut dump = serde_json::Map::new();
        for path in ["/mobile/stock/kline?symbol=600519&period=day&limit=300", "/mobile/stock/kline?symbol=600519&period=week", "/mobile/stock/kline?symbol=300308&period=m5&adjust=none",
            "/mobile/stock/kline?symbol=920575&period=day", "/mobile/stock/intraday?symbol=600519", "/mobile/stock/intraday?symbol=600519&days=5", "/mobile/stock/snapshot?symbol=600519"] {
            let start = std::time::Instant::now();
            let v = match path.split('?').next().unwrap() {
                "/mobile/stock/kline" => kline(&client, path).await,
                "/mobile/stock/intraday" => intraday(&client, path).await,
                _ => snapshot(&client, path).await,
            };
            match v {
                Ok(v) => {
                    println!("{path} {:?} status={} source={} rows={} days={} dropped={} fallback={}", start.elapsed(), v["status"], v["source"],
                        v["rows"].as_array().map_or(0, Vec::len), v["days"].as_array().map_or(0, Vec::len), v["dropped"], v["fallback_reason"]);
                    dump.insert(path.to_string(), v);
                }
                Err(e) => println!("{path} ERROR {e:?}"),
            }
        }
        for (p, q) in [(Period::Day, true), (Period::Day, false), (Period::Week, true), (Period::Month, true), (Period::M60, false), (Period::M5, false)] {
            for code in ["600519", "920575"] {
                let k = normalize_stock_symbol(code).unwrap();
                match tencent_kline(&client, &k, p, q, 120).await {
                    Ok((rows, dropped, _)) => println!("tencent {code} {:?} qfq={q} rows={} dropped={dropped} last={:?}", p, rows.len(), rows.last().map(|r| (r.0.clone(), r.2))),
                    Err(e) => println!("tencent {code} {:?} qfq={q} ERROR {e}", p),
                }
            }
        }
        if let Ok(out) = std::env::var("VIBE_CHART_DUMP") {
            std::fs::write(format!("{out}/chart.json"), Value::Object(dump).to_string()).unwrap();
        }
    }
}
