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

pub const GROUP_MAX_CHARS: usize = 12;
pub const TAG_MAX_CHARS: usize = 8;
pub const TAGS_MAX: usize = 5;

/// 分组、标签是用户自己写的:去空白、不含逗号和换行,超长/超个数拒收,不截断
pub fn clean_group(raw: &str) -> Result<String> {
    let text = raw.trim();
    if text.chars().count() > GROUP_MAX_CHARS {
        return Err(anyhow!("分组名最多 {GROUP_MAX_CHARS} 个字"));
    }
    if text.contains(|c: char| c == ',' || c == '，' || c.is_control()) {
        return Err(anyhow!("分组名不能含逗号或换行"));
    }
    Ok(text.to_string())
}

pub fn clean_tags(raw: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for tag in raw {
        let text = tag.trim();
        if text.is_empty() || out.iter().any(|t| t == text) {
            continue;
        }
        if text.chars().count() > TAG_MAX_CHARS {
            return Err(anyhow!("单个标签最多 {TAG_MAX_CHARS} 个字:{text}"));
        }
        if text.contains(|c: char| c == ',' || c == '，' || c.is_control()) {
            return Err(anyhow!("标签不能含逗号或换行:{text}"));
        }
        out.push(text.to_string());
    }
    if out.len() > TAGS_MAX {
        return Err(anyhow!("每只最多 {TAGS_MAX} 个标签"));
    }
    Ok(out)
}

pub fn tags_of(stored: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(stored).unwrap_or_default()
}

pub fn list(db_path: &Path) -> Result<Value> {
    let conn = open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT symbol, COALESCE(name, ''), created_at, updated_at, COALESCE(grp, ''), COALESCE(tags, ''), sort_order
         FROM watchlist ORDER BY sort_order IS NULL, sort_order, created_at ASC, symbol ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let tags: String = row.get(5)?;
        Ok(json!({
            "symbol": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "created_at": row.get::<_, String>(2)?,
            "updated_at": row.get::<_, String>(3)?,
            "group": row.get::<_, String>(4)?,
            "tags": tags_of(&tags),
            "sort_order": row.get::<_, Option<i64>>(6)?,
        }))
    })?;
    let items = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    let mut groups: Vec<String> = Vec::new();
    for item in &items {
        let group = item["group"].as_str().unwrap_or("");
        if !group.is_empty() && !groups.iter().any(|g| g == group) {
            groups.push(group.to_string());
        }
    }
    Ok(json!({ "items": items, "groups": groups }))
}

/// 改一只自选的分组与标签(只改这两项,不动名称和顺序)
pub fn set_meta(db_path: &Path, body: &Value) -> Result<Value> {
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
    let group = clean_group(body.get("group").and_then(Value::as_str).unwrap_or(""))?;
    let tags_in: Vec<String> = match body.get("tags") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(ToOwned::to_owned))
            .collect(),
        Some(Value::String(text)) => text
            .split([',', '，', ' ', '\n'])
            .map(ToOwned::to_owned)
            .collect(),
        _ => Vec::new(),
    };
    let tags = clean_tags(&tags_in)?;
    let conn = open(db_path)?;
    let changed = conn.execute(
        "UPDATE watchlist SET grp = ?2, tags = ?3, updated_at = ?4 WHERE symbol = ?1",
        params![
            symbol,
            group,
            serde_json::to_string(&tags)?,
            Utc::now().to_rfc3339()
        ],
    )?;
    if changed == 0 {
        return Err(anyhow!("自选里没有 {symbol}"));
    }
    list(db_path)
}

/// 自定义顺序:必须给出自选里的全部代码(不多不少、不重复),按这个先后存;不接受只排一部分
pub fn set_order(db_path: &Path, body: &Value) -> Result<Value> {
    let requested: Vec<String> = body
        .get("symbols")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("symbols 必须是代码数组"))?
        .iter()
        .map(|v| {
            let raw = v.as_str().unwrap_or("");
            normalize_stock_symbol(raw)
                .map(|key| key.digits)
                .map_err(|error| anyhow!("无效代码 {raw}: {error}"))
        })
        .collect::<Result<_>>()?;
    let mut conn = open(db_path)?;
    let tx = conn.transaction()?;
    let mut existing: Vec<String> = tx
        .prepare("SELECT symbol FROM watchlist")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut asked = requested.clone();
    existing.sort();
    asked.sort();
    if asked.windows(2).any(|w| w[0] == w[1]) {
        return Err(anyhow!("顺序里有重复代码"));
    }
    if existing != asked {
        return Err(anyhow!(
            "顺序必须正好包含自选里的全部 {} 只代码",
            existing.len()
        ));
    }
    for (index, symbol) in requested.iter().enumerate() {
        tx.execute(
            "UPDATE watchlist SET sort_order = ?2 WHERE symbol = ?1",
            params![symbol, index as i64],
        )?;
    }
    tx.commit()?;
    list(db_path)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn db(name: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("vibe-storage-{name}-{nonce}.sqlite3"));
        init(&path).unwrap();
        crate::journal::init(&path).unwrap();
        crate::daily_review::init(&path).unwrap();
        crate::ai_chat::init(&path).unwrap();
        crate::ai_config::init(&path).unwrap();
        crate::schema::migrate(&path).unwrap();
        path
    }

    #[test]
    fn meta_validation_rejects_instead_of_truncating() {
        assert!(clean_group(&"字".repeat(13))
            .unwrap_err()
            .to_string()
            .contains("12"));
        assert!(clean_group("白酒,消费").is_err());
        assert_eq!(clean_group("  白酒 ").unwrap(), "白酒");
        assert!(clean_tags(&["字".repeat(9)]).is_err());
        assert!(clean_tags(&[
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
            "f".into()
        ])
        .is_err());
        assert_eq!(
            clean_tags(&["核心".into(), " ".into(), "核心".into(), "龙头".into()]).unwrap(),
            vec!["核心", "龙头"]
        );
    }

    #[test]
    fn order_must_name_every_watchlist_symbol_exactly_once() {
        let path = db("order");
        for symbol in ["600519", "000001"] {
            upsert(&path, &json!({"symbol": symbol, "name": symbol})).unwrap();
        }
        assert!(set_order(&path, &json!({"symbols": ["600519"]}))
            .unwrap_err()
            .to_string()
            .contains("全部 2 只"));
        assert!(set_order(&path, &json!({"symbols": ["600519", "600519"]})).is_err());
        assert!(
            set_order(&path, &json!({"symbols": ["600519", "300308"]})).is_err(),
            "不在自选里的代码不收"
        );
        assert!(set_meta(&path, &json!({"symbol": "300308", "group": "x"}))
            .unwrap_err()
            .to_string()
            .contains("自选里没有"));
        set_order(&path, &json!({"symbols": ["000001", "600519"]})).unwrap();
        assert_eq!(list(&path).unwrap()["items"][0]["symbol"], "000001");
        // 之后新加的排在末尾,已有顺序不变
        upsert(&path, &json!({"symbol": "300308", "name": "新"})).unwrap();
        let items = list(&path).unwrap();
        let order: Vec<&str> = items["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["symbol"].as_str().unwrap())
            .collect();
        assert_eq!(order, ["000001", "600519", "300308"]);
        let _ = std::fs::remove_file(path);
    }
}
