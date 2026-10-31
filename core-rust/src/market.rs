use anyhow::{anyhow, Result};
use chrono::{FixedOffset, Utc};
use reqwest::Client;
use serde_json::{json, Map, Value};

const EM_UT: &str = "fa5fd1943c7b386f172d6893dbfba10b";
const ZTB_UT: &str = "7eea3edcaed734bea9cbfc24409ed989";

pub async fn overview(client: &Client) -> Result<Value> {
    let today = china_today();
    let request_date = today.replace('-', "");
    let mut errors = Vec::<Value>::new();
    let (index_result, distribution_result, zt_result, zb_result, dt_result) = futures_util::join!(
        fetch_index_and_breadth(client),
        fetch_distribution(client),
        fetch_pool_count(client, "getTopicZTPool", "fbt:asc", &request_date),
        fetch_pool_count(client, "getTopicZBPool", "fbt:asc", &request_date),
        fetch_pool_count(client, "getTopicDTPool", "fund:asc", &request_date)
    );

    let (indexes, exchange_breadth) = match index_result {
        Ok(value) => value,
        Err(error) => {
            errors.push(json!({ "part": "indexes", "message": error.to_string() }));
            (
                Vec::new(),
                json!({ "up": null, "down": null, "flat": null, "amount_yuan": null }),
            )
        }
    };

    let (distribution, distribution_date) = match distribution_result {
        Ok(value) => value,
        Err(error) => {
            errors.push(json!({ "part": "distribution", "message": error.to_string() }));
            (Vec::new(), None)
        }
    };

    let mut pool_dates = Vec::new();
    let zt = match zt_result {
        Ok(value) => {
            if let Some(date) = &value.1 {
                pool_dates.push(date.clone());
            }
            Some(value.0)
        }
        Err(error) => {
            errors.push(json!({ "part": "limit_up", "message": error.to_string() }));
            None
        }
    };
    let zb = match zb_result {
        Ok(value) => {
            if let Some(date) = &value.1 {
                pool_dates.push(date.clone());
            }
            Some(value.0)
        }
        Err(error) => {
            errors.push(json!({ "part": "broken_board", "message": error.to_string() }));
            None
        }
    };
    let dt = match dt_result {
        Ok(value) => {
            if let Some(date) = &value.1 {
                pool_dates.push(date.clone());
            }
            Some(value.0)
        }
        Err(error) => {
            errors.push(json!({ "part": "limit_down", "message": error.to_string() }));
            None
        }
    };

    let seal_rate = match (zt, zb) {
        (Some(zt), Some(zb)) if zt + zb > 0 => Some((zt as f64) * 100.0 / ((zt + zb) as f64)),
        _ => None,
    };

    let trading_date = distribution_date
        .or_else(|| pool_dates.into_iter().max())
        .map(format_yyyymmdd)
        .unwrap_or_else(|| today.clone());
    let stale = trading_date != today;

    Ok(json!({
        "status": if errors.is_empty() && exchange_breadth.get("complete").and_then(Value::as_bool)==Some(true) { "ok" } else { "partial" },
        "source": "eastmoney",
        "fetched_at": Utc::now().to_rfc3339(),
        "trading_date": trading_date,
        "stale": stale,
        "indexes": indexes,
        "breadth": {
            "exchange": exchange_breadth,
            "distribution": distribution
        },
        "sentiment": {
            "limit_up": zt,
            "broken_board": zb,
            "limit_down": dt,
            "seal_rate": seal_rate
        },
        "errors": errors
    }))
}

async fn fetch_index_and_breadth(client: &Client) -> Result<(Vec<Value>, Value)> {
    let root: Value = client
        .get("https://push2.eastmoney.com/api/qt/ulist.np/get")
        .query(&[
            ("fltt", "2"),
            ("invt", "2"),
            ("fields", "f12,f14,f2,f3,f4,f6,f104,f105,f106"),
            (
                "secids",
                "1.000001,0.399106,0.899050,0.399001,0.399006,1.000300",
            ),
        ])
        .header("Referer", "https://quote.eastmoney.com/")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    parse_index_and_breadth(&root)
}

