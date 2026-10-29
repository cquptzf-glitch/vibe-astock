use crate::symbol::normalize_stock_symbol;
use anyhow::{anyhow, Context, Result};
use chrono::{NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::Path;

const KINDS: [&str; 4] = ["company", "ledger", "macro", "reminder"];

pub fn list(db_path: &Path, path: &str) -> Result<Value> {
    list_on(db_path, path, crate::emotion::now_china().0)
}

fn list_on(db_path: &Path, path: &str, today: NaiveDate) -> Result<Value> {
    let from = query_param(path, "from")
        .map(|value| parse_date(&value, "from"))
        .transpose()?
        .unwrap_or_else(|| today - chrono::Duration::days(7));
    let to = query_param(path, "to")
        .map(|value| parse_date(&value, "to"))
        .transpose()?
        .unwrap_or_else(|| today + chrono::Duration::days(90));
    if to < from {
        return Err(anyhow!("to 不能早于 from"));
    }
    if (to - from).num_days() > 370 {
        return Err(anyhow!("单次日历查询范围不能超过 370 天"));
    }

    let conn = open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT id, date, kind, title, detail, symbol, status, source, created_at, updated_at
         FROM mobile_calendar_events
         WHERE date >= ?1 AND date <= ?2
         ORDER BY date ASC, status ASC, id ASC",
    )?;
    let today_text = today.format("%Y-%m-%d").to_string();
    let rows = stmt.query_map(params![from.format("%Y-%m-%d").to_string(), to.format("%Y-%m-%d").to_string()], |row| {
        let date: String = row.get(1)?;
        let status: String = row.get(6)?;
        let overdue = date < today_text && status != "done";
        Ok(json!({
            "id": row.get::<_, i64>(0)?,
            "date": date,
            "kind": row.get::<_, String>(2)?,
            "title": row.get::<_, String>(3)?,
            "detail": row.get::<_, String>(4)?,
            "symbol": row.get::<_, Option<String>>(5)?,
            "status": status,
            "source": row.get::<_, String>(7)?,
            "created_at": row.get::<_, String>(8)?,
            "updated_at": row.get::<_, String>(9)?,
            "overdue": overdue,
        }))
    })?;
    let items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(json!({
        "from": from.format("%Y-%m-%d").to_string(),
        "to": to.format("%Y-%m-%d").to_string(),
        "today": today_text,
        "count": items.len(),
        "items": items,
    }))
}

pub fn save(db_path: &Path, body: &Value) -> Result<Value> {
    let date = required_text(body, "date", 10)?;
    parse_date(&date, "date")?;
    let kind = required_text(body, "kind", 32)?;
    if !KINDS.contains(&kind.as_str()) {
        return Err(anyhow!("kind 无效: {kind}"));
    }
    let title = required_text(body, "title", 500)?;
    let detail = optional_text(body, "detail", 20_000)?.unwrap_or_default();
    let status = optional_text(body, "status", 16)?.unwrap_or_else(|| "open".to_string());
    if status != "open" && status != "done" {
        return Err(anyhow!("status 无效: {status}"));
    }
    let source = optional_text(body, "source", 300)?.unwrap_or_else(|| "manual".to_string());
    let symbol = match optional_text(body, "symbol", 32)? {
        Some(value) if !value.trim().is_empty() => Some(normalize_stock_symbol(&value)?.digits),
        _ => None,
    };

    let conn = open(db_path)?;
    let now = Utc::now().to_rfc3339();
    let id = if let Some(id) = body.get("id").and_then(Value::as_i64) {
        if id <= 0 {
            return Err(anyhow!("id 必须是正整数"));
        }
        let changed = conn.execute(
            "UPDATE mobile_calendar_events
             SET date=?1, kind=?2, title=?3, detail=?4, symbol=?5, status=?6, source=?7, updated_at=?8
             WHERE id=?9",
            params![date, kind, title, detail, symbol, status, source, now, id],
        )?;
        if changed == 0 {
            return Err(anyhow!("日历事件不存在: {id}"));
        }
        id
    } else {
        conn.execute(
            "INSERT INTO mobile_calendar_events(date, kind, title, detail, symbol, status, source, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![date, kind, title, detail, symbol, status, source, now],
        )?;
        conn.last_insert_rowid()
    };
    get(&conn, id)
}

