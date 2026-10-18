use crate::symbol::{normalize_stock_symbol, SymbolKey};
use anyhow::{anyhow, Result};
use chrono::{FixedOffset, TimeZone, Utc};
use encoding_rs::GBK;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Instant;

pub enum FetchError {
    BadRequest(String),
    NotMigrated(String),
    Other(anyhow::Error),
}

impl From<anyhow::Error> for FetchError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(value)
    }
}

pub async fn fetch_endpoint(client: &Client, body: &Value) -> std::result::Result<Value, FetchError> {
    let endpoint = body.get("endpoint").and_then(Value::as_str).unwrap_or("");
    let raw_symbol = body.get("symbol").and_then(Value::as_str).unwrap_or("");
    if raw_symbol.trim().is_empty() {
        return Err(FetchError::BadRequest("fetch.symbol 不能为空".into()));
    }
    let symbol = normalize_stock_symbol(raw_symbol)
        .map_err(|error| FetchError::BadRequest(error.to_string()))?;

    let started = Instant::now();
    let envelope = match endpoint {
        "tx_quote" | "quote" | "fetch_quote" => fetch_quote(client, &symbol).await?,
        "em_kline" | "kline" | "fetch_kline" => {
            let bars = body
                .get("args")
                .and_then(|v| v.get("bars"))
                .and_then(Value::as_u64)
                .unwrap_or(250)
                .clamp(20, 1000) as usize;
            fetch_kline(client, &symbol, bars).await?
        }
        _ => return Err(FetchError::NotMigrated(endpoint.to_string())),
    };

    Ok(json!({
        "envelope": envelope,
        "duration_ms": started.elapsed().as_millis() as u64,
        "cached": false,
        "fetched_at": Utc::now().to_rfc3339(),
    }))
}

pub(crate) async fn fetch_quote(client: &Client, symbol: &SymbolKey) -> Result<Value> {
    match fetch_tencent_quote(client, symbol).await {
        Ok(value) => Ok(value),
        Err(primary_error) => {
            let mut fallback = fetch_eastmoney_quote(client, symbol).await?;
            if let Some(obj) = fallback.as_object_mut() {
                obj.insert("status".into(), Value::String("partial".into()));
                obj.insert(
                    "errors".into(),
                    json!([{
                        "source": "tencent",
                        "endpoint": "qt.gtimg.cn/q",
                        "message": primary_error.to_string()
                    }]),
                );
                if let Some(extra) = obj.get_mut("extra").and_then(Value::as_object_mut) {
                    extra.insert(
                        "degraded".into(),
                        Value::String("主源腾讯失败,已降级东财 push2".into()),
                    );
                }
            }
            Ok(fallback)
        }
    }
}

async fn fetch_tencent_quote(client: &Client, symbol: &SymbolKey) -> Result<Value> {
    let url = format!("https://qt.gtimg.cn/q={}", symbol.tencent);
    let bytes = client.get(url).send().await?.error_for_status()?.bytes().await?;
    let (decoded, _, _) = GBK.decode(&bytes);
    let text = decoded.into_owned();
    let payload = text
        .split('"')
        .nth(1)
        .ok_or_else(|| anyhow!("腾讯返回无载荷"))?;
    let fields: Vec<&str> = payload.split('~').collect();
    if fields.len() < 53 || fields.get(3).copied().unwrap_or("").is_empty() {
        return Err(anyhow!("腾讯字段不足或无报价"));
    }

    let now = Utc::now().to_rfc3339();
    let quote_time = fields.get(30).copied().unwrap_or("");
    let period = quote_date(quote_time).unwrap_or_else(china_date);
    let price = number(fields.get(3).copied());
    let last_close = number(fields.get(4).copied());
    let amount = number(fields.get(37).copied());
    let stale = matches!(
        (price, last_close, amount),
        (Some(p), Some(c), Some(a)) if a == 0.0 && p == c && p > 0.0
    );
    let specs = [
        ("price", price, "元", "none"),
        ("last_close", last_close, "元", "none"),
        ("change_pct", number(fields.get(32).copied()), "%", "none"),
        ("turnover_pct", number(fields.get(38).copied()), "%", "none"),
        ("pe_ttm", number(fields.get(39).copied()), "倍", "not_applicable"),
        ("pe_static", number(fields.get(52).copied()), "倍", "not_applicable"),
        ("pb", number(fields.get(46).copied()), "倍", "not_applicable"),
        ("float_market_cap", number(fields.get(44).copied()), "亿元", "not_applicable"),
        ("total_market_cap", number(fields.get(45).copied()), "亿元", "not_applicable"),
        ("amount", amount, "万元", "not_applicable"),
    ];
    let (evidence, missing) = evidence_set(
        symbol,
        &now,
        &period,
        "tencent",
        "qt.gtimg.cn/q",
        &specs,
    );

    Ok(json!({
        "script": "fetch_quote",
        "symbol": symbol.digits,
        "market": symbol.market.as_str(),
        "status": if stale { "partial" } else { "ok" },
        "fetched_at": now,
        "primary_source": "tencent",
        "used_sources": ["tencent"],
        "evidence": evidence,
        "extra": {
            "name": fields.get(1).copied().unwrap_or(""),
            "quote_date": period,
            "quote_time": if quote_time.is_empty() {
                Value::Null
            } else {
                Value::String(quote_time.to_string())
            },
            "is_stale": stale,
            "degraded": if stale {
                Value::String("成交额为 0 且现价等于昨收,可能是盘前/停牌/无效报价".into())
            } else {
                Value::Null
            }
        },
        "errors": [],
        "missing": missing,
    }))
}