fn parse_index_and_breadth(root: &Value) -> Result<(Vec<Value>, Value)> {
    ensure_rc_ok(root, "东财指数")?;
    let rows = diff_rows(root.pointer("/data/diff"));
    if rows.is_empty() {
        return Err(anyhow!("东财指数列表为空"));
    }
    let mut indexes = Vec::new();
    let mut exchange_rows = std::collections::BTreeMap::new();
    for row in rows {
        let code = row.get("f12").and_then(value_text).unwrap_or_default();
        if matches!(code.as_str(), "000001" | "399106" | "899050") {
            exchange_rows.insert(code.clone(), row.clone());
        }
        if matches!(
            code.as_str(),
            "000001" | "399106" | "899050" | "399001" | "399006" | "000300"
        ) {
            indexes.push(
                json!({"code":code,"name":row.get("f14").and_then(value_text).unwrap_or_default(),
                "price":value_f64(row.get("f2")),"change_pct":value_f64(row.get("f3")),
                "change":value_f64(row.get("f4")),"amount_yuan":value_f64(row.get("f6"))}),
            );
        }
    }
    let sum_counts = |field: &str| -> Option<i64> {
        if exchange_rows.len() != 3 {
            return None;
        }
        exchange_rows.values().try_fold(0_i64, |sum, row| {
            let number = value_i64(row.get(field)).filter(|v| *v >= 0)?;
            sum.checked_add(number)
        })
    };
    let amount = if exchange_rows.len() == 3 {
        exchange_rows
            .values()
            .map(|row| value_f64(row.get("f6")).filter(|v| *v >= 0.0))
            .collect::<Option<Vec<_>>>()
            .map(|values| values.iter().sum::<f64>())
            .filter(|sum| sum.is_finite())
    } else {
        None
    };
    let up = sum_counts("f104");
    let down = sum_counts("f105");
    let flat = sum_counts("f106");
    let exchange = json!({"up":up,"down":down,"flat":flat,"amount_yuan":amount,
        "complete":up.is_some()&&down.is_some()&&flat.is_some()&&amount.is_some(),
        "reported_markets":exchange_rows.len(),"expected_markets":3});
    Ok((indexes, exchange))
}

async fn fetch_distribution(client: &Client) -> Result<(Vec<Value>, Option<String>)> {
    let root: Value = client
        .get("https://push2ex.eastmoney.com/getTopicZDFenBu")
        .query(&[("ut", EM_UT), ("dpt", "wz.ztzt")])
        .header("Referer", "https://quote.eastmoney.com/")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure_rc_ok(&root, "东财涨跌分布")?;
    let data = root
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("东财涨跌分布缺少 data"))?;
    let qdate = data.get("qdate").and_then(value_text);
    let rows = data
        .get("fenbu")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("东财涨跌分布为空"))?;

    let labels = [
        ("1", "跌停"),
        ("2", "<-7%"),
        ("3", "-7~-5%"),
        ("4", "-5~-3%"),
        ("5", "-3~0%"),
        ("6", "0~3%"),
        ("7", "3~5%"),
        ("8", "5~7%"),
        ("9", ">7%"),
        ("10", "涨停"),
        ("11", "平盘"),
    ];
    let mut out = Vec::new();
    for (key, label) in labels {
        let count = rows.iter().find_map(|row| {
            let k = row.get("k").and_then(value_text)?;
            if k == key {
                value_i64(row.get("c"))
            } else {
                None
            }
        });
        out.push(json!({ "key": key, "label": label, "count": count }));
    }
    Ok((out, qdate))
}