pub fn delete(db_path: &Path, body: &Value) -> Result<Value> {
    let id = body.get("id").and_then(Value::as_i64).filter(|id| *id > 0)
        .ok_or_else(|| anyhow!("id 必须是正整数"))?;
    let conn = open(db_path)?;
    let changed = conn.execute("DELETE FROM mobile_calendar_events WHERE id=?1", [id])?;
    if changed == 0 {
        return Err(anyhow!("日历事件不存在: {id}"));
    }
    Ok(json!({"deleted": true, "id": id}))
}

fn get(conn: &Connection, id: i64) -> Result<Value> {
    conn.query_row(
        "SELECT id, date, kind, title, detail, symbol, status, source, created_at, updated_at
         FROM mobile_calendar_events WHERE id=?1",
        [id],
        |row| Ok(json!({
            "id": row.get::<_, i64>(0)?,
            "date": row.get::<_, String>(1)?,
            "kind": row.get::<_, String>(2)?,
            "title": row.get::<_, String>(3)?,
            "detail": row.get::<_, String>(4)?,
            "symbol": row.get::<_, Option<String>>(5)?,
            "status": row.get::<_, String>(6)?,
            "source": row.get::<_, String>(7)?,
            "created_at": row.get::<_, String>(8)?,
            "updated_at": row.get::<_, String>(9)?,
        })),
    ).context("read saved calendar event")
}

fn open(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path).context("open mobile calendar sqlite")
}

fn parse_date(value: &str, label: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .with_context(|| format!("{label} 必须是 YYYY-MM-DD"))
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
    use rusqlite::Connection;
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};

    fn temp_db() -> std::path::PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("vibe-calendar-{nonce}.sqlite3"));
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE mobile_calendar_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                detail TEXT NOT NULL DEFAULT '',
                symbol TEXT,
                status TEXT NOT NULL DEFAULT 'open',
                source TEXT NOT NULL DEFAULT 'manual',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );"
        ).unwrap();
        path
    }

    #[test]
    fn china_midnight_changes_today_window_and_overdue_status() {
        use chrono::{FixedOffset, TimeZone};
        let path = temp_db();
        save(&path, &json!({"date":"2026-10-28", "kind":"reminder", "title":"昨天"})).unwrap();
        save(&path, &json!({"date":"2026-10-29", "kind":"reminder", "title":"今天"})).unwrap();
        let utc = Utc.with_ymd_and_hms(2026, 10, 28, 16, 30, 0).single().unwrap();
        let china_date = utc.with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap()).date_naive();
        let listed = list_on(&path, "/mobile/calendar", china_date).unwrap();
        assert_eq!(listed["today"], "2026-10-29");
        assert_eq!(listed["from"], "2026-10-22");
        assert_eq!(listed["items"][0]["overdue"], true);
        assert_eq!(listed["items"][1]["overdue"], false);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn crud_roundtrip() {
        let path = temp_db();
        let saved = save(&path, &json!({
            "date":"2026-10-20", "kind":"reminder", "title":"验证假设", "symbol":"600519"
        })).unwrap();
        let id = saved["id"].as_i64().unwrap();
        assert_eq!(saved["symbol"], "600519");
        let updated = save(&path, &json!({
            "id":id, "date":"2026-10-21", "kind":"reminder", "title":"验证完成", "status":"done"
        })).unwrap();
        assert_eq!(updated["status"], "done");
        let listed = list(&path, "/mobile/calendar?from=2026-10-01&to=2026-10-31").unwrap();
        assert_eq!(listed["count"], 1);
        assert_eq!(delete(&path, &json!({"id":id})).unwrap()["deleted"], true);
        let _ = fs::remove_file(path);
    }
}
