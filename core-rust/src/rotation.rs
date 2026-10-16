use anyhow::{anyhow, Result};
use chrono::Utc;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashSet;

pub async fn snapshot(client: &Client, request_path: &str) -> Result<Value> {
    let board_type = query_param(request_path, "type").unwrap_or_else(|| "concept".to_string());
    let top_n = query_param(request_path, "top")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(5, 30);

    if board_type != "concept" && board_type != "industry" {
        return Err(anyhow!("type 必须是 concept 或 industry"));
    }

    let (today_total, mut today_rows) = board_rank(client, &board_type, "today", top_n).await?;
    let (five_day_total, mut five_day_rows) = board_rank(client, &board_type, "5d", top_n).await?;

    let today_codes = codes(&today_rows);
    let five_day_codes = codes(&five_day_rows);

    annotate(&mut today_rows, &five_day_codes, "new_hot");
    annotate(&mut five_day_rows, &today_codes, "cooling");

    let persistent = brief_rows(
        today_rows
            .iter()
            .filter(|row| row.get("rotation_signal").and_then(Value::as_str) == Some("persistent")),
    );
    let new_hot = brief_rows(
        today_rows
            .iter()
            .filter(|row| row.get("rotation_signal").and_then(Value::as_str) == Some("new_hot")),
    );
    let cooling = brief_rows(
        five_day_rows
            .iter()
            .filter(|row| row.get("rotation_signal").and_then(Value::as_str) == Some("cooling")),
    );

    Ok(json!({
        "status": "ok",
        "source": "eastmoney",
        "board_type": board_type,
        "fetched_at": Utc::now().to_rfc3339(),
        "summary": {
            "persistent_count": persistent.len(),
            "new_hot_count": new_hot.len(),
            "cooling_count": cooling.len(),
            "persistent": persistent,
            "new_hot": new_hot,
            "cooling": cooling
        },
        "today": {
            "period": "today",
            "total": today_total,
            "rows": today_rows
        },
        "five_day": {
            "period": "5d",
            "total": five_day_total,
            "rows": five_day_rows
        }
    }))
}

async fn board_rank(
    client: &Client,
    board_type: &str,
    period: &str,
    top_n: usize,
) -> Result<(usize, Vec<Value>)> {
    let fs = match board_type {
        "concept" => "m:90+t:3",
        "industry" => "m:90+t:2",
        _ => return Err(anyhow!("unsupported board type")),
    };

    let (fid, main_field, main_pct_field, change_field, leader_field) = match period {
        "today" => ("f62", "f62", "f184", "f3", "f204"),
        "5d" => ("f164", "f164", "f165", "f109", "f257"),
        _ => return Err(anyhow!("unsupported period")),
    };

    let fields = format!(
        "f12,f14,{change_field},{main_field},{main_pct_field},{leader_field},f104,f105"
    );
    let params = vec![
        ("pn", "1".to_string()),
        ("pz", "100".to_string()),
        ("po", "1".to_string()),
        ("np", "1".to_string()),
        ("fltt", "2".to_string()),
        ("invt", "2".to_string()),
        ("fid", fid.to_string()),
        ("fs", fs.to_string()),
        ("fields", fields),
    ];

    let root: Value = client
        .get("https://push2.eastmoney.com/api/qt/clist/get")
        .query(&params)
        .header("Referer", "https://quote.eastmoney.com/")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if let Some(rc) = root.get("rc").and_then(Value::as_i64) {
        if rc != 0 {
            return Err(anyhow!("东财板块排行返回业务错误 rc={rc}"));
        }
    }

    let data = root
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("东财板块排行缺少 data"))?;
    let total = value_usize(data.get("total")).unwrap_or(0);
    let items = diff_items(data.get("diff"));

    let mut rows = Vec::with_capacity(top_n.min(items.len()));
    for (index, item) in items.into_iter().take(top_n).enumerate() {
        let Some(obj) = item.as_object() else { continue };
        let code = text(obj.get("f12")).unwrap_or_default();
        let name = text(obj.get("f14")).unwrap_or_default();
        if code.is_empty() || name.is_empty() {
            continue;
        }

        rows.push(json!({
            "rank": index + 1,
            "code": code,
            "name": name,
            "change_pct": value_f64(obj.get(change_field)),
            "main_net_yuan": value_f64(obj.get(main_field)),
            "main_pct": value_f64(obj.get(main_pct_field)),
            "leader": text(obj.get(leader_field)).unwrap_or_default(),
            "up_count": value_i64(obj.get("f104")),
            "down_count": value_i64(obj.get("f105")),
            "rotation_signal": null
        }));
    }

    Ok((total.max(rows.len()), rows))
}

fn annotate(rows: &mut [Value], other_codes: &HashSet<String>, missing_signal: &str) {
    for row in rows {
        let code = row
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let signal = if other_codes.contains(&code) {
            "persistent"
        } else {
            missing_signal
        };
        if let Some(obj) = row.as_object_mut() {
            obj.insert("rotation_signal".to_string(), Value::String(signal.to_string()));
        }
    }
}

fn codes(rows: &[Value]) -> HashSet<String> {
    rows.iter()
        .filter_map(|row| row.get("code").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn brief_rows<'a>(rows: impl Iterator<Item = &'a Value>) -> Vec<Value> {
    rows.take(6)
        .map(|row| {
            json!({
                "code": row.get("code").cloned().unwrap_or(Value::Null),
                "name": row.get("name").cloned().unwrap_or(Value::Null),
                "rank": row.get("rank").cloned().unwrap_or(Value::Null),
                "change_pct": row.get("change_pct").cloned().unwrap_or(Value::Null),
                "main_net_yuan": row.get("main_net_yuan").cloned().unwrap_or(Value::Null)
            })
        })
        .collect()
}

fn diff_items(value: Option<&Value>) -> Vec<Value> {
    match value {
        Some(Value::Array(items)) => items.clone(),
        Some(Value::Object(items)) => items.values().cloned().collect(),
        _ => Vec::new(),
    }
}

fn query_param(path: &str, key: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    query.split('&').find_map(|part| {
        let (k, v) = part.split_once('=')?;
        if k == key { Some(v.to_string()) } else { None }
    })
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
    value_f64(value).map(|value| value as i64)
}

fn value_usize(value: Option<&Value>) -> Option<usize> {
    value_i64(value).and_then(|value| usize::try_from(value).ok())
}
