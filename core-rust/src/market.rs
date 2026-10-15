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

    let (indexes, exchange_breadth) = match fetch_index_and_breadth(client).await {
        Ok(value) => value,
        Err(error) => {
            errors.push(json!({ "part": "indexes", "message": error.to_string() }));
            (Vec::new(), json!({ "up": null, "down": null, "flat": null, "amount_yuan": null }))
        }
    };

    let (distribution, distribution_date) = match fetch_distribution(client).await {
        Ok(value) => value,
        Err(error) => {
            errors.push(json!({ "part": "distribution", "message": error.to_string() }));
            (Vec::new(), None)
        }
    };

    let mut pool_dates = Vec::new();
    let zt = match fetch_pool_count(client, "getTopicZTPool", "fbt:asc", &request_date).await {
        Ok(value) => {
            if let Some(date) = &value.1 { pool_dates.push(date.clone()); }
            Some(value.0)
        }
        Err(error) => {
            errors.push(json!({ "part": "limit_up", "message": error.to_string() }));
            None
        }
    };
    let zb = match fetch_pool_count(client, "getTopicZBPool", "fbt:asc", &request_date).await {
        Ok(value) => {
            if let Some(date) = &value.1 { pool_dates.push(date.clone()); }
            Some(value.0)
        }
        Err(error) => {
            errors.push(json!({ "part": "broken_board", "message": error.to_string() }));
            None
        }
    };
    let dt = match fetch_pool_count(client, "getTopicDTPool", "fund:asc", &request_date).await {
        Ok(value) => {
            if let Some(date) = &value.1 { pool_dates.push(date.clone()); }
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
        "status": if errors.is_empty() { "ok" } else { "partial" },
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
    ensure_rc_ok(&root, "东财指数")?;
    let rows = diff_rows(root.pointer("/data/diff"));
    if rows.is_empty() {
        return Err(anyhow!("东财指数列表为空"));
    }

    let mut indexes = Vec::new();
    let mut up = 0_i64;
    let mut down = 0_i64;
    let mut flat = 0_i64;
    let mut amount = 0.0_f64;
    let mut breadth_rows = 0_i64;

    for row in rows {
        let code = row.get("f12").and_then(value_text).unwrap_or_default();
        if matches!(code.as_str(), "000001" | "399106" | "899050") {
            if let Some(v) = value_i64(row.get("f104")) { up += v; }
            if let Some(v) = value_i64(row.get("f105")) { down += v; }
            if let Some(v) = value_i64(row.get("f106")) { flat += v; }
            if let Some(v) = value_f64(row.get("f6")) { amount += v; }
            breadth_rows += 1;
        }

        if matches!(code.as_str(), "000001" | "399001" | "399006" | "000300") {
            indexes.push(json!({
                "code": code,
                "name": row.get("f14").and_then(value_text).unwrap_or_default(),
                "price": value_f64(row.get("f2")),
                "change_pct": value_f64(row.get("f3")),
                "change": value_f64(row.get("f4")),
                "amount_yuan": value_f64(row.get("f6"))
            }));
        }
    }

    let exchange = if breadth_rows > 0 {
        json!({ "up": up, "down": down, "flat": flat, "amount_yuan": amount })
    } else {
        json!({ "up": null, "down": null, "flat": null, "amount_yuan": null })
    };
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
            if k == key { value_i64(row.get("c")) } else { None }
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
    Utc::now().with_timezone(&offset).format("%Y-%m-%d").to_string()
}

fn format_yyyymmdd(raw: String) -> String {
    if raw.len() == 8 && raw.chars().all(|c| c.is_ascii_digit()) {
        format!("{}-{}-{}", &raw[0..4], &raw[4..6], &raw[6..8])
    } else {
        raw
    }
}
