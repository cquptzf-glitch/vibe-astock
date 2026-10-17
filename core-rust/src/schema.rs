use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::Path;

pub const CURRENT_SCHEMA_VERSION: i64 = 9;

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

const REQUIRED_V4_TABLES: [&str; 9] = [
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

const REQUIRED_V5_TABLES: [&str; 11] = [
    "watchlist",
    "journal_entries",
    "journal_fills",
    "mobile_daily_reviews",
    "mobile_ai_config",
    "mobile_ai_messages",
    "mobile_documents",
    "mobile_calendar_events",
    "mobile_reminders",
    "mobile_verifications",
    "mobile_shortline_days",
];

const REQUIRED_V6_TABLES: [&str; 17] = [
    "watchlist",
    "journal_entries",
    "journal_fills",
    "mobile_daily_reviews",
    "mobile_ai_config",
    "mobile_ai_messages",
    "mobile_documents",
    "mobile_calendar_events",
    "mobile_reminders",
    "mobile_verifications",
    "mobile_shortline_days",
    "mobile_watch_alerts",
    "mobile_watch_turnover",
    "mobile_intraday_snapshots",
    "mobile_intraday_status",
    "mobile_samples_days",
    "mobile_journal_settings",
];

/// v7 在 v6 之上新增:交易日志模式卡(用户数据,进备份)
const REQUIRED_TABLES: &[&str] = &[
    "watchlist",
    "journal_entries",
    "journal_fills",
    "mobile_daily_reviews",
    "mobile_ai_config",
    "mobile_ai_messages",
    "mobile_documents",
    "mobile_calendar_events",
    "mobile_reminders",
    "mobile_verifications",
    "mobile_shortline_days",
    "mobile_watch_alerts",
    "mobile_watch_turnover",
    "mobile_intraday_snapshots",
    "mobile_intraday_status",
    "mobile_samples_days",
    "mobile_journal_settings",
    "mobile_journal_modes",
    "mobile_journal_bars",
    "mobile_journal_env_days",
    "mobile_ths_reasons",
    "mobile_structure_days",
    "mobile_regime_events",
    "mobile_dividends",
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
            migrate_4_to_5(&conn)?;
            migrate_5_to_6(&conn)?;
            migrate_6_to_7(&conn)?;
            migrate_7_to_8(&conn)?;
            migrate_8_to_9(&conn)?;
        }
        1 => {
            migrate_1_to_2(&conn)?;
            migrate_2_to_3(&conn)?;
            migrate_3_to_4(&conn)?;
            migrate_4_to_5(&conn)?;
            migrate_5_to_6(&conn)?;
            migrate_6_to_7(&conn)?;
            migrate_7_to_8(&conn)?;
            migrate_8_to_9(&conn)?;
        }
        2 => {
            migrate_2_to_3(&conn)?;
            migrate_3_to_4(&conn)?;
            migrate_4_to_5(&conn)?;
            migrate_5_to_6(&conn)?;
            migrate_6_to_7(&conn)?;
            migrate_7_to_8(&conn)?;
            migrate_8_to_9(&conn)?;
        }
        3 => {
            migrate_3_to_4(&conn)?;
            migrate_4_to_5(&conn)?;
            migrate_5_to_6(&conn)?;
            migrate_6_to_7(&conn)?;
            migrate_7_to_8(&conn)?;
            migrate_8_to_9(&conn)?;
        }
        4 => {
            migrate_4_to_5(&conn)?;
            migrate_5_to_6(&conn)?;
            migrate_6_to_7(&conn)?;
            migrate_7_to_8(&conn)?;
            migrate_8_to_9(&conn)?;
        }
        5 => {
            migrate_5_to_6(&conn)?;
            migrate_6_to_7(&conn)?;
            migrate_7_to_8(&conn)?;
            migrate_8_to_9(&conn)?;
        }
        6 => {
            migrate_6_to_7(&conn)?;
            migrate_7_to_8(&conn)?;
            migrate_8_to_9(&conn)?;
        }
        7 => {
            migrate_7_to_8(&conn)?;
            migrate_8_to_9(&conn)?;
        }
        8 => migrate_8_to_9(&conn)?,
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
    verify_tables(conn, &REQUIRED_V4_TABLES)?;
    conn.pragma_update(None, "user_version", 4)?;
    Ok(())
}

