use crate::symbol::normalize_stock_symbol;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, Transaction};
use serde_json::{json, Value};
use std::path::Path;

const PLAYBOOKS: [&str; 6] = ["打板", "低吸", "接力", "半路", "套利", "其它"];

pub fn init(db_path: &Path) -> Result<()> {
    let conn = open(db_path)?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS journal_entries (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           kind TEXT NOT NULL CHECK(kind IN ('episode', 'manual')),
           symbol TEXT NOT NULL,
           name TEXT NOT NULL DEFAULT '',
           date TEXT NOT NULL,
           playbook TEXT,
           as_planned INTEGER,
           planned_stop REAL,
           planned_target REAL,
           note TEXT NOT NULL DEFAULT '',
           pnl_pct REAL,
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS journal_fills (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           entry_id INTEGER NOT NULL REFERENCES journal_entries(id) ON DELETE CASCADE,
           side TEXT NOT NULL CHECK(side IN ('buy', 'sell', 'opening', 'split')),
           date TEXT NOT NULL,
           price REAL NOT NULL CHECK(price > 0),
           shares REAL NOT NULL CHECK(shares > 0),
           fee REAL,
           sort_order INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_journal_entries_date ON journal_entries(date DESC, id DESC);
         CREATE INDEX IF NOT EXISTS idx_journal_entries_symbol ON journal_entries(symbol);
         CREATE INDEX IF NOT EXISTS idx_journal_fills_entry ON journal_fills(entry_id, sort_order, id);",
    )?;
    Ok(())
}

pub fn view(db_path: &Path) -> Result<Value> {
    let conn = open(db_path)?;
    let settings = crate::journal_settings::load(db_path)?;
    let (fees, fees_default) = (settings.fees, settings.fees_default);
    let settled: std::collections::HashMap<_,_> = crate::journal_risk::load_entries(db_path)?.into_iter().enumerate().map(|(i,e)| (e.id,crate::journal_risk::settle(e,&fees,i))).collect();
    let mut stmt = conn.prepare(
        "SELECT id, kind, symbol, name, date, playbook, as_planned,
                planned_stop, planned_target, note, pnl_pct, created_at, updated_at
         FROM journal_entries
         ORDER BY date DESC, id DESC",
    )?;

    let raw = stmt.query_map([], |row| {
        Ok(EntryRaw {
            id: row.get(0)?,
            kind: row.get(1)?,
            symbol: row.get(2)?,
            name: row.get(3)?,
            date: row.get(4)?,
            playbook: row.get(5)?,
            as_planned: row.get::<_, Option<i64>>(6)?.map(|v| v != 0),
            planned_stop: row.get(7)?,
            planned_target: row.get(8)?,
            note: row.get(9)?,
            pnl_pct: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
        })
    })?;

    let mut rows = Vec::new();
    let mut scored = 0usize;
    let mut wins = 0usize;
    let mut closed = 0usize;
    let mut net_pnl = 0.0f64;
    let mut money_scored = 0usize;
    let mut planned_count = 0usize;
    let mut planned_known = 0usize;

    for entry in raw {
        let entry = entry?;
        let fills = if entry.kind == "episode" {
            fills_for(&conn, entry.id, &fees)?
        } else {
            Vec::new()
        };
        let mut metrics = metrics(&entry, &fills);
        if let Some(r) = settled.get(&entry.id) { metrics.realized_pct=r.pnl_pct; metrics.realized_pnl=r.realized_pnl; metrics.closed=r.closed; metrics.open_shares=r.open_shares; }
        if let Some(pct) = metrics.realized_pct {
            scored += 1;
            if pct > 0.0 {
                wins += 1;
            }
        }
        if metrics.closed {
            closed += 1;
        }
        if let Some(pnl) = metrics.realized_pnl {
            net_pnl += pnl;
            money_scored += 1;
        }
        if let Some(value) = entry.as_planned {
            planned_known += 1;
            if value {
                planned_count += 1;
            }
        }

        rows.push(json!({
            "id": entry.id,
            "key": format!("mobile:{}", entry.id),
            "kind": entry.kind,
            "symbol": entry.symbol,
            "name": entry.name,
            "date": entry.date,
            "playbook": entry.playbook,
            "as_planned": entry.as_planned,
            "planned_stop": entry.planned_stop,
            "planned_target": entry.planned_target,
            "note": entry.note,
            "pnl_pct": metrics.realized_pct,
            "realized_pnl": metrics.realized_pnl,
            "closed": metrics.closed,
            "open_shares": metrics.open_shares,
            "amount": metrics.amount,
            "fees": metrics.fees,
            "fees_are_estimated": metrics.fees_are_estimated,
            "fills": fills,
            "created_at": entry.created_at,
            "updated_at": entry.updated_at
        }));
    }

    Ok(json!({
        "available": true,
        "source": "mobile_sqlite",
        "playbooks": PLAYBOOKS,
        "fees": {
            "commission_rate": fees.commission_rate,
            "commission_min": fees.commission_min,
            "stamp_tax_rate": fees.stamp_tax_rate,
            "transfer_fee_rate": fees.transfer_fee_rate,
            "is_default": fees_default,
            "note": "未填写实际费用时按这组费率估算(默认值只是初值,不是投资或券商费率建议;可在「风险与纪律」里改成自己的)"
        },
        "summary": {
            "entries": rows.len(),
            "scored": scored,
            "closed": closed,
            "wins": wins,
            "win_rate": if scored > 0 { Some(wins as f64 / scored as f64) } else { None },
            "money_scored": money_scored,
            "net_pnl": if money_scored > 0 { Some(round2(net_pnl)) } else { None },
            "planned_known": planned_known,
            "planned_count": planned_count,
            "execution_rate": if planned_known > 0 { Some(planned_count as f64 / planned_known as f64) } else { None }
        },
        "rows": rows
    }))
}

pub fn act(db_path: &Path, body: &Value) -> Result<Value> {
    match body.get("action").and_then(Value::as_str).unwrap_or("") {
        "record" => create_record(db_path, body),
        "manual" => create_manual(db_path, body),
        "delete" => delete(db_path, body),
        "annotate" => annotate(db_path, body),
        action => Err(anyhow!("unsupported journal action: {action}")),
    }
}

/// 在已开的事务里写一条「记录 + 成交明细」,返回记录 id。手动记一笔和表格导入共用这一份检查与写入。
pub(crate) fn insert_record_tx(tx: &Transaction<'_>, body: &Value) -> Result<i64> {
    let symbol = symbol(body)?;
    let fills = body
        .get("fills")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("成交明细不能为空"))?;
    if fills.is_empty() {
        return Err(anyhow!("至少需要一笔成交"));
    }

    let parsed = fills
        .iter()
        .enumerate()
        .map(|(i, fill)| parse_fill(fill, i))
        .collect::<Result<Vec<_>>>()?;
    validate_fill_balance(&parsed)?;

    let date = parsed
        .iter()
        .map(|fill| fill.date.as_str())
        .min()
        .unwrap_or("")
        .to_string();
    let now = Utc::now().to_rfc3339();
    let id = insert_entry(tx, "episode", &symbol, body, &date, None, &now)?;

    for fill in parsed {
        tx.execute(
            "INSERT INTO journal_fills(entry_id, side, date, price, shares, fee, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, fill.side, fill.date, fill.price, fill.shares, fill.fee, fill.sort_order],
        )?;
    }
    Ok(id)
}

