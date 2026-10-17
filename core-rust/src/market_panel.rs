//! 每日复盘 · 市场面板(移植自桌面版「每日复盘」的 `/page/review` 各块:orchestrator/src/finance/page_queries.ts 的 review)。
//!
//! 一屏看全:主要指数的中长期位置、两市成交额与均值的比、两融余额、国债收益率、全球指数、行业资金、成交额居前、龙虎榜。
//! 每一块各自取数、各自标缺口:一块没取到不拖垮其余,也不显示成 0 —— 页面照写原因。
//!
//! 🔴 口径(都是按固定公式算的事实,不是趋势判断,不给买卖建议):
//!   - 指数收盘价取腾讯日线(北证 50 腾讯没有历史,走东财);只用复盘日及以前的日线,盘中看今天时最后一根还没收完 → 标 partial,
//!     不拿半天的量和整天的均值比。
//!   - 两市成交额 = 上证指数 + 深证综指(都是整个交易所的成交额),取东财日线;20 / 60 日均值含复盘日自己。
//!   - 两融 T 日数据 T+1 才发布,最新一条通常是上一个交易日。
//!   - 行业资金、成交额居前、涨跌分布只给「当天此刻」:看过去的复盘日时它们不是同一天,页面按 `matches_review_day` 标出来。
//!   - 行业资金用新浪口径(东财列表接口成片取不到):流入 − 流出,和东财「主力净额」不是一个口径,别互相比。
//! 没接的:沪深 300 的市盈率 / 股息率 —— 中证官网只给 xls 文件,手机端还没有 xls 解析,明说没有,不拿别的口径顶替。

use crate::{
    discovery::{self, DiscoveryError},
    emotion, market, watch,
};
use anyhow::{anyhow, Result};
use chrono::{Datelike, Utc};
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

const DATACENTER: &str = "https://datacenter-web.eastmoney.com/api/data/v1/get";
const CHINABOND: &str = "https://yield.chinabond.com.cn/cbweb-pbc-web/pbc/historyQuery";
/// (腾讯代码, 东财 secid, 名称)。北证 50 腾讯没有历史,只走东财
const INDICES: [(&str, &str, &str); 8] = [
    ("sh000001", "1.000001", "上证指数"),
    ("sz399001", "0.399001", "深证成指"),
    ("sz399006", "0.399006", "创业板指"),
    ("sh000300", "1.000300", "沪深300"),
    ("sh000905", "1.000905", "中证500"),
    ("sh000852", "1.000852", "中证1000"),
    ("sh000688", "1.000688", "科创50"),
    ("bj899050", "0.899050", "北证50"),
];
const AMOUNT_SH: &str = "1.000001";
const AMOUNT_SZ: &str = "0.399106";
const GLOBAL: [(&str, &str, &str); 5] = [
    ("usDJI", "道琼斯", "美股"),
    ("usIXIC", "纳斯达克综合", "美股"),
    ("usINX", "标普500", "美股"),
    ("hkHSI", "恒生指数", "港股"),
    ("hkHSTECH", "恒生科技", "港股"),
];

fn r2(v: f64) -> f64 {
    emotion::r2(v)
}

fn r4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

fn mean(vals: &[f64]) -> f64 {
    vals.iter().sum::<f64>() / vals.len() as f64
}

fn fail(reason: impl Into<String>) -> Value {
    json!({"available": false, "reason": reason.into()})
}

// ---------------------------------------------------------------- 指数中长线(纯计算)

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Bar {
    pub date: String,
    pub close: f64,
}

/// n 个交易日前的收盘比:close / closes[len-1-n] − 1(%)。样本不够 = None
fn change_over(closes: &[f64], n: usize) -> Option<f64> {
    let last = *closes.last()?;
    let base = *closes.get(closes.len().checked_sub(n + 1)?)?;
    (base > 0.0).then(|| r2((last / base - 1.0) * 100.0))
}

fn ma(closes: &[f64], n: usize) -> Option<f64> {
    (closes.len() >= n).then(|| r2(mean(&closes[closes.len() - n..])))
}

/// 一条指数的中长线读数。`bars` 旧 → 新,已截到复盘日
pub(crate) fn trend_row(code: &str, name: &str, source: &str, bars: &[Bar]) -> Option<Value> {
    let last = bars.last()?;
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let year = &last.date[..4];
    // 年初至今:和上一年最后一个交易日的收盘比;日线里没有上一年的数据就不给
    let prev_year_close = bars.iter().rev().find(|b| &b.date[..4] < year).map(|b| b.close);
    let ytd = prev_year_close.filter(|c| *c > 0.0).map(|c| r2((last.close / c - 1.0) * 100.0));
    let window: &[f64] = &closes[closes.len().saturating_sub(250)..];
    let (high, low) = (window.iter().cloned().fold(f64::MIN, f64::max), window.iter().cloned().fold(f64::MAX, f64::min));
    let full = closes.len() >= 250;
    let (m20, m60, m250) = (ma(&closes, 20), ma(&closes, 60), ma(&closes, 250));
    let above = |m: Option<f64>| m.map(|m| last.close >= m);
    Some(json!({
        "code": code, "name": name, "source": source, "date": last.date, "close": last.close,
        "chg1": change_over(&closes, 1), "chg5": change_over(&closes, 5), "chg20": change_over(&closes, 20), "chg60": change_over(&closes, 60), "chg250": change_over(&closes, 250), "ytd": ytd,
        "ma20": m20, "ma60": m60, "ma250": m250, "above_ma20": above(m20), "above_ma60": above(m60), "above_ma250": above(m250),
        "high250": r2(high), "low250": r2(low), "from_high_pct": r2((last.close / high - 1.0) * 100.0), "from_low_pct": r2((last.close / low - 1.0) * 100.0),
        "window_full": full, "rows": closes.len(),
    }))
}

