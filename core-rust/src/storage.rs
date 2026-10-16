use crate::symbol::normalize_stock_symbol;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::Path;

pub fn init(db_path: &Path) -> Result<()> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("open mobile sqlite: {}", db_path.display()))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS watchlist (
           symbol TEXT PRIMARY KEY NOT NULL,
           name TEXT,
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL
         );",
    )?;
    Ok(())
}

fn open(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path).context("open mobile sqlite")
}

pub fn list(db_path: &Path) -> Result<Value> {
    let conn = open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT symbol, COALESCE(name, ''), created_at, updated_at
         FROM watchlist ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(json!({
            "symbol": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "created_at": row.get::<_, String>(2)?,
            "updated_at": row.get::<_, String>(3)?,
        }))
    })?;
    let items = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(json!({ "items": items }))
}

pub fn upsert(db_path: &Path, body: &Value) -> Result<Value> {
    let raw_symbol = body
        .get("symbol")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if raw_symbol.is_empty() {
        return Err(anyhow!("watchlist symbol is required"));
    }
    let symbol = normalize_stock_symbol(raw_symbol)
        .map_err(|error| anyhow!("invalid watchlist symbol: {error}"))?
        .digits;
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let now = Utc::now().to_rfc3339();
    let conn = open(db_path)?;
    conn.execute(
        "INSERT INTO watchlist(symbol, name, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?3)
         ON CONFLICT(symbol) DO UPDATE
         SET name = excluded.name, updated_at = excluded.updated_at",
        params![symbol, name, now],
    )?;
    list(db_path)
}

pub fn delete(db_path: &Path, body: &Value) -> Result<Value> {
    let raw_symbol = body
        .get("symbol")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if raw_symbol.is_empty() {
        return Err(anyhow!("watchlist symbol is required"));
    }
    let symbol = normalize_stock_symbol(raw_symbol)
        .map_err(|error| anyhow!("invalid watchlist symbol: {error}"))?
        .digits;
    let conn = open(db_path)?;
    let removed = conn.execute("DELETE FROM watchlist WHERE symbol = ?1", params![symbol])? > 0;
    Ok(json!({ "removed": removed, "symbol": symbol }))
}