fn create_record(db_path: &Path, body: &Value) -> Result<Value> {
    let mut conn = open(db_path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    let tx = conn.transaction()?;
    let id = insert_record_tx(&tx, body)?;
    tx.commit()?;
    Ok(json!({ "result": { "id": id }, "view": view(db_path)? }))
}

fn create_manual(db_path: &Path, body: &Value) -> Result<Value> {
    let symbol = symbol(body)?;
    let date = required_date(body.get("date").and_then(Value::as_str).unwrap_or(""))?;
    let pnl_pct = optional_number(body.get("pnl_pct"), "盈亏%")?;
    let mut conn = open(db_path)?;
    let tx = conn.transaction()?;
    let now = Utc::now().to_rfc3339();
    let id = insert_entry(&tx, "manual", &symbol, body, &date, pnl_pct, &now)?;
    tx.commit()?;
    Ok(json!({ "result": { "id": id }, "view": view(db_path)? }))
}

fn delete(db_path: &Path, body: &Value) -> Result<Value> {
    let id = body.get("id").and_then(Value::as_i64).ok_or_else(|| anyhow!("缺少日志 id"))?;
    let conn = open(db_path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    let removed = conn.execute("DELETE FROM journal_entries WHERE id = ?1", params![id])? > 0;
    Ok(json!({ "result": { "id": id, "removed": removed }, "view": view(db_path)? }))
}

fn annotate(db_path: &Path, body: &Value) -> Result<Value> {
    let id = body.get("id").and_then(Value::as_i64).ok_or_else(|| anyhow!("缺少日志 id"))?;
    let playbook = playbook(body)?;
    let as_planned = planned(body)?;
    let stop = positive_optional(body.get("planned_stop"), "计划止损价")?;
    let target = positive_optional(body.get("planned_target"), "计划目标价")?;
    let note = body.get("note").and_then(Value::as_str).unwrap_or("").trim();
    let now = Utc::now().to_rfc3339();
    let conn = open(db_path)?;
    let changed = conn.execute(
        "UPDATE journal_entries
         SET playbook = ?2, as_planned = ?3, planned_stop = ?4, planned_target = ?5,
             note = ?6, updated_at = ?7
         WHERE id = ?1",
        params![id, playbook, bool_db(as_planned), stop, target, note, now],
    )?;
    if changed == 0 {
        return Err(anyhow!("没有找到这条交易日志"));
    }
    Ok(json!({ "result": { "id": id }, "view": view(db_path)? }))
}

fn insert_entry(
    tx: &Transaction<'_>,
    kind: &str,
    symbol: &str,
    body: &Value,
    date: &str,
    pnl_pct: Option<f64>,
    now: &str,
) -> Result<i64> {
    let name = body.get("name").and_then(Value::as_str).unwrap_or("").trim();
    let playbook = playbook(body)?;
    let as_planned = planned(body)?;
    let stop = positive_optional(body.get("planned_stop"), "计划止损价")?;
    let target = positive_optional(body.get("planned_target"), "计划目标价")?;
    let note = body.get("note").and_then(Value::as_str).unwrap_or("").trim();

    tx.execute(
        "INSERT INTO journal_entries(
             kind, symbol, name, date, playbook, as_planned, planned_stop, planned_target,
             note, pnl_pct, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
        params![kind, symbol, name, date, playbook, bool_db(as_planned), stop, target, note, pnl_pct, now],
    )?;
    Ok(tx.last_insert_rowid())
}

fn fills_for(conn: &Connection, entry_id: i64, fees: &crate::journal_settings::Fees) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT id, side, date, price, shares, fee
         FROM journal_fills WHERE entry_id = ?1 ORDER BY sort_order ASC, id ASC",
    )?;
    let fills = stmt.query_map(params![entry_id], |row| {
        let side = row.get::<_, String>(1)?;
        let price = row.get::<_, f64>(3)?;
        let shares = row.get::<_, f64>(4)?;
        let actual_fee = row.get::<_, Option<f64>>(5)?;
        let estimated = actual_fee.is_none();
        let fee = actual_fee.unwrap_or_else(|| fees.estimate(&side, price, shares));
        Ok(json!({
            "id": row.get::<_, i64>(0)?,
            "type": side,
            "date": row.get::<_, String>(2)?,
            "price": price,
            "shares": shares,
            "fee": round2(fee),
            "fee_estimated": estimated
        }))
    })?;
    Ok(fills.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn metrics(entry: &EntryRaw, fills: &[Value]) -> Metrics {
    if entry.kind == "manual" {
        return Metrics {
            realized_pct: entry.pnl_pct,
            realized_pnl: None,
            closed: entry.pnl_pct.is_some(),
            open_shares: 0.0,
            amount: None,
            fees: None,
            fees_are_estimated: None,
        };
    }

    let mut buy_cost = 0.0;
    let mut sell_value = 0.0;
    let mut buy_shares = 0.0;
    let mut sell_shares = 0.0;
    let mut fees = 0.0;
    let mut any_estimated = false;
    for fill in fills {
        let side = fill.get("type").and_then(Value::as_str).unwrap_or("");
        let price = fill.get("price").and_then(Value::as_f64).unwrap_or(0.0);
        let shares = fill.get("shares").and_then(Value::as_f64).unwrap_or(0.0);
        if matches!(side, "buy" | "opening") {
            buy_cost += price * shares;
            buy_shares += shares;
        } else if side == "sell" {
            sell_value += price * shares;
            sell_shares += shares;
        }
        fees += fill.get("fee").and_then(Value::as_f64).unwrap_or(0.0);
        any_estimated |= fill.get("fee_estimated").and_then(Value::as_bool).unwrap_or(false);
    }

    let open_shares = (buy_shares - sell_shares).max(0.0);
    let closed = buy_shares > 0.0 && (buy_shares - sell_shares).abs() < 0.000_001;
    let realized_pnl = if closed { Some(round2(sell_value - buy_cost - fees)) } else { None };
    let realized_pct = realized_pnl.and_then(|pnl| {
        if buy_cost > 0.0 { Some(pnl / buy_cost * 100.0) } else { None }
    });

    Metrics {
        realized_pct: realized_pct.map(round2),
        realized_pnl,
        closed,
        open_shares,
        amount: if buy_cost > 0.0 { Some(round2(buy_cost)) } else { None },
        fees: if fills.is_empty() { None } else { Some(round2(fees)) },
        fees_are_estimated: if fills.is_empty() { None } else { Some(any_estimated) },
    }
}

fn validate_fill_balance(fills: &[Fill]) -> Result<()> {
    let mut balance = 0.0;
    let mut ordered: Vec<_> = fills.iter().collect();
    ordered.sort_by(|a,b|a.date.cmp(&b.date).then((a.side!="split").cmp(&(b.side!="split"))).then(a.sort_order.cmp(&b.sort_order)));
    for fill in ordered {
        if fill.side == "split" {
            if balance <= 0.0 { return Err(anyhow!("送转拆股当时没有持仓")); }
            balance *= fill.price;
        } else if matches!(fill.side.as_str(), "buy" | "opening") {
            if fill.side=="opening" && balance>0.000_001{return Err(anyhow!("已有持仓不能重复建立期初"));}
            balance += fill.shares;
        } else {
            balance -= fill.shares;
            if balance < -0.000_001 {
                return Err(anyhow!("卖出股数不能超过此前买入股数"));
            }
        }
        if !balance.is_finite()||balance>1e15{return Err(anyhow!("结转股数超过可计算范围"));}
    }
    Ok(())
}

fn parse_fill(value: &Value, sort_order: usize) -> Result<Fill> {
    let side = value.get("type").and_then(Value::as_str).unwrap_or("");
    if !matches!(side, "buy" | "sell" | "opening" | "split") {
        return Err(anyhow!("成交类型只能是 buy / sell / opening / split"));
    }
    let date = required_date(value.get("date").and_then(Value::as_str).unwrap_or(""))?;
    let price = positive_required(value.get("price"), "成交价")?;
    let shares = positive_required(value.get("shares"), "股数")?;
    if side == "split" && (shares != 1.0 || !(0.001..=1000.0).contains(&price)) { return Err(anyhow!("送转比例为新股数/原股数，范围 0.001–1000，数量字段必须为 1")); }
    if matches!(side,"split"|"opening") && optional_number(value.get("fee"),"费用")?.is_some_and(|v|v!=0.0){return Err(anyhow!("期初持仓和送转不产生费用；实际费用请按对应成交记录"));}
    let fee = if matches!(side,"split"|"opening") { Some(0.0) } else { optional_number(value.get("fee"), "费用")? };
    if fee.is_some_and(|v| v < 0.0) {
        return Err(anyhow!("费用不能是负数"));
    }
    Ok(Fill { side: side.to_string(), date, price, shares, fee, sort_order: sort_order as i64 })
}

fn symbol(body: &Value) -> Result<String> {
    let value = body.get("symbol").and_then(Value::as_str).unwrap_or("").trim();
    if value.is_empty() {
        return Err(anyhow!("股票代码不能为空"));
    }
    normalize_stock_symbol(value)
        .map(|symbol| symbol.digits)
        .map_err(|error| anyhow!("股票代码不正确: {error}"))
}

fn playbook(body: &Value) -> Result<Option<String>> {
    let value = body.get("playbook").and_then(Value::as_str).unwrap_or("").trim();
    if value.is_empty() {
        return Ok(None);
    }
    if !PLAYBOOKS.contains(&value) {
        return Err(anyhow!("不支持的打法: {value}"));
    }
    Ok(Some(value.to_string()))
}

fn planned(body: &Value) -> Result<Option<bool>> {
    match body.get("as_planned") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(v)) => Ok(Some(*v)),
        _ => Err(anyhow!("as_planned 必须是布尔值")),
    }
}

