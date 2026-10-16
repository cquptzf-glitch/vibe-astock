use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use reqwest::{Client, Url};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

use crate::journal_review;

const SYSTEM_PROMPT: &str = "你是交易复盘助手。你只能基于用户主动提供的历史复盘统计，分析交易纪律、样本特征、风险暴露和可验证的改进方向。不要预测未来价格，不要推荐具体股票，不要给出买入/卖出/加仓/减仓指令，不要虚构缺失数据。样本不足时必须明确指出。输出中文，优先使用：1. 核心发现；2. 纪律问题；3. 风险问题；4. 下一阶段可执行的复盘改进清单。";

#[derive(Debug, Clone)]
struct AiConfig {
    base_url: String,
    model: String,
    api_key: String,
    updated_at: String,
}

#[derive(Debug)]
pub enum RunError {
    BadRequest(String),
    Upstream(String),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for RunError {
    fn from(value: anyhow::Error) -> Self {
        Self::Internal(value)
    }
}

pub fn init(db_path: &Path) -> Result<()> {
    let conn = open(db_path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_ai_config(
            id INTEGER PRIMARY KEY CHECK(id = 1),
            base_url TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            api_key TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL
        );",
    )?;
    Ok(())
}

pub fn config_view(db_path: &Path) -> Result<Value> {
    let config = load_config(db_path)?;
    Ok(match config {
        Some(config) => public_config(&config),
        None => empty_config(),
    })
}

pub fn save_config(db_path: &Path, body: &Value) -> Result<Value> {
    let current = load_config(db_path)?;
    let base_url = match body.get("base_url") {
        Some(Value::String(value)) => normalize_base_url(value)?,
        Some(_) => return Err(anyhow!("base_url 必须是字符串")),
        None => current.as_ref().map(|value| value.base_url.clone()).unwrap_or_default(),
    };
    let model = match body.get("model") {
        Some(Value::String(value)) => normalize_model(value)?,
        Some(_) => return Err(anyhow!("model 必须是字符串")),
        None => current.as_ref().map(|value| value.model.clone()).unwrap_or_default(),
    };
    if base_url.is_empty() {
        return Err(anyhow!("请填写模型 API Base URL"));
    }
    if model.is_empty() {
        return Err(anyhow!("请填写模型名称"));
    }

    // Never carry a credential across provider endpoints implicitly. If the Base URL changes,
    // the old key is cleared unless the user explicitly supplies a new non-empty key.
    let base_changed = current.as_ref().is_some_and(|item| item.base_url != base_url);
    let existing_key = || current.as_ref().map(|item| item.api_key.clone()).unwrap_or_default();
    let clear_key = body.get("clear_key").and_then(Value::as_bool).unwrap_or(false);
    let api_key = if clear_key {
        String::new()
    } else if let Some(value) = body.get("api_key") {
        let value = value.as_str().ok_or_else(|| anyhow!("api_key 必须是字符串"))?.trim();
        if value.is_empty() {
            if base_changed { String::new() } else { existing_key() }
        } else {
            if value.len() > 4096 {
                return Err(anyhow!("API Key 过长"));
            }
            value.to_string()
        }
    } else if base_changed {
        String::new()
    } else {
        existing_key()
    };

    let updated_at = Utc::now().to_rfc3339();
    let conn = open(db_path)?;
    conn.execute(
        "INSERT INTO mobile_ai_config(id, base_url, model, api_key, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET
           base_url = excluded.base_url,
           model = excluded.model,
           api_key = excluded.api_key,
           updated_at = excluded.updated_at",
        params![base_url, model, api_key, updated_at],
    )?;
    config_view(db_path)
}

pub fn clear_config(db_path: &Path) -> Result<Value> {
    let conn = open(db_path)?;
    conn.execute("DELETE FROM mobile_ai_config WHERE id = 1", [])?;
    Ok(empty_config())
}

pub fn preview(db_path: &Path, body: &Value) -> Result<Value> {
    let include_positions = body.get("include_positions").and_then(Value::as_bool).unwrap_or(false);
    let focus = custom_focus(body)?;
    let review = journal_review::view(db_path)?;
    let payload = build_payload(&review, include_positions);
    let prompt = build_user_prompt(&payload, &focus)?;
    let fields = if include_positions {
        vec!["performance", "recent_10", "discipline", "playbooks", "risk", "positions"]
    } else {
        vec!["performance", "recent_10", "discipline", "playbooks", "risk"]
    };
    Ok(json!({
        "network_used": false,
        "scope": if include_positions { "aggregate_plus_open_positions" } else { "aggregate_only" },
        "include_positions": include_positions,
        "sent_fields": fields,
        "excluded": ["raw_fills", "journal_notes", "api_key"],
        "payload": payload,
        "prompt_characters": prompt.chars().count(),
        "custom_focus": if focus.is_empty() { Value::Null } else { Value::String(focus) }
    }))
}

pub async fn run(client: &Client, db_path: &Path, body: &Value) -> std::result::Result<Value, RunError> {
    let config = load_config(db_path)
        .map_err(RunError::Internal)?
        .ok_or_else(|| RunError::BadRequest("还没有配置模型 API".to_string()))?;
    if config.base_url.is_empty() || config.model.is_empty() {
        return Err(RunError::BadRequest("模型 API 配置不完整".to_string()));
    }

    let include_positions = body.get("include_positions").and_then(Value::as_bool).unwrap_or(false);
    let focus = custom_focus(body).map_err(RunError::Internal)?;
    let review = journal_review::view(db_path).map_err(RunError::Internal)?;
    let payload = build_payload(&review, include_positions);
    let prompt = build_user_prompt(&payload, &focus).map_err(RunError::Internal)?;
    let endpoint = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));

    let mut request = client
        .post(&endpoint)
        .timeout(Duration::from_secs(60))
        .header("Accept", "application/json")
        .json(&json!({
            "model": config.model,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": prompt }
            ]
        }));
    if !config.api_key.is_empty() {
        request = request.bearer_auth(&config.api_key);
    }

    let response = request
        .send()
        .await
        .map_err(|error| RunError::Upstream(format!("连接模型 API 失败: {error}")))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| RunError::Upstream(format!("读取模型响应失败: {error}")))?;
    if !status.is_success() {
        let short = text.chars().take(500).collect::<String>();
        return Err(RunError::Upstream(format!("模型 API 返回 HTTP {}: {}", status.as_u16(), short)));
    }

    let root: Value = serde_json::from_str(&text)
        .map_err(|error| RunError::Upstream(format!("模型响应不是有效 JSON: {error}")))?;
    let content = root
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RunError::Upstream("模型响应缺少 choices[0].message.content".to_string()))?;

    Ok(json!({
        "status": "ok",
        "protocol": "openai_compatible_chat_completions",
        "model": config.model,
        "base_url": config.base_url,
        "scope": if include_positions { "aggregate_plus_open_positions" } else { "aggregate_only" },
        "include_positions": include_positions,
        "sent_characters": prompt.chars().count(),
        "response": content,
        "generated_at": Utc::now().to_rfc3339()
    }))
}

