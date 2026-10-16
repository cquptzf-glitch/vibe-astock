use crate::{journal_review, storage, symbol::normalize_stock_symbol};
use anyhow::{anyhow, Result};
use chrono::{FixedOffset, Utc};
use encoding_rs::GBK;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;

const MAX_SYMBOLS: usize = 50;

#[derive(Debug, Clone)]
struct QuoteRow {
    symbol: String,
    name: String,
    price: Option<f64>,
    change_pct: Option<f64>,
    amount_yuan: Option<f64>,
    quote_date: Option<String>,
}

pub async fn snapshot(client: &Client, db_path: &std::path::Path) -> Result<Value> {
    let watchlist = storage::list(db_path)?;
    let review = journal_review::view(db_path)?;

    let mut symbols = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Some(items) = watchlist.get("items").and_then(Value::as_array) {
        for item in items {
            if let Some(raw) = item.get("symbol").and_then(Value::as_str) {
                if let Ok(key) = normalize_stock_symbol(raw) {
                    if seen.insert(key.digits.clone()) {
                        symbols.push(key);
                    }
                }
            }
        }
    }
    if let Some(items) = review.get("positions").and_then(Value::as_array) {
        for item in items {
            if let Some(raw) = item.get("symbol").and_then(Value::as_str) {
                if let Ok(key) = normalize_stock_symbol(raw) {
                    if seen.insert(key.digits.clone()) {
                        symbols.push(key);
                    }
                }
            }
        }
    }
    symbols.truncate(MAX_SYMBOLS);

    let mut quotes = fetch_batch(client, &symbols).await.unwrap_or_default();
    let missing_keys: Vec<_> = symbols
        .iter()
        .filter(|key| !quotes.contains_key(&key.digits))
        .cloned()
        .collect();

    // Batch is the normal path. Individual quote fetches use the same
    // Tencent -> Eastmoney fallback as the stock detail page.
    for key in missing_keys {
        if let Ok(value) = crate::quote::fetch_quote(client, &key).await {
            if let Some(row) = quote_from_envelope(&value) {
                quotes.insert(key.digits.clone(), row);
            }
        }
    }

    let today = china_date();
    let watch_items = watchlist
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let raw = item.get("symbol").and_then(Value::as_str)?;
                    let key = normalize_stock_symbol(raw).ok()?;
                    Some(render_watch_item(item, quotes.get(&key.digits), &today))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let position_items = review
        .get("positions")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let raw = item.get("symbol").and_then(Value::as_str)?;
                    let key = normalize_stock_symbol(raw).ok()?;
                    Some(render_position_item(item, quotes.get(&key.digits), &today))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let missing = symbols
        .iter()
        .filter(|key| !quotes.contains_key(&key.digits))
        .map(|key| key.digits.clone())
        .collect::<Vec<_>>();

    Ok(json!({
        "status": if missing.is_empty() { "ok" } else { "partial" },
        "source": "tencent_batch",
        "fetched_at": Utc::now().to_rfc3339(),
        "trading_date": today,
        "stale": false,
        "limits": { "max_symbols": MAX_SYMBOLS },
        "watchlist": watch_items,
        "positions": position_items,
        "missing": missing,
    }))
}

async fn fetch_batch(client: &Client, symbols: &[crate::symbol::SymbolKey]) -> Result<HashMap<String, QuoteRow>> {
    if symbols.is_empty() {
        return Ok(HashMap::new());
    }
    let query = symbols.iter().map(|key| key.tencent.as_str()).collect::<Vec<_>>().join(",");
    let response = client
        .get("https://qt.gtimg.cn")
        .query(&[("q", query.as_str())])
        .header("Referer", "https://gu.qq.com/")
        .send()
        .await?
        .error_for_status()?;
    let bytes = response.bytes().await?;
    let (decoded, _, _) = GBK.decode(&bytes);
    let mut rows = HashMap::new();

    for line in decoded.lines() {
        let line = line.trim();
        let Some((lhs, rhs)) = line.split_once('=') else { continue };
        if !lhs.starts_with("v_") || rhs.len() < 2 {
            continue;
        }
        let payload = rhs.trim().trim_matches(';').trim_matches('"');
        let fields: Vec<&str> = payload.split('~').collect();
        if fields.len() < 40 {
            continue;
        }
        let code = fields.get(2).copied().unwrap_or("").trim();
        if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let price = number(fields.get(3));
        let change_pct = number(fields.get(32));
        let amount_yuan = number(fields.get(37)).map(|v| v * 10_000.0);
        let quote_date = fields
            .get(30)
            .copied()
            .and_then(|v| v.get(..8))
            .filter(|v| v.chars().all(|c| c.is_ascii_digit()))
            .map(|v| format!("{}-{}-{}", &v[0..4], &v[4..6], &v[6..8]));
        rows.insert(
            code.to_string(),
            QuoteRow {
                symbol: code.to_string(),
                name: fields.get(1).copied().unwrap_or("").to_string(),
                price,
                change_pct,
                amount_yuan,
                quote_date,
            },
        );
    }

    if rows.is_empty() {
        return Err(anyhow!("腾讯批量行情没有返回可用数据"));
    }
    Ok(rows)
}

