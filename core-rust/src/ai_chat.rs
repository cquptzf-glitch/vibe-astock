use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use std::{path::Path, time::Duration};

const SYSTEM_PROMPT: &str = "你是 Vibe AStock 的投研助手。回答用户明确输入的问题，区分事实、推断和不确定性；涉及时效性数据时，如果消息中没有数据就明确说明不能凭空知道最新值。不要声称读取了用户的自选、持仓、交易日志或其他本地数据，因为通用聊天默认没有这些权限。避免把一般研究讨论包装成确定收益承诺或确定性买卖结论。使用中文，表达简洁、结构清楚。";
const MAX_MESSAGE_CHARS: usize = 8_000;
const MAX_CONTEXT_MESSAGES: usize = 20;
const MAX_CONTEXT_CHARS: usize = 30_000;

#[derive(Debug)]
pub enum ChatError {
    BadRequest(String),
    Upstream(String),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for ChatError {
    fn from(value: anyhow::Error) -> Self {
        Self::Internal(value)
    }
}

#[derive(Debug)]
struct Config {
    base_url: String,
    model: String,
    api_key: String,
}

#[derive(Debug, Clone, Serialize)]
struct ChatMessage {
    id: i64,
    role: String,
    content: String,
    created_at: String,
}

pub fn init(db_path: &Path) -> Result<()> {
    let conn = open(db_path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_ai_messages(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            role TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
            content TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
         CREATE INDEX IF NOT EXISTS idx_mobile_ai_messages_id ON mobile_ai_messages(id);",
    )?;
    Ok(())
}

pub fn history(db_path: &Path) -> Result<Value> {
    let conn = open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT id, role, content, created_at
         FROM mobile_ai_messages
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ChatMessage {
            id: row.get(0)?,
            role: row.get(1)?,
            content: row.get(2)?,
            created_at: row.get(3)?,
        })
    })?;
    let messages = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    let count = messages.len();
    Ok(json!({
        "messages": messages,
        "count": count,
        "storage": "mobile_sqlite",
        "local_data_attached": false
    }))
}

pub fn clear(db_path: &Path) -> Result<Value> {
    let conn = open(db_path)?;
    let removed = conn.execute("DELETE FROM mobile_ai_messages", [])?;
    Ok(json!({ "removed": removed, "messages": [], "count": 0 }))
}

pub async fn send(client: &Client, db_path: &Path, body: &Value) -> std::result::Result<Value, ChatError> {
    let content = body
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if content.is_empty() {
        return Err(ChatError::BadRequest("聊天内容不能为空".to_string()));
    }
    if content.chars().count() > MAX_MESSAGE_CHARS {
        return Err(ChatError::BadRequest(format!("单条消息最多 {MAX_MESSAGE_CHARS} 个字符")));
    }

    let config = load_config(db_path)
        .map_err(ChatError::Internal)?
        .ok_or_else(|| ChatError::BadRequest("还没有配置模型 API".to_string()))?;
    if config.base_url.is_empty() || config.model.is_empty() {
        return Err(ChatError::BadRequest("模型 API 配置不完整".to_string()));
    }

    let context = context_messages(db_path).map_err(ChatError::Internal)?;
    let mut messages = Vec::with_capacity(context.len() + 2);
    messages.push(json!({ "role": "system", "content": SYSTEM_PROMPT }));
    for item in &context {
        messages.push(json!({ "role": &item.role, "content": &item.content }));
    }
    messages.push(json!({ "role": "user", "content": content }));

    let endpoint = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let mut request = client
        .post(&endpoint)
        .timeout(Duration::from_secs(60))
        .header("Accept", "application/json")
        .json(&json!({
            "model": &config.model,
            "messages": messages
        }));
    if !config.api_key.is_empty() {
        request = request.bearer_auth(&config.api_key);
    }

    let response = request
        .send()
        .await
        .map_err(|error| ChatError::Upstream(format!("连接模型 API 失败: {error}")))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| ChatError::Upstream(format!("读取模型响应失败: {error}")))?;
    if !status.is_success() {
        let short = text.chars().take(500).collect::<String>();
        return Err(ChatError::Upstream(format!("模型 API 返回 HTTP {}: {}", status.as_u16(), short)));
    }

    let root: Value = serde_json::from_str(&text)
        .map_err(|error| ChatError::Upstream(format!("模型响应不是有效 JSON: {error}")))?;
    let answer = root
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChatError::Upstream("模型响应缺少 choices[0].message.content".to_string()))?;

    let now = Utc::now().to_rfc3339();
    let mut conn = open(db_path).map_err(ChatError::Internal)?;
    let tx = conn.transaction().map_err(|error| ChatError::Internal(error.into()))?;
    tx.execute(
        "INSERT INTO mobile_ai_messages(role, content, created_at) VALUES ('user', ?1, ?2)",
        params![content, now],
    )
    .map_err(|error| ChatError::Internal(error.into()))?;
    tx.execute(
        "INSERT INTO mobile_ai_messages(role, content, created_at) VALUES ('assistant', ?1, ?2)",
        params![answer, Utc::now().to_rfc3339()],
    )
    .map_err(|error| ChatError::Internal(error.into()))?;
    tx.commit().map_err(|error| ChatError::Internal(error.into()))?;

    let result = history(db_path).map_err(ChatError::Internal)?;
    Ok(json!({
        "status": "ok",
        "protocol": "openai_compatible_chat_completions",
        "model": config.model,
        "base_url": config.base_url,
        "sent_characters": content.chars().count(),
        "local_data_attached": false,
        "response": answer,
        "history": result.get("messages").cloned().unwrap_or_else(|| json!([])),
        "generated_at": Utc::now().to_rfc3339()
    }))
}

fn context_messages(db_path: &Path) -> Result<Vec<ChatMessage>> {
    let conn = open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT id, role, content, created_at
         FROM mobile_ai_messages
         ORDER BY id DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![MAX_CONTEXT_MESSAGES as i64], |row| {
        Ok(ChatMessage {
            id: row.get(0)?,
            role: row.get(1)?,
            content: row.get(2)?,
            created_at: row.get(3)?,
        })
    })?;
    let mut items = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    items.reverse();

    let mut total = 0usize;
    let mut kept = Vec::new();
    for item in items.into_iter().rev() {
        let chars = item.content.chars().count();
        if total + chars > MAX_CONTEXT_CHARS && !kept.is_empty() {
            break;
        }
        total += chars;
        kept.push(item);
    }
    kept.reverse();
    Ok(kept)
}

fn load_config(db_path: &Path) -> Result<Option<Config>> {
    let conn = open(db_path)?;
    conn.query_row(
        "SELECT base_url, model, api_key FROM mobile_ai_config WHERE id = 1",
        [],
        |row| {
            Ok(Config {
                base_url: row.get(0)?,
                model: row.get(1)?,
                api_key: row.get(2)?,
            })
        },
    )
    .optional()
    .context("load mobile AI config for chat")
}

fn open(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path).context("open mobile AI chat sqlite")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db() -> std::path::PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("vibe-ai-chat-{stamp}.sqlite3"))
    }

    #[test]
    fn history_is_local_and_clearable() {
        let path = temp_db();
        init(&path).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO mobile_ai_messages(role, content, created_at) VALUES ('user', 'hello', 'now')",
            [],
        ).unwrap();
        drop(conn);
        let current = history(&path).unwrap();
        assert_eq!(current["count"], 1);
        assert_eq!(current["local_data_attached"], false);
        let cleared = clear(&path).unwrap();
        assert_eq!(cleared["removed"], 1);
        let _ = std::fs::remove_file(path);
    }
}