// ---------------------------------------------------------------- 成交额(纯计算)

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Amount {
    pub date: String,
    pub total: f64,
}

/// 两市成交额与 20 / 60 日均值的比。`partial` = 最后一根是盘中累计:不和整天的均值比,均值只用它之前的整天
pub(crate) fn amount_stats(rows: &[Amount], partial: bool) -> Value {
    let Some(last) = rows.last() else { return fail("两市成交额没取到") };
    let full: &[Amount] = if partial { &rows[..rows.len() - 1] } else { rows };
    let avg = |n: usize| (full.len() >= n).then(|| mean(&full[full.len() - n..].iter().map(|a| a.total).collect::<Vec<_>>()));
    let (ma20, ma60) = (avg(20), avg(60));
    let vs = |m: Option<f64>| if partial { None } else { m.filter(|m| *m > 0.0).map(|m| r2((last.total / m - 1.0) * 100.0)) };
    json!({
        "available": true, "date": last.date, "last": last.total, "partial": partial,
        "prev": if partial { full.last().map(|a| a.total) } else if rows.len() >= 2 { Some(rows[rows.len() - 2].total) } else { None },
        "ma20": ma20, "ma60": ma60, "vs_ma20": vs(ma20), "vs_ma60": vs(ma60),
        "rows": rows[rows.len().saturating_sub(60)..].iter().map(|a| json!({"date": a.date, "total": a.total})).collect::<Vec<_>>(),
    })
}

/// 东财指数日线 "date,close,amount" → 行
pub(crate) fn parse_em_klines(root: &Value) -> Vec<(String, f64, f64)> {
    root["data"]["klines"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|line| {
            let p: Vec<&str> = line.split(',').collect();
            Some((p.first()?.to_string(), p.get(1)?.parse().ok()?, p.get(2)?.parse().ok()?))
        })
        .collect()
}

/// 腾讯指数日线 → Bar(close 在第 3 列)
pub(crate) fn parse_tx_klines(root: &Value, symbol: &str) -> Vec<Bar> {
    root["data"][symbol]["day"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| Some(Bar { date: row.get(0)?.as_str()?.to_string(), close: row.get(2)?.as_str()?.parse().ok()? }))
        .collect()
}

// ---------------------------------------------------------------- 取数

async fn tencent_bars(client: &Client, symbol: &str, count: usize) -> Result<Vec<Bar>> {
    let root: Value = client
        .get("https://web.ifzq.gtimg.cn/appstock/app/fqkline/get")
        .query(&[("param", format!("{symbol},day,,,{count},"))])
        .timeout(Duration::from_secs(12))
        .send().await?.error_for_status()?.json().await?;
    let bars = parse_tx_klines(&root, symbol);
    if bars.is_empty() {
        return Err(anyhow!("腾讯日线没有数据"));
    }
    Ok(bars)
}

async fn em_index_daily(client: &Client, secid: &str, count: usize) -> Result<Vec<(String, f64, f64)>> {
    let root: Value = client
        .get("https://push2his.eastmoney.com/api/qt/stock/kline/get")
        .query(&[("secid", secid), ("fields1", "f1,f2,f3"), ("fields2", "f51,f53,f57"), ("klt", "101"), ("fqt", "0"), ("end", "20500101"), ("lmt", &count.to_string())])
        .header("Referer", "https://quote.eastmoney.com/")
        .timeout(Duration::from_secs(12))
        .send().await?.error_for_status()?.json().await?;
    let rows = parse_em_klines(&root);
    if rows.is_empty() {
        return Err(anyhow!("东财日线没有数据"));
    }
    Ok(rows)
}

fn clip<T>(rows: Vec<T>, date_of: impl Fn(&T) -> &str, end: &str) -> Vec<T> {
    rows.into_iter().filter(|r| date_of(r) <= end).collect()
}

async fn trend_block(client: &Client, end: &str, partial: bool) -> Value {
    let jobs = INDICES.iter().map(|(tx, secid, name)| async move {
        let mut errors: Vec<String> = Vec::new();
        if !tx.starts_with("bj") {
            match tencent_bars(client, tx, 320).await {
                Ok(bars) => return Ok((*tx, *name, "腾讯", clip(bars, |b| &b.date, end))),
                Err(e) => errors.push(format!("腾讯 {}", emotion::error_message("日线", &e))),
            }
        }
        match em_index_daily(client, secid, 320).await {
            Ok(rows) => Ok((*tx, *name, "东财", clip(rows.into_iter().map(|(date, close, _)| Bar { date, close }).collect(), |b| &b.date, end))),
            Err(e) => Err((*tx, *name, format!("{};东财 {}", errors.join(";"), emotion::error_message("日线", &e)))),
        }
    });
    let results = futures_util::future::join_all(jobs).await;
    let (mut rows, mut failed) = (Vec::new(), Vec::new());
    for r in results {
        match r {
            Ok((code, name, source, bars)) => match trend_row(&code[2..], name, source, &bars) {
                Some(row) => rows.push(row),
                None => failed.push(json!({"code": &code[2..], "name": name, "error": "截到复盘日之后没有日线"})),
            },
            Err((code, name, error)) => failed.push(json!({"code": &code[2..], "name": name, "error": error.trim_matches(';')})),
        }
    }
    if rows.is_empty() {
        return fail(format!("主要指数日线一条都没取到:{}", failed.iter().filter_map(|f| f["error"].as_str()).next().unwrap_or("")));
    }
    let count = |key: &str| {
        let known: Vec<bool> = rows.iter().filter_map(|r| r[key].as_bool()).collect();
        json!({"above": known.iter().filter(|b| **b).count(), "total": known.len()})
    };
    json!({"available": true, "partial": partial, "rows": rows, "failed": failed,
        "above": {"ma20": count("above_ma20"), "ma60": count("above_ma60"), "ma250": count("above_ma250")}})
}