fn load_config(db_path: &Path) -> Result<Option<AiConfig>> {
    let conn = open(db_path)?;
    conn.query_row(
        "SELECT base_url, model, api_key, updated_at FROM mobile_ai_config WHERE id = 1",
        [],
        |row| {
            Ok(AiConfig {
                base_url: row.get(0)?,
                model: row.get(1)?,
                api_key: row.get(2)?,
                updated_at: row.get(3)?,
            })
        },
    )
    .optional()
    .context("load mobile AI config")
}

fn public_config(config: &AiConfig) -> Value {
    json!({
        "protocol": "openai_compatible_chat_completions",
        "base_url": config.base_url,
        "model": config.model,
        "key_present": !config.api_key.is_empty(),
        "configured": !config.base_url.is_empty() && !config.model.is_empty(),
        "updated_at": config.updated_at,
        "storage": "android_private_sqlite",
        "key_echoed": false
    })
}

fn empty_config() -> Value {
    json!({
        "protocol": "openai_compatible_chat_completions",
        "base_url": "",
        "model": "",
        "key_present": false,
        "configured": false,
        "updated_at": null,
        "storage": "android_private_sqlite",
        "key_echoed": false
    })
}

fn build_payload(review: &Value, include_positions: bool) -> Value {
    let mut payload = json!({
        "performance": review.get("performance").cloned().unwrap_or(Value::Null),
        "recent_10": review.get("recent_10").cloned().unwrap_or(Value::Null),
        "discipline": review.get("discipline").cloned().unwrap_or(Value::Null),
        "playbooks": review.get("playbooks").cloned().unwrap_or(Value::Array(Vec::new())),
        "risk": review.get("risk").cloned().unwrap_or(Value::Null)
    });
    if include_positions {
        payload["positions"] = review.get("positions").cloned().unwrap_or(Value::Array(Vec::new()));
    }
    payload
}

fn build_user_prompt(payload: &Value, focus: &str) -> Result<String> {
    let data = serde_json::to_string_pretty(payload)?;
    let focus_line = if focus.is_empty() {
        "没有额外关注点。".to_string()
    } else {
        format!("用户额外关注点：{focus}")
    };
    Ok(format!(
        "请基于下面这份交易复盘统计做结构化复盘。\n{focus_line}\n\n统计数据(JSON)：\n{data}\n\n要求：明确区分事实、推断与样本不足；重点找纪律偏差、打法差异、风险未覆盖和可量化改进项。不要给具体个股买卖建议。"
    ))
}

fn custom_focus(body: &Value) -> Result<String> {
    let focus = body.get("focus").and_then(Value::as_str).unwrap_or("").trim();
    if focus.chars().count() > 1000 {
        return Err(anyhow!("额外关注点最多 1000 个字符"));
    }
    Ok(focus.to_string())
}

fn normalize_model(value: &str) -> Result<String> {
    let value = value.trim();
    if value.chars().count() > 200 {
        return Err(anyhow!("模型名称过长"));
    }
    Ok(value.to_string())
}

fn normalize_base_url(value: &str) -> Result<String> {
    let value = value.trim().trim_end_matches('/');
    if value.is_empty() {
        return Ok(String::new());
    }
    let url = Url::parse(value).map_err(|_| anyhow!("Base URL 不是有效 URL"))?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(anyhow!("Base URL 只支持 http 或 https"));
    }
    if url.host_str().is_none() {
        return Err(anyhow!("Base URL 缺少主机名"));
    }
    if !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err(anyhow!("Base URL 不应包含账号、密码、查询参数或片段"));
    }
    Ok(value.to_string())
}

fn open(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path).context("open mobile AI sqlite")
}
