use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::Path;

use crate::journal;

const PLAYBOOKS: [&str; 6] = ["打板", "低吸", "接力", "半路", "套利", "其它"];

pub fn append_fills(db_path: &Path, body: &Value) -> Result<Value> {
    let id = body
        .get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("缺少日志 id"))?;
    let values = body
        .get("fills")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("成交明细不能为空"))?;
    if values.is_empty() {
        return Err(anyhow!("至少需要一笔追加成交"));
    }

    let mut conn = open(db_path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    let (kind, entry_date): (String, String) = conn
        .query_row(
            "SELECT kind, date FROM journal_entries WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| anyhow!("没有找到这条交易日志"))?;
    if kind != "episode" {
        return Err(anyhow!("盈亏补记不能追加成交明细"));
    }

    let mut all = load_balance_fills(&conn, id)?;
    let start_order: i64 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM journal_fills WHERE entry_id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    let appended = values
        .iter()
        .enumerate()
        .map(|(index, value)| parse_fill(value, start_order + index as i64))
        .collect::<Result<Vec<_>>>()?;
    all.extend(appended.iter().cloned());
    validate_balance(&all)?;

    let mut new_date = entry_date;
    for fill in &appended {
        if fill.date < new_date {
            new_date = fill.date.clone();
        }
    }
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction()?;
    for fill in appended {
        tx.execute(
            "INSERT INTO journal_fills(entry_id, side, date, price, shares, fee, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, fill.side, fill.date, fill.price, fill.shares, fill.fee, fill.sort_order],
        )?;
    }
    tx.execute(
        "UPDATE journal_entries SET date = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, new_date, now],
    )?;
    tx.commit()?;

    Ok(json!({ "result": { "id": id, "appended": values.len() }, "view": journal::view(db_path)? }))
}

pub fn update_entry(db_path: &Path, body: &Value) -> Result<Value> {
    let id = body
        .get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("缺少日志 id"))?;
    let conn = open(db_path)?;
    let current = conn
        .query_row(
            "SELECT kind, name, date, playbook, as_planned, planned_stop, planned_target, note, pnl_pct
             FROM journal_entries WHERE id = ?1",
            params![id],
            |row| {
                Ok(CurrentEntry {
                    kind: row.get(0)?,
                    name: row.get(1)?,
                    date: row.get(2)?,
                    playbook: row.get(3)?,
                    as_planned: row.get::<_, Option<i64>>(4)?.map(|value| value != 0),
                    planned_stop: row.get(5)?,
                    planned_target: row.get(6)?,
                    note: row.get(7)?,
                    pnl_pct: row.get(8)?,
                })
            },
        )
        .map_err(|_| anyhow!("没有找到这条交易日志"))?;

    let name = body
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or(current.name.as_str())
        .to_string();
    let playbook = if body.get("playbook").is_some() {
        parse_playbook(body.get("playbook"))?
    } else {
        current.playbook
    };
    let as_planned = if body.get("as_planned").is_some() {
        parse_planned(body.get("as_planned"))?
    } else {
        current.as_planned
    };
    let planned_stop = if body.get("planned_stop").is_some() {
        positive_optional(body.get("planned_stop"), "计划止损价")?
    } else {
        current.planned_stop
    };
    let planned_target = if body.get("planned_target").is_some() {
        positive_optional(body.get("planned_target"), "计划目标价")?
    } else {
        current.planned_target
    };
    let note = body
        .get("note")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or(current.note.as_str())
        .to_string();

    let (date, pnl_pct) = if current.kind == "manual" {
        let date = if let Some(value) = body.get("date").and_then(Value::as_str) {
            required_date(value)?
        } else {
            current.date
        };
        let pnl_pct = if body.get("pnl_pct").is_some() {
            optional_number(body.get("pnl_pct"), "盈亏%")?
        } else {
            current.pnl_pct
        };
        (date, pnl_pct)
    } else {
        (current.date, current.pnl_pct)
    };

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE journal_entries
         SET name = ?2, date = ?3, playbook = ?4, as_planned = ?5,
             planned_stop = ?6, planned_target = ?7, note = ?8, pnl_pct = ?9, updated_at = ?10
         WHERE id = ?1",
        params![
            id,
            name,
            date,
            playbook,
            as_planned.map(|value| if value { 1 } else { 0 }),
            planned_stop,
            planned_target,
            note,
            pnl_pct,
            now
        ],
    )?;

    Ok(json!({ "result": { "id": id, "updated": true }, "view": journal::view(db_path)? }))
}

fn open(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path).context("open mobile journal sqlite")
}

fn load_balance_fills(conn: &Connection, entry_id: i64) -> Result<Vec<Fill>> {
    let mut stmt = conn.prepare(
        "SELECT side, date, price, shares, fee, sort_order
         FROM journal_fills WHERE entry_id = ?1 ORDER BY sort_order ASC, id ASC",
    )?;
    let rows = stmt.query_map(params![entry_id], |row| {
        Ok(Fill {
            side: row.get(0)?,
            date: row.get(1)?,
            price: row.get(2)?,
            shares: row.get(3)?,
            fee: row.get(4)?,
            sort_order: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

fn parse_fill(value: &Value, sort_order: i64) -> Result<Fill> {
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
    if fee.is_some_and(|value| value < 0.0) {
        return Err(anyhow!("费用不能是负数"));
    }
    Ok(Fill { side: side.to_string(), date, price, shares, fee, sort_order })
}

fn validate_balance(fills: &[Fill]) -> Result<()> {
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

fn parse_playbook(value: Option<&Value>) -> Result<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
        Some(Value::String(text)) if PLAYBOOKS.contains(&text.trim()) => Ok(Some(text.trim().to_string())),
        Some(Value::String(text)) => Err(anyhow!("不支持的打法: {}", text.trim())),
        _ => Err(anyhow!("打法格式不正确")),
    }
}

fn parse_planned(value: Option<&Value>) -> Result<Option<bool>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        _ => Err(anyhow!("as_planned 必须是布尔值或 null")),
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
        Some(Value::Number(value)) => value.as_f64().map(Some).ok_or_else(|| anyhow!("{label} 要是数字")),
        Some(Value::String(value)) if value.trim().is_empty() => Ok(None),
        Some(Value::String(value)) => value.trim().parse::<f64>().map(Some).map_err(|_| anyhow!("{label} 要是数字")),
        _ => Err(anyhow!("{label} 要是数字")),
    }
}

fn positive_optional(value: Option<&Value>, label: &str) -> Result<Option<f64>> {
    let value = optional_number(value, label)?;
    if value.is_some_and(|value| value <= 0.0) {
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

#[derive(Clone, Debug)]
struct Fill {
    side: String,
    date: String,
    price: f64,
    shares: f64,
    fee: Option<f64>,
    sort_order: i64,
}

struct CurrentEntry {
    kind: String,
    name: String,
    date: String,
    playbook: Option<String>,
    as_planned: Option<bool>,
    planned_stop: Option<f64>,
    planned_target: Option<f64>,
    note: String,
    pnl_pct: Option<f64>,
}