async fn amount_block(client: &Client, end: &str, partial: bool) -> Value {
    let (sh, sz) = futures_util::join!(em_index_daily(client, AMOUNT_SH, 140), em_index_daily(client, AMOUNT_SZ, 140));
    match (sh, sz) {
        (Ok(sh), Ok(sz)) => {
            let sz_by: std::collections::HashMap<&str, f64> = sz.iter().map(|(d, _, a)| (d.as_str(), *a)).collect();
            let rows: Vec<Amount> = sh.iter().filter(|(d, _, _)| d.as_str() <= end).filter_map(|(d, _, a)| sz_by.get(d.as_str()).map(|z| Amount { date: d.clone(), total: a + z })).collect();
            if rows.is_empty() {
                return fail("两市成交额两边的日期对不上");
            }
            let mut v = amount_stats(&rows, partial && rows.last().is_some_and(|r| r.date == end));
            v["source"] = json!("东财上证指数 + 深证综指日线成交额");
            v
        }
        (a, b) => fail(format!("两市成交额没取到(东财日线):{}", [a.err(), b.err()].into_iter().flatten().map(|e| emotion::error_message("成交额日线", &e)).next().unwrap_or_default())),
    }
}

/// 数据中心一页;只有 code 9201(查无记录)和空列表算「没有」,其余失败都报错
async fn datacenter(client: &Client, report: &str, filter: &str, page_size: usize, sort: &str, types: &str, page: usize) -> Result<Vec<Value>> {
    let root = discovery::get_json(client, DATACENTER, &[
        ("reportName", report.into()), ("columns", "ALL".into()), ("filter", filter.into()), ("pageNumber", page.to_string()), ("pageSize", page_size.to_string()),
        ("sortColumns", sort.into()), ("sortTypes", types.into()), ("source", "WEB".into()), ("client", "WEB".into()),
    ]).await?;
    if root["code"].as_i64() == Some(9201) {
        return Ok(Vec::new());
    }
    if root["success"] == false || root["code"].as_i64().is_some_and(|c| c != 0) {
        return Err(anyhow!("东财数据中心 {report} 返回失败:code={} {}", root["code"], root["message"].as_str().unwrap_or("")));
    }
    root["result"]["data"].as_array().cloned().ok_or_else(|| anyhow!("东财数据中心 {report} 返回结构不对"))
}

/// 两融历史(新 → 旧)→ 余额与变化。融资净买入近 5 日合计、融资余额占流通市值
pub(crate) fn margin_stats(rows: &[Value]) -> Value {
    let num = |r: &Value, k: &str| r[k].as_f64().filter(|v| v.is_finite());
    let Some(latest) = rows.first() else { return fail("两融没有数据") };
    let Some(balance) = num(latest, "RZRQYE") else { return fail("两融余额缺失") };
    let at = |n: usize| rows.get(n).and_then(|r| num(r, "RZRQYE"));
    let net5: Vec<f64> = rows.iter().take(5).filter_map(|r| num(r, "RZJME")).collect();
    json!({
        "available": true, "date": latest["DIM_DATE"].as_str().map(|d| &d[..d.len().min(10)]), "balance": balance,
        "chg5": at(5).map(|b| balance - b), "chg20": at(20).map(|b| balance - b),
        "net_buy5": (net5.len() == 5).then(|| net5.iter().sum::<f64>()), "float_pct": num(latest, "RZYEZB"),
        "financing_balance": num(latest, "RZYE"), "securities_balance": num(latest, "RQYE"),
        "rows": rows.iter().take(60).rev().filter_map(|r| Some(json!({"date": r["DIM_DATE"].as_str()?[..10].to_string(), "balance": num(r, "RZRQYE")?}))).collect::<Vec<_>>(),
    })
}

async fn margin_block(client: &Client) -> Value {
    match datacenter(client, "RPTA_RZRQ_LSHJ", "", 60, "DIM_DATE", "-1", 1).await {
        Ok(rows) => {
            let mut v = margin_stats(&rows);
            v["note"] = json!("T 日数据 T+1 才发布,最新一条通常是上一个交易日");
            v
        }
        Err(e) => fail(format!("两融没取到:{}", emotion::error_message("东财两融", &e))),
    }
}

// ---------------------------------------------------------------- 国债收益率

fn strip_tags(cell: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in cell.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag && !c.is_whitespace() => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", "")
}

/// 中债历史查询页的表格 → 行(每行是各单元格去掉标签后的文字)。表头对不上就拒绝,不按列序猜
pub(crate) fn parse_chinabond(html: &str) -> Result<Vec<Vec<String>>> {
    let mut text = html.to_string();
    while let (Some(a), Some(b)) = (text.find("<!--"), text.find("-->")) {
        if b < a { break }
        text.replace_range(a..b + 3, "");
    }
    let tables: Vec<&str> = text.split("<table").collect();
    if tables.len() < 3 {
        return Err(anyhow!("中债页面没有收益率表格"));
    }
    let table = tables[tables.len() - 1];
    let mut parsed: Vec<Vec<String>> = Vec::new();
    for tr in table.split("<tr").skip(1) {
        let tr = tr.split("</tr>").next().unwrap_or("");
        let mut cells = Vec::new();
        let mut rest = tr;
        while let Some(start) = rest.find("<t") {
            let after = &rest[start..];
            let Some(open_end) = after.find('>') else { break };
            let tag = &after[..open_end];
            if !(tag.starts_with("<td") || tag.starts_with("<th")) {
                rest = &after[open_end..];
                continue;
            }
            let body = &after[open_end + 1..];
            let close = body.find("</t").unwrap_or(body.len());
            cells.push(strip_tags(&body[..close]));
            rest = &body[close..];
        }
        if !cells.is_empty() {
            parsed.push(cells);
        }
    }
    const HEADER: [&str; 10] = ["曲线名称", "日期", "3月", "6月", "1年", "3年", "5年", "7年", "10年", "30年"];
    if parsed.first().map(|h| h.iter().map(String::as_str).collect::<Vec<_>>()) != Some(HEADER.to_vec()) {
        return Err(anyhow!("中债收益率页面表头改变,不能按原列序解析"));
    }
    Ok(parsed.split_off(1))
}

