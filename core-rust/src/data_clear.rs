//! 统一清除全部本地数据:自选、交易日志、成交、日复盘、AI 聊天与模型配置、资料库、日历、提醒、验证记录、模式卡、分红、制度事件,
//! 以及所有行情缓存表。表结构和版本号(user_version)保留,清完应用还能直接用。
//!
//! 🔴 这里只清 SQLite。Android Keystore 里的 API Key 由 src-tauri 的命令层在同一次请求里删(Core 碰不到 Keystore),
//!    提醒调度也由命令层按清空后的规则重新同步(取消所有已排的提醒)。
//! 🔴 必须带 `confirm: true` 和确认短语,缺一不清;不提供「只清一部分」,要选择性删除请用各页面自己的删除。
//! 🔴 清之前不会自动备份:页面会先提示导出备份。

use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::Path;

pub const PHRASE: &str = "清除全部本地数据";

pub fn run(db: &Path, body: &Value) -> Result<Value> {
    if body.get("confirm").and_then(Value::as_bool) != Some(true) || body.get("phrase").and_then(Value::as_str) != Some(PHRASE) {
        return Err(anyhow!("清除全部本地数据需要 confirm=true 和确认短语「{PHRASE}」"));
    }
    let mut conn = Connection::open(db).context("open mobile sqlite")?;
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    let tx = conn.transaction()?;
    let mut cleared: Vec<Value> = Vec::new();
    let mut total = 0i64;
    for table in &tables {
        // 表名来自 sqlite_master,不是用户输入;仍然加引号
        let quoted = format!("\"{}\"", table.replace('"', "\"\""));
        let rows: i64 = tx.query_row(&format!("SELECT COUNT(*) FROM {quoted}"), [], |r| r.get(0))?;
        tx.execute(&format!("DELETE FROM {quoted}"), [])?;
        if rows > 0 {
            cleared.push(json!({"table": table, "rows": rows}));
        }
        total += rows;
    }
    // 自增编号从头开始
    let has_sequence: i64 = tx.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name = 'sqlite_sequence'", [], |r| r.get(0))?;
    if has_sequence > 0 {
        tx.execute("DELETE FROM sqlite_sequence", [])?;
    }
    tx.commit()?;
    conn.execute_batch("VACUUM;")?;
    Ok(json!({
        "status": "cleared", "tables": tables.len(), "rows_removed": total, "cleared": cleared,
        "note": "SQLite 里的业务数据与行情缓存已清空;Android Keystore 里的 API Key 与提醒调度由命令层同时清理;浏览器里的本机偏好由页面清理。",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn db(name: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("vibe-clear-{name}-{nonce}.sqlite3"));
        crate::storage::init(&path).unwrap();
        crate::journal::init(&path).unwrap();
        crate::daily_review::init(&path).unwrap();
        crate::ai_chat::init(&path).unwrap();
        crate::ai_config::init(&path).unwrap();
        crate::schema::migrate(&path).unwrap();
        path
    }

    #[test]
    fn refuses_without_the_exact_confirmation() {
        let path = db("refuse");
        crate::storage::upsert(&path, &json!({"symbol": "600519", "name": "x"})).unwrap();
        for body in [json!({}), json!({"confirm": true}), json!({"phrase": PHRASE}), json!({"confirm": true, "phrase": "清除"}), json!({"confirm": "true", "phrase": PHRASE})] {
            assert!(run(&path, &body).is_err(), "{body}");
        }
        assert_eq!(crate::storage::list(&path).unwrap()["items"].as_array().unwrap().len(), 1, "没确认就一行也不能少");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn clears_every_table_keeps_the_schema_and_the_app_keeps_working() {
        let path = db("all");
        crate::storage::upsert(&path, &json!({"symbol": "600519", "name": "贵州茅台"})).unwrap();
        crate::journal::act(&path, &json!({"action": "record", "symbol": "600519", "fills": [{"type": "buy", "date": "2026-02-02", "price": 1450.0, "shares": 100.0}]})).unwrap();
        crate::portfolio::act(&path, &json!({"action": "save", "symbol": "600519", "date": "2026-06-01", "amount": 300.0})).unwrap();
        crate::structure::act(&path, &json!({"action": "save", "date": "2026-01-01", "title": "规则"})).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute("INSERT INTO mobile_ai_messages(role, content, created_at) VALUES ('user', 'hi', 'a')", []).unwrap_or(0);
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        drop(conn);
        let done = run(&path, &json!({"confirm": true, "phrase": PHRASE})).unwrap();
        assert_eq!(done["status"], "cleared");
        assert!(done["rows_removed"].as_i64().unwrap() >= 5, "{done}");
        let conn = Connection::open(&path).unwrap();
        let tables: Vec<String> = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").unwrap().query_map([], |r| r.get(0)).unwrap().collect::<std::result::Result<_, _>>().unwrap();
        assert!(tables.len() >= 20, "表结构还在:{}", tables.len());
        for t in &tables {
            let n: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM \"{t}\""), [], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "{t}");
        }
        assert_eq!(conn.query_row::<i64, _, _>("PRAGMA user_version", [], |r| r.get(0)).unwrap(), version, "版本号保留");
        drop(conn);
        // 清完照常可用,自增编号从 1 开始
        let again = crate::journal::act(&path, &json!({"action": "record", "symbol": "000001", "fills": [{"type": "buy", "date": "2026-03-02", "price": 10.0, "shares": 100.0}]})).unwrap();
        assert_eq!(again["result"]["id"], 1);
        assert_eq!(crate::storage::list(&path).unwrap()["items"].as_array().unwrap().len(), 0);
        let _ = std::fs::remove_file(path);
    }
}
