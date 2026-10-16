use anyhow::{anyhow, Context};
use chrono::Utc;
use encoding_rs::GBK;
use reqwest::Client;
use serde_json::{json, Value};

use crate::symbol::{natural_market, Market};

#[derive(Debug)]
pub enum SearchError {
    BadRequest(String),
    Upstream(anyhow::Error),
}

impl From<anyhow::Error> for SearchError {
    fn from(value: anyhow::Error) -> Self {
        Self::Upstream(value)
    }
}

pub async fn symbols(client: &Client, body: &Value) -> std::result::Result<Value, SearchError> {
    let query = body
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if query.is_empty() {
        return Err(SearchError::BadRequest("搜索关键词不能为空".into()));
    }
    if query.chars().count() > 64 {
        return Err(SearchError::BadRequest("搜索关键词过长".into()));
    }

    let limit = body
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(8)
        .clamp(1, 20) as usize;

    let bytes = client
        .get("https://smartbox.gtimg.cn/s3/")
        .query(&[("v", "2"), ("t", "all"), ("c", "1"), ("q", query)])
        .send()
        .await
        .context("连接腾讯股票搜索失败")?
        .error_for_status()
        .context("腾讯股票搜索返回失败状态")?
        .bytes()
        .await
        .context("读取腾讯股票搜索响应失败")?;
    let (decoded, _, _) = GBK.decode(&bytes);
    let items = parse_suggestions(decoded.as_ref(), limit)?;
    let count = items.len();

    Ok(json!({
        "query": query,
        "items": items,
        "count": count,
        "source": "tencent_smartbox",
        "fetched_at": Utc::now().to_rfc3339()
    }))
}

fn parse_suggestions(text: &str, limit: usize) -> anyhow::Result<Vec<Value>> {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("v_hint="))
        .ok_or_else(|| anyhow!("腾讯股票搜索返回缺少 v_hint"))?;
    let literal = line
        .strip_prefix("v_hint=")
        .unwrap_or(line)
        .trim()
        .trim_end_matches(';');
    let payload: String = serde_json::from_str(literal)
        .context("腾讯股票搜索返回格式异常")?;
    if payload.is_empty() || payload.starts_with('N') {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for row in payload.split('^') {
        let fields: Vec<&str> = row.split('~').collect();
        if fields.len() < 5 {
            continue;
        }
        let exchange = fields[0].trim().to_ascii_lowercase();
        let code = fields[1].trim();
        let name = fields[2].trim();
        let pinyin = fields[3].trim();
        let category = fields[4].trim();

        let Some(market) = natural_market(code) else {
            continue;
        };
        if !matches!(market, Market::SH | Market::SZ) || market.tencent_prefix() != exchange {
            continue;
        }
        if name.is_empty() || !category.starts_with("GP-A") {
            continue;
        }

        items.push(json!({
            "symbol": code,
            "name": name,
            "market": market.as_str(),
            "pinyin": pinyin,
            "category": category,
            "source": "tencent_smartbox"
        }));
        if items.len() >= limit {
            break;
        }
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::parse_suggestions;

    #[test]
    fn parses_only_a_share_suggestions() {
        let raw = r#"v_hint="sh~600519~贵州茅台~GZMT~GP-A^sz~300308~中际旭创~ZJXC~GP-A^sh~000001~上证指数~SZZS~ZS^hk~00700~腾讯控股~TXKG~GP";"#;
        let items = parse_suggestions(raw, 8).expect("parse suggestions");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["symbol"], "600519");
        assert_eq!(items[0]["market"], "SH");
        assert_eq!(items[1]["symbol"], "300308");
    }

    #[test]
    fn rejects_exchange_code_mismatch() {
        let raw = r#"v_hint="sz~600519~贵州茅台~GZMT~GP-A^sh~300308~中际旭创~ZJXC~GP-A";"#;
        assert!(parse_suggestions(raw, 8)
            .expect("parse mismatched suggestions")
            .is_empty());
    }

    #[test]
    fn respects_limit_and_no_result_marker() {
        let raw = r#"v_hint="sh~600519~贵州茅台~GZMT~GP-A^sz~000001~平安银行~PAYH~GP-A";"#;
        assert_eq!(parse_suggestions(raw, 1).expect("parse suggestions").len(), 1);
        assert!(parse_suggestions(r#"v_hint="N";"#, 8)
            .expect("parse empty suggestions")
            .is_empty());
    }
}
