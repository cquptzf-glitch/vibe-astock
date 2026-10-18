//! Full A-share snapshot scan, distinct from exchange tick-by-tick data.
use crate::{discovery, storage, watch};
use anyhow::{anyhow, Result};
use chrono::Datelike;
use futures_util::{stream, StreamExt};
use reqwest::Client;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

pub(crate) fn parse_row(row: &Value) -> Option<Value> {
    let code = row["f12"].as_str()?;
    let key = crate::normalize_stock_symbol(code).ok()?;
    if !watch::is_a_share(&key.tencent) {
        return None;
    }
    Some(
        json!({"code":code,"name":row["f14"].as_str().unwrap_or(""),"price":discovery::number(row.get("f2")),"pct":discovery::number(row.get("f3")),"amount":discovery::number(row.get("f6")),"turnover":discovery::number(row.get("f8")),"pe":discovery::number(row.get("f9")),"pb":discovery::number(row.get("f23")),"volume":discovery::number(row.get("f5")),"quote_timestamp":discovery::number(row.get("f124"))}),
    )
}

async fn page(client: &Client, page: usize) -> Result<Value> {
    discovery::get_json(
        client,
        "https://push2.eastmoney.com/api/qt/clist/get",
        &[
            ("pn", page.to_string()),
            ("pz", "100".into()),
            ("po", "0".into()),
            ("np", "1".into()),
            ("fltt", "2".into()),
            ("invt", "2".into()),
            ("fid", "f12".into()),
            ("fs", "m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23,m:0+t:81".into()),
            ("fields", "f2,f3,f5,f6,f8,f9,f12,f14,f23,f124".into()),
        ],
    )
    .await
}
pub async fn universe(client: &Client) -> Result<(Vec<Value>, Vec<Value>)> {
    let root = page(client, 1).await?;
    let total = root["data"]["total"]
        .as_u64()
        .filter(|v| *v <= 10000)
        .ok_or_else(|| anyhow!("全市场源未给出有效总数"))? as usize;
    let mut raw = discovery::provider_rows(&root)?;
    let mut gaps = Vec::new();
    let results =
        stream::iter((2..=total.div_ceil(100)).map(|p| async move { (p, page(client, p).await) }))
            .buffer_unordered(4)
            .collect::<Vec<_>>()
            .await;
    for (p, result) in results {
        match result.and_then(|r| discovery::provider_rows(&r)) {
            Ok(rows) => raw.extend(rows),
            Err(e) => gaps.push(json!({"page":p,"reason":e.to_string()})),
        }
    }
    let mut seen = HashSet::new();
    let rows: Vec<Value> = raw
        .iter()
        .filter_map(parse_row)
        .filter(|r| seen.insert(r["code"].as_str().unwrap_or("").to_string()))
        .collect();
    if rows.len() < total {
        gaps.push(json!({"reason":format!("已解析 {} / 源站总数 {total};可能存在重复、页间更新或缺页",rows.len())}));
    }
    Ok((rows, gaps))
}

type ScanHistory = HashMap<String, (String, Vec<(i64, f64)>)>;
fn history() -> &'static Mutex<ScanHistory> {
    static H: OnceLock<Mutex<ScanHistory>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(HashMap::new()))
}
fn number(body: &Value, name: &str) -> Result<Option<f64>> {
    match body.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|v| v.is_finite())
            .map(Some)
            .ok_or_else(|| anyhow!("{name} 必须是数字")),
    }
}

