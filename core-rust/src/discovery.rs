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

/// 同时在途的取数请求上限:详情页一次发十几路请求时,不把手机的网络和上游一起压满
const MAX_IN_FLIGHT: usize = 8;
/// 失败后最多再试几次(只重试网络错误、超时、429 和 5xx;4xx 这类「请求本身有问题」不重试)
const RETRIES: usize = 2;
const RETRY_BASE_MS: u64 = 300;

fn limiter() -> &'static tokio::sync::Semaphore {
    static LIMITER: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();
    LIMITER.get_or_init(|| tokio::sync::Semaphore::new(MAX_IN_FLIGHT))
}

/// 一次尝试:Err 里带「值不值得重试」
async fn get_bytes_once(
    client: &Client,
    url: &str,
    params: &[(&str, String)],
) -> std::result::Result<Vec<u8>, (bool, anyhow::Error)> {
    let response = client.get(url).query(params)
        .timeout(Duration::from_secs(12))
        .header("Accept", "application/json, application/rss+xml, application/atom+xml, application/xml;q=0.9, */*;q=0.5")
        .send().await
        .map_err(|e| (true, anyhow::Error::from(e)))?;
    // 保留 reqwest::Error(带状态码):`failure()` 靠它写「数据源返回 HTTP xxx」
    let mut response = response.error_for_status().map_err(|e| {
        let retry = e
            .status()
            .is_some_and(|s| s.as_u16() == 429 || s.is_server_error());
        (retry, anyhow::Error::from(e))
    })?;
    const MAX_BYTES: usize = 2 * 1024 * 1024;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_BYTES as u64)
    {
        return Err((false, anyhow!("数据源响应超过 2 MiB")));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| (true, anyhow::Error::from(e)))?
    {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err((false, anyhow!("数据源响应超过 2 MiB")));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// 统一取数入口:并发上限 + 瞬时错误退避重试(300ms、600ms)。幂等的 GET 才走这里
pub async fn get_bytes(client: &Client, url: &str, params: &[(&str, String)]) -> Result<Vec<u8>> {
    let _permit = limiter()
        .acquire()
        .await
        .map_err(|_| anyhow!("取数通道已关闭"))?;
    let mut attempt = 0usize;
    loop {
        match get_bytes_once(client, url, params).await {
            Ok(bytes) => return Ok(bytes),
            Err((retry, error)) => {
                if !retry || attempt >= RETRIES {
                    return Err(if attempt > 0 {
                        error.context(format!("已重试 {attempt} 次仍失败"))
                    } else {
                        error
                    });
                }
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(RETRY_BASE_MS * attempt as u64)).await;
            }
        }
    }
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

#[cfg(test)]
mod retry_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// 本地模拟上游:按顺序给每个连接一个状态码和正文,返回 (地址, 连接计数线程)
    fn serve(responses: Vec<(u16, &'static str)>) -> (String, std::thread::JoinHandle<usize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/data", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut served = 0;
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let reply = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).unwrap();
                served += 1;
            }
            served
        });
        (url, handle)
    }

    #[tokio::test]
    async fn transient_server_errors_are_retried_until_success() {
        let (url, server) = serve(vec![(503, "busy"), (502, "bad"), (200, "{\"ok\":true}")]);
        let bytes = get_bytes(&Client::new(), &url, &[]).await.unwrap();
        assert_eq!(bytes, b"{\"ok\":true}");
        assert_eq!(server.join().unwrap(), 3, "两次失败 + 一次成功");
    }

    #[tokio::test]
    async fn client_errors_are_not_retried_and_keep_their_status_for_failure_messages() {
        let (url, server) = serve(vec![(404, "nope")]);
        let error = get_bytes(&Client::new(), &url, &[]).await.unwrap_err();
        assert_eq!(server.join().unwrap(), 1, "404 只请求一次");
        assert_eq!(failure("x", &error)["message"], "数据源返回 HTTP 404");
    }

    #[tokio::test]
    async fn gives_up_after_the_retry_budget_and_says_so() {
        let (url, server) = serve(vec![(500, "a"), (500, "b"), (500, "c")]);
        let error = get_bytes(&Client::new(), &url, &[]).await.unwrap_err();
        assert_eq!(server.join().unwrap(), 3, "首次 + 2 次重试");
        assert!(format!("{error:#}").contains("已重试 2 次"), "{error:#}");
        assert_eq!(
            failure("x", &error)["message"],
            "数据源返回 HTTP 500",
            "包了重试说明后状态码仍能取到"
        );
    }
}