fn parse_cn_date(text: &str) -> Option<String> {
    let digits: Vec<String> = text.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).map(String::from).collect();
    if digits.len() >= 3 && digits[0].len() == 4 {
        return Some(format!("{}-{:0>2}-{:0>2}", digits[0], digits[1], digits[2]));
    }
    (digits.len() == 1 && digits[0].len() == 8).then(|| format!("{}-{}-{}", &digits[0][..4], &digits[0][4..6], &digits[0][6..]))
}

/// 复盘日及以前最近一天的国债收益率(%);10 年期在第 9 列
pub(crate) fn bond_row(rows: &[Vec<String>], end: &str) -> Option<Value> {
    let mut best: Option<(String, &Vec<String>)> = None;
    for r in rows.iter().filter(|r| r.len() == 10) {
        let Some(day) = parse_cn_date(&r[1]) else { continue };
        if day.as_str() <= end && best.as_ref().map_or(true, |(d, _)| day > *d) {
            best = Some((day, r));
        }
    }
    let (date, r) = best?;
    let y = |i: usize| r[i].parse::<f64>().ok().filter(|v| v.is_finite());
    Some(json!({"date": date, "y3m": y(2), "y1": y(4), "y3": y(5), "y5": y(6), "y10": y(8), "y30": y(9), "curve": r[0]}))
}

async fn bond_block(client: &Client, end: &str) -> Value {
    let Ok(end_day) = chrono::NaiveDate::parse_from_str(end, "%Y-%m-%d") else { return fail("复盘日无效") };
    let start = (end_day - chrono::Duration::days(20)).format("%Y-%m-%d").to_string();
    let html = async {
        let bytes = client.get(CHINABOND)
            .query(&[("startDate", start.as_str()), ("endDate", end), ("gjqx", "0"), ("qxId", "hzsylqx"), ("locale", "cn_ZH")])
            .timeout(Duration::from_secs(15)).send().await?.error_for_status()?.bytes().await?;
        Ok::<String, anyhow::Error>(String::from_utf8_lossy(&bytes).into_owned())
    }.await;
    match html.and_then(|h| parse_chinabond(&h)) {
        Ok(rows) => match bond_row(&rows, end) {
            Some(mut v) => {
                v["available"] = json!(true);
                v["note"] = json!("中债国债收益率曲线(%),只是收益率,不是估值判断");
                v
            }
            None => fail(format!("中债 {start} ~ {end} 没有国债收益率")),
        },
        Err(e) => fail(format!("国债收益率没取到:{}", emotion::error_message("中债", &e))),
    }
}

// ---------------------------------------------------------------- 全球指数

/// 腾讯行情里的美股 / 港股指数:名称、点位、昨收、涨跌幅(%)、行情时间
pub(crate) fn parse_global(text: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((lhs, rhs)) = line.trim().split_once('=') else { continue };
        let Some(symbol) = lhs.trim().strip_prefix("v_") else { continue };
        let fields: Vec<&str> = rhs.trim().trim_end_matches(';').trim_matches('"').split('~').collect();
        if fields.len() < 33 {
            continue;
        }
        let num = |i: usize| fields[i].trim().parse::<f64>().ok().filter(|v| v.is_finite());
        let Some(price) = num(3).filter(|p| *p > 0.0) else { continue };
        out.push(json!({"symbol": symbol, "price": price, "prev_close": num(4), "pct": num(32), "time": fields.get(30).map(|t| t.trim()).filter(|t| !t.is_empty())}));
    }
    out
}

async fn global_block(client: &Client) -> Value {
    let query = GLOBAL.iter().map(|(c, _, _)| *c).collect::<Vec<_>>().join(",");
    let text = async {
        let bytes = client.get("https://qt.gtimg.cn").query(&[("q", query.as_str())]).header("Referer", "https://gu.qq.com/")
            .timeout(Duration::from_secs(10)).send().await?.error_for_status()?.bytes().await?;
        Ok::<String, anyhow::Error>(encoding_rs::GBK.decode(&bytes).0.into_owned())
    }.await;
    match text {
        Ok(text) => {
            let parsed = parse_global(&text);
            let rows: Vec<Value> = GLOBAL.iter().map(|(code, name, area)| {
                let hit = parsed.iter().find(|p| p["symbol"].as_str().is_some_and(|s| s.eq_ignore_ascii_case(code)));
                json!({"symbol": code, "name": name, "area": area, "price": hit.and_then(|h| h["price"].as_f64()), "pct": hit.and_then(|h| h["pct"].as_f64()),
                    "time": hit.and_then(|h| h["time"].as_str()), "available": hit.is_some()})
            }).collect();
            if rows.iter().all(|r| r["available"] == false) {
                return fail("全球指数腾讯行情一条都没返回");
            }
            json!({"available": true, "rows": rows, "note": "全球指数也沿用红涨绿跌(和本产品一致),不是国际惯例"})
        }
        Err(e) => fail(format!("全球指数没取到:{}", emotion::error_message("腾讯行情", &e))),
    }
}

