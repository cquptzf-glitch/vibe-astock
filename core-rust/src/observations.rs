use crate::{discovery::DiscoveryError, quote, stock_context};
use anyhow::{anyhow, Result};
use chrono::Utc;
use reqwest::Client;
use serde_json::{json, Value};

pub async fn fetch(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let key = stock_context::symbol(path)?;
    let envelope = quote::fetch_kline(client, &key, 100).await?;
    let rows = envelope
        .pointer("/extra/rows")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("K 线序列缺失"))?;
    let mut result = calculate(rows)?;
    result["symbol"] = json!(key.digits);
    result["market"] = json!(key.market.as_str());
    let date = result["date"]
        .as_str()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    result["same_day"] = json!(date == Some(stock_context::today()));
    result["stale"] =
        json!(date.is_some_and(|d| stock_context::today().signed_duration_since(d).num_days() > 7));
    result["fetched_at"] = json!(Utc::now().to_rfc3339());
    Ok(result)
}

fn average(rows: &[Value], end: usize, count: usize) -> Option<f64> {
    if end < count {
        return None;
    }
    let sum = rows[end - count..end]
        .iter()
        .map(|v| v[2].as_f64())
        .collect::<Option<Vec<_>>>()?
        .iter()
        .sum::<f64>();
    let value = sum / (count as f64);
    value.is_finite().then_some(value)
}
fn returns(rows: &[Value], days: usize) -> Option<f64> {
    if rows.len() <= days {
        return None;
    }
    let latest = rows.last()?[2].as_f64()?;
    let base = rows[rows.len() - 1 - days][2].as_f64()?;
    (base > 0.0)
        .then_some((latest / base - 1.0) * 100.0)
        .filter(|n| n.is_finite())
}
fn calculate(rows: &[Value]) -> Result<Value> {
    let last = rows.last().ok_or_else(|| anyhow!("量价观察缺少 K 线"))?;
    let n = rows.len();
    let close = last[2].as_f64().ok_or_else(|| anyhow!("收盘价缺失"))?;
    let ma5 = average(rows, n, 5);
    let ma10 = average(rows, n, 10);
    let ma20 = average(rows, n, 20);
    let ma60 = average(rows, n, 60);
    let cross = if let (Some(now5), Some(now20), Some(prev5), Some(prev20)) =
        (ma5, ma20, average(rows, n - 1, 5), average(rows, n - 1, 20))
    {
        if now5 > now20 && prev5 <= prev20 {
            Some("MA5 上穿 MA20")
        } else if now5 < now20 && prev5 >= prev20 {
            Some("MA5 下穿 MA20")
        } else {
            Some("本根未发生 MA5/MA20 交叉")
        }
    } else {
        None
    };
    // Compare against PRIOR 20 bars. Including today's high makes a breakout impossible.
    let (prior_high, prior_low) = if n >= 21 {
        let prior = &rows[n - 21..n - 1];
        (
            prior.iter().filter_map(|v| v[3].as_f64()).reduce(f64::max),
            prior.iter().filter_map(|v| v[4].as_f64()).reduce(f64::min),
        )
    } else {
        (None, None)
    };
    let breakout = match (prior_high, prior_low) {
        (Some(high), Some(low)) => Some(if close > high {
            "收盘高于此前20根最高价"
        } else if close < low {
            "收盘低于此前20根最低价"
        } else {
            "收盘位于此前20根区间内"
        }),
        _ => None,
    };
    let volume_ratio = if n >= 6 {
        let prior = rows[n - 6..n - 1]
            .iter()
            .map(|v| v[5].as_f64().filter(|v| *v > 0.0))
            .collect::<Option<Vec<_>>>();
        prior
            .and_then(|v| {
                let base = v.iter().sum::<f64>() / 5.0;
                last[5].as_f64().map(|current| current / base)
            })
            .filter(|v| v.is_finite())
    } else {
        None
    };
    let bias = ma20
        .filter(|v| *v > 0.0)
        .map(|ma| (close / ma - 1.0) * 100.0)
        .filter(|v| v.is_finite());
    Ok(
        json!({"status":if n<61{"partial"}else{"ok"},"date":last[0],"bars":n,
        "close":close,"ma5":ma5,"ma10":ma10,"ma20":ma20,"ma60":ma60,
        "return5":returns(rows,5),"return20":returns(rows,20),"return60":returns(rows,60),
        "ma20_bias_pct":bias,"ma_cross":cross,"range_observation":breakout,
        "prior20_high":prior_high,"prior20_low":prior_low,"volume_ratio_prev5":volume_ratio,
        "source":"eastmoney","adjustment":"qfq",
        "note":"前复权日线的已发生价格结构；成交量比为本根成交量/此前5根平均成交量，不是盘中量比。日线可能尚未收盘，不输出买卖建议。"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bars(count: usize) -> Vec<Value> {
        (0..count)
            .map(|i| json!([format!("bar{i}"), 10, 10, 11, 9, 100]))
            .collect()
    }
    #[test]
    fn exact_windows_do_not_use_short_history_or_current_bar_as_baseline() {
        let mut rows = bars(21);
        rows[20] = json!(["last", 10, 12, 13, 10, 200]);
        let data = calculate(&rows).unwrap();
        assert_eq!(data["range_observation"], "收盘高于此前20根最高价");
        assert_eq!(data["prior20_high"], 11.0);
        assert_eq!(data["volume_ratio_prev5"], 2.0);
        assert!((data["return20"].as_f64().unwrap() - 20.0).abs() < 1e-9);
        assert!(data["return60"].is_null());
        assert!(data["ma60"].is_null());
        assert!(calculate(&bars(5)).unwrap()["return5"].is_null());
        assert!(calculate(&bars(5)).unwrap()["volume_ratio_prev5"].is_null());
        assert!(calculate(&bars(19)).unwrap()["ma20"].is_null());
    }
    #[test]
    fn crossing_and_zero_volume_have_explicit_semantics() {
        let mut rows = bars(21);
        rows[20] = json!(["last", 10, 12, 13, 10, 200]);
        assert_eq!(calculate(&rows).unwrap()["ma_cross"], "MA5 上穿 MA20");
        rows[18][5] = json!(0);
        assert!(calculate(&rows).unwrap()["volume_ratio_prev5"].is_null());
        rows[20] = json!(["last", 10, 8, 10, 7, 0]);
        assert_eq!(calculate(&rows).unwrap()["ma_cross"], "MA5 下穿 MA20");
        assert!(calculate(&[]).is_err());
    }
}
