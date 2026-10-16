use crate::symbol::normalize_stock_symbol;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::Path;

const KINDS: [&str; 5] = [
    "price_above",
    "price_below",
    "stop_near",
    "target_near",
    "daily_review",
];

pub fn list(db_path: &Path) -> Result<Value> {
    let conn = open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT id, kind, symbol, name, threshold, tolerance_pct, enabled,
                cooldown_minutes, review_hour, quote_key, created_at, updated_at
         FROM mobile_reminders
         ORDER BY enabled DESC, updated_at DESC, id DESC",
    )?;
    let rows = stmt.query_map([], row_json)?;
    let items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({ "count": items.len(), "items": items }))
}

pub fn native_snapshot(db_path: &Path) -> Result<Value> {
    let conn = open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT id, kind, symbol, name, threshold, tolerance_pct, enabled,
                cooldown_minutes, review_hour, quote_key, created_at, updated_at
         FROM mobile_reminders
         WHERE enabled = 1
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], row_json)?;
    let mut items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    for item in &mut items {
        item["route"] = Value::String(notification_route(item));
    }
    Ok(json!({
        "schema": 1,
        "generated_at": Utc::now().to_rfc3339(),
        "rules": items,
    }))
}

fn notification_route(rule: &Value) -> String {
    if rule["kind"] == "daily_review" {
        return "/mobile/journal/daily".to_string();
    }
    rule["symbol"]
        .as_str()
        .and_then(|raw| normalize_stock_symbol(raw).ok())
        .map(|key| format!("/mobile/stock/{}", key.normalized))
        .unwrap_or_else(|| "/mobile/reminders".to_string())
}

pub fn save(db_path: &Path, body: &Value) -> Result<Value> {
    let kind = required_text(body, "kind", 32)?;
    if !KINDS.contains(&kind.as_str()) {
        return Err(anyhow!("kind 无效: {kind}"));
    }

    let enabled = body.get("enabled").and_then(Value::as_bool).unwrap_or(true);
    let cooldown_minutes = body
        .get("cooldown_minutes")
        .and_then(Value::as_i64)
        .unwrap_or(60)
        .clamp(15, 10_080);
    let tolerance_pct = body
        .get("tolerance_pct")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    if !tolerance_pct.is_finite() || !(0.05..=20.0).contains(&tolerance_pct) {
        return Err(anyhow!("tolerance_pct 必须在 0.05..=20 之间"));
    }

    let raw_symbol = optional_text(body, "symbol", 32)?;
    let raw_name = optional_text(body, "name", 300)?.unwrap_or_default();
    let threshold = body.get("threshold").and_then(Value::as_f64);
    let review_hour = body.get("review_hour").and_then(Value::as_i64);

    let (symbol, name, quote_key, threshold, review_hour) = if kind == "daily_review" {
        let hour = review_hour.unwrap_or(16);
        if !(0..=23).contains(&hour) {
            return Err(anyhow!("review_hour 必须在 0..=23 之间"));
        }
        (None, raw_name, None, None, Some(hour))
    } else {
        let raw_symbol = raw_symbol
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("股票提醒必须填写 symbol"))?;
        let key = normalize_stock_symbol(raw_symbol)?;
        let threshold = threshold
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| anyhow!("股票提醒必须填写大于 0 的 threshold"))?;
        (
            Some(key.digits),
            raw_name,
            Some(key.tencent),
            Some(threshold),
            None,
        )
    };

    let conn = open(db_path)?;
    let now = Utc::now().to_rfc3339();
    let id = if let Some(id) = body.get("id").and_then(Value::as_i64) {
        if id <= 0 {
            return Err(anyhow!("id 必须是正整数"));
        }
        let changed = conn.execute(
            "UPDATE mobile_reminders
             SET kind=?1, symbol=?2, name=?3, threshold=?4, tolerance_pct=?5,
                 enabled=?6, cooldown_minutes=?7, review_hour=?8, quote_key=?9, updated_at=?10
             WHERE id=?11",
            params![
                kind,
                symbol,
                name,
                threshold,
                tolerance_pct,
                if enabled { 1_i64 } else { 0_i64 },
                cooldown_minutes,
                review_hour,
                quote_key,
                now,
                id,
            ],
        )?;
        if changed == 0 {
            return Err(anyhow!("提醒规则不存在: {id}"));
        }
        id
    } else {
        conn.execute(
            "INSERT INTO mobile_reminders(
                kind, symbol, name, threshold, tolerance_pct, enabled,
                cooldown_minutes, review_hour, quote_key, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
            params![
                kind,
                symbol,
                name,
                threshold,
                tolerance_pct,
                if enabled { 1_i64 } else { 0_i64 },
                cooldown_minutes,
                review_hour,
                quote_key,
                now,
            ],
        )?;
        conn.last_insert_rowid()
    };

    get(&conn, id)
}