fn required_date(value: &str) -> Result<String> {
    let value = value.trim();
    if chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_err() {
        return Err(anyhow!("日期格式应为 YYYY-MM-DD"));
    }
    Ok(value.to_string())
}

fn optional_number(value: Option<&Value>, label: &str) -> Result<Option<f64>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(v)) => v.as_f64().map(Some).ok_or_else(|| anyhow!("{label} 要是数字")),
        Some(Value::String(v)) if v.trim().is_empty() => Ok(None),
        Some(Value::String(v)) => v.trim().parse::<f64>().map(Some).map_err(|_| anyhow!("{label} 要是数字")),
        _ => Err(anyhow!("{label} 要是数字")),
    }
}

fn positive_optional(value: Option<&Value>, label: &str) -> Result<Option<f64>> {
    let value = optional_number(value, label)?;
    if value.is_some_and(|v| v <= 0.0) {
        return Err(anyhow!("{label} 要大于 0"));
    }
    Ok(value)
}

fn positive_required(value: Option<&Value>, label: &str) -> Result<f64> {
    let value = optional_number(value, label)?.ok_or_else(|| anyhow!("{label} 不能为空"))?;
    if value <= 0.0 {
        return Err(anyhow!("{label} 要大于 0"));
    }
    Ok(value)
}

fn bool_db(value: Option<bool>) -> Option<i64> {
    value.map(|v| if v { 1 } else { 0 })
}

fn open(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path).context("open mobile journal sqlite")
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[derive(Debug)]
struct EntryRaw {
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

#[derive(Debug)]
struct Fill {
    side: String,
    date: String,
    price: f64,
    shares: f64,
    fee: Option<f64>,
    sort_order: i64,
}

#[derive(Debug)]
struct Metrics {
    realized_pct: Option<f64>,
    realized_pnl: Option<f64>,
    closed: bool,
    open_shares: f64,
    amount: Option<f64>,
    fees: Option<f64>,
    fees_are_estimated: Option<bool>,
}
