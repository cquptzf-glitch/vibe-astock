use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::Path;

pub const CURRENT_SCHEMA_VERSION: i64 = 1;

const REQUIRED_TABLES: [&str; 6] = [
    "watchlist",
    "journal_entries",
    "journal_fills",
    "mobile_daily_reviews",
    "mobile_ai_config",
    "mobile_ai_messages",
];

pub fn migrate(db_path: &Path) -> Result<()> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("open mobile sqlite for schema migration: {}", db_path.display()))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;

    let version = user_version(&conn)?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(anyhow!(
            "database schema {version} is newer than supported schema {CURRENT_SCHEMA_VERSION}"
        ));
    }

    match version {
        0 => migrate_0_to_1(&conn)?,
        CURRENT_SCHEMA_VERSION => verify_required_tables(&conn)?,
        other => {
            return Err(anyhow!(
                "unsupported database schema {other}; expected 0..={CURRENT_SCHEMA_VERSION}"
            ));
        }
    }
    Ok(())
}

pub fn info(db_path: &Path) -> Result<Value> {
    let conn = Connection::open(db_path).context("open mobile sqlite for schema info")?;
    let version = user_version(&conn)?;
    let missing = missing_required_tables(&conn)?;
    Ok(json!({
        "version": version,
        "current_version": CURRENT_SCHEMA_VERSION,
        "up_to_date": version == CURRENT_SCHEMA_VERSION && missing.is_empty(),
        "missing_tables": missing,
    }))
}

fn migrate_0_to_1(conn: &Connection) -> Result<()> {
    // Existing Android MVP builds created their tables in module initializers but
    // did not record a schema version. Validate that legacy layout first, then
    // adopt it as schema v1 without destroying user data.
    verify_required_tables(conn)?;
    conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;
    Ok(())
}

fn verify_required_tables(conn: &Connection) -> Result<()> {
    let missing = missing_required_tables(conn)?;
    if missing.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "mobile database is missing required tables: {}",
            missing.join(", ")
        ))
    }
}

fn missing_required_tables(conn: &Connection) -> Result<Vec<String>> {
    let mut missing = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT EXISTS(
             SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
         )",
    )?;
    for table in REQUIRED_TABLES {
        let exists: i64 = stmt.query_row([table], |row| row.get(0))?;
        if exists == 0 {
            missing.push(table.to_string());
        }
    }
    Ok(missing)
}

fn user_version(conn: &Connection) -> Result<i64> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .context("read PRAGMA user_version")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};

    fn temp_db(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("vibe-astock-{name}-{nonce}.sqlite3"))
    }

    #[test]
    fn adopts_legacy_unversioned_schema_without_deleting_data() {
        let path = temp_db("schema-adopt");
        let conn = Connection::open(&path).expect("open temp db");
        for table in REQUIRED_TABLES {
            conn.execute_batch(&format!("CREATE TABLE {table}(id INTEGER);"))
                .expect("create required table");
        }
        conn.execute("INSERT INTO watchlist(id) VALUES (42)", [])
            .expect("insert sentinel");
        drop(conn);

        migrate(&path).expect("migrate legacy schema");
        let conn = Connection::open(&path).expect("reopen temp db");
        assert_eq!(user_version(&conn).expect("user_version"), CURRENT_SCHEMA_VERSION);
        let sentinel: i64 = conn
            .query_row("SELECT id FROM watchlist", [], |row| row.get(0))
            .expect("sentinel preserved");
        assert_eq!(sentinel, 42);
        drop(conn);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_future_schema() {
        let path = temp_db("schema-future");
        let conn = Connection::open(&path).expect("open temp db");
        conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
            .expect("set future version");
        drop(conn);

        let error = migrate(&path).expect_err("future schema must fail");
        assert!(error.to_string().contains("newer than supported"));
        let _ = fs::remove_file(path);
    }
}