async fn fetch_eastmoney_quote(client: &Client, symbol: &SymbolKey) -> Result<Value> {
    let response = client
        .get("https://push2.eastmoney.com/api/qt/stock/get")
        .query(&[
            ("fltt", "2"),
            ("invt", "2"),
            ("secid", symbol.eastmoney.as_str()),
            (
                "fields",
                "f43,f48,f58,f60,f86,f116,f117,f163,f164,f167,f168,f170",
            ),
        ])
        .send()
        .await?
        .error_for_status()?;
    let root: Value = response.json().await?;
    let data = root
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("东财 push2 无数据"))?;
    let price = value_number(data.get("f43"));
    if price.is_none() {
        return Err(anyhow!("东财 push2 无报价"));
    }

    let now = Utc::now().to_rfc3339();
    let period = data
        .get("f86")
        .and_then(value_epoch_china_date)
        .unwrap_or_else(china_date);
    let specs = [
        ("price", price, "元", "none"),
        ("last_close", value_number(data.get("f60")), "元", "none"),
        ("change_pct", value_number(data.get("f170")), "%", "none"),
        ("turnover_pct", value_number(data.get("f168")), "%", "none"),
        ("pe_ttm", value_number(data.get("f164")), "倍", "not_applicable"),
        ("pe_static", value_number(data.get("f163")), "倍", "not_applicable"),
        ("pb", value_number(data.get("f167")), "倍", "not_applicable"),
        ("float_market_cap", value_number(data.get("f117")), "元", "not_applicable"),
        ("total_market_cap", value_number(data.get("f116")), "元", "not_applicable"),
        ("amount", value_number(data.get("f48")), "元", "not_applicable"),
    ];
    let (evidence, missing) = evidence_set(
        symbol,
        &now,
        &period,
        "eastmoney",
        "push2/api/qt/stock/get",
        &specs,
    );

    Ok(json!({
        "script": "fetch_quote",
        "symbol": symbol.digits,
        "market": symbol.market.as_str(),
        "status": "ok",
        "fetched_at": now,
        "primary_source": "eastmoney",
        "used_sources": ["eastmoney"],
        "evidence": evidence,
        "extra": {
            "name": data.get("f58").cloned().unwrap_or(Value::Null),
            "quote_date": period,
            "quote_time": data.get("f86").cloned().unwrap_or(Value::Null),
            "is_stale": "unknown"
        },
        "errors": [],
        "missing": missing,
    }))
}

