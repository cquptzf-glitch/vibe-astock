//! 我的持仓 · 已实现盈亏与现金分红(移动版)。
//!
//! 口径(对照 orchestrator/src/finance/holdings.ts):
//!  - **现金分红单独记为收入,不改持仓成本**,也不并进已实现盈亏;页面把两项分开写再给合计。
//!  - 已实现盈亏沿用手机端「交易统计」的口径:**清仓后**才计入(净额,扣买卖费用,没填费用的按设置里的费率估);
//!    没卖完的交易只显示持有数量和成本,它已经卖出的那一部分不在这里算(桌面版按卖出就算,这是已知差异)。
//!  - 成本法默认移动加权平均;设置里可改成先进先出(只影响这里和交易日志的结转口径,不改原始成交)。
//!  - 分红是用户手填的(代码、日期、到手金额),不会自动抓;日期不能是未来,金额必须是正数,同代码同日同金额不重复收。
//!  - 个人持仓、盈亏、分红只留在本机、进备份,不进任何 AI。

use crate::{journal_risk, journal_settings, symbol::normalize_stock_symbol};
use anyhow::{anyhow, Context, Result};
use chrono::{NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

pub(crate) const DIVIDENDS_MAX: usize = 5000;
const NOTE_MAX: usize = 100;
const AMOUNT_MAX: f64 = 1_000_000_000.0;
const EPS: f64 = 1e-8;

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct Dividend {
    pub id: i64,
    pub symbol: String,
    pub date: String,
    pub amount: f64,
    pub note: String,
    pub created_at: String,
    pub updated_at: String,
}

fn open(db: &Path) -> Result<Connection> {
    Connection::open(db).context("open mobile portfolio sqlite")
}

pub(crate) fn read_dividends(conn: &Connection) -> Result<Vec<Dividend>> {
    let mut stmt = conn.prepare("SELECT id, symbol, date, amount, note, created_at, updated_at FROM mobile_dividends ORDER BY date DESC, id DESC")?;
    let rows = stmt.query_map([], |r| Ok(Dividend { id: r.get(0)?, symbol: r.get(1)?, date: r.get(2)?, amount: r.get(3)?, note: r.get(4)?, created_at: r.get(5)?, updated_at: r.get(6)? }))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub(crate) fn write_dividend(conn: &Connection, d: &Dividend) -> Result<()> {
    conn.execute(
        "INSERT INTO mobile_dividends(id, symbol, date, amount, note, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![d.id, d.symbol, d.date, d.amount, d.note, d.created_at, d.updated_at],
    )?;
    Ok(())
}

/// 一条分红哪里不对;没问题返回 None。today 为空时不查「未来日期」(备份恢复时按备份当时的口径放行)
pub(crate) fn dividend_problem(d: &Dividend, today: Option<&str>) -> Option<String> {
    if normalize_stock_symbol(&d.symbol).is_err() {
        return Some(format!("分红代码无效:{}", d.symbol));
    }
    if NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").is_err() {
        return Some(format!("分红日期无效:{}", d.date));
    }
    if today.is_some_and(|t| d.date.as_str() > t) {
        return Some("分红日期不能是未来(到账了再记)".into());
    }
    if !d.amount.is_finite() || d.amount <= 0.0 {
        return Some("分红金额必须是大于 0 的数(到手现金,元)".into());
    }
    if d.amount > AMOUNT_MAX {
        return Some("分红金额超出合理范围".into());
    }
    if d.note.chars().count() > NOTE_MAX {
        return Some(format!("分红备注最多 {NOTE_MAX} 字(现在 {} 字)", d.note.chars().count()));
    }
    None
}

#[derive(Default)]
struct Agg {
    name: String,
    open_shares: f64,
    open_cost: f64,
    realized: f64,
    closed_trades: usize,
    wins: usize,
    losses: usize,
    last_date: String,
    dividends: f64,
}

/// 纯计算:交易记录 + 分红 → 持仓 / 已清仓 / 合计
pub(crate) fn build(rows: &[journal_risk::Row], dividends: &[Dividend]) -> Value {
    let mut by: BTreeMap<String, Agg> = BTreeMap::new();
    for r in rows {
        if r.e.kind != "episode" && r.realized_pnl.is_none() {
            continue;
        }
        let a = by.entry(r.e.symbol.clone()).or_default();
        if a.name.is_empty() {
            a.name = r.e.name.clone();
        }
        if r.e.kind == "episode" && !r.closed && r.open_shares > EPS {
            a.open_shares += r.open_shares;
            a.open_cost += r.open_shares * r.avg_cost;
        }
        if let (true, Some(pnl)) = (r.e.kind == "episode", r.realized_pnl) {
            a.realized += pnl;
            a.closed_trades += 1;
            if pnl > 0.0 {
                a.wins += 1;
            } else if pnl < 0.0 {
                a.losses += 1;
            }
            let day = r.last_sell.clone().unwrap_or_else(|| r.e.date.clone());
            if day > a.last_date {
                a.last_date = day;
            }
        }
    }
    for d in dividends {
        let a = by.entry(d.symbol.clone()).or_default();
        a.dividends += d.amount;
        if d.date > a.last_date {
            a.last_date = d.date.clone();
        }
    }
    let names: BTreeMap<&str, &str> = rows.iter().filter(|r| !r.e.name.is_empty()).map(|r| (r.e.symbol.as_str(), r.e.name.as_str())).collect();
    let (mut realized_total, mut dividends_total) = (0.0, 0.0);
    let (mut closed_trades, mut wins, mut losses) = (0usize, 0usize, 0usize);
    let (mut holdings, mut closed): (Vec<Value>, Vec<Value>) = (Vec::new(), Vec::new());
    for (symbol, a) in &by {
        realized_total += a.realized;
        dividends_total += a.dividends;
        closed_trades += a.closed_trades;
        wins += a.wins;
        losses += a.losses;
        let name = names.get(symbol.as_str()).copied().unwrap_or(a.name.as_str());
        if a.open_shares > EPS {
            holdings.push(json!({
                "symbol": symbol, "name": name, "open_shares": r2(a.open_shares), "avg_cost": r2(a.open_cost / a.open_shares), "open_cost": r2(a.open_cost),
                "realized_pnl": r2(a.realized), "closed_trades": a.closed_trades, "dividends": r2(a.dividends),
            }));
        } else if a.closed_trades > 0 || a.dividends > 0.0 {
            closed.push(json!({
                "symbol": symbol, "name": name, "realized_pnl": r2(a.realized), "closed_trades": a.closed_trades, "dividends": r2(a.dividends), "last_date": a.last_date,
            }));
        }
    }
    holdings.sort_by(|x, y| y["open_cost"].as_f64().partial_cmp(&x["open_cost"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
    closed.sort_by(|x, y| y["last_date"].as_str().cmp(&x["last_date"].as_str()).then(x["symbol"].as_str().cmp(&y["symbol"].as_str())));
    json!({
        "totals": {
            "realized_pnl": r2(realized_total), "dividends": r2(dividends_total), "combined": r2(realized_total + dividends_total),
            "closed_trades": closed_trades, "wins": wins, "losses": losses, "open_symbols": holdings.len(), "closed_symbols": closed.len(),
        },
        "holdings": holdings,
        "closed": closed,
    })
}

pub fn view(db: &Path) -> Result<Value> {
    let conn = open(db)?;
    let dividends = read_dividends(&conn)?;
    let settings = journal_settings::load(db)?;
    let rows = journal_risk::rows_of(journal_risk::load_entries(db)?, &settings.fees);
    let mut value = build(&rows, &dividends);
    let names: BTreeMap<&str, &str> = rows.iter().filter(|r| !r.e.name.is_empty()).map(|r| (r.e.symbol.as_str(), r.e.name.as_str())).collect();
    value["dividends"] = json!(dividends
        .iter()
        .map(|d| json!({"id": d.id, "symbol": d.symbol, "name": names.get(d.symbol.as_str()).copied().unwrap_or(""), "date": d.date, "amount": d.amount, "note": d.note}))
        .collect::<Vec<_>>());
    value["fees_default"] = json!(settings.fees_default);
    value["method"] = json!(if settings.fees.fifo { "先进先出" } else { "移动加权平均" });
    value["note"] = json!("已实现盈亏只计清仓的交易(净额,扣买卖费用,没填费用的按设置里的费率估);现金分红单独记为收入,不改持仓成本、不并进已实现盈亏。分红是手填的,不会自动抓取。个人持仓、盈亏与分红只在本机,不进任何 AI。");
    Ok(value)
}

/// 登记 / 修改 / 删除一条现金分红。`{action:"save", id?, symbol, date, amount, note?}`、`{action:"delete", id}`
pub fn act(db: &Path, body: &Value) -> Result<Value> {
    let now = Utc::now().to_rfc3339();
    let today = crate::emotion::now_china().0.format("%Y-%m-%d").to_string();
    let conn = open(db)?;
    match body.get("action").and_then(Value::as_str) {
        Some("save") => {
            let text = |k: &str| body.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
            let symbol = normalize_stock_symbol(&text("symbol")).map_err(|e| anyhow!("分红代码无效:{e}"))?.digits;
            let amount = body.get("amount").and_then(Value::as_f64).ok_or_else(|| anyhow!("分红金额必须是数字"))?;
            let d = Dividend { id: 0, symbol, date: text("date"), amount, note: text("note"), created_at: now.clone(), updated_at: now };
            if let Some(problem) = dividend_problem(&d, Some(&today)) {
                return Err(anyhow!(problem));
            }
            let id = body.get("id").and_then(Value::as_i64);
            let dup: i64 = conn.query_row(
                "SELECT COUNT(*) FROM mobile_dividends WHERE symbol = ?1 AND date = ?2 AND ABS(amount - ?3) < 0.005 AND id != ?4",
                params![d.symbol, d.date, d.amount, id.unwrap_or(-1)],
                |r| r.get(0),
            )?;
            if dup > 0 {
                return Err(anyhow!("已经记过 {} 在 {} 的这笔分红(同金额),不重复收", d.symbol, d.date));
            }
            match id {
                Some(id) => {
                    let n = conn.execute(
                        "UPDATE mobile_dividends SET symbol = ?2, date = ?3, amount = ?4, note = ?5, updated_at = ?6 WHERE id = ?1",
                        params![id, d.symbol, d.date, d.amount, d.note, d.updated_at],
                    )?;
                    if n == 0 {
                        return Err(anyhow!("没有这条分红(可能已经删了)"));
                    }
                }
                None => {
                    let count: i64 = conn.query_row("SELECT COUNT(*) FROM mobile_dividends", [], |r| r.get(0))?;
                    if count as usize >= DIVIDENDS_MAX {
                        return Err(anyhow!("分红最多 {DIVIDENDS_MAX} 条"));
                    }
                    conn.execute(
                        "INSERT INTO mobile_dividends(symbol, date, amount, note, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![d.symbol, d.date, d.amount, d.note, d.created_at, d.updated_at],
                    )?;
                }
            }
        }
        Some("delete") => {
            let id = body.get("id").and_then(Value::as_i64).ok_or_else(|| anyhow!("要给出分红的 id"))?;
            if conn.execute("DELETE FROM mobile_dividends WHERE id = ?1", [id])? == 0 {
                return Err(anyhow!("没有这条分红(可能已经删了)"));
            }
        }
        _ => return Err(anyhow!("action 只能是 save 或 delete")),
    }
    view(db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal_risk::{Entry, Fill};

    fn fill(side: &str, date: &str, price: f64, shares: f64) -> Fill {
        Fill { side: side.into(), date: date.into(), price, shares, fee: Some(0.0) }
    }

    fn row(id: i64, symbol: &str, name: &str, fills: Vec<Fill>) -> journal_risk::Row {
        let fees = journal_settings::DEFAULT_FEES;
        journal_risk::settle(Entry { id, kind: "episode".into(), symbol: symbol.into(), name: name.into(), date: "2026-01-01".into(), fills, ..Entry::default() }, &fees, id as usize)
    }

    fn dividend(id: i64, symbol: &str, date: &str, amount: f64) -> Dividend {
        Dividend { id, symbol: symbol.into(), date: date.into(), amount, note: String::new(), created_at: "a".into(), updated_at: "a".into() }
    }

    #[test]
    fn realized_counts_only_closed_trades_and_dividends_stay_separate() {
        let rows = vec![
            row(1, "600519", "贵州茅台", vec![fill("buy", "2026-01-05", 100.0, 100.0), fill("sell", "2026-01-20", 110.0, 100.0)]), // +1000 已清仓
            row(2, "600519", "贵州茅台", vec![fill("buy", "2026-02-01", 120.0, 50.0), fill("buy", "2026-02-02", 130.0, 50.0)]),    // 在手 100 股,均价 125
            row(3, "000001", "平安银行", vec![fill("buy", "2026-03-01", 10.0, 1000.0), fill("sell", "2026-03-05", 9.0, 1000.0)]),   // -1000 已清仓
            row(4, "300308", "中际旭创", vec![fill("buy", "2026-03-01", 50.0, 200.0), fill("sell", "2026-03-03", 60.0, 100.0)]),     // 卖了一半:不算已实现
        ];
        let v = build(&rows, &[dividend(1, "600519", "2026-06-01", 300.0), dividend(2, "000002", "2026-06-02", 50.0)]);
        assert_eq!(v["totals"]["realized_pnl"], json!(0.0), "+1000 与 -1000 抵消;没清仓的 300308 不计");
        assert_eq!(v["totals"]["dividends"], json!(350.0));
        assert_eq!(v["totals"]["combined"], json!(350.0));
        assert_eq!((v["totals"]["closed_trades"].clone(), v["totals"]["wins"].clone(), v["totals"]["losses"].clone()), (json!(2), json!(1), json!(1)));
        let hold: Vec<&str> = v["holdings"].as_array().unwrap().iter().map(|h| h["symbol"].as_str().unwrap()).collect();
        assert_eq!(hold, ["600519", "300308"], "在手按占用成本从大到小");
        let mt = &v["holdings"][0];
        assert_eq!((mt["open_shares"].clone(), mt["avg_cost"].clone(), mt["realized_pnl"].clone(), mt["dividends"].clone()), (json!(100.0), json!(125.0), json!(1000.0), json!(300.0)));
        let closed: Vec<&str> = v["closed"].as_array().unwrap().iter().map(|h| h["symbol"].as_str().unwrap()).collect();
        assert_eq!(closed, ["000002", "000001"], "只有分红没有交易的也列在已清仓,按最近日期从新到旧");
        assert_eq!(v["closed"][0]["name"], "", "没有交易记录的代码不编名字");
        assert_eq!(v["closed"][1]["realized_pnl"], json!(-1000.0));
    }

    #[test]
    fn dividend_rules_reject_future_dates_bad_amounts_and_overlong_notes() {
        let ok = dividend(0, "600519", "2026-06-01", 300.0);
        assert!(dividend_problem(&ok, Some("2026-10-17")).is_none());
        assert!(dividend_problem(&ok, Some("2026-05-31")).unwrap().contains("未来"));
        assert!(dividend_problem(&ok, None).is_none(), "恢复备份时不查未来日期");
        assert!(dividend_problem(&dividend(0, "600519", "2026-13-40", 1.0), None).unwrap().contains("日期无效"));
        assert!(dividend_problem(&dividend(0, "600519", "2026-06-01", 0.0), None).unwrap().contains("大于 0"));
        assert!(dividend_problem(&dividend(0, "600519", "2026-06-01", f64::NAN), None).is_some());
        assert!(dividend_problem(&dividend(0, "abc", "2026-06-01", 1.0), None).unwrap().contains("代码无效"));
        let mut long = ok.clone();
        long.note = "字".repeat(101);
        assert!(dividend_problem(&long, None).unwrap().contains("最多 100 字"));
    }

    fn db(name: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("vibe-portfolio-{name}-{nonce}.sqlite3"));
        crate::storage::init(&path).unwrap();
        crate::journal::init(&path).unwrap();
        crate::daily_review::init(&path).unwrap();
        crate::ai_chat::init(&path).unwrap();
        crate::ai_config::init(&path).unwrap();
        crate::schema::migrate(&path).unwrap();
        path
    }

    #[test]
    fn save_edit_delete_and_duplicates_through_the_database() {
        let path = db("act");
        let v = act(&path, &json!({"action": "save", "symbol": "600519", "date": "2026-06-01", "amount": 300.5, "note": "年度分红"})).unwrap();
        assert_eq!(v["totals"]["dividends"], json!(300.5));
        let id = v["dividends"][0]["id"].as_i64().unwrap();
        assert!(act(&path, &json!({"action": "save", "symbol": "600519", "date": "2026-06-01", "amount": 300.5})).unwrap_err().to_string().contains("不重复收"));
        let edited = act(&path, &json!({"action": "save", "id": id, "symbol": "600519", "date": "2026-06-01", "amount": 400.0})).unwrap();
        assert_eq!(edited["totals"]["dividends"], json!(400.0), "改自己这条不算重复");
        assert!(act(&path, &json!({"action": "save", "symbol": "600519", "date": "2999-01-01", "amount": 1.0})).unwrap_err().to_string().contains("未来"));
        assert!(act(&path, &json!({"action": "save", "symbol": "600519", "date": "2026-06-01", "amount": "abc"})).is_err());
        assert!(act(&path, &json!({"action": "save", "id": 999, "symbol": "600519", "date": "2026-06-02", "amount": 1.0})).unwrap_err().to_string().contains("没有这条分红"));
        let gone = act(&path, &json!({"action": "delete", "id": id})).unwrap();
        assert_eq!(gone["totals"]["dividends"], json!(0.0));
        assert!(act(&path, &json!({"action": "delete", "id": id})).is_err());
        assert!(act(&path, &json!({"action": "x"})).is_err());
        let _ = std::fs::remove_file(path);
    }
}