// ---------------------------------------------------------------- 行业资金、成交额居前、龙虎榜

async fn sina_json(client: &Client, path: &str, params: &[(&str, String)]) -> Result<Vec<Value>> {
    let bytes = client.get(format!("https://vip.stock.finance.sina.com.cn/quotes_service/api/json_v2.php/{path}")).query(params)
        .header("Referer", "https://finance.sina.com.cn/").timeout(Duration::from_secs(12)).send().await?.error_for_status()?.bytes().await?;
    watch::parse_sina_list(&bytes)
}

fn sina_num(row: &Value, key: &str) -> Option<f64> {
    discovery::number(row.get(key))
}

/// 新浪板块资金流(行业)→ 净额居前 / 居后各 10 个。新浪给的涨跌幅是小数(0.0123 = 1.23%)
pub(crate) fn board_flow_rows(arr: &[Value]) -> Vec<Value> {
    let mut rows: Vec<Value> = arr.iter().filter(|x| x["name"].as_str().is_some_and(|n| !n.is_empty())).map(|x| {
        let pct = |k: &str| sina_num(x, k).map(|v| r4(v * 100.0));
        json!({"code": x["category"].as_str().unwrap_or(""), "name": x["name"].as_str().unwrap_or(""), "change_pct": pct("avg_changeratio"),
            "net": sina_num(x, "netamount"), "inflow": sina_num(x, "inamount"), "outflow": sina_num(x, "outamount"), "net_ratio": pct("ratioamount"),
            "leader": x["ts_name"].as_str().unwrap_or(""), "leader_pct": pct("ts_changeratio")})
    }).collect();
    rows.sort_by(|a, b| b["net"].as_f64().partial_cmp(&a["net"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
    rows
}

async fn board_flow_block(client: &Client, matches_day: bool) -> Value {
    match sina_json(client, "MoneyFlow.ssl_bkzj_bk", &[("page", "1".into()), ("num", "500".into()), ("sort", "netamount".into()), ("asc", "0".into()), ("fenlei", "0".into())]).await {
        Ok(arr) => {
            let rows = board_flow_rows(&arr);
            if rows.is_empty() {
                return fail("新浪板块资金流是空的");
            }
            let out: Vec<Value> = rows.iter().take(10).cloned().collect();
            let bottom: Vec<Value> = rows.iter().rev().take(10).cloned().collect();
            json!({"available": true, "total": rows.len(), "top": out, "bottom": bottom, "matches_review_day": matches_day,
                "source": "新浪行业资金流(流入 − 流出,新浪自己的分类约 48 个)", "note": "和东财「主力净额」不是一个口径,两边的数不要互相比较;只给当天此刻"})
        }
        Err(e) => fail(format!("行业资金没取到:{}", emotion::error_message("新浪资金流", &e))),
    }
}

async fn turnover_block(client: &Client, matches_day: bool) -> Value {
    match sina_json(client, "Market_Center.getHQNodeData", &[("page", "1".into()), ("num", "40".into()), ("sort", "amount".into()), ("asc", "0".into()), ("node", "hs_a".into()), ("symbol", "".into()), ("_s_r_a", "page".into())]).await {
        Ok(arr) => {
            let rows: Vec<Value> = arr.iter().filter(|x| x["code"].as_str().is_some_and(|c| c.len() == 6 && c.bytes().all(|b| b.is_ascii_digit()))).take(20).enumerate().map(|(i, x)| {
                json!({"rank": i + 1, "code": x["code"], "name": x["name"].as_str().unwrap_or(""), "price": sina_num(x, "trade"), "pct": sina_num(x, "changepercent"),
                    "amount": sina_num(x, "amount"), "mcap": sina_num(x, "mktcap").map(|v| v * 1e4)})
            }).collect();
            if rows.is_empty() {
                return fail("新浪成交额榜是空的");
            }
            json!({"available": true, "rows": rows, "matches_review_day": matches_day, "source": "新浪「沪深京 A 股」成交额榜(不给行业)", "note": "客观公开榜单,非推荐、非预测;只给当天此刻"})
        }
        Err(e) => fail(format!("成交额榜没取到:{}", emotion::error_message("新浪成交额榜", &e))),
    }
}

/// 龙虎榜日榜 → 一条上榜理由一行,金额(万元)。按净买额排好后取净买居前 15、净卖居前 10
pub(crate) fn dragon_rows(data: &[Value]) -> Vec<Value> {
    let wan = |v: &Value, k: &str| v[k].as_f64().filter(|x| x.is_finite()).map(|x| r2(x / 1e4));
    let mut rows: Vec<Value> = data.iter().map(|r| json!({
        "code": r["SECURITY_CODE"].as_str().unwrap_or(""), "name": r["SECURITY_NAME_ABBR"].as_str().unwrap_or(""), "reason": r["EXPLANATION"].as_str().unwrap_or(""),
        "close": r["CLOSE_PRICE"].as_f64(), "change_pct": r["CHANGE_RATE"].as_f64().map(r2), "net_buy_wan": wan(r, "BILLBOARD_NET_AMT"),
        "buy_wan": wan(r, "BILLBOARD_BUY_AMT"), "sell_wan": wan(r, "BILLBOARD_SELL_AMT"), "turnover_pct": r["TURNOVERRATE"].as_f64().map(r2),
    })).collect();
    rows.sort_by(|a, b| b["net_buy_wan"].as_f64().partial_cmp(&a["net_buy_wan"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
    rows
}

async fn dragon_block(client: &Client, date: &str) -> Value {
    let filter = format!("(TRADE_DATE>='{date}')(TRADE_DATE<='{date}')");
    let mut data: Vec<Value> = Vec::new();
    let mut truncated = false;
    for page in 1..=4 {
        match datacenter(client, "RPT_DAILYBILLBOARD_DETAILSNEW", &filter, 500, "BILLBOARD_NET_AMT,TRADE_ID", "-1,1", page).await {
            Ok(rows) => {
                let n = rows.len();
                data.extend(rows);
                if n < 500 { break }
                if page == 4 { truncated = true }
            }
            Err(e) => return fail(format!("龙虎榜没取到:{}", emotion::error_message("东财龙虎榜", &e))),
        }
    }
    if data.is_empty() {
        return fail(format!("{date} 没有龙虎榜数据(非交易日或盘后还没更新),不当成 0 条"));
    }
    let rows = dragon_rows(&data);
    let top: Vec<Value> = rows.iter().take(15).cloned().collect();
    let sell: Vec<Value> = rows.iter().rev().take(10).filter(|r| r["net_buy_wan"].as_f64().is_some_and(|v| v < 0.0)).cloned().collect();
    json!({"available": true, "date": date, "total": rows.len(), "truncated": truncated, "top_buy": top, "top_sell": sell,
        "note": "按净买额排;同一只可能因多条上榜理由重复出现;客观公开榜单,非推荐"})
}

// ---------------------------------------------------------------- 入口

/// 「此刻」的行情快照属于哪个交易日:今天在交易日历里且已过 09:15 竞价开始 = 今天,否则是上一个交易日
pub(crate) fn snapshot_day_of(calendar: &[String], today: &str, hm: u32) -> Option<String> {
    if hm >= 915 && calendar.iter().any(|d| d == today) {
        return Some(today.to_string());
    }
    calendar.iter().rev().find(|d| d.as_str() < today).cloned()
}

pub async fn fetch(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let requested = emotion::date_parameter(path)?;
    let now = emotion::now_china();
    let calendar = emotion::trade_dates(client).await?;
    let (date, _) = emotion::review_day(&calendar, requested.as_deref(), now)?;
    let settled = emotion::is_settled(&date, &calendar, now);
    let today = now.0.format("%Y-%m-%d").to_string();
    // 「当天此刻」的快照(涨跌分布、行业资金、成交额榜)属于哪个交易日:今天开市且过了 09:15 是今天,否则是上一个交易日(周末取到的是周五收盘)
    let snapshot_day = snapshot_day_of(&calendar, &today, now.1);
    let matches_day = snapshot_day.as_deref() == Some(date.as_str());
    let partial = !settled && date == today;

    let (trend, amount, margin, bond, global, flow, turnover, dragon, snapshot) = futures_util::join!(
        trend_block(client, &date, partial), amount_block(client, &date, partial), margin_block(client), bond_block(client, &date),
        global_block(client), board_flow_block(client, matches_day), turnover_block(client, matches_day), dragon_block(client, &date), market::overview(client),
    );
    let breadth = match snapshot {
        Ok(v) => json!({"available": true, "matches_review_day": matches_day, "trading_date": v["trading_date"], "exchange": v["breadth"]["exchange"], "distribution": v["breadth"]["distribution"]}),
        Err(e) => fail(format!("涨跌分布没取到:{}", emotion::error_message("市场概览", &e))),
    };
    let mut notes: Vec<String> = Vec::new();
    if !matches_day {
        notes.push(format!("涨跌分布、行业资金、成交额榜只给当天此刻(属于 {}),复盘日 {date} 看到的不是同一天,已标出", snapshot_day.as_deref().unwrap_or("未知")));
    }
    if partial {
        notes.push("复盘日还没收盘:指数最后一根和两市成交额是盘中累计,不和整天的均值比".into());
    }
    Ok(json!({
        "date": date, "settled": settled, "partial": partial, "today": today, "generated_at": Utc::now().to_rfc3339(), "year": now.0.year(),
        "trend": trend, "amount": amount, "margin": margin, "bond": bond, "global": global, "board_flow": flow, "turnover": turnover, "dragon": dragon, "breadth": breadth,
        "valuation": fail("沪深 300 市盈率 / 股息率:中证官网只给 xls 文件,手机端还没有 xls 解析,这一项暂无,不拿别的口径顶替"),
        "notes": notes,
        "source": "腾讯指数日线 / 东财日线、两融、龙虎榜 / 中债 / 新浪资金流与成交额榜 / 腾讯全球指数",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(closes: &[f64], start_year: i32) -> Vec<Bar> {
        // 每年 250 个交易日(2 位日、月只为排序),年份递增
        closes.iter().enumerate().map(|(i, c)| {
            let year = start_year + (i / 250) as i32;
            Bar { date: format!("{year}-{:02}-{:02}", (i % 250) / 28 % 12 + 1, (i % 28) + 1), close: *c }
        }).collect()
    }

    #[test]
    fn trend_row_computes_changes_averages_and_distance_from_the_high() {
        // 300 根:前 250 根 100,之后线性涨到 130
        let closes: Vec<f64> = (0..300).map(|i| if i < 250 { 100.0 } else { 100.0 + (i - 249) as f64 * 0.6 }).collect();
        let row = trend_row("000001", "上证指数", "腾讯", &bars(&closes, 2025)).unwrap();
        let last = *closes.last().unwrap();
        assert_eq!(row["close"], last);
        assert_eq!(row["chg1"], r2((last / closes[298] - 1.0) * 100.0));
        assert_eq!(row["chg5"], r2((last / closes[294] - 1.0) * 100.0));
        assert_eq!(row["chg250"], r2((last / closes[49] - 1.0) * 100.0));
        assert_eq!(row["ma20"], r2(closes[280..].iter().sum::<f64>() / 20.0));
        assert_eq!(row["above_ma20"], true);
        assert_eq!(row["from_high_pct"], 0.0, "收在一年最高点");
        assert_eq!(row["window_full"], true);
        assert!(row["ytd"].is_number(), "有上一年的日线才给年初至今");
        let short = trend_row("000001", "上证指数", "腾讯", &bars(&closes[..30], 2026)).unwrap();
        assert_eq!((short["chg60"].clone(), short["ma60"].clone(), short["ma250"].clone(), short["chg250"].clone(), short["ytd"].clone()), (Value::Null, Value::Null, Value::Null, Value::Null, Value::Null));
        assert_eq!(short["window_full"], false);
        assert!(trend_row("x", "x", "x", &[]).is_none());
    }

    #[test]
    fn amount_stats_do_not_compare_half_a_day_with_full_day_averages() {
        let rows: Vec<Amount> = (1..=61).map(|i| Amount { date: format!("2026-08-{i:02}"), total: 1000.0 + i as f64 }).collect();
        let full = amount_stats(&rows, false);
        let ma20 = mean(&rows[41..].iter().map(|a| a.total).collect::<Vec<_>>());
        assert_eq!(full["ma20"], ma20);
        assert_eq!(full["vs_ma20"], r2((1061.0 / ma20 - 1.0) * 100.0));
        assert_eq!(full["prev"], 1060.0);
        let partial = amount_stats(&rows, true);
        assert_eq!((partial["vs_ma20"].clone(), partial["vs_ma60"].clone(), partial["prev"].clone()), (Value::Null, Value::Null, json!(1060.0)));
        assert_eq!(partial["ma20"], mean(&rows[40..60].iter().map(|a| a.total).collect::<Vec<_>>()), "盘中时均值只用之前的整天");
        assert_eq!(amount_stats(&rows[..30], false)["ma60"], Value::Null, "不够 60 天不给 60 日均");
        assert_eq!(amount_stats(&[], false)["available"], false);
    }

    #[test]
    fn snapshot_day_follows_the_trading_calendar() {
        let cal: Vec<String> = ["2026-10-14", "2026-10-15", "2026-10-16"].iter().map(|s| s.to_string()).collect();
        assert_eq!(snapshot_day_of(&cal, "2026-10-17", 1000).as_deref(), Some("2026-10-16"), "周六取到的是周五收盘");
        assert_eq!(snapshot_day_of(&cal, "2026-10-16", 1000).as_deref(), Some("2026-10-16"));
        assert_eq!(snapshot_day_of(&cal, "2026-10-16", 850).as_deref(), Some("2026-10-15"), "开盘前还是前一天的收盘");
        assert_eq!(snapshot_day_of(&cal, "2026-10-19", 1000).as_deref(), Some("2026-10-16"), "日历里还没有今天(节假日或还没更新)");
    }

    #[test]
    fn kline_parsers_read_provider_shapes() {
        let em = json!({"data": {"klines": ["2026-10-15,3900.5,812345678900", "2026-10-16,3920.1,900000000000", "bad", "2026-10-17,x,1"]}});
        assert_eq!(parse_em_klines(&em), vec![("2026-10-15".into(), 3900.5, 812345678900.0), ("2026-10-16".into(), 3920.1, 900000000000.0)]);
        let tx = json!({"data": {"sh000001": {"day": [["2026-10-15", "3890", "3900.5", "3910", "3880", "123"], ["2026-10-16", "3900", "3920.1", "3930", "3890", "456"]]}}});
        assert_eq!(parse_tx_klines(&tx, "sh000001"), vec![Bar { date: "2026-10-15".into(), close: 3900.5 }, Bar { date: "2026-10-16".into(), close: 3920.1 }]);
        assert!(parse_tx_klines(&tx, "sz399001").is_empty());
        let clipped = clip(parse_tx_klines(&tx, "sh000001"), |b| &b.date, "2026-10-15");
        assert_eq!(clipped.len(), 1);
    }

    #[test]
    fn margin_stats_use_report_order_and_do_not_zero_missing_fields() {
        let rows: Vec<Value> = (0..25).map(|i| json!({"DIM_DATE": format!("2026-10-{:02} 00:00:00", 25 - i), "RZRQYE": 25000.0e8 - i as f64 * 1.0e8, "RZYE": 24000.0e8, "RQYE": 100.0e8, "RZJME": 10.0e8, "RZYEZB": 2.5})).collect();
        let v = margin_stats(&rows);
        assert_eq!((v["date"].clone(), v["chg5"].clone(), v["chg20"].clone(), v["net_buy5"].clone()), (json!("2026-10-25"), json!(5.0e8), json!(20.0e8), json!(50.0e8)));
        assert_eq!(v["rows"].as_array().unwrap().len(), 25);
        assert_eq!(v["rows"][0]["date"], "2026-10-01", "走势图旧 → 新");
        let mut gappy = rows.clone();
        gappy[2]["RZJME"] = Value::Null;
        assert_eq!(margin_stats(&gappy)["net_buy5"], Value::Null, "近 5 日有缺项就不加总");
        assert_eq!(margin_stats(&[])["available"], false);
        assert_eq!(margin_stats(&[json!({"DIM_DATE": "2026-10-01"})])["available"], false);
    }

    #[test]
    fn chinabond_page_is_parsed_only_when_the_header_matches() {
        let page = |header: &str| format!(
            "<html><!-- 注释 --><table><tr><td>x</td></tr></table><table><tr><td>y</td></tr></table><table>{header}\
             <tr><td>中债国债收益率曲线</td><td>2026-10-15</td><td>1.30</td><td>1.40</td><td>1.45</td><td>1.60</td><td>1.70</td><td>1.80</td><td>1.85</td><td>2.10</td></tr>\
             <tr><td>中债国债收益率曲线</td><td>2026年10月16日</td><td>1.31</td><td>1.41</td><td>1.46</td><td>1.61</td><td>1.71</td><td>1.81</td><td>1.86</td><td>2.11</td></tr></table></html>");
        let good = "<tr><th>曲线名称</th><th>日期</th><th>3月</th><th>6月</th><th>1年</th><th>3年</th><th>5年</th><th>7年</th><th>10年</th><th>30年</th></tr>";
        let rows = parse_chinabond(&page(good)).unwrap();
        assert_eq!(rows.len(), 2);
        let v = bond_row(&rows, "2026-10-16").unwrap();
        assert_eq!((v["date"].clone(), v["y10"].clone(), v["y30"].clone()), (json!("2026-10-16"), json!(1.86), json!(2.11)));
        assert_eq!(bond_row(&rows, "2026-10-15").unwrap()["y10"], 1.85, "复盘日之后的不用");
        assert_eq!(bond_row(&rows, "2026-10-14"), None);
        let bad = good.replace("10年", "十年");
        assert!(parse_chinabond(&page(&bad)).unwrap_err().to_string().contains("表头改变"));
        assert!(parse_chinabond("<html></html>").is_err());
    }

    #[test]
    fn global_quotes_parse_name_free_fields_and_skip_empty_prices() {
        let mut dj = vec![""; 40];
        dj[1] = "道琼斯"; dj[3] = "43500.12"; dj[4] = "43400.00"; dj[30] = "2026-10-16 16:00:03"; dj[32] = "0.23";
        let mut halted = dj.clone();
        halted[3] = "0.00";
        let text = format!("v_usDJI=\"{}\";\nv_hkHSI=\"{}\";\nv_junk=\"a~b\";\n", dj.join("~"), halted.join("~"));
        let rows = parse_global(&text);
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0]["symbol"].clone(), rows[0]["price"].clone(), rows[0]["pct"].clone()), (json!("usDJI"), json!(43500.12), json!(0.23)));
    }

    #[test]
    fn board_flow_and_dragon_tiger_rows_sort_and_scale() {
        let flow = vec![
            json!({"category": "a", "name": "银行", "avg_changeratio": "0.0123", "netamount": "-5000", "inamount": "100", "outamount": "5100", "ratioamount": "-0.01", "ts_name": "招商银行", "ts_changeratio": "0.02"}),
            json!({"category": "b", "name": "半导体", "avg_changeratio": 0.031, "netamount": 9000, "inamount": 9500, "outamount": 500, "ratioamount": 0.05, "ts_name": "", "ts_changeratio": null}),
            json!({"category": "c", "name": "", "netamount": 1}),
        ];
        let rows = board_flow_rows(&flow);
        assert_eq!(rows.iter().map(|r| r["name"].as_str().unwrap()).collect::<Vec<_>>(), vec!["半导体", "银行"]);
        assert_eq!((rows[1]["change_pct"].clone(), rows[0]["leader_pct"].clone()), (json!(1.23), Value::Null));
        let dragon = vec![
            json!({"SECURITY_CODE": "600001", "SECURITY_NAME_ABBR": "甲", "EXPLANATION": "日涨幅偏离值达7%", "CLOSE_PRICE": 10.5, "CHANGE_RATE": 10.004, "BILLBOARD_NET_AMT": -30000000.0, "BILLBOARD_BUY_AMT": 1.0e7, "BILLBOARD_SELL_AMT": 4.0e7, "TURNOVERRATE": 8.123}),
            json!({"SECURITY_CODE": "600002", "SECURITY_NAME_ABBR": "乙", "EXPLANATION": "连续三个交易日", "CLOSE_PRICE": 20.0, "CHANGE_RATE": -3.0, "BILLBOARD_NET_AMT": 50000000.0, "BILLBOARD_BUY_AMT": 6.0e7, "BILLBOARD_SELL_AMT": 1.0e7, "TURNOVERRATE": null}),
        ];
        let rows = dragon_rows(&dragon);
        assert_eq!((rows[0]["code"].clone(), rows[0]["net_buy_wan"].clone(), rows[1]["net_buy_wan"].clone(), rows[1]["change_pct"].clone(), rows[0]["turnover_pct"].clone()), (json!("600002"), json!(5000.0), json!(-3000.0), json!(10.0), Value::Null));
    }

    /// 对真实上游:cargo test --locked market_panel::tests::live_market_panel_probe -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_market_panel_probe() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-mpanel-live-{nonce}"));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let started = std::time::Instant::now();
        let r = app.handle(crate::MobileApiRequest { path: "/mobile/review/market".into(), method: "GET".into(), body: Value::Null }).await;
        println!("status {} in {:?}; date {} settled {}", r.status, started.elapsed(), r.body["date"], r.body["settled"]);
        for key in ["trend", "amount", "margin", "bond", "global", "board_flow", "turnover", "dragon", "breadth", "valuation"] {
            let b = &r.body[key];
            println!("{key}: available {} {}", b["available"], if b["available"] == true { String::new() } else { b["reason"].to_string() });
        }
        println!("amount {} margin {} bond {} dragon total {}", r.body["amount"]["last"], r.body["margin"]["balance"], r.body["bond"]["y10"], r.body["dragon"]["total"]);
        if let Ok(out) = std::env::var("VIBE_MARKET_PANEL_FIXTURE") {
            std::fs::write(out, serde_json::to_string_pretty(&r.body).unwrap()).unwrap();
        }
        assert_eq!(r.status, 200);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