pub fn delete(db_path: &Path, body: &Value) -> Result<Value> {
    let id = body
        .get("id")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or_else(|| anyhow!("id 必须是正整数"))?;
    let conn = open(db_path)?;
    let changed = conn.execute("DELETE FROM mobile_reminders WHERE id=?1", [id])?;
    if changed == 0 {
        return Err(anyhow!("提醒规则不存在: {id}"));
    }
    Ok(json!({ "deleted": true, "id": id }))
}

fn get(conn: &Connection, id: i64) -> Result<Value> {
    conn.query_row(
        "SELECT id, kind, symbol, name, threshold, tolerance_pct, enabled,
                cooldown_minutes, review_hour, quote_key, created_at, updated_at
         FROM mobile_reminders WHERE id=?1",
        [id],
        row_json,
    )
    .context("read saved reminder rule")
}

fn row_json(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": row.get::<_, i64>(0)?,
        "kind": row.get::<_, String>(1)?,
        "symbol": row.get::<_, Option<String>>(2)?,
        "name": row.get::<_, String>(3)?,
        "threshold": row.get::<_, Option<f64>>(4)?,
        "tolerance_pct": row.get::<_, f64>(5)?,
        "enabled": row.get::<_, i64>(6)? != 0,
        "cooldown_minutes": row.get::<_, i64>(7)?,
        "review_hour": row.get::<_, Option<i64>>(8)?,
        "quote_key": row.get::<_, Option<String>>(9)?,
        "created_at": row.get::<_, String>(10)?,
        "updated_at": row.get::<_, String>(11)?,
    }))
}

fn open(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path).context("open mobile reminders sqlite")
}

fn required_text(body: &Value, key: &str, max: usize) -> Result<String> {
    optional_text(body, key, max)?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("{key} 不能为空"))
}

fn optional_text(body: &Value, key: &str, max: usize) -> Result<Option<String>> {
    let Some(value) = body.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let text = value
        .as_str()
        .ok_or_else(|| anyhow!("{key} 必须是字符串"))?
        .trim()
        .to_string();
    if text.chars().count() > max {
        return Err(anyhow!("{key} 超过最大长度 {max}"));
    }
    Ok(Some(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};

    fn temp_db() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("vibe-reminders-{nonce}.sqlite3"));
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE mobile_reminders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                symbol TEXT,
                name TEXT NOT NULL DEFAULT '',
                threshold REAL,
                tolerance_pct REAL NOT NULL DEFAULT 1.0,
                enabled INTEGER NOT NULL DEFAULT 1,
                cooldown_minutes INTEGER NOT NULL DEFAULT 60,
                review_hour INTEGER,
                quote_key TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
        )
        .unwrap();
        path
    }

    #[test]
    fn stock_rule_normalizes_symbol_and_quote_key() {
        let path = temp_db();
        let saved = save(
            &path,
            &json!({
                "kind": "price_above",
                "symbol": "600519.SH",
                "name": "贵州茅台",
                "threshold": 1800.0
            }),
        )
        .unwrap();
        assert_eq!(saved["symbol"], "600519");
        assert_eq!(saved["quote_key"], "sh600519");
        let snapshot = native_snapshot(&path).unwrap();
        assert_eq!(snapshot["rules"].as_array().unwrap().len(), 1);
        assert_eq!(snapshot["rules"][0]["route"], "/mobile/stock/SH600519");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn notification_routes_use_shared_symbol_normalization() {
        for (symbol, expected) in [
            ("600519", "SH600519"),
            ("000001.SZ", "SZ000001"),
            ("430047", "BJ430047"),
            ("920002", "BJ920002"),
        ] {
            assert_eq!(
                notification_route(&json!({"kind": "price_above", "symbol": symbol})),
                format!("/mobile/stock/{expected}")
            );
        }
        assert_eq!(notification_route(&json!({"symbol": "SZ600519"})), "/mobile/reminders");
        assert_eq!(notification_route(&json!({"symbol": "../settings"})), "/mobile/reminders");
        assert_eq!(notification_route(&json!({"kind": "daily_review"})), "/mobile/journal/daily");
    }

    #[test]
    fn daily_review_rule_has_no_quote_key() {
        let path = temp_db();
        let saved = save(
            &path,
            &json!({"kind":"daily_review","name":"收盘复盘","review_hour":17}),
        )
        .unwrap();
        assert!(saved["symbol"].is_null());
        assert_eq!(saved["review_hour"], 17);
        let id = saved["id"].as_i64().unwrap();
        assert_eq!(delete(&path, &json!({"id": id})).unwrap()["deleted"], true);
        let _ = fs::remove_file(path);
    }
}
