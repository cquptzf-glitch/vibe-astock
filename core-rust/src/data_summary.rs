use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{fs, path::{Path, PathBuf}};

fn open(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path).context("open mobile sqlite for data summary")
}

fn scalar_i64(conn: &Connection, sql: &str) -> Result<i64> {
    Ok(conn.query_row(sql, [], |row| row.get(0))?)
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn sidecar(db_path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{}", db_path.display(), suffix))
}

pub fn summary(db_path: &Path) -> Result<Value> {
    let conn = open(db_path)?;
    let watchlist = scalar_i64(&conn, "SELECT COUNT(*) FROM watchlist")?;
    let journal_entries = scalar_i64(&conn, "SELECT COUNT(*) FROM journal_entries")?;
    let journal_fills = scalar_i64(&conn, "SELECT COUNT(*) FROM journal_fills")?;
    let daily_reviews = scalar_i64(&conn, "SELECT COUNT(*) FROM mobile_daily_reviews")?;
    let ai_messages = scalar_i64(&conn, "SELECT COUNT(*) FROM mobile_ai_messages")?;
    let open_positions = scalar_i64(
        &conn,
        "SELECT COUNT(*) FROM (
            SELECT e.id
            FROM journal_entries e
            JOIN journal_fills f ON f.entry_id = e.id
            WHERE e.kind = 'episode'
            GROUP BY e.id
            HAVING SUM(CASE WHEN f.side = 'buy' THEN f.shares ELSE -f.shares END) > 0.000001
        )",
    )?;
    let ai_configured = scalar_i64(
        &conn,
        "SELECT COUNT(*) FROM mobile_ai_config
         WHERE id = 1 AND TRIM(base_url) <> '' AND TRIM(model) <> ''",
    )? > 0;
    let ai_key_present = scalar_i64(
        &conn,
        "SELECT COUNT(*) FROM mobile_ai_config
         WHERE id = 1 AND LENGTH(TRIM(api_key)) > 0",
    )? > 0;

    let db_bytes = file_size(db_path);
    let wal_bytes = file_size(&sidecar(db_path, "-wal"));
    let shm_bytes = file_size(&sidecar(db_path, "-shm"));

    Ok(json!({
        "source": "mobile_sqlite",
        "generated_at": Utc::now().to_rfc3339(),
        "storage": {
            "scope": "android_app_private",
            "database_bytes": db_bytes,
            "wal_bytes": wal_bytes,
            "shm_bytes": shm_bytes,
            "total_bytes": db_bytes + wal_bytes + shm_bytes
        },
        "counts": {
            "watchlist": watchlist,
            "journal_entries": journal_entries,
            "journal_fills": journal_fills,
            "daily_reviews": daily_reviews,
            "ai_messages": ai_messages,
            "open_positions": open_positions
        },
        "ai": {
            "configured": ai_configured,
            "key_present": ai_key_present,
            "storage": "sqlite_app_private",
            "key_echoed": false
        }
    }))
}