async fn fetch_pool_count(
    client: &Client,
    endpoint: &str,
    sort: &str,
    date: &str,
) -> Result<(u64, Option<String>)> {
    let url = format!("https://push2ex.eastmoney.com/{endpoint}");
    let root: Value = client
        .get(url)
        .query(&[
            ("ut", ZTB_UT),
            ("dpt", "wz.ztzt"),
            ("Pageindex", "0"),
            ("pagesize", "10000"),
            ("sort", sort),
            ("date", date),
        ])
        .header("Referer", "https://quote.eastmoney.com/")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure_rc_ok(&root, endpoint)?;
    let data = root
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("东财 {endpoint} 缺少 data"))?;
    let qdate = data.get("qdate").and_then(value_text);
    let count = value_u64(data.get("tc")).or_else(|| {
        data.get("pool")
            .and_then(Value::as_array)
            .map(|items| items.len() as u64)
    });
    Ok((count.unwrap_or(0), qdate))
}

fn ensure_rc_ok(root: &Value, label: &str) -> Result<()> {
    if let Some(rc) = root.get("rc").and_then(Value::as_i64) {
        if rc != 0 {
            return Err(anyhow!("{label} 返回业务错误 rc={rc}"));
        }
    }
    Ok(())
}

fn diff_rows(value: Option<&Value>) -> Vec<&Map<String, Value>> {
    match value {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_object).collect(),
        Some(Value::Object(items)) => items.values().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    }
}

fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(v) => Some(v.clone()),
        Value::Number(v) => Some(v.to_string()),
        _ => None,
    }
}

fn value_f64(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(v) => v.as_f64(),
        Value::String(v) => v.parse::<f64>().ok(),
        _ => None,
    }
}

fn value_i64(value: Option<&Value>) -> Option<i64> {
    value_f64(value).map(|v| v as i64)
}

fn value_u64(value: Option<&Value>) -> Option<u64> {
    value_f64(value).filter(|v| *v >= 0.0).map(|v| v as u64)
}

fn china_today() -> String {
    let offset = FixedOffset::east_opt(8 * 3600).expect("valid China timezone offset");
    Utc::now()
        .with_timezone(&offset)
        .format("%Y-%m-%d")
        .to_string()
}

fn format_yyyymmdd(raw: String) -> String {
    if raw.len() == 8 && raw.chars().all(|c| c.is_ascii_digit()) {
        format!("{}-{}-{}", &raw[0..4], &raw[4..6], &raw[6..8])
    } else {
        raw
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;
    #[test]
    fn all_six_indexes_visible_and_exchange_totals_keep_missing_values() {
        let mut root = json!({"rc":0,"data":{"diff":[
            {"f12":"000001","f14":"上证指数","f2":3000,"f104":10,"f105":20,"f106":3,"f6":1000},
            {"f12":"399106","f14":"深证综指","f2":2000,"f104":20,"f105":30,"f106":4,"f6":2000},
            {"f12":"899050","f14":"北证50","f2":1000,"f104":30,"f105":40,"f106":5,"f6":3000},
            {"f12":"399001","f14":"深证成指","f2":10000},
            {"f12":"399006","f14":"创业板指","f2":2000},
            {"f12":"000300","f14":"沪深300","f2":4000}] }});
        let (indexes, exchange) = parse_index_and_breadth(&root).unwrap();
        assert_eq!(indexes.len(), 6);
        assert_eq!(exchange["up"], 60);
        assert_eq!(exchange["amount_yuan"], 6000.0);
        assert_eq!(exchange["complete"], true);
        root["data"]["diff"][1]["f104"] = json!("-");
        let (_, missing) = parse_index_and_breadth(&root).unwrap();
        assert!(missing["up"].is_null());
        assert_eq!(missing["down"], 90);
        assert_eq!(missing["complete"], false);
        root["data"]["diff"].as_array_mut().unwrap().remove(2);
        let (_, incomplete) = parse_index_and_breadth(&root).unwrap();
        assert!(incomplete["amount_yuan"].is_null());
        assert_eq!(incomplete["reported_markets"], 2);
    }
}