/// 明日验证条件(用户写的市场读数判断,进备份)与已收盘交易日的情绪读数缓存(公开行情,不进备份)。
/// 东财池子只留约一个月,缓存让情绪历史越用越长。
fn migrate_4_to_5(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_verifications (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            date TEXT NOT NULL,
            metric TEXT NOT NULL,
            direction TEXT NOT NULL CHECK(direction IN ('up', 'down', 'flat')),
            reason TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(date, metric)
        );
        CREATE TABLE IF NOT EXISTS mobile_shortline_days (
            date TEXT PRIMARY KEY,
            payload TEXT NOT NULL,
            fetched_at TEXT NOT NULL
        );",
    )?;
    verify_tables(conn, &REQUIRED_V5_TABLES)?;
    conn.pragma_update(None, "user_version", 5)?;
    Ok(())
}

/// 盯盘异动记录(公开行情事件,只留 30 天)、收盘后记下的成交额前十(公开行情缓存)、
/// 竞价与盘中时点快照及各时点的存取状态(机器存的公开盘面数据,只留一年)、
/// 涨停样本每个已收盘交易日的逐只明细缓存(公开行情,东财池子只留约一个月,缓存让样本越用越长)。这几张都不进备份。
/// 交易日志设置(费率、规矩、账户规模)是用户自己的口径,进备份。
fn migrate_5_to_6(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_watch_alerts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            day TEXT NOT NULL,
            ts TEXT NOT NULL,
            code TEXT NOT NULL,
            name TEXT NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('急拉', '急跌', '封板', '开板')),
            change_pct REAL,
            price REAL,
            pct REAL,
            msg TEXT NOT NULL,
            sources TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS mobile_watch_alerts_day ON mobile_watch_alerts(day);
        CREATE TABLE IF NOT EXISTS mobile_watch_turnover (
            date TEXT PRIMARY KEY,
            payload TEXT NOT NULL,
            fetched_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS mobile_intraday_snapshots (
            date TEXT NOT NULL,
            slot TEXT NOT NULL,
            payload TEXT NOT NULL,
            captured_at TEXT NOT NULL,
            PRIMARY KEY(date, slot)
        );
        CREATE TABLE IF NOT EXISTS mobile_intraday_status (
            date TEXT NOT NULL,
            slot TEXT NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('saved', 'failed', 'skipped')),
            reason TEXT,
            at TEXT NOT NULL,
            tries INTEGER NOT NULL,
            PRIMARY KEY(date, slot)
        );
        CREATE TABLE IF NOT EXISTS mobile_samples_days (
            date TEXT PRIMARY KEY,
            payload TEXT NOT NULL,
            fetched_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS mobile_journal_settings (
            id INTEGER PRIMARY KEY CHECK(id = 1),
            fees TEXT,
            rules TEXT,
            equity_base REAL,
            updated_at TEXT NOT NULL
        );",
    )?;
    verify_tables(conn, &REQUIRED_V6_TABLES)?;
    conn.pragma_update(None, "user_version", 6)?;
    Ok(())
}

