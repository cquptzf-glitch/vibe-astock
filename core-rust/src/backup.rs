use crate::symbol::normalize_stock_symbol;
use anyhow::{anyhow, Context, Result};
use chrono::{NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, path::Path};

const BACKUP_FORMAT: &str = "vibe_astock_mobile_backup";
const SCHEMA_VERSION: u32 = 10;
const MIN_SUPPORTED_SCHEMA_VERSION: u32 = 1;
const APP_VERSION: &str = env!("VIBE_APP_VERSION");
const MAX_BACKUP_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupEnvelope {
    format: String,
    schema_version: u32,
    source_version: String,
    created_at: String,
    security: BackupSecurity,
    data: BackupData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupSecurity {
    credentials_included: bool,
    ai_model_config_included: bool,
    preserved_on_restore: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupData {
    watchlist: Vec<WatchlistRow>,
    journal_entries: Vec<JournalEntryRow>,
    journal_fills: Vec<JournalFillRow>,
    daily_reviews: Vec<DailyReviewRow>,
    ai_messages: Vec<AiMessageRow>,
    #[serde(default)]
    ai_threads: Vec<AiThreadRow>,
    #[serde(default)]
    documents: Vec<DocumentRow>,
    #[serde(default)]
    calendar_events: Vec<CalendarEventRow>,
    #[serde(default)]
    reminders: Vec<ReminderRow>,
    #[serde(default)]
    verifications: Vec<VerificationRow>,
    #[serde(default)]
    journal_settings: Option<crate::journal_settings::Row>,
    /// 交易日志模式卡(v7 起;旧备份没有,恢复后为空)
    #[serde(default)]
    journal_modes: Vec<crate::journal_modes::Card>,
    #[serde(default)]
    regime_events: Vec<crate::structure::Event>,
    /// v8 起:现金分红(用户手填);旧备份没有,恢复后为空
    #[serde(default)]
    dividends: Vec<crate::portfolio::Dividend>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VerificationRow {
    id: i64,
    date: String,
    metric: String,
    direction: String,
    reason: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WatchlistRow {
    symbol: String,
    name: String,
    created_at: String,
    updated_at: String,
    /// v8 起:用户写的分组、标签与自定义顺序;旧备份没有,恢复后为空
    #[serde(default)]
    group: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    sort_order: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JournalEntryRow {
    id: i64,
    kind: String,
    symbol: String,
    name: String,
    date: String,
    playbook: Option<String>,
    as_planned: Option<bool>,
    planned_stop: Option<f64>,
    planned_target: Option<f64>,
    note: String,
    pnl_pct: Option<f64>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JournalFillRow {
    id: i64,
    entry_id: i64,
    side: String,
    date: String,
    price: f64,
    shares: f64,
    fee: Option<f64>,
    sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DailyReviewRow {
    date: String,
    observation: String,
    tomorrow_check: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AiMessageRow {
    id: i64,
    role: String,
    content: String,
    created_at: String,
    /// v9 起:消息属于哪个对话;旧备份没有,恢复后都归「默认对话」(编号 1)
    #[serde(default = "default_thread")]
    thread_id: i64,
}

fn default_thread() -> i64 {
    crate::ai_chat::DEFAULT_THREAD
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AiThreadRow {
    id: i64,
    title: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DocumentRow {
    id: i64,
    kind: String,
    category: String,
    title: String,
    body: String,
    status: String,
    source: String,
    symbol: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CalendarEventRow {
    id: i64,
    date: String,
    kind: String,
    title: String,
    detail: String,
    symbol: Option<String>,
    status: String,
    source: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReminderRow {
    id: i64,
    kind: String,
    symbol: Option<String>,
    name: String,
    threshold: Option<f64>,
    tolerance_pct: f64,
    enabled: bool,
    cooldown_minutes: i64,
    review_hour: Option<i64>,
    created_at: String,
    updated_at: String,
}

fn open(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path).context("open mobile sqlite for backup")
}

pub fn export(db_path: &Path) -> Result<Value> {
    let conn = open(db_path)?;
    let backup = BackupEnvelope {
        format: BACKUP_FORMAT.to_string(),
        schema_version: SCHEMA_VERSION,
        source_version: format!("{APP_VERSION}-mobile"),
        created_at: Utc::now().to_rfc3339(),
        security: BackupSecurity {
            credentials_included: false,
            ai_model_config_included: false,
            preserved_on_restore: vec![
                "android_keystore_credentials".to_string(),
                "ai_model_config".to_string(),
            ],
        },
        data: BackupData {
            watchlist: read_watchlist(&conn)?,
            journal_entries: read_journal_entries(&conn)?,
            journal_fills: read_journal_fills(&conn)?,
            daily_reviews: read_daily_reviews(&conn)?,
            ai_messages: read_ai_messages(&conn)?,
            ai_threads: read_ai_threads(&conn)?,
            documents: read_documents(&conn)?,
            calendar_events: read_calendar_events(&conn)?,
            reminders: read_reminders(&conn)?,
            verifications: read_verifications(&conn)?,
            journal_settings: crate::journal_settings::read_row(&conn)?,
            journal_modes: crate::journal_modes::read_cards(&conn)?,
            regime_events: crate::structure::read_events(&conn)?,
            dividends: crate::portfolio::read_dividends(&conn)?,
        },
    };
    validate_envelope(&backup)?;
    let value = serde_json::to_value(backup)?;
    ensure_size(&value)?;
    Ok(value)
}

pub fn validate(body: &Value) -> Result<Value> {
    let backup = parse_backup(body)?;
    Ok(preview(&backup))
}

pub fn restore(db_path: &Path, body: &Value) -> Result<Value> {
    let mode = body.get("mode").and_then(Value::as_str).unwrap_or("");
    if mode != "replace" {
        return Err(anyhow!("当前仅支持 replace 恢复模式"));
    }
    if body.get("confirm").and_then(Value::as_bool) != Some(true) {
        return Err(anyhow!("恢复本地数据前必须明确确认"));
    }
    let backup = parse_backup(body)?;

    let mut conn = open(db_path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    let tx = conn.transaction().context("start backup restore transaction")?;

    tx.execute("DELETE FROM journal_fills", [])?;
    tx.execute("DELETE FROM journal_entries", [])?;
    tx.execute("DELETE FROM watchlist", [])?;
    tx.execute("DELETE FROM mobile_daily_reviews", [])?;
    tx.execute("DELETE FROM mobile_ai_messages", [])?;
    tx.execute("DELETE FROM mobile_ai_threads", [])?;
    tx.execute("DELETE FROM mobile_documents", [])?;
    tx.execute("DELETE FROM mobile_calendar_events", [])?;
    tx.execute("DELETE FROM mobile_reminders", [])?;
    tx.execute("DELETE FROM mobile_verifications", [])?;
    tx.execute("DELETE FROM mobile_journal_settings", [])?;
    tx.execute("DELETE FROM mobile_journal_modes", [])?;
    tx.execute("DELETE FROM mobile_regime_events", [])?;
    tx.execute("DELETE FROM mobile_dividends", [])?;
    tx.execute(
        "DELETE FROM sqlite_sequence WHERE name IN (
            'journal_entries', 'journal_fills', 'mobile_ai_messages', 'mobile_ai_threads',
            'mobile_documents', 'mobile_calendar_events', 'mobile_reminders',
            'mobile_verifications', 'mobile_journal_modes', 'mobile_regime_events', 'mobile_dividends'
        )",
        [],
    )?;

    for row in &backup.data.watchlist {
        let symbol = normalize_stock_symbol(&row.symbol)
            .map_err(|error| anyhow!("备份中的自选代码无效 {}: {error}", row.symbol))?
            .digits;
        tx.execute(
            "INSERT INTO watchlist(symbol, name, created_at, updated_at, grp, tags, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![symbol, &row.name, &row.created_at, &row.updated_at, &row.group, serde_json::to_string(&row.tags)?, row.sort_order],
        )?;
    }

    for row in &backup.data.journal_entries {
        let symbol = normalize_stock_symbol(&row.symbol)
            .map_err(|error| anyhow!("备份中的交易代码无效 {}: {error}", row.symbol))?
            .digits;
        let as_planned = row.as_planned.map(|value| if value { 1_i64 } else { 0_i64 });
        tx.execute(
            "INSERT INTO journal_entries(
                id, kind, symbol, name, date, playbook, as_planned,
                planned_stop, planned_target, note, pnl_pct, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                row.id,
                &row.kind,
                symbol,
                &row.name,
                &row.date,
                row.playbook.as_deref(),
                as_planned,
                row.planned_stop,
                row.planned_target,
                &row.note,
                row.pnl_pct,
                &row.created_at,
                &row.updated_at,
            ],
        )?;
    }

    for row in &backup.data.journal_fills {
        tx.execute(
            "INSERT INTO journal_fills(id, entry_id, side, date, price, shares, fee, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                row.id,
                row.entry_id,
                &row.side,
                &row.date,
                row.price,
                row.shares,
                row.fee,
                row.sort_order,
            ],
        )?;
    }

    for row in &backup.data.daily_reviews {
        tx.execute(
            "INSERT INTO mobile_daily_reviews(date, observation, tomorrow_check, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                &row.date,
                &row.observation,
                &row.tomorrow_check,
                &row.created_at,
                &row.updated_at,
            ],
        )?;
    }

    for row in &backup.data.ai_threads {
        tx.execute(
            "INSERT INTO mobile_ai_threads(id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![row.id, &row.title, &row.created_at, &row.updated_at],
        )?;
    }
    crate::ai_chat::ensure_default_thread(&tx)?;
    for row in &backup.data.ai_messages {
        tx.execute(
            "INSERT INTO mobile_ai_messages(id, role, content, created_at, thread_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![row.id, &row.role, &row.content, &row.created_at, row.thread_id],
        )?;
    }

    for row in &backup.data.documents {
        let symbol = normalize_optional_symbol(row.symbol.as_deref(), "资料关联代码")?;
        tx.execute(
            "INSERT INTO mobile_documents(
                id, kind, category, title, body, status, source, symbol, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.id,
                &row.kind,
                &row.category,
                &row.title,
                &row.body,
                &row.status,
                &row.source,
                symbol,
                &row.created_at,
                &row.updated_at,
            ],
        )?;
    }

    for row in &backup.data.calendar_events {
        let symbol = normalize_optional_symbol(row.symbol.as_deref(), "日历关联代码")?;
        tx.execute(
            "INSERT INTO mobile_calendar_events(
                id, date, kind, title, detail, symbol, status, source, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.id,
                &row.date,
                &row.kind,
                &row.title,
                &row.detail,
                symbol,
                &row.status,
                &row.source,
                &row.created_at,
                &row.updated_at,
            ],
        )?;
    }

    for row in &backup.data.reminders {
        let (symbol, quote_key) = if row.kind == "daily_review" {
            (None, None)
        } else {
            let raw_symbol = row
                .symbol
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("备份中的股票提醒缺少 symbol: {}", row.id))?;
            let key = normalize_stock_symbol(raw_symbol)
                .map_err(|error| anyhow!("备份中的提醒代码无效 {raw_symbol}: {error}"))?;
            (Some(key.digits), Some(key.tencent))
        };
        tx.execute(
            "INSERT INTO mobile_reminders(
                id, kind, symbol, name, threshold, tolerance_pct, enabled,
                cooldown_minutes, review_hour, quote_key, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                row.id,
                &row.kind,
                symbol,
                &row.name,
                row.threshold,
                row.tolerance_pct,
                if row.enabled { 1_i64 } else { 0_i64 },
                row.cooldown_minutes,
                row.review_hour,
                quote_key,
                &row.created_at,
                &row.updated_at,
            ],
        )?;
    }

    for row in &backup.data.verifications {
        tx.execute(
            "INSERT INTO mobile_verifications(id, date, metric, direction, reason, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![row.id, &row.date, &row.metric, &row.direction, &row.reason, &row.created_at, &row.updated_at],
        )?;
    }

    if let Some(row) = &backup.data.journal_settings {
        crate::journal_settings::write_row(&tx, row)?;
    }

    for event in &backup.data.regime_events {
        crate::structure::write_event(&tx, event)?;
    }
    for dividend in &backup.data.dividends {
        crate::portfolio::write_dividend(&tx, dividend)?;
    }
    for card in &backup.data.journal_modes {
        crate::journal_modes::write_card(&tx, card)?;
    }

    tx.commit().context("commit backup restore")?;

    Ok(json!({
        "status": "restored",
        "mode": "replace",
        "restored_at": Utc::now().to_rfc3339(),
        "backup": preview(&backup),
        "preserved": ["android_keystore_credentials", "ai_model_config"],
        "credentials_imported": false,
        "restart_required": false
    }))
}

fn parse_backup(body: &Value) -> Result<BackupEnvelope> {
    let value = body.get("backup").unwrap_or(body);
    ensure_size(value)?;
    let backup: BackupEnvelope = serde_json::from_value(value.clone())
        .context("备份文件结构无效")?;
    validate_envelope(&backup)?;
    Ok(backup)
}

fn ensure_size(value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(value)?.len();
    if bytes > MAX_BACKUP_BYTES {
        return Err(anyhow!("备份文件过大，最大允许 32 MiB"));
    }
    Ok(())
}

fn validate_envelope(backup: &BackupEnvelope) -> Result<()> {
    if backup.format != BACKUP_FORMAT {
        return Err(anyhow!("不是 Vibe AStock Android 备份文件"));
    }
    if !(MIN_SUPPORTED_SCHEMA_VERSION..=SCHEMA_VERSION).contains(&backup.schema_version) {
        return Err(anyhow!(
            "不支持的备份版本 {}，当前支持 {}..={} ",
            backup.schema_version,
            MIN_SUPPORTED_SCHEMA_VERSION,
            SCHEMA_VERSION
        ));
    }
    if backup.security.credentials_included || backup.security.ai_model_config_included {
        return Err(anyhow!("拒绝导入声明包含模型凭据或模型配置的备份"));
    }
    if backup.data.watchlist.len() > 10_000
        || backup.data.journal_entries.len() > 100_000
        || backup.data.journal_fills.len() > 500_000
        || backup.data.daily_reviews.len() > 10_000
        || backup.data.ai_messages.len() > 100_000
        || backup.data.documents.len() > 100_000
        || backup.data.calendar_events.len() > 100_000
        || backup.data.reminders.len() > 10_000
        || backup.data.verifications.len() > 100_000
        || backup.data.journal_modes.len() > crate::journal_modes::CARDS_MAX
        || backup.data.regime_events.len() > crate::structure::EVENTS_MAX
        || backup.data.dividends.len() > crate::portfolio::DIVIDENDS_MAX
    {
        return Err(anyhow!("备份记录数量超过安全上限"));
    }

    let mut symbols = HashSet::new();
    for row in &backup.data.watchlist {
        let symbol = normalize_stock_symbol(&row.symbol)
            .map_err(|error| anyhow!("自选代码无效 {}: {error}", row.symbol))?;
        if !symbols.insert(symbol.digits) {
            return Err(anyhow!("备份中存在重复自选代码: {}", row.symbol));
        }
        ensure_text(&row.name, 300, "自选名称")?;
        crate::storage::clean_group(&row.group).map_err(|error| anyhow!("自选 {} 的分组无效: {error}", row.symbol))?;
        crate::storage::clean_tags(&row.tags).map_err(|error| anyhow!("自选 {} 的标签无效: {error}", row.symbol))?;
    }

    let mut entry_ids = HashSet::new();
    let mut episode_ids = HashSet::new();
    for row in &backup.data.journal_entries {
        if row.id <= 0 || !entry_ids.insert(row.id) {
            return Err(anyhow!("交易日志包含无效或重复 ID: {}", row.id));
        }
        if row.kind != "episode" && row.kind != "manual" {
            return Err(anyhow!("交易日志 kind 无效: {}", row.kind));
        }
        if row.kind == "episode" {
            episode_ids.insert(row.id);
        }
        normalize_stock_symbol(&row.symbol)
            .map_err(|error| anyhow!("交易代码无效 {}: {error}", row.symbol))?;
        ensure_text(&row.name, 300, "交易名称")?;
        ensure_text(&row.date, 32, "交易日期")?;
        ensure_text(&row.note, 20_000, "交易备注")?;
        if let Some(playbook) = &row.playbook {
            ensure_text(playbook, 100, "交易模式")?;
        }
        ensure_optional_number(row.planned_stop, "计划止损")?;
        ensure_optional_number(row.planned_target, "计划目标")?;
        ensure_optional_number(row.pnl_pct, "盈亏比例")?;
    }

    let mut fill_ids = HashSet::new();
    for row in &backup.data.journal_fills {
        if row.id <= 0 || !fill_ids.insert(row.id) {
            return Err(anyhow!("成交明细包含无效或重复 ID: {}", row.id));
        }
        if !episode_ids.contains(&row.entry_id) {
            return Err(anyhow!("成交明细引用了不存在的交易记录: {}", row.entry_id));
        }
        if row.side != "buy" && row.side != "sell" {
            return Err(anyhow!("成交方向无效: {}", row.side));
        }
        if !row.price.is_finite() || row.price <= 0.0 || !row.shares.is_finite() || row.shares <= 0.0 {
            return Err(anyhow!("成交价格或数量无效"));
        }
        if let Some(fee) = row.fee {
            if !fee.is_finite() || fee < 0.0 {
                return Err(anyhow!("成交费用无效"));
            }
        }
        ensure_text(&row.date, 32, "成交日期")?;
    }

    let mut dates = HashSet::new();
    for row in &backup.data.daily_reviews {
        if !dates.insert(row.date.clone()) {
            return Err(anyhow!("备份中存在重复日复盘日期: {}", row.date));
        }
        ensure_text(&row.date, 32, "复盘日期")?;
        ensure_text(&row.observation, 6000, "今日观察")?;
        ensure_text(&row.tomorrow_check, 6000, "明日验证")?;
    }

    let mut thread_ids = HashSet::new();
    thread_ids.insert(crate::ai_chat::DEFAULT_THREAD);
    if backup.data.ai_threads.len() as i64 > crate::ai_chat::THREADS_MAX {
        return Err(anyhow!("AI 对话数量超过上限 {}", crate::ai_chat::THREADS_MAX));
    }
    let mut seen_threads = HashSet::new();
    for row in &backup.data.ai_threads {
        if row.id <= 0 || !seen_threads.insert(row.id) {
            return Err(anyhow!("AI 对话包含无效或重复 ID: {}", row.id));
        }
        if row.title.trim().is_empty() || row.title.chars().count() > 30 {
            return Err(anyhow!("AI 对话标题无效(要 1–30 个字): {}", row.id));
        }
        thread_ids.insert(row.id);
    }
    let mut message_ids = HashSet::new();
    for row in &backup.data.ai_messages {
        if !thread_ids.contains(&row.thread_id) {
            return Err(anyhow!("AI 聊天消息 {} 属于不存在的对话 {}", row.id, row.thread_id));
        }
        if row.id <= 0 || !message_ids.insert(row.id) {
            return Err(anyhow!("AI 聊天包含无效或重复 ID: {}", row.id));
        }
        if row.role != "user" && row.role != "assistant" {
            return Err(anyhow!("AI 聊天角色无效: {}", row.role));
        }
        ensure_text(&row.content, 100_000, "AI 聊天内容")?;
    }

    let mut document_ids = HashSet::new();
    for row in &backup.data.documents {
        if row.id <= 0 || !document_ids.insert(row.id) {
            return Err(anyhow!("资料库包含无效或重复 ID: {}", row.id));
        }
        if !matches!(row.kind.as_str(), "note" | "report" | "inbox") {
            return Err(anyhow!("资料库 kind 无效: {}", row.kind));
        }
        if !matches!(row.status.as_str(), "active" | "archived" | "unread" | "read") {
            return Err(anyhow!("资料库 status 无效: {}", row.status));
        }
        ensure_text(&row.category, 100, "资料分类")?;
        ensure_text(&row.title, 500, "资料标题")?;
        ensure_text(&row.body, 250_000, "资料正文")?;
        ensure_text(&row.source, 300, "资料来源")?;
        normalize_optional_symbol(row.symbol.as_deref(), "资料关联代码")?;
    }

    let mut calendar_ids = HashSet::new();
    for row in &backup.data.calendar_events {
        if row.id <= 0 || !calendar_ids.insert(row.id) {
            return Err(anyhow!("事件日历包含无效或重复 ID: {}", row.id));
        }
        NaiveDate::parse_from_str(&row.date, "%Y-%m-%d")
            .with_context(|| format!("事件日期无效: {}", row.date))?;
        if !matches!(row.kind.as_str(), "company" | "ledger" | "macro" | "reminder") {
            return Err(anyhow!("事件日历 kind 无效: {}", row.kind));
        }
        if !matches!(row.status.as_str(), "open" | "done") {
            return Err(anyhow!("事件日历 status 无效: {}", row.status));
        }
        ensure_text(&row.title, 500, "事件标题")?;
        ensure_text(&row.detail, 20_000, "事件详情")?;
        ensure_text(&row.source, 300, "事件来源")?;
        normalize_optional_symbol(row.symbol.as_deref(), "日历关联代码")?;
    }

    let mut reminder_ids = HashSet::new();
    for row in &backup.data.reminders {
        if row.id <= 0 || !reminder_ids.insert(row.id) {
            return Err(anyhow!("提醒规则包含无效或重复 ID: {}", row.id));
        }
        if !matches!(
            row.kind.as_str(),
            "price_above" | "price_below" | "stop_near" | "target_near" | "daily_review"
        ) {
            return Err(anyhow!("提醒规则 kind 无效: {}", row.kind));
        }
        ensure_text(&row.name, 300, "提醒名称")?;
        if !row.tolerance_pct.is_finite() || !(0.05..=20.0).contains(&row.tolerance_pct) {
            return Err(anyhow!("提醒 tolerance_pct 无效: {}", row.id));
        }
        if !(15..=10_080).contains(&row.cooldown_minutes) {
            return Err(anyhow!("提醒 cooldown_minutes 无效: {}", row.id));
        }
        if row.kind == "daily_review" {
            if row.symbol.is_some() || row.threshold.is_some() {
                return Err(anyhow!("收盘复盘提醒不能包含股票代码或价格: {}", row.id));
            }
            if !row.review_hour.is_some_and(|hour| (0..=23).contains(&hour)) {
                return Err(anyhow!("收盘复盘提醒 review_hour 无效: {}", row.id));
            }
        } else {
            let symbol = row
                .symbol
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("股票提醒缺少代码: {}", row.id))?;
            normalize_stock_symbol(symbol)
                .map_err(|error| anyhow!("提醒代码无效 {symbol}: {error}"))?;
            if !row.threshold.is_some_and(|value| value.is_finite() && value > 0.0) {
                return Err(anyhow!("股票提醒 threshold 无效: {}", row.id));
            }
            if row.review_hour.is_some() {
                return Err(anyhow!("股票提醒不能包含 review_hour: {}", row.id));
            }
        }
    }

    if let Some(row) = &backup.data.journal_settings {
        if let Some(problem) = crate::journal_settings::row_problem(row) {
            return Err(anyhow!("交易日志设置无效: {problem}"));
        }
    }

    let mut mode_ids = HashSet::new();
    let mut event_ids = HashSet::new();
    for event in &backup.data.regime_events {
        if event.id <= 0 || !event_ids.insert(event.id) {
            return Err(anyhow!("制度事件包含无效或重复 ID: {}", event.id));
        }
        if let Some(problem) = crate::structure::event_problem(event) {
            return Err(anyhow!("{problem}"));
        }
    }
    let mut dividend_ids = HashSet::new();
    for dividend in &backup.data.dividends {
        if dividend.id <= 0 || !dividend_ids.insert(dividend.id) {
            return Err(anyhow!("分红包含无效或重复 ID: {}", dividend.id));
        }
        if let Some(problem) = crate::portfolio::dividend_problem(dividend, None) {
            return Err(anyhow!("{problem}"));
        }
    }
    for card in &backup.data.journal_modes {
        if let Some(problem) = crate::journal_modes::card_problem(card) {
            return Err(anyhow!("交易日志模式卡无效: {problem}"));
        }
        if !mode_ids.insert(card.id) {
            return Err(anyhow!("模式卡包含重复 ID: {}", card.id));
        }
    }

    let mut verification_ids = HashSet::new();
    let mut verification_keys = HashSet::new();
    for row in &backup.data.verifications {
        if row.id <= 0 || !verification_ids.insert(row.id) {
            return Err(anyhow!("明日验证包含无效或重复 ID: {}", row.id));
        }
        if NaiveDate::parse_from_str(&row.date, "%Y-%m-%d").is_err() {
            return Err(anyhow!("明日验证日期无效: {}", row.date));
        }
        if !crate::emotion_history::is_metric(&row.metric) {
            return Err(anyhow!("明日验证读数无效: {}", row.metric));
        }
        if !matches!(row.direction.as_str(), "up" | "down" | "flat") {
            return Err(anyhow!("明日验证方向无效: {}", row.direction));
        }
        if !verification_keys.insert((row.date.clone(), row.metric.clone())) {
            return Err(anyhow!("明日验证同一晚同一读数重复: {} {}", row.date, row.metric));
        }
        ensure_text(&row.reason, 200, "明日验证理由")?;
    }

    Ok(())
}

fn normalize_optional_symbol(value: Option<&str>, label: &str) -> Result<Option<String>> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => Ok(Some(
            normalize_stock_symbol(value)
                .map_err(|error| anyhow!("{label}无效 {value}: {error}"))?
                .digits,
        )),
        None => Ok(None),
    }
}

fn ensure_text(value: &str, max_chars: usize, label: &str) -> Result<()> {
    if value.chars().count() > max_chars {
        return Err(anyhow!("{label} 超过最大长度 {max_chars}"));
    }
    Ok(())
}

fn ensure_optional_number(value: Option<f64>, label: &str) -> Result<()> {
    if value.is_some_and(|number| !number.is_finite()) {
        return Err(anyhow!("{label} 不是有效数字"));
    }
    Ok(())
}

fn preview(backup: &BackupEnvelope) -> Value {
    json!({
        "valid": true,
        "format": backup.format,
        "schema_version": backup.schema_version,
        "source_version": backup.source_version,
        "created_at": backup.created_at,
        "counts": {
            "watchlist": backup.data.watchlist.len(),
            "journal_entries": backup.data.journal_entries.len(),
            "journal_fills": backup.data.journal_fills.len(),
            "daily_reviews": backup.data.daily_reviews.len(),
            "ai_messages": backup.data.ai_messages.len(),
            "ai_threads": backup.data.ai_threads.len(),
            "documents": backup.data.documents.len(),
            "calendar_events": backup.data.calendar_events.len(),
            "reminders": backup.data.reminders.len(),
            "verifications": backup.data.verifications.len(),
            "journal_settings": backup.data.journal_settings.is_some() as usize,
            "journal_modes": backup.data.journal_modes.len(),
            "regime_events": backup.data.regime_events.len(),
            "dividends": backup.data.dividends.len()
        },
        "compatibility": {
            "current_schema_version": SCHEMA_VERSION,
            "legacy_backup": backup.schema_version < SCHEMA_VERSION,
            "legacy_missing_documents_are_empty": backup.schema_version == 1,
            "legacy_missing_calendar_events_are_empty": backup.schema_version <= 2,
            "legacy_missing_reminders_are_empty": backup.schema_version <= 3,
            "legacy_missing_verifications_are_empty": backup.schema_version <= 4,
            "legacy_missing_journal_settings_are_default": backup.schema_version <= 5,
            "legacy_missing_journal_modes_are_empty": backup.schema_version <= 6,
            "legacy_missing_regime_events_are_empty": backup.schema_version <= 6,
            "legacy_missing_watchlist_groups_are_empty": backup.schema_version <= 7,
            "legacy_missing_dividends_are_empty": backup.schema_version <= 7,
            "legacy_missing_ai_threads_are_default": backup.schema_version <= 8,
            "legacy_missing_cost_method_is_average": backup.schema_version <= 9
        },
        "security": {
            "credentials_included": false,
            "ai_model_config_included": false,
            "preserved_on_restore": backup.security.preserved_on_restore
        }
    })
}

fn read_watchlist(conn: &Connection) -> Result<Vec<WatchlistRow>> {
    let mut stmt = conn.prepare(
        "SELECT symbol, COALESCE(name, ''), created_at, updated_at, COALESCE(grp, ''), COALESCE(tags, ''), sort_order FROM watchlist ORDER BY created_at, symbol",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(WatchlistRow {
                symbol: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                group: row.get(4)?,
                tags: crate::storage::tags_of(&row.get::<_, String>(5)?),
                sort_order: row.get(6)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_journal_entries(conn: &Connection) -> Result<Vec<JournalEntryRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, symbol, name, date, playbook, as_planned, planned_stop, planned_target,
                note, pnl_pct, created_at, updated_at
         FROM journal_entries ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(JournalEntryRow {
                id: row.get(0)?,
                kind: row.get(1)?,
                symbol: row.get(2)?,
                name: row.get(3)?,
                date: row.get(4)?,
                playbook: row.get(5)?,
                as_planned: row.get::<_, Option<i64>>(6)?.map(|value| value != 0),
                planned_stop: row.get(7)?,
                planned_target: row.get(8)?,
                note: row.get(9)?,
                pnl_pct: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_journal_fills(conn: &Connection) -> Result<Vec<JournalFillRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, entry_id, side, date, price, shares, fee, sort_order FROM journal_fills ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(JournalFillRow {
                id: row.get(0)?,
                entry_id: row.get(1)?,
                side: row.get(2)?,
                date: row.get(3)?,
                price: row.get(4)?,
                shares: row.get(5)?,
                fee: row.get(6)?,
                sort_order: row.get(7)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_daily_reviews(conn: &Connection) -> Result<Vec<DailyReviewRow>> {
    let mut stmt = conn.prepare(
        "SELECT date, observation, tomorrow_check, created_at, updated_at FROM mobile_daily_reviews ORDER BY date",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(DailyReviewRow {
                date: row.get(0)?,
                observation: row.get(1)?,
                tomorrow_check: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_ai_threads(conn: &Connection) -> Result<Vec<AiThreadRow>> {
    let mut stmt = conn.prepare("SELECT id, title, created_at, updated_at FROM mobile_ai_threads ORDER BY id")?;
    let rows = stmt
        .query_map([], |row| Ok(AiThreadRow { id: row.get(0)?, title: row.get(1)?, created_at: row.get(2)?, updated_at: row.get(3)? }))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_ai_messages(conn: &Connection) -> Result<Vec<AiMessageRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, role, content, created_at, thread_id FROM mobile_ai_messages ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(AiMessageRow {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
                thread_id: row.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_documents(conn: &Connection) -> Result<Vec<DocumentRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, category, title, body, status, source, symbol, created_at, updated_at
         FROM mobile_documents ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(DocumentRow {
                id: row.get(0)?,
                kind: row.get(1)?,
                category: row.get(2)?,
                title: row.get(3)?,
                body: row.get(4)?,
                status: row.get(5)?,
                source: row.get(6)?,
                symbol: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_calendar_events(conn: &Connection) -> Result<Vec<CalendarEventRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, date, kind, title, detail, symbol, status, source, created_at, updated_at
         FROM mobile_calendar_events ORDER BY date, id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CalendarEventRow {
                id: row.get(0)?,
                date: row.get(1)?,
                kind: row.get(2)?,
                title: row.get(3)?,
                detail: row.get(4)?,
                symbol: row.get(5)?,
                status: row.get(6)?,
                source: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_reminders(conn: &Connection) -> Result<Vec<ReminderRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, symbol, name, threshold, tolerance_pct, enabled,
                cooldown_minutes, review_hour, created_at, updated_at
         FROM mobile_reminders ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ReminderRow {
                id: row.get(0)?,
                kind: row.get(1)?,
                symbol: row.get(2)?,
                name: row.get(3)?,
                threshold: row.get(4)?,
                tolerance_pct: row.get(5)?,
                enabled: row.get::<_, i64>(6)? != 0,
                cooldown_minutes: row.get(7)?,
                review_hour: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_verifications(conn: &Connection) -> Result<Vec<VerificationRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, date, metric, direction, reason, created_at, updated_at
         FROM mobile_verifications ORDER BY date, id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(VerificationRow {
                id: row.get(0)?,
                date: row.get(1)?,
                metric: row.get(2)?,
                direction: row.get(3)?,
                reason: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db(label: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("vibe-backup-{label}-{stamp}.sqlite3"))
    }

    fn init_all(path: &Path) {
        crate::storage::init(path).unwrap();
        crate::journal::init(path).unwrap();
        crate::daily_review::init(path).unwrap();
        crate::ai_chat::init(path).unwrap();
        crate::ai_config::init(path).unwrap();
        crate::schema::migrate(path).unwrap();
    }

    #[test]
    fn export_never_contains_ai_credentials() {
        let path = temp_db("export");
        init_all(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO mobile_ai_config(id, base_url, model, api_key, updated_at)
             VALUES (1, 'https://example.com/v1', 'demo-model', 'super-secret-key', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO watchlist(symbol, name, created_at, updated_at) VALUES ('600519', '贵州茅台', 'a', 'b')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO mobile_documents(kind, category, title, body, status, source, symbol, created_at, updated_at)
             VALUES ('note', '研究', '茅台跟踪', '正文', 'active', 'manual', '600519', 'a', 'b')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO mobile_calendar_events(date, kind, title, detail, symbol, status, source, created_at, updated_at)
             VALUES ('2026-10-20', 'reminder', '验证茅台假设', '观察量价', '600519', 'open', 'manual', 'a', 'b')",
            [],
        )
        .unwrap();
        drop(conn);
        crate::reminders::save(
            &path,
            &json!({
                "kind": "price_below",
                "symbol": "600519",
                "name": "茅台止损观察",
                "threshold": 1500.0,
                "tolerance_pct": 1.0,
                "cooldown_minutes": 60
            }),
        )
        .unwrap();

        let backup = export(&path).unwrap();
        let text = serde_json::to_string(&backup).unwrap();
        assert!(!text.contains("super-secret-key"));
        assert!(!text.contains("demo-model"));
        assert_eq!(backup["security"]["credentials_included"], false);
        assert_eq!(backup["schema_version"], 10);
        assert_eq!(backup["data"]["journal_settings"], Value::Null);
        assert_eq!(backup["data"]["verifications"], json!([]));
        assert_eq!(backup["data"]["journal_modes"], json!([]));
        assert_eq!(backup["data"]["watchlist"][0]["symbol"], "600519");
        assert_eq!(backup["data"]["documents"][0]["title"], "茅台跟踪");
        assert_eq!(backup["data"]["calendar_events"][0]["title"], "验证茅台假设");
        assert_eq!(backup["data"]["reminders"][0]["name"], "茅台止损观察");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn restore_replaces_business_data_but_preserves_ai_config() {
        let source = temp_db("source");
        init_all(&source);
        let conn = Connection::open(&source).unwrap();
        conn.execute(
            "INSERT INTO watchlist(symbol, name, created_at, updated_at) VALUES ('000001', '平安银行', 'a', 'b')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO mobile_ai_messages(role, content, created_at) VALUES ('user', 'hello', 'now')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO mobile_documents(kind, category, title, body, status, source, symbol, created_at, updated_at)
             VALUES ('report', '周报', '来源报告', 'source body', 'active', 'manual', NULL, 'a', 'b')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO mobile_calendar_events(date, kind, title, detail, symbol, status, source, created_at, updated_at)
             VALUES ('2026-10-22', 'company', '公司事件', '详情', '000001', 'open', 'manual', 'a', 'b')",
            [],
        )
        .unwrap();
        drop(conn);
        crate::reminders::save(
            &source,
            &json!({
                "kind": "price_above",
                "symbol": "000001",
                "name": "平安突破",
                "threshold": 15.0,
                "enabled": true,
                "cooldown_minutes": 30
            }),
        )
        .unwrap();
        let backup = export(&source).unwrap();

        let target = temp_db("target");
        init_all(&target);
        crate::ai_config::save(
            &target,
            &json!({"base_url": "https://device.example/v1", "model": "device-model"}),
            true,
        )
        .unwrap();
        crate::storage::upsert(&target, &json!({"symbol": "600519", "name": "旧数据"})).unwrap();
        crate::documents::save(
            &target,
            &json!({"kind": "note", "title": "旧资料", "body": "old"}),
        )
        .unwrap();
        crate::calendar::save(
            &target,
            &json!({"date": "2026-10-30", "kind": "reminder", "title": "旧事件"}),
        )
        .unwrap();
        crate::reminders::save(
            &target,
            &json!({"kind": "daily_review", "name": "旧提醒", "review_hour": 16}),
        )
        .unwrap();

        let result = restore(
            &target,
            &json!({"backup": backup, "mode": "replace", "confirm": true}),
        )
        .unwrap();
        assert_eq!(result["credentials_imported"], false);

        let watchlist = crate::storage::list(&target).unwrap();
        assert_eq!(watchlist["items"].as_array().unwrap().len(), 1);
        assert_eq!(watchlist["items"][0]["symbol"], "000001");
        let config = crate::ai_config::load(&target).unwrap().unwrap();
        assert_eq!(config.base_url, "https://device.example/v1");
        assert_eq!(config.model, "device-model");
        let history = crate::ai_chat::history(&target, 1).unwrap();
        assert_eq!(history["count"], 1);
        let documents = crate::documents::list(&target, "/mobile/documents?kind=report").unwrap();
        assert_eq!(documents["count"], 1);
        assert_eq!(documents["items"][0]["title"], "来源报告");
        let old_notes = crate::documents::list(&target, "/mobile/documents?kind=note").unwrap();
        assert_eq!(old_notes["count"], 0);
        let calendar = crate::calendar::list(
            &target,
            "/mobile/calendar?from=2026-10-01&to=2026-11-30",
        )
        .unwrap();
        assert_eq!(calendar["count"], 1);
        assert_eq!(calendar["items"][0]["title"], "公司事件");
        let reminders = crate::reminders::list(&target).unwrap();
        assert_eq!(reminders["count"], 1);
        assert_eq!(reminders["items"][0]["name"], "平安突破");
        assert_eq!(reminders["items"][0]["quote_key"], "sz000001");

        let _ = std::fs::remove_file(source);
        let _ = std::fs::remove_file(target);
    }

    #[test]
    fn accepts_legacy_v1_backup_with_empty_newer_collections() {
        let path = temp_db("legacy-v1");
        init_all(&path);
        let current = export(&path).unwrap();
        let mut legacy = current;
        legacy["schema_version"] = json!(1);
        let data = legacy["data"].as_object_mut().unwrap();
        data.remove("documents");
        data.remove("calendar_events");
        data.remove("reminders");
        let preview = validate(&legacy).unwrap();
        assert_eq!(preview["valid"], true);
        assert_eq!(preview["compatibility"]["legacy_backup"], true);
        assert_eq!(preview["counts"]["documents"], 0);
        assert_eq!(preview["counts"]["calendar_events"], 0);
        assert_eq!(preview["counts"]["reminders"], 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn accepts_legacy_v2_backup_without_calendar_or_reminders() {
        let path = temp_db("legacy-v2");
        init_all(&path);
        let current = export(&path).unwrap();
        let mut legacy = current;
        legacy["schema_version"] = json!(2);
        let data = legacy["data"].as_object_mut().unwrap();
        data.remove("calendar_events");
        data.remove("reminders");
        let preview = validate(&legacy).unwrap();
        assert_eq!(preview["valid"], true);
        assert_eq!(preview["counts"]["calendar_events"], 0);
        assert_eq!(preview["counts"]["reminders"], 0);
        assert_eq!(preview["compatibility"]["legacy_missing_calendar_events_are_empty"], true);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn accepts_legacy_v3_backup_without_reminders() {
        let path = temp_db("legacy-v3");
        init_all(&path);
        let current = export(&path).unwrap();
        let mut legacy = current;
        legacy["schema_version"] = json!(3);
        legacy["data"].as_object_mut().unwrap().remove("reminders");
        let preview = validate(&legacy).unwrap();
        assert_eq!(preview["valid"], true);
        assert_eq!(preview["counts"]["reminders"], 0);
        assert_eq!(preview["compatibility"]["legacy_missing_reminders_are_empty"], true);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn journal_settings_round_trip_and_legacy_backups_reset_them() {
        let source = temp_db("jset-source");
        init_all(&source);
        crate::journal_settings::save(
            &source,
            &json!({"fees": {"commission_rate": 0.0001, "commission_min": 0, "stamp_tax_rate": 0.0005, "transfer_fee_rate": 0.00001},
                    "rules": {"max_positions": 4}, "equity_base": 150000}),
        )
        .unwrap();
        let backup = export(&source).unwrap();
        assert_eq!(backup["schema_version"], 10);
        assert_eq!(backup["data"]["journal_settings"]["equity_base"], 150000.0);
        assert_eq!(validate(&backup).unwrap()["counts"]["journal_settings"], 1);

        let target = temp_db("jset-target");
        init_all(&target);
        crate::journal_settings::save(&target, &json!({"equity_base": 1})).unwrap();
        restore(&target, &json!({"mode": "replace", "confirm": true, "backup": backup.clone()})).unwrap();
        let restored = crate::journal_settings::load(&target).unwrap();
        assert_eq!((restored.fees.commission_rate, restored.fees_default, restored.rules.max_positions, restored.equity_base), (0.0001, false, 4.0, Some(150000.0)));

        // v5 备份没有设置:恢复后回到默认,不留着设备上原来的
        let mut legacy = backup.clone();
        legacy["schema_version"] = json!(5);
        legacy["data"].as_object_mut().unwrap().remove("journal_settings");
        let preview = validate(&legacy).unwrap();
        assert_eq!(preview["compatibility"]["legacy_missing_journal_settings_are_default"], true);
        restore(&target, &json!({"mode": "replace", "confirm": true, "backup": legacy})).unwrap();
        let reset = crate::journal_settings::load(&target).unwrap();
        assert_eq!((reset.fees_default, reset.rules_default, reset.equity_base), (true, true, None));

        // 设置里有不合规则的值:整份备份拒收
        let mut bad = backup;
        bad["data"]["journal_settings"]["fees"] = json!({"commission_rate": 2.5, "commission_min": 5, "stamp_tax_rate": 0.0005, "transfer_fee_rate": 0.00001});
        assert!(validate(&bad).unwrap_err().to_string().contains("交易日志设置无效"));
        let _ = std::fs::remove_file(source);
        let _ = std::fs::remove_file(target);
    }

    #[test]
    fn journal_modes_round_trip_and_older_backups_restore_with_no_cards() {
        let source = temp_db("modes-source");
        init_all(&source);
        let made = |name: &str, setup: &str| json!({"action": "save", "name": name, "playbook": "打板", "setup": setup, "entry": "排板"});
        crate::journal_modes::act(&source, &made("首板早封", "10 点前封")).unwrap();
        let card_id = crate::journal_modes::view(&source).unwrap()["modes"]["cards"][0]["id"].as_i64().unwrap();
        crate::journal_modes::act(&source, &json!({"action": "save", "card_id": card_id, "name": "首板早封", "playbook": "打板", "setup": "9:45 前封", "entry": "排板", "changes": "收紧"})).unwrap();
        crate::journal_modes::act(&source, &made("另一张", "x")).unwrap();
        let backup = export(&source).unwrap();
        assert_eq!(backup["schema_version"], 10);
        assert_eq!(backup["data"]["journal_modes"].as_array().unwrap().len(), 2);
        assert_eq!(backup["data"]["journal_modes"][0]["versions"].as_array().unwrap().len(), 2);
        let preview = validate(&backup).unwrap();
        assert_eq!(preview["counts"]["journal_modes"], 2);
        assert_eq!(preview["compatibility"]["legacy_missing_journal_modes_are_empty"], false);

        let target = temp_db("modes-target");
        init_all(&target);
        crate::journal_modes::act(&target, &made("设备上原有的", "y")).unwrap();
        restore(&target, &json!({"mode": "replace", "confirm": true, "backup": backup.clone()})).unwrap();
        let restored = crate::journal_modes::read_cards(&Connection::open(&target).unwrap()).unwrap();
        let original = crate::journal_modes::read_cards(&Connection::open(&source).unwrap()).unwrap();
        assert_eq!(restored, original, "整张卡含全部版本原样还原,设备上原有的被替换");
        // 恢复后继续新建不会撞上还原的 id
        crate::journal_modes::act(&target, &made("恢复后新建", "z")).unwrap();
        assert_eq!(crate::journal_modes::read_cards(&Connection::open(&target).unwrap()).unwrap().len(), 3);

        // v1~v6 备份没有模式卡:仍可恢复,恢复后为空(不留着设备上原来的)
        for version in [1, 2, 3, 4, 5, 6] {
            let mut legacy = backup.clone();
            legacy["schema_version"] = json!(version);
            legacy["data"].as_object_mut().unwrap().remove("journal_modes");
            let preview = validate(&legacy).unwrap();
            assert_eq!(preview["compatibility"]["legacy_missing_journal_modes_are_empty"], true, "v{version}");
            restore(&target, &json!({"mode": "replace", "confirm": true, "backup": legacy})).unwrap();
            assert!(crate::journal_modes::read_cards(&Connection::open(&target).unwrap()).unwrap().is_empty(), "v{version}");
        }

        // 不合规则的模式卡:整份备份拒收
        let mut bad = backup.clone();
        bad["data"]["journal_modes"][0]["versions"][0]["setup"] = json!("字".repeat(801));
        assert!(validate(&bad).unwrap_err().to_string().contains("模式卡无效"));
        let mut dup = backup.clone();
        dup["data"]["journal_modes"][1]["id"] = dup["data"]["journal_modes"][0]["id"].clone();
        assert!(validate(&dup).unwrap_err().to_string().contains("重复 ID"));
        let mut empty_versions = backup;
        empty_versions["data"]["journal_modes"][0]["versions"] = json!([]);
        assert!(validate(&empty_versions).is_err());
        let _ = std::fs::remove_file(source);
        let _ = std::fs::remove_file(target);
    }

    #[test]
    fn regime_events_round_trip_and_older_backups_restore_with_none() {
        let source = temp_db("regime-source");
        init_all(&source);
        crate::structure::act(&source, &json!({"action": "save", "date": "2026-07-06", "title": "主板 ST 放宽到 10%", "note": "公告"})).unwrap();
        crate::structure::act(&source, &json!({"action": "save", "date": "2999-01-01", "title": "未来规则"})).unwrap();
        let backup = export(&source).unwrap();
        assert_eq!(backup["data"]["regime_events"].as_array().unwrap().len(), 2);
        let preview = validate(&backup).unwrap();
        assert_eq!((preview["counts"]["regime_events"].clone(), preview["compatibility"]["legacy_missing_regime_events_are_empty"].clone()), (json!(2), json!(false)));

        let target = temp_db("regime-target");
        init_all(&target);
        crate::structure::act(&target, &json!({"action": "save", "date": "2026-01-01", "title": "设备上原有的"})).unwrap();
        restore(&target, &json!({"mode": "replace", "confirm": true, "backup": backup.clone()})).unwrap();
        let restored = crate::structure::read_events(&Connection::open(&target).unwrap()).unwrap();
        assert_eq!(restored.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), vec!["主板 ST 放宽到 10%", "未来规则"], "恢复后是备份里的两条,设备上原有的被替换");
        // 恢复后新建的编号接在后面,不撞
        crate::structure::act(&target, &json!({"action": "save", "date": "2026-10-01", "title": "新登记"})).unwrap();
        assert_eq!(crate::structure::read_events(&Connection::open(&target).unwrap()).unwrap().len(), 3);

        // v1–v6 备份没有这一项:恢复后清空
        let mut legacy = backup.clone();
        legacy["schema_version"] = json!(6);
        legacy["data"].as_object_mut().unwrap().remove("regime_events");
        assert_eq!(validate(&legacy).unwrap()["compatibility"]["legacy_missing_regime_events_are_empty"], true);
        restore(&target, &json!({"mode": "replace", "confirm": true, "backup": legacy})).unwrap();
        assert!(crate::structure::read_events(&Connection::open(&target).unwrap()).unwrap().is_empty());

        // 日期无效 / 标题超长 / 重复 id:整份拒收
        let mut bad = backup.clone();
        bad["data"]["regime_events"][0]["date"] = json!("2026-13-40");
        assert!(validate(&bad).unwrap_err().to_string().contains("日期无效"));
        let mut long = backup.clone();
        long["data"]["regime_events"][0]["title"] = json!("字".repeat(61));
        assert!(validate(&long).unwrap_err().to_string().contains("最多 60 字"));
        let mut dup = backup;
        dup["data"]["regime_events"][1]["id"] = dup["data"]["regime_events"][0]["id"].clone();
        assert!(validate(&dup).unwrap_err().to_string().contains("重复 ID"));
        let _ = std::fs::remove_file(source);
        let _ = std::fs::remove_file(target);
    }

    #[test]
    fn watchlist_groups_tags_and_order_round_trip_and_older_backups_restore_empty() {
        let source = temp_db("wl-source");
        let target = temp_db("wl-target");
        init_all(&source);
        for (symbol, name) in [("600519", "贵州茅台"), ("000001", "平安银行"), ("300308", "中际旭创")] {
            crate::storage::upsert(&source, &json!({"symbol": symbol, "name": name})).unwrap();
        }
        crate::storage::set_meta(&source, &json!({"symbol": "600519", "group": "白酒", "tags": ["核心", "高股息", "核心"]})).unwrap();
        crate::storage::set_order(&source, &json!({"symbols": ["300308", "600519", "000001"]})).unwrap();
        let listed = crate::storage::list(&source).unwrap();
        let symbols: Vec<&str> = listed["items"].as_array().unwrap().iter().map(|r| r["symbol"].as_str().unwrap()).collect();
        assert_eq!(symbols, ["300308", "600519", "000001"], "自定义顺序");
        assert_eq!(listed["items"][1]["group"], "白酒");
        assert_eq!(listed["items"][1]["tags"], json!(["核心", "高股息"]), "标签去重");
        assert_eq!(listed["groups"], json!(["白酒"]));

        let backup = export(&source).unwrap();
        assert_eq!(validate(&backup).unwrap()["compatibility"]["legacy_missing_watchlist_groups_are_empty"], false);
        init_all(&target);
        restore(&target, &json!({"mode": "replace", "confirm": true, "backup": backup.clone()})).unwrap();
        let back = crate::storage::list(&target).unwrap();
        assert_eq!(back["items"], listed["items"], "分组、标签、顺序随备份恢复");

        // v7 及更早的备份没有这三项:恢复后为空分组、无标签、没有自定义顺序
        let mut legacy = backup.clone();
        legacy["schema_version"] = json!(7);
        for row in legacy["data"]["watchlist"].as_array_mut().unwrap() {
            let obj = row.as_object_mut().unwrap();
            obj.remove("group");
            obj.remove("tags");
            obj.remove("sort_order");
        }
        assert_eq!(validate(&legacy).unwrap()["compatibility"]["legacy_missing_watchlist_groups_are_empty"], true);
        restore(&target, &json!({"mode": "replace", "confirm": true, "backup": legacy})).unwrap();
        let old = crate::storage::list(&target).unwrap();
        assert!(old["items"].as_array().unwrap().iter().all(|r| r["group"] == "" && r["tags"] == json!([]) && r["sort_order"].is_null()));
        assert_eq!(old["groups"], json!([]));

        // 过长 / 带逗号 / 太多标签:整份拒收
        let mut long = backup.clone();
        long["data"]["watchlist"][0]["group"] = json!("字".repeat(13));
        assert!(validate(&long).unwrap_err().to_string().contains("分组"));
        let mut many = backup;
        many["data"]["watchlist"][0]["tags"] = json!(["a", "b", "c", "d", "e", "f"]);
        assert!(validate(&many).unwrap_err().to_string().contains("标签"));
        let _ = std::fs::remove_file(source);
        let _ = std::fs::remove_file(target);
    }

    #[test]
    fn dividends_round_trip_and_older_backups_restore_with_none() {
        let source = temp_db("div-source");
        let target = temp_db("div-target");
        init_all(&source);
        crate::portfolio::act(&source, &json!({"action": "save", "symbol": "600519", "date": "2026-06-01", "amount": 300.5, "note": "年度分红"})).unwrap();
        crate::portfolio::act(&source, &json!({"action": "save", "symbol": "000001", "date": "2026-07-01", "amount": 88.0})).unwrap();
        let backup = export(&source).unwrap();
        assert_eq!(backup["data"]["dividends"].as_array().unwrap().len(), 2);
        let preview = validate(&backup).unwrap();
        assert_eq!((preview["counts"]["dividends"].clone(), preview["compatibility"]["legacy_missing_dividends_are_empty"].clone()), (json!(2), json!(false)));

        init_all(&target);
        crate::portfolio::act(&target, &json!({"action": "save", "symbol": "300308", "date": "2026-01-01", "amount": 1.0})).unwrap();
        restore(&target, &json!({"mode": "replace", "confirm": true, "backup": backup.clone()})).unwrap();
        let back = crate::portfolio::view(&target).unwrap();
        assert_eq!(back["totals"]["dividends"], json!(388.5), "恢复后是备份里的两条,设备上原有的被替换");
        assert_eq!(back["dividends"].as_array().unwrap().len(), 2);
        // 恢复后新建的编号接在后面,不撞
        crate::portfolio::act(&target, &json!({"action": "save", "symbol": "600519", "date": "2026-08-01", "amount": 5.0})).unwrap();
        assert_eq!(crate::portfolio::view(&target).unwrap()["dividends"].as_array().unwrap().len(), 3);

        // v7 及更早的备份没有这一项:恢复后清空
        let mut legacy = backup.clone();
        legacy["schema_version"] = json!(7);
        legacy["data"].as_object_mut().unwrap().remove("dividends");
        assert_eq!(validate(&legacy).unwrap()["compatibility"]["legacy_missing_dividends_are_empty"], true);
        restore(&target, &json!({"mode": "replace", "confirm": true, "backup": legacy})).unwrap();
        assert_eq!(crate::portfolio::view(&target).unwrap()["totals"]["dividends"], json!(0.0));

        // 金额非正 / 日期无效 / 重复 id:整份拒收
        let mut bad = backup.clone();
        bad["data"]["dividends"][0]["amount"] = json!(-1.0);
        assert!(validate(&bad).unwrap_err().to_string().contains("大于 0"));
        let mut date = backup.clone();
        date["data"]["dividends"][0]["date"] = json!("2026-13-40");
        assert!(validate(&date).unwrap_err().to_string().contains("日期无效"));
        let mut dup = backup;
        dup["data"]["dividends"][1]["id"] = dup["data"]["dividends"][0]["id"].clone();
        assert!(validate(&dup).unwrap_err().to_string().contains("重复 ID"));
        let _ = std::fs::remove_file(source);
        let _ = std::fs::remove_file(target);
    }
}