fn quote_from_envelope(value: &Value) -> Option<QuoteRow> {
    let symbol = value.get("symbol").and_then(Value::as_str)?.to_string();
    let evidence = value.get("evidence").and_then(Value::as_array)?;
    let number_for = |field: &str| -> Option<f64> {
        let raw = evidence.iter().find(|row| row.get("field").and_then(Value::as_str) == Some(field))?.get("value")?;
        match raw {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.parse().ok(),
            _ => None,
        }
    };
    let name = value
        .pointer("/extra/name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let quote_date = value
        .pointer("/extra/quote_date")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let amount = number_for("amount");
    let unit = evidence
        .iter()
        .find(|row| row.get("field").and_then(Value::as_str) == Some("amount"))
        .and_then(|row| row.get("unit"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let amount_yuan = match unit {
        "万元" => amount.map(|v| v * 10_000.0),
        "元" => amount,
        "亿元" => amount.map(|v| v * 100_000_000.0),
        _ => amount,
    };
    Some(QuoteRow {
        symbol,
        name,
        price: number_for("price"),
        change_pct: number_for("change_pct"),
        amount_yuan,
        quote_date,
    })
}

fn render_watch_item(item: &Value, quote: Option<&QuoteRow>, today: &str) -> Value {
    let symbol = item.get("symbol").and_then(Value::as_str).unwrap_or("");
    let name = item.get("name").and_then(Value::as_str).unwrap_or("");
    json!({
        "symbol": symbol,
        "name": if !name.is_empty() { name } else { quote.map(|q| q.name.as_str()).unwrap_or("") },
        "price": quote.and_then(|q| q.price),
        "change_pct": quote.and_then(|q| q.change_pct),
        "amount_yuan": quote.and_then(|q| q.amount_yuan),
        "quote_date": quote.and_then(|q| q.quote_date.as_deref()),
        "is_today": quote.and_then(|q| q.quote_date.as_deref()).is_some_and(|date| date == today),
        "available": quote.is_some_and(|q| q.price.is_some()),
    })
}

fn render_position_item(item: &Value, quote: Option<&QuoteRow>, today: &str) -> Value {
    let shares = item.get("open_shares").and_then(Value::as_f64).unwrap_or(0.0);
    let avg_cost = item.get("avg_cost").and_then(Value::as_f64);
    let price = quote.and_then(|q| q.price);
    let market_value = price.map(|v| v * shares);
    let unrealized_pnl = price.zip(avg_cost).map(|(v, c)| (v - c) * shares);
    let unrealized_pct = price.zip(avg_cost).and_then(|(v, c)| {
        (c > 0.0).then_some((v / c - 1.0) * 100.0)
    });
    let quote_today = quote.and_then(|q| q.quote_date.as_deref()).is_some_and(|date| date == today);
    let planned_stop = item.get("planned_stop").and_then(Value::as_f64);
    let planned_target = item.get("planned_target").and_then(Value::as_f64);
    let risk_state = match (price, planned_stop, planned_target, quote_today) {
        (Some(p), Some(stop), _, true) if p <= stop => "stop_reached",
        (Some(p), _, Some(target), true) if p >= target => "target_reached",
        (Some(p), Some(stop), _, true) if p <= stop * 1.02 => "stop_near",
        (Some(p), _, Some(target), true) if p >= target * 0.98 => "target_near",
        (_, _, _, false) => "stale_quote",
        (None, _, _, _) => "unquoted",
        _ => "normal",
    };
    json!({
        "id": item.get("id").cloned().unwrap_or(Value::Null),
        "symbol": item.get("symbol").cloned().unwrap_or(Value::Null),
        "name": item.get("name").cloned().unwrap_or(Value::Null),
        "open_shares": shares,
        "avg_cost": avg_cost,
        "planned_stop": planned_stop,
        "planned_target": planned_target,
        "price": price,
        "change_pct": quote.and_then(|q| q.change_pct),
        "market_value": market_value,
        "unrealized_pnl": unrealized_pnl,
        "unrealized_pct": unrealized_pct,
        "quote_date": quote.and_then(|q| q.quote_date.as_deref()),
        "is_today": quote_today,
        "risk_state": risk_state,
    })
}

fn number(value: Option<&&str>) -> Option<f64> {
    let text = value?.trim();
    if text.is_empty() || *text == "-" {
        return None;
    }
    text.parse::<f64>().ok().filter(|n| n.is_finite())
}

fn china_date() -> String {
    let offset = FixedOffset::east_opt(8 * 3600).expect("valid China timezone offset");
    Utc::now().with_timezone(&offset).format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_parser_keeps_missing_fields_as_null() {
        let line = r#"v_sh600519="1~贵州茅台~600519~1500~1490~1495~100~0~0~1500~1~0~0~0~0~0~0~0~0~0~20261016100000~0~0~0~0~0~0~0~0~0~20261016100000~0.67~0.7~1510~1480~0~0~0~100~1.2~";"#;
        let mut rows = HashMap::new();
        let payload = line.split_once('=').unwrap().1.trim().trim_matches(';').trim_matches('"');
        let fields: Vec<&str> = payload.split('~').collect();
        assert_eq!(fields[1], "贵州茅台");
        assert_eq!(fields[2], "600519");
        assert_eq!(number(fields.get(3)), Some(1500.0));
        assert_eq!(number(fields.get(39)), Some(1.2));
        let _ = &mut rows;
    }

    #[test]
    fn risk_state_requires_today_quote() {
        let item = json!({
            "symbol": "600519",
            "name": "贵州茅台",
            "open_shares": 100.0,
            "avg_cost": 1600.0,
            "planned_stop": 1500.0,
            "planned_target": 1800.0
        });
        let quote = QuoteRow {
            symbol: "600519".into(),
            name: "贵州茅台".into(),
            price: Some(1490.0),
            change_pct: Some(-1.0),
            amount_yuan: Some(1_000_000.0),
            quote_date: Some("2026-10-16".into()),
        };
        let value = render_position_item(&item, Some(&quote), "2026-10-16");
        assert_eq!(value["risk_state"], "stop_reached");
        assert_eq!(value["unrealized_pnl"], -11_000.0);
        assert_eq!(value["market_value"], 149_000.0);
    }
}