pub(crate) async fn fetch_kline(client: &Client, symbol: &SymbolKey, bars: usize) -> Result<Value> {
    match fetch_kline_primary(client,symbol,bars).await{
        Ok(result)=>Ok(result),
        Err(error)=>{
            let rows=crate::chart::backtest_fallback(client,symbol,bars).await?;
            Ok(json!({"script":"fetch_kline","symbol":symbol.digits,"market":symbol.market.as_str(),"status":"partial","primary_source":"tencent","used_sources":["tencent"],"fetched_at":Utc::now().to_rfc3339(),"extra":{"rows":rows,"fallback_reason":format!("东财日线失败:{error}")},"missing":[],"errors":[]}))
        }
    }
}

async fn fetch_kline_primary(client: &Client, symbol: &SymbolKey, bars: usize) -> Result<Value> {
    let root = crate::discovery::get_json(client,
        "https://push2his.eastmoney.com/api/qt/stock/kline/get",
        &[("secid", symbol.eastmoney.clone()), ("fields1", "f1,f2,f3,f4,f5,f6".into()),
          ("fields2", "f51,f52,f53,f54,f55,f56,f57,f58".into()), ("klt","101".into()),
          ("fqt","1".into()), ("end","20500101".into()), ("lmt",bars.to_string())]).await?;
    let rows = parse_kline_rows(&root, symbol, crate::stock_context::today())?;
    let last = rows
        .last()
        .ok_or_else(|| anyhow!("东财 K 线无合格行"))?;
    let period = last
        .get(0)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let close = last.get(2).and_then(Value::as_f64).unwrap_or(0.0);
    let volume = last.get(5).and_then(Value::as_f64).unwrap_or(0.0);
    let now = Utc::now().to_rfc3339();
    let start = rows
        .first()
        .and_then(|r| r.get(0))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let evidence = vec![
        evidence(
            symbol,
            &now,
            &period,
            "eastmoney",
            "push2his/api/qt/stock/kline/get",
            "volume_latest",
            volume,
            "源单位",
            "not_applicable",
        ),
        evidence(
            symbol,
            &now,
            &period,
            "eastmoney",
            "push2his/api/qt/stock/kline/get",
            "close_qfq_latest",
            close,
            "元",
            "qfq",
        ),
        evidence(
            symbol,
            &now,
            &format!("{}..{}", start, period),
            "eastmoney",
            "push2his/api/qt/stock/kline/get",
            "kline_points",
            rows.len() as f64,
            "根",
            "qfq",
        ),
    ];

    Ok(json!({
        "script": "fetch_kline",
        "symbol": symbol.digits,
        "market": symbol.market.as_str(),
        "status": "ok",
        "fetched_at": now,
        "primary_source": "eastmoney",
        "used_sources": ["eastmoney"],
        "evidence": evidence,
        "extra": {
            "start": start,
            "end": period,
            "bars": rows.len(),
            "rows": rows,
            "adjustment": "qfq"
        },
        "errors": [],
        "missing": [],
    }))
}

pub(crate) fn parse_kline_rows(root: &Value, symbol: &SymbolKey, today: chrono::NaiveDate) -> Result<Vec<Value>> {
    let data = crate::stock_context::provider_data(root, symbol)?;
    let raw = data["klines"].as_array().ok_or_else(|| anyhow!("东财 K 线序列缺失"))?;
    let mut rows = Vec::new();
    let mut previous = None;
    for raw_row in raw {
        let line = raw_row.as_str().ok_or_else(|| anyhow!("东财 K 线行格式错误"))?;
        let fields: Vec<_> = line.split(',').collect();
        if fields.len() < 6 { return Err(anyhow!("东财 K 线字段不足")); }
        let date = chrono::NaiveDate::parse_from_str(fields[0], "%Y-%m-%d")?;
        if date > today || previous.is_some_and(|d| date <= d) { return Err(anyhow!("K 线日期未来、重复或乱序")); }
        previous = Some(date);
        let values = fields[1..6].iter().map(|s| number(Some(s)).ok_or_else(||anyhow!("K 线数值缺失"))).collect::<Result<Vec<_>>>()?;
        let (open,close,high,low,volume)=(values[0],values[1],values[2],values[3],values[4]);
        if open<=0.0 || close<=0.0 || low<=0.0 || low>open.min(close) || high<open.max(close) || high<low || volume<0.0 {
            return Err(anyhow!("K 线价格或成交量异常"));
        }
        rows.push(json!([date.to_string(),open,close,high,low,volume]));
    }
    Ok(rows)
}

