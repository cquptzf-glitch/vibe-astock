use anyhow::{anyhow, Result};
use chrono::{FixedOffset, Timelike, Utc};
use reqwest::Client;
use serde_json::{json, Value};

const ZTB_UT: &str = "7eea3edcaed734bea9cbfc24409ed989";

pub async fn limit_up_pool(client: &Client, request_path: &str) -> Result<Value> {
    let requested = query_param(request_path, "date")
        .map(normalize_date)
        .transpose()?
        .unwrap_or_else(china_yyyymmdd);
    guard_today_before_open(&requested)?;

    let root: Value = client
        .get("https://push2ex.eastmoney.com/getTopicZTPool")
        .query(&[
            ("ut", ZTB_UT),
            ("dpt", "wz.ztzt"),
            ("Pageindex", "0"),
            ("pagesize", "10000"),
            ("sort", "fbt:asc"),
            ("date", requested.as_str()),
        ])
        .header("Referer", "https://quote.eastmoney.com/")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if let Some(rc) = root.get("rc").and_then(Value::as_i64) {
        if rc != 0 {
            return Err(anyhow!("东财涨停池返回业务错误 rc={rc}"));
        }
    }
    let data = root
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("东财涨停池缺少 data"))?;
    let qdate = text(data.get("qdate")).unwrap_or_default();
    if qdate.len() == 8 && requested > qdate {
        return Err(anyhow!(
            "东财还没有 {} 的涨停池数据(上游最新行情日 {})",
            requested,
            qdate
        ));
    }
    let pool = match data.get("pool") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items.clone(),
        _ => return Err(anyhow!("东财涨停池 pool 格式错误")),
    };

    let mut items = Vec::with_capacity(pool.len());
    for row in pool {
        let Some(obj) = row.as_object() else { continue };
        let code = text(obj.get("c")).unwrap_or_default();
        let name = text(obj.get("n")).unwrap_or_default();
        if code.is_empty() || name.is_empty() {
            continue;
        }
        let stat = obj.get("zttj").and_then(Value::as_object);
        let stat_days = stat.and_then(|v| value_i64(v.get("days")));
        let stat_count = stat.and_then(|v| value_i64(v.get("ct")));
        let zt_stat = match (stat_days, stat_count) {
            (Some(days), Some(count)) => format!("{days}天{count}板"),
            _ => "—".to_string(),
        };
        items.push(json!({
            "code": code,
            "name": name,
            "price": value_f64(obj.get("p")).map(|v| v / 1000.0),
            "pct": value_f64(obj.get("zdp")),
            "amount_yuan": value_f64(obj.get("amount")),
            "float_mcap_yuan": value_f64(obj.get("ltsz")),
            "turnover_pct": value_f64(obj.get("hs")),
            "limit_days": value_i64(obj.get("lbc")),
            "first_seal": value_i64(obj.get("fbt")).map(format_pool_time),
            "last_seal": value_i64(obj.get("lbt")).map(format_pool_time),
            "seal_fund_yuan": value_f64(obj.get("fund")),
            "break_times": value_i64(obj.get("zbc")),
            "industry": text(obj.get("hybk")).unwrap_or_default(),
            "zt_stat": zt_stat
        }));
    }

    items.sort_by(|a, b| {
        let a_days = a.get("limit_days").and_then(Value::as_i64).unwrap_or(0);
        let b_days = b.get("limit_days").and_then(Value::as_i64).unwrap_or(0);
        b_days.cmp(&a_days).then_with(|| {
            let a_time = a.get("first_seal").and_then(Value::as_str).unwrap_or("99:99:99");
            let b_time = b.get("first_seal").and_then(Value::as_str).unwrap_or("99:99:99");
            a_time.cmp(b_time)
        })
    });

    let actual_date = if qdate.len() == 8 { qdate } else { requested.clone() };
    Ok(json!({
        "status": "ok",
        "source": "eastmoney",
        "date": format_yyyymmdd(&actual_date),
        "requested_date": format_yyyymmdd(&requested),
        "stale": actual_date != china_yyyymmdd(),
        "count": items.len(),
        "items": items,
        "fetched_at": Utc::now().to_rfc3339()
    }))
}

fn guard_today_before_open(requested: &str) -> Result<()> {
    let offset = FixedOffset::east_opt(8 * 3600).expect("valid China timezone offset");
    let now = Utc::now().with_timezone(&offset);
    if requested == now.format("%Y%m%d").to_string()
        && (now.hour() < 9 || (now.hour() == 9 && now.minute() < 25))
    {
        return Err(anyhow!("还没到 09:25 集合竞价撮合,今天的涨停池还不可能有票"));
    }
    Ok(())
}

fn query_param(path: &str, key: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    query.split('&').find_map(|part| {
        let (k, v) = part.split_once('=')?;
        if k == key { Some(v.to_string()) } else { None }
    })
}

fn normalize_date(raw: String) -> Result<String> {
    let compact = raw.replace('-', "");
    if compact.len() == 8 && compact.chars().all(|c| c.is_ascii_digit()) {
        Ok(compact)
    } else {
        Err(anyhow!("date 必须是 YYYYMMDD 或 YYYY-MM-DD"))
    }
}

fn china_yyyymmdd() -> String {
    let offset = FixedOffset::east_opt(8 * 3600).expect("valid China timezone offset");
    Utc::now().with_timezone(&offset).format("%Y%m%d").to_string()
}

fn format_yyyymmdd(raw: &str) -> String {
    if raw.len() == 8 && raw.chars().all(|c| c.is_ascii_digit()) {
        format!("{}-{}-{}", &raw[0..4], &raw[4..6], &raw[6..8])
    } else {
        raw.to_string()
    }
}

fn format_pool_time(value: i64) -> String {
    let text = format!("{value:06}");
    format!("{}:{}:{}", &text[0..2], &text[2..4], &text[4..6])
}

fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(v) => Some(v.clone()),
        Value::Number(v) => Some(v.to_string()),
        _ => None,
    }
}

fn value_f64(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(v) => v.as_f64(),
        Value::String(v) => v.parse().ok(),
        _ => None,
    }
}

fn value_i64(value: Option<&Value>) -> Option<i64> {
    value_f64(value).map(|v| v as i64)
}
