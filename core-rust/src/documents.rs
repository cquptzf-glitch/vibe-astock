use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::Path;

const KINDS: [&str; 3] = ["note", "report", "inbox"];

pub fn list(db_path: &Path, path: &str) -> Result<Value> {
    let kind = query_param(path, "kind").unwrap_or_else(|| "note".to_string());
    validate_kind(&kind)?;
    let conn = open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT id, kind, category, title, body, status, source, symbol, created_at, updated_at
         FROM mobile_documents WHERE kind = ?1 ORDER BY updated_at DESC, id DESC",
    )?;
    let rows = stmt.query_map([&kind], |row| {
        Ok(json!({
            "id": row.get::<_, i64>(0)?,
            "kind": row.get::<_, String>(1)?,
            "category": row.get::<_, String>(2)?,
            "title": row.get::<_, String>(3)?,
            "body": row.get::<_, String>(4)?,
            "status": row.get::<_, String>(5)?,
            "source": row.get::<_, String>(6)?,
            "symbol": row.get::<_, Option<String>>(7)?,
            "created_at": row.get::<_, String>(8)?,
            "updated_at": row.get::<_, String>(9)?,
        }))
    })?;
    let items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({ "kind": kind, "count": items.len(), "items": items }))
}

pub fn save(db_path: &Path, body: &Value) -> Result<Value> {
    let kind = required_text(body, "kind", 32)?;
    validate_kind(&kind)?;
    let title = required_text(body, "title", 500)?;
    let document_body = optional_text(body, "body", 250_000)?.unwrap_or_default();
    let category = optional_text(body, "category", 100)?.unwrap_or_default();
    let status = optional_text(body, "status", 32)?.unwrap_or_else(|| {
        if kind == "inbox" { "unread".to_string() } else { "active".to_string() }
    });
    let source = optional_text(body, "source", 300)?.unwrap_or_else(|| "manual".to_string());
    let symbol = optional_text(body, "symbol", 32)?;
    if !matches!(status.as_str(), "active" | "archived" | "unread" | "read") {
        return Err(anyhow!("status 无效: {status}"));
    }

    let conn = open(db_path)?;
    let now = chrono::Utc::now().to_rfc3339();
    let id = body.get("id").and_then(Value::as_i64);
    let id = if let Some(id) = id {
        if id <= 0 {
            return Err(anyhow!("id 必须是正整数"));
        }
        let changed = conn.execute(
            "UPDATE mobile_documents
             SET category=?1, title=?2, body=?3, status=?4, source=?5, symbol=?6, updated_at=?7
             WHERE id=?8 AND kind=?9",
            params![category, title, document_body, status, source, symbol, now, id, kind],
        )?;
        if changed == 0 {
            return Err(anyhow!("文档不存在或类型不匹配: {id}"));
        }
        id
    } else {
        conn.execute(
            "INSERT INTO mobile_documents(kind, category, title, body, status, source, symbol, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![kind, category, title, document_body, status, source, symbol, now],
        )?;
        conn.last_insert_rowid()
    };

    get(&conn, id)
}

pub fn delete(db_path: &Path, body: &Value) -> Result<Value> {
    let id = body
        .get("id")
        .and_then(Value::as_i64)
        .filter(|id| *id > 0)
        .ok_or_else(|| anyhow!("id 必须是正整数"))?;
    let conn = open(db_path)?;
    let changed = conn.execute("DELETE FROM mobile_documents WHERE id=?1", [id])?;
    if changed == 0 {
        return Err(anyhow!("文档不存在: {id}"));
    }
    Ok(json!({ "deleted": true, "id": id }))
}

pub fn clear(db_path: &Path, body: &Value) -> Result<Value> {
    let kind = required_text(body, "kind", 32)?;
    validate_kind(&kind)?;
    if body.get("confirm").and_then(Value::as_bool) != Some(true) {
        return Err(anyhow!("清空文档前必须明确确认"));
    }
    let conn = open(db_path)?;
    let count = conn.execute("DELETE FROM mobile_documents WHERE kind=?1", [&kind])?;
    Ok(json!({ "cleared": true, "kind": kind, "count": count }))
}

fn get(conn: &Connection, id: i64) -> Result<Value> {
    conn.query_row(
        "SELECT id, kind, category, title, body, status, source, symbol, created_at, updated_at
         FROM mobile_documents WHERE id=?1",
        [id],
        |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "kind": row.get::<_, String>(1)?,
                "category": row.get::<_, String>(2)?,
                "title": row.get::<_, String>(3)?,
                "body": row.get::<_, String>(4)?,
                "status": row.get::<_, String>(5)?,
                "source": row.get::<_, String>(6)?,
                "symbol": row.get::<_, Option<String>>(7)?,
                "created_at": row.get::<_, String>(8)?,
                "updated_at": row.get::<_, String>(9)?,
            }))
        },
    )
    .context("read saved mobile document")
}

fn open(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path).context("open mobile documents sqlite")
}

fn validate_kind(kind: &str) -> Result<()> {
    if KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(anyhow!("kind 无效: {kind}"))
    }
}

fn required_text(body: &Value, key: &str, max: usize) -> Result<String> {
    optional_text(body, key, max)?.filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("{key} 不能为空"))
}

fn optional_text(body: &Value, key: &str, max: usize) -> Result<Option<String>> {
    let Some(value) = body.get(key) else { return Ok(None); };
    if value.is_null() { return Ok(None); }
    let text = value.as_str().ok_or_else(|| anyhow!("{key} 必须是字符串"))?.trim().to_string();
    if text.chars().count() > max {
        return Err(anyhow!("{key} 超过最大长度 {max}"));
    }
    Ok(Some(text))
}

fn query_param(path: &str, key: &str) -> Option<String> {
    path.split('?').nth(1).and_then(|query| {
        query.split('&').find_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some(found), Some(value)) if found == key => Some(value.to_string()),
                _ => None,
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};

    fn temp_db() -> std::path::PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("vibe-documents-{nonce}.sqlite3"));
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE mobile_documents(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL,
                body TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'active',
                source TEXT NOT NULL DEFAULT 'manual',
                symbol TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );"
        ).unwrap();
        path
    }

    #[test]
    fn create_update_delete_roundtrip() {
        let path = temp_db();
        let saved = save(&path, &json!({"kind":"note","title":"测试","body":"正文"})).unwrap();
        let id = saved["id"].as_i64().unwrap();
        assert_eq!(saved["title"], "测试");

        let updated = save(&path, &json!({"id":id,"kind":"note","title":"更新","body":"正文2"})).unwrap();
        assert_eq!(updated["title"], "更新");
        assert_eq!(list(&path, "/mobile/documents?kind=note").unwrap()["count"], 1);
        assert_eq!(delete(&path, &json!({"id":id})).unwrap()["deleted"], true);
        let _ = fs::remove_file(path);
    }
}