#[cfg(test)]
mod kline_fixture_tests {
    use super::*;
    fn day()->chrono::NaiveDate{chrono::NaiveDate::from_ymd_opt(2026,10,16).unwrap()}
    #[test]
    fn frozen_public_kline_keeps_qfq_prices_and_volume_units() {
        let key=normalize_stock_symbol("600519").unwrap();
        let root=serde_json::from_str(include_str!("../tests/fixtures/market_kline.json")).unwrap();
        let rows=parse_kline_rows(&root,&key,day()).unwrap();
        assert_eq!(rows.len(),65);
        assert_eq!(rows[64],json!(["2026-10-16",1311.57,1329.0,1329.88,1301.68,38502.0]));
    }
    #[test]
    fn malformed_bar_does_not_create_a_false_technical_observation() {
        let key=normalize_stock_symbol("600519").unwrap();
        for lines in [
            vec!["2026-10-16,10,11,12,9,1","2026-10-16,10,11,12,9,1"],
            vec!["2026-10-16,10,11,9,8,1"],
            vec!["2026-10-16,10,11,12,9,NaN"],
            vec!["2026-10-16,10,11,12,9,-1"],
            vec!["2099-10-16,10,11,12,9,1"],
        ] { assert!(parse_kline_rows(&json!({"data":{"code":"600519","klines":lines}}),&key,day()).is_err()); }
        assert!(parse_kline_rows(&json!({"data":{"code":"000001","klines":[]}}),&key,day()).is_err());
    }
}

fn china_offset() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("valid China timezone offset")
}

fn china_date() -> String {
    Utc::now()
        .with_timezone(&china_offset())
        .format("%Y-%m-%d")
        .to_string()
}

fn value_epoch_china_date(value: &Value) -> Option<String> {
    let seconds = value.as_i64().or_else(|| value.as_f64().map(|v| v as i64))?;
    let dt = Utc.timestamp_opt(seconds, 0).single()?;
    Some(
        dt.with_timezone(&china_offset())
            .format("%Y-%m-%d")
            .to_string(),
    )
}

fn number(value: Option<&str>) -> Option<f64> {
    let text = value?.trim();
    if text.is_empty() || text == "-" {
        return None;
    }
    text.parse::<f64>().ok().filter(|n| n.is_finite())
}

fn value_number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => number(Some(s.as_str())),
        _ => None,
    }
}

fn quote_date(ts: &str) -> Option<String> {
    if ts.len() < 8 || !ts[..8].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("{}-{}-{}", &ts[0..4], &ts[4..6], &ts[6..8]))
}

fn evidence_set(
    symbol: &SymbolKey,
    fetched_at: &str,
    period: &str,
    source: &str,
    endpoint: &str,
    specs: &[(&str, Option<f64>, &str, &str)],
) -> (Vec<Value>, Vec<Value>) {
    let mut evidence_rows = Vec::new();
    let mut missing = Vec::new();
    for (field, value, unit, adjustment) in specs {
        match value {
            Some(value) => evidence_rows.push(evidence(
                symbol,
                fetched_at,
                period,
                source,
                endpoint,
                field,
                *value,
                unit,
                adjustment,
            )),
            None => missing.push(json!({ "field": field, "period": period })),
        }
    }
    (evidence_rows, missing)
}

fn evidence(
    symbol: &SymbolKey,
    fetched_at: &str,
    period: &str,
    source: &str,
    endpoint: &str,
    field: &str,
    value: f64,
    unit: &str,
    adjustment: &str,
) -> Value {
    json!({
        "id": format!("mobile:{}:{}:{}:{}", source, symbol.digits, field, period),
        "symbol": symbol.digits,
        "market": symbol.market.as_str(),
        "field": field,
        "value": value,
        "unit": unit,
        "currency": if unit == "元" || unit == "万元" || unit == "亿元" { "CNY" } else { "n/a" },
        "period": period,
        "as_of": period,
        "source": source,
        "endpoint": endpoint,
        "fetched_at": fetched_at,
        "adjustment": adjustment,
        "raw_ref": null,
        "record_key": null
    })
}