/// 交易日志模式卡(用户自己写的打法与各版本,进备份 v7)。版本放在 versions 里(JSON 数组),
/// 开新版本只追加、旧版本不动;生效日、打法、五段文字都在版本里。
/// 最大浮盈浮亏的逐笔日线缓存(公开行情,按「代码 + 买入日 + 卖出日」存,取到永久保留;不进备份,丢了会重取)。
/// 入场环境分组用的逐日涨停 / 炸板池成员缓存(公开行情,已收盘且两张池都取到的日子才存;不进备份)。
fn migrate_6_to_7(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_journal_modes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            playbook TEXT NOT NULL,
            versions TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS mobile_journal_bars (
            symbol TEXT NOT NULL,
            start_day TEXT NOT NULL,
            end_day TEXT NOT NULL,
            payload TEXT NOT NULL,
            fetched_at TEXT NOT NULL,
            PRIMARY KEY(symbol, start_day, end_day)
        );
        CREATE TABLE IF NOT EXISTS mobile_journal_env_days (
            date TEXT PRIMARY KEY,
            payload TEXT NOT NULL,
            fetched_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS mobile_ths_reasons (
            date TEXT PRIMARY KEY,
            payload TEXT NOT NULL,
            note TEXT NOT NULL,
            fetched_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS mobile_structure_days (
            date TEXT PRIMARY KEY,
            payload TEXT NOT NULL,
            fetched_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS mobile_regime_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            date TEXT NOT NULL,
            title TEXT NOT NULL,
            note TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );",
    )?;
    // v7 的表清单 = 当前清单去掉 v8 才加的表
    let v7: Vec<&str> = REQUIRED_TABLES.iter().copied().filter(|t| *t != "mobile_dividends").collect();
    verify_tables(conn, &v7)?;
    conn.pragma_update(None, "user_version", 7)?;
    Ok(())
}

/// 自选的分组、标签与自定义顺序(用户自己写的,进备份 v8)。旧库的 watchlist 只有 symbol/name/时间,这里补三列。
fn migrate_7_to_8(conn: &Connection) -> Result<()> {
    let have: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('watchlist')")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    for (column, ddl) in [
        ("grp", "ALTER TABLE watchlist ADD COLUMN grp TEXT NOT NULL DEFAULT ''"),
        ("tags", "ALTER TABLE watchlist ADD COLUMN tags TEXT NOT NULL DEFAULT ''"),
        ("sort_order", "ALTER TABLE watchlist ADD COLUMN sort_order INTEGER"),
    ] {
        if !have.iter().any(|name| name == column) {
            conn.execute(ddl, [])?;
        }
    }
    // 现金分红(用户手填,进备份 v8):代码、日期、到手金额;不改持仓成本
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_dividends (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            symbol TEXT NOT NULL,
            date TEXT NOT NULL,
            amount REAL NOT NULL,
            note TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );",
    )?;
    verify_tables(conn, REQUIRED_TABLES)?;
    conn.pragma_update(None, "user_version", 8)?;
    Ok(())
}

/// 模型配置多一个「支持图片识别」开关(用户自己标;不标就不会把截图发给模型)。幂等:先查列再补。
fn migrate_8_to_9(conn: &Connection) -> Result<()> {
    let have: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('mobile_ai_config')")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    if !have.iter().any(|name| name == "vision") {
        conn.execute("ALTER TABLE mobile_ai_config ADD COLUMN vision INTEGER NOT NULL DEFAULT 0", [])?;
    }
    verify_tables(conn, REQUIRED_TABLES)?;
    conn.pragma_update(None, "user_version", 9)?;
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
        for version in [1_i64, 2, 3, 4, 5, 6, 7, 8] {
            let path = temp_db(&format!("schema-{version}"));
            let conn = Connection::open(&path).unwrap();
            create_legacy_tables(&conn);
            if version >= 2 {
                migrate_1_to_2(&conn).unwrap();
            }
            if version >= 3 {
                migrate_2_to_3(&conn).unwrap();
            }
            if version >= 4 {
                migrate_3_to_4(&conn).unwrap();
            }
            if version >= 5 {
                migrate_4_to_5(&conn).unwrap();
            }
            if version >= 6 {
                migrate_5_to_6(&conn).unwrap();
            }
            if version >= 7 {
                migrate_6_to_7(&conn).unwrap();
            }
            if version >= 8 {
                migrate_7_to_8(&conn).unwrap();
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
