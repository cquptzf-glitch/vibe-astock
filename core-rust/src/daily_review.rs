use anyhow::{anyhow, Context, Result};
use chrono::{FixedOffset, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;

fn open(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path).context("open mobile sqlite for daily review")
}

fn china_today() -> String {
    let offset = FixedOffset::east_opt(8 * 3600).expect("valid China timezone offset");
    Utc::now().with_timezone(&offset).format("%Y-%m-%d").to_string()
}

fn normalize_date(value: Option<&str>) -> Result<String> {
    let date = value.unwrap_or("").trim();
    if date.is_empty() {
        return Ok(china_today());
    }
    if date.len() != 10
        || date.as_bytes().get(4) != Some(&b'-')
        || date.as_bytes().get(7) != Some(&b'-')
        || !date.chars().enumerate().all(|(index, ch)| index == 4 || index == 7 || ch.is_ascii_digit())
    {
        return Err(anyhow!("复盘日期格式必须为 YYYY-MM-DD"));
    }
    Ok(date.to_string())
}

fn text(body: &Value, key: &str, max: usize) -> Result<String> {
    let value = body.get(key).and_then(Value::as_str).unwrap_or("").trim();
    if value.chars().count() > max {
        return Err(anyhow!("{key} 内容过长，最多 {max} 个字符"));
    }
    Ok(value.to_string())
}

pub fn init(db_path: &Path) -> Result<()> {
    let conn = open(db_path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_daily_reviews(
            date TEXT PRIMARY KEY NOT NULL,
            observation TEXT NOT NULL DEFAULT '',
            tomorrow_check TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );",
    )?;
    Ok(())
}

pub fn get(db_path: &Path, path: &str) -> Result<Value> {
    let date_param = path
        .split('?')
        .nth(1)
        .and_then(|query| query.split('&').find_map(|pair| {
            let mut pieces = pair.splitn(2, '=');
            match (pieces.next(), pieces.next()) {
                (Some("date"), Some(value)) => Some(value),
                _ => None,
            }
        }));
    let date = normalize_date(date_param)?;
    view(db_path, &date)
}

pub fn save(db_path: &Path, body: &Value) -> Result<Value> {
    let date = normalize_date(body.get("date").and_then(Value::as_str))?;
    let observation = text(body, "observation", 6000)?;
    let tomorrow_check = text(body, "tomorrow_check", 6000)?;
    let now = Utc::now().to_rfc3339();
    let conn = open(db_path)?;
    conn.execute(
        "INSERT INTO mobile_daily_reviews(date, observation, tomorrow_check, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)
         ON CONFLICT(date) DO UPDATE SET
           observation = excluded.observation,
           tomorrow_check = excluded.tomorrow_check,
           updated_at = excluded.updated_at",
        params![date, observation, tomorrow_check, now],
    )?;
    view(db_path, &date)
}

fn view(db_path: &Path, date: &str) -> Result<Value> {
    let conn = open(db_path)?;
    let row = conn
        .query_row(
            "SELECT observation, tomorrow_check, created_at, updated_at
             FROM mobile_daily_reviews WHERE date = ?1",
            params![date],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;

    Ok(match row {
        Some((observation, tomorrow_check, created_at, updated_at)) => json!({
            "date": date,
            "saved": true,
            "observation": observation,
            "tomorrow_check": tomorrow_check,
            "created_at": created_at,
            "updated_at": updated_at,
            "storage": "mobile_sqlite"
        }),
        None => json!({
            "date": date,
            "saved": false,
            "observation": "",
            "tomorrow_check": "",
            "created_at": null,
            "updated_at": null,
            "storage": "mobile_sqlite"
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db() -> std::path::PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("vibe-daily-review-{stamp}.sqlite3"))
    }

    #[test]
    fn saves_and_reads_daily_review() {
        let path = temp_db();
        init(&path).unwrap();
        let saved = save(&path, &json!({
            "date": "2026-10-16",
            "observation": "缩量分化",
            "tomorrow_check": "验证高标承接"
        })).unwrap();
        assert_eq!(saved["saved"], true);
        let loaded = get(&path, "/mobile/daily-review?date=2026-10-16").unwrap();
        assert_eq!(loaded["observation"], "缩量分化");
        assert_eq!(loaded["tomorrow_check"], "验证高标承接");
        let _ = std::fs::remove_file(path);
    }
}
