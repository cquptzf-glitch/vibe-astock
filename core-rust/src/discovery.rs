use anyhow::{anyhow, Result};
use reqwest::{Client, Url};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug)]
pub enum DiscoveryError {
    BadRequest(String),
    Upstream(anyhow::Error),
}

impl From<anyhow::Error> for DiscoveryError {
    fn from(error: anyhow::Error) -> Self {
        Self::Upstream(error)
    }
}

pub fn parameter(path: &str, key: &str) -> Option<String> {
    Url::parse(&format!("https://mobile.invalid{path}"))
        .ok()?
        .query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
}

pub fn integer(
    path: &str,
    key: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize, DiscoveryError> {
    let Some(raw) = parameter(path, key) else {
        return Ok(default);
    };
    let value = raw
        .parse::<usize>()
        .map_err(|_| DiscoveryError::BadRequest(format!("{key} 必须是整数")))?;
    if !(min..=max).contains(&value) {
        return Err(DiscoveryError::BadRequest(format!(
            "{key} 必须在 {min}..={max} 之间"
        )));
    }
    Ok(value)
}

pub async fn get_bytes(client: &Client, url: &str, params: &[(&str, String)]) -> Result<Vec<u8>> {
    let mut response = client.get(url).query(params)
        .timeout(Duration::from_secs(12))
        .header("Accept", "application/json, application/rss+xml, application/atom+xml, application/xml;q=0.9, */*;q=0.5")
        .send().await?.error_for_status()?;
    const MAX_BYTES: usize = 2 * 1024 * 1024;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_BYTES as u64)
    {
        return Err(anyhow!("数据源响应超过 2 MiB"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err(anyhow!("数据源响应超过 2 MiB"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn get_json(client: &Client, url: &str, params: &[(&str, String)]) -> Result<Value> {
    let bytes = get_bytes(client, url, params).await?;
    serde_json::from_slice(&bytes).map_err(Into::into)
}

pub fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|value| value.is_finite())
}

pub fn provider_rows(root: &Value) -> Result<Vec<Value>> {
    if root
        .get("rc")
        .and_then(Value::as_i64)
        .is_some_and(|rc| rc != 0)
    {
        return Err(anyhow!("行情源返回业务错误"));
    }
    let data = root
        .get("data")
        .filter(|data| data.is_object())
        .ok_or_else(|| anyhow!("行情源缺少 data"))?;
    match data.get("diff") {
        Some(Value::Array(rows)) => Ok(rows.clone()),
        Some(Value::Object(rows)) => Ok(rows.values().cloned().collect()),
        None | Some(Value::Null) if data.get("total").and_then(Value::as_u64) == Some(0) => {
            Ok(Vec::new())
        }
        _ => Err(anyhow!("行情源缺少 diff 列表")),
    }
}

pub fn failure(part: &str, error: &anyhow::Error) -> Value {
    // Static provider requests contain no user credential or local business data.
    let message = if error
        .downcast_ref::<reqwest::Error>()
        .is_some_and(|error| error.is_timeout())
    {
        "数据源请求超时".to_string()
    } else if let Some(status) = error
        .downcast_ref::<reqwest::Error>()
        .and_then(reqwest::Error::status)
    {
        format!("数据源返回 HTTP {}", status.as_u16())
    } else {
        "数据源暂不可用或响应格式已变化".to_string()
    };
    serde_json::json!({"source": part, "message": message})
}
