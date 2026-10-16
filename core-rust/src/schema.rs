use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::Path;

pub const CURRENT_SCHEMA_VERSION: i64 = 4;

const LEGACY_REQUIRED_TABLES: [&str; 6] = [
    "watchlist",
    "journal_entries",
    "journal_fills",
    "mobile_daily_reviews",
    "mobile_ai_config",
    "mobile_ai_messages",
];

const REQUIRED_V2_TABLES: [&str; 7] = [
    "watchlist",
    "journal_entries",
    "journal_fills",
    "mobile_daily_reviews",
    "mobile_ai_config",
    "mobile_ai_messages",
    "mobile_documents",
];

const REQUIRED_V3_TABLES: [&str; 8] = [
    "watchlist",
    "journal_entries",
    "journal_fills",
    "mobile_daily_reviews",
    "mobile_ai_config",
    "mobile_ai_messages",
    "mobile_documents",
    "mobile_calendar_events",
];

const REQUIRED_TABLES: [&str; 9] = [
    "watchlist",
    "journal_entries",
    "journal_fills",
    "mobile_daily_reviews",
    "mobile_ai_config",
    "mobile_ai_messages",
    "mobile_documents",
    "mobile_calendar_events",
    "mobile_reminders",
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
        0 => {
            verify_tables(&conn, &LEGACY_REQUIRED_TABLES)?;
            migrate_1_to_2(&conn)?;
            migrate_2_to_3(&conn)?;
            migrate_3_to_4(&conn)?;
        }
        1 => {
            migrate_1_to_2(&conn)?;
            migrate_2_to_3(&conn)?;
            migrate_3_to_4(&conn)?;
        }
        2 => {
            migrate_2_to_3(&conn)?;
            migrate_3_to_4(&conn)?;
        }
        3 => migrate_3_to_4(&conn)?,
        CURRENT_SCHEMA_VERSION => verify_tables(&conn, &REQUIRED_TABLES)?,
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
    let missing = missing_tables(&conn, &REQUIRED_TABLES)?;
    Ok(json!({
        "version": version,
        "current_version": CURRENT_SCHEMA_VERSION,
        "up_to_date": version == CURRENT_SCHEMA_VERSION && missing.is_empty(),
        "missing_tables": missing,
    }))
}

fn migrate_1_to_2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_documents (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL CHECK(kind IN ('note', 'report', 'inbox')),
            category TEXT NOT NULL DEFAULT '',
            title TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'active',
            source TEXT NOT NULL DEFAULT 'manual',
            symbol TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_mobile_documents_kind_updated
            ON mobile_documents(kind, updated_at DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_mobile_documents_symbol
            ON mobile_documents(symbol) WHERE symbol IS NOT NULL;",
    )?;
    verify_tables(conn, &REQUIRED_V2_TABLES)?;
    conn.pragma_update(None, "user_version", 2)?;
    Ok(())
}

fn migrate_2_to_3(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_calendar_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            date TEXT NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('company', 'ledger', 'macro', 'reminder')),
            title TEXT NOT NULL,
            detail TEXT NOT NULL DEFAULT '',
            symbol TEXT,
            status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open', 'done')),
            source TEXT NOT NULL DEFAULT 'manual',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_mobile_calendar_date
            ON mobile_calendar_events(date, status, id);
        CREATE INDEX IF NOT EXISTS idx_mobile_calendar_symbol
            ON mobile_calendar_events(symbol) WHERE symbol IS NOT NULL;",
    )?;
    verify_tables(conn, &REQUIRED_V3_TABLES)?;
    conn.pragma_update(None, "user_version", 3)?;
    Ok(())
}

fn migrate_3_to_4(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_reminders (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL CHECK(kind IN ('price_above', 'price_below', 'stop_near', 'target_near', 'daily_review')),
            symbol TEXT,
            name TEXT NOT NULL DEFAULT '',
            threshold REAL,
            tolerance_pct REAL NOT NULL DEFAULT 1.0,
            enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
            cooldown_minutes INTEGER NOT NULL DEFAULT 60,
            review_hour INTEGER,
            quote_key TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_mobile_reminders_enabled
            ON mobile_reminders(enabled, kind, id);
        CREATE INDEX IF NOT EXISTS idx_mobile_reminders_symbol
            ON mobile_reminders(symbol) WHERE symbol IS NOT NULL;",
    )?;
    verify_tables(conn, &REQUIRED_TABLES)?;
    conn.pragma_update(None, "user_version", 4)?;
    Ok(())
}

fn verify_tables(conn: &Connection, required: &[&str]) -> Result<()> {
    let missing = missing_tables(conn, required)?;
    if missing.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "mobile database is missing required tables: {}",
            missing.join(", ")
        ))
    }
}

fn missing_tables(conn: &Connection, required: &[&str]) -> Result<Vec<String>> {
    let mut missing = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT EXISTS(
             SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
         )",
    )?;
    for table in required {
        let exists: i64 = stmt.query_row([*table], |row| row.get(0))?;
        if exists == 0 {
            missing.push((*table).to_string());
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
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_db(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("vibe-astock-{name}-{nonce}.sqlite3"))
    }

    fn create_legacy_tables(conn: &Connection) {
        for table in LEGACY_REQUIRED_TABLES {
            conn.execute_batch(&format!("CREATE TABLE {table}(id INTEGER);"))
                .expect("create required table");
        }
    }

    #[test]
    fn adopts_legacy_unversioned_schema_without_deleting_data() {
        let path = temp_db("schema-adopt");
        let conn = Connection::open(&path).expect("open temp db");
        create_legacy_tables(&conn);
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
        assert!(missing_tables(&conn, &REQUIRED_TABLES).unwrap().is_empty());
        drop(conn);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn upgrades_every_supported_schema_to_current() {
        for version in [1_i64, 2, 3] {
            let path = temp_db(&format!("schema-{version}"));
            let conn = Connection::open(&path).unwrap();
            create_legacy_tables(&conn);
            if version >= 2 {
                migrate_1_to_2(&conn).unwrap();
            }
            if version >= 3 {
                migrate_2_to_3(&conn).unwrap();
            }
            conn.pragma_update(None, "user_version", version).unwrap();
            drop(conn);

            migrate(&path).unwrap();
            let conn = Connection::open(&path).unwrap();
            assert_eq!(user_version(&conn).unwrap(), CURRENT_SCHEMA_VERSION);
            assert!(missing_tables(&conn, &REQUIRED_TABLES).unwrap().is_empty());
            drop(conn);
            let _ = fs::remove_file(path);
        }
    }

    #[test]
    fn migration_is_idempotent() {
        let path = temp_db("schema-idempotent");
        let conn = Connection::open(&path).unwrap();
        create_legacy_tables(&conn);
        drop(conn);
        migrate(&path).unwrap();
        migrate(&path).unwrap();
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