pub async fn scan(client: &Client, db: &Path, body: &Value) -> Result<Value> {
    let scope = body["scope"].as_str().unwrap_or("watch");
    let minimum = number(body, "min_pct")?;
    let maximum = number(body, "max_pct")?;
    let amount = number(body, "min_amount")?;
    let turnover = number(body, "min_turnover")?;
    let surge = number(body, "min_surge")?;
    let fall=number(body,"max_surge")?;
    if amount.is_some_and(|v|v<0.0)||turnover.is_some_and(|v|v<0.0){return Err(anyhow!("成交额和换手率下限不能为负数"));}
    if surge.zip(fall).is_some_and(|(a,b)|a>b){return Err(anyhow!("采样变化下限不能高于上限"));}
    if minimum.zip(maximum).is_some_and(|(a, b)| a > b) {
        return Err(anyhow!("涨跌幅下限不能高于上限"));
    }
    let (mut rows, gaps) = if scope == "all" {
        universe(client).await?
    } else if scope == "watch" {
        let list = storage::list(db)?;
        let codes: Vec<String> = list["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["symbol"].as_str().map(str::to_owned))
            .collect();
        let (quotes, _, failed) = watch::fetch_quotes(client, &codes).await;
        let rows=codes.iter().filter_map(|code|quotes.get(code).map(|q|json!({"code":code,"name":q.name,"price":q.price,"pct":q.pct,"amount":q.amount_yuan,"turnover":Value::Null,"quote_time":q.quote_time}))).collect();
        (
            rows,
            if failed > 0 {
                vec![json!({"reason":format!("{failed} 个行情批次失败")})]
            } else {
                vec![]
            },
        )
    } else {
        return Err(anyhow!("范围只能是 watch / all"));
    };
    let (day, hm) = crate::emotion::now_china();
    let day = day.to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let trading = crate::emotion::now_china().0.weekday().number_from_monday() <= 5
        && ((930..=1130).contains(&hm) || (1300..=1500).contains(&hm));
    let mut memory = history().lock().map_err(|_| anyhow!("扫描缓存不可用"))?;
    let total = rows.len();
    let mut missing = 0;
    let mut matched = Vec::new();
    for row in &mut rows {
        let Some(code) = row["code"].as_str().map(str::to_owned) else {
            continue;
        };
        let key = format!("{}:{code}", db.display());
        let mut delta = None;
        let quote_ms = row["quote_timestamp"]
            .as_f64()
            .map(|t| (t * 1000.0) as i64)
            .or_else(|| {
                let t = row["quote_time"].as_str()?;
                ["%Y%m%d%H%M%S", "%Y-%m-%d %H:%M:%S"]
                    .iter()
                    .find_map(|fmt| chrono::NaiveDateTime::parse_from_str(t, fmt).ok())
                    .map(|t| t.and_utc().timestamp_millis() - 8 * 3600 * 1000)
            });
        let fresh = quote_ms.is_some_and(|t| (0..=180000).contains(&(now - t)));
        row["fresh"] = json!(fresh);
        if trading && fresh {
            if let Some(price) = row["price"].as_f64().filter(|p| *p > 0.0) {
                let entry = memory
                    .entry(key)
                    .or_insert_with(|| (day.clone(), Vec::new()));
                if entry.0 != day {
                    entry.0 = day.clone();
                    entry.1.clear();
                }
                entry.1.retain(|(t, _)| now - *t <= 180000);
                if let Some((t, p)) = entry.1.first().filter(|(_, p)| *p > 0.0) {
                    if now - *t >= 60000 {
                        delta = Some((price / p - 1.0) * 100.0);
                    }
                }
                if entry.1.last().is_none_or(|(t, _)| now - *t >= 10000) {
                    entry.1.push((now, price));
                }
            }
        }
        row["surge_pct"] = json!(delta);
        let fields = [
            ("pct", minimum, true),
            ("pct", maximum, false),
            ("amount", amount, true),
            ("turnover", turnover, true),
            ("surge_pct", surge, true),
            ("surge_pct", fall, false),
        ];
        let mut unknown = false;
        let mut passes = true;
        for (field, limit, is_min) in fields {
            if let Some(limit) = limit {
                match row[field].as_f64() {
                    Some(v) => {
                        if (is_min && v < limit) || (!is_min && v > limit) {
                            passes = false;
                        }
                    }
                    None => unknown = true,
                }
            }
        }
        if unknown {
            missing += 1;
        } else if passes {
            matched.push(row.clone());
        }
    }
    drop(memory);
    let count = matched.len();
    matched.sort_by(|a, b| a["code"].as_str().cmp(&b["code"].as_str()));
    Ok(
        json!({"scope":scope,"total":total,"matched":count,"missing":missing,"rows":matched.into_iter().take(200).collect::<Vec<_>>(),"gaps":gaps,"sampled_at":chrono::Utc::now().to_rfc3339(),"note":"全市场为东财当前分页快照(含北交所),页间行情可能变化;自选为腾讯当前报价。分钟变化来自本机前台扫描采样,需连续采样至少一分钟,不等同逐笔行情。未知字段单列缺失,只展示按代码排序的前二百条,不做推荐排序。"}),
    )
}

/// Cache the public universe for native subscription selection, never a quote cache.
pub async fn native_symbols(client: &Client, db: &Path, scope: &str) -> Result<Vec<String>> {
    if scope == "watch" {
        return Ok(storage::list(db)?["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["symbol"].as_str().map(str::to_owned))
            .collect());
    }
    if scope != "all" {
        return Err(anyhow!("范围无效"));
    }
    type Cached = Option<(Instant, Vec<String>)>;
    static CACHE: OnceLock<Mutex<Cached>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Some((at, codes)) = cache
        .lock()
        .map_err(|_| anyhow!("名单缓存不可用"))?
        .as_ref()
    {
        if at.elapsed() < Duration::from_secs(3600) {
            return Ok(codes.clone());
        }
    }
    let (rows, gaps) = universe(client).await?;
    if !gaps.is_empty() {
        return Err(anyhow!("全市场名单不完整,没有启动后台全市场订阅:{gaps:?}"));
    }
    let codes: Vec<String> = rows
        .iter()
        .filter_map(|r| r["code"].as_str().map(str::to_owned))
        .collect();
    *cache.lock().map_err(|_| anyhow!("名单缓存不可用"))? = Some((Instant::now(), codes.clone()));
    Ok(codes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_fields_are_null_and_etfs_are_excluded() {
        let a = parse_row(&json!({"f12":"600519","f14":"样本","f2":"-","f3":0})).unwrap();
        assert!(a["price"].is_null());
        assert_eq!(a["pct"], 0.0);
        assert!(parse_row(&json!({"f12":"510300","f2":2.0})).is_none());
    }
}
