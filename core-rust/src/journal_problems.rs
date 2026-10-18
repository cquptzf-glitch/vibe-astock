//! 交易日志 · 问题记录与修复(对应桌面版交易日志列出的「不参与计算的流水 / 挂不上的标注 / 不是 A 股的代码」,口径见 orchestrator/src/finance/journal/{episodes,rows}.ts)。
//!
//! 手机端的一笔交易是一条记录(带成交明细),不像桌面版那样从流水里切出来,所以问题长得不一样:
//!   - 卖出比当时持有的多(含「卖在买之前」):结算按日期顺序重放,超出的部分会被忽略 —— 这里把它们找出来,让用户改日期 / 股数或删掉那笔;
//!   - 成交日期不是有效日期:同样不能参与按日期的结算;
//!   - 没有任何成交的记录:算不出盈亏;
//!   - 没填盈亏 % 的补记:不进统计;
//!   - 不是 A 股股票的记录(ETF、基金、债券、回购):交易日志只算 A 股股票,它们只计入持仓、不进交易日志统计 —— 只列出来,不当成错误。
//! 🔴 修复只动用户点的那一笔:改成交 / 删成交 / 删记录 / 补盈亏。改完不能比改之前更糟(问题成交数不增加),并且一条记录至少留一笔成交;
//!    改了成交日期,记录的入场日跟着最早一笔成交走。
//! ⛔ 这些数据只留本机,不进任何 AI。

use crate::journal_risk::is_journal_symbol;
use anyhow::{anyhow, Context, Result};
use chrono::{NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

const EPS: f64 = 1e-6;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FillRow {
    pub id: i64,
    pub side: String,
    pub date: String,
    pub price: f64,
    pub shares: f64,
    pub fee: Option<f64>,
    pub sort_order: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct EntryRow {
    pub id: i64,
    pub kind: String,
    pub symbol: String,
    pub name: String,
    pub date: String,
    pub pnl_pct: Option<f64>,
    pub fills: Vec<FillRow>,
}

fn open(db: &Path) -> Result<Connection> {
    let conn = Connection::open(db).context("open mobile journal problems sqlite")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    Ok(conn)
}

fn valid_day(day: &str) -> bool {
    day.len() == 10 && NaiveDate::parse_from_str(day, "%Y-%m-%d").is_ok()
}

fn read_fills(conn: &Connection, entry_id: i64) -> Result<Vec<FillRow>> {
    let mut stmt = conn.prepare("SELECT id, side, date, price, shares, fee, sort_order FROM journal_fills WHERE entry_id = ?1 ORDER BY sort_order, id")?;
    let rows = stmt
        .query_map([entry_id], |r| Ok(FillRow { id: r.get(0)?, side: r.get(1)?, date: r.get(2)?, price: r.get(3)?, shares: r.get(4)?, fee: r.get(5)?, sort_order: r.get(6)? }))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub(crate) fn read_entries(conn: &Connection) -> Result<Vec<EntryRow>> {
    let mut stmt = conn.prepare("SELECT id, kind, symbol, name, date, pnl_pct FROM journal_entries ORDER BY date DESC, id DESC")?;
    let raw = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, Option<f64>>(5)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    raw.into_iter()
        .map(|(id, kind, symbol, name, date, pnl_pct)| Ok(EntryRow { fills: if kind == "episode" { read_fills(conn, id)? } else { Vec::new() }, id, kind, symbol, name, date, pnl_pct }))
        .collect()
}

/// 按日期顺序重放(同一天按录入顺序):卖出比当时持有多的成交 → (成交 id, 当时持有, 卖出股数)。日期无效的单列
pub(crate) fn fill_problems(fills: &[FillRow]) -> Vec<(i64, &'static str, String)> {
    let mut sorted: Vec<&FillRow> = fills.iter().filter(|f| valid_day(&f.date)).collect();
    sorted.sort_by(|a, b| a.date.cmp(&b.date).then((a.side!="split").cmp(&(b.side!="split"))).then(a.sort_order.cmp(&b.sort_order)).then(a.id.cmp(&b.id)));
    let mut out: Vec<(i64, &'static str, String)> = fills.iter().filter(|f| !valid_day(&f.date)).map(|f| (f.id, "bad_date", format!("成交日期 {:?} 不是有效日期,没法按日期结算", f.date))).collect();
    let mut pos = 0.0_f64;
    for f in sorted {
        if f.side == "split" {
            if pos <= EPS { out.push((f.id,"oversell",format!("{} 送转拆股时没有持仓",f.date))); }
            pos *= f.price;
        } else if matches!(f.side.as_str(), "buy" | "opening") {
            pos += f.shares;
        } else {
            if f.shares > pos + EPS {
                let why = if pos <= EPS { format!("{} 卖出 {} 股,但那天之前没有买入 —— 卖在买之前,或买入日期写晚了", f.date, f.shares) } else { format!("{} 卖出 {} 股,当时只持有 {} 股,超出的部分结算时会被忽略", f.date, f.shares, pos) };
                out.push((f.id, "oversell", why));
            }
            pos = (pos - f.shares).max(0.0);
        }
    }
    out
}

fn fill_value(f: &FillRow) -> Value {
    json!({"id": f.id, "type": f.side, "date": f.date, "price": f.price, "shares": f.shares, "fee": f.fee})
}

fn entry_card(e: &EntryRow, problems: Vec<Value>) -> Value {
    json!({"entry_id": e.id, "kind": e.kind, "symbol": e.symbol, "name": e.name, "date": e.date, "pnl_pct": e.pnl_pct, "problems": problems, "fills": e.fills.iter().map(fill_value).collect::<Vec<_>>()})
}

/// 纯计算:从全部记录里找出问题
pub(crate) fn find_problems(entries: &[EntryRow]) -> Value {
    let (mut cards, mut non_stock): (Vec<Value>, Vec<Value>) = (Vec::new(), Vec::new());
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let (mut fills_total, mut fills_no_fee) = (0usize, 0usize);
    for e in entries {
        if !is_journal_symbol(&e.symbol) {
            non_stock.push(json!({"entry_id": e.id, "symbol": e.symbol, "name": e.name, "date": e.date, "fills": e.fills.len()}));
            continue;
        }
        let mut problems: Vec<Value> = Vec::new();
        let mut add = |kind: &'static str, fill_id: Option<i64>, why: String| {
            *counts.entry(kind).or_default() += 1;
            problems.push(json!({"kind": kind, "fill_id": fill_id, "why": why}));
        };
        if e.kind == "episode" {
            fills_total += e.fills.len();
            fills_no_fee += e.fills.iter().filter(|f| f.fee.is_none()).count();
            if e.fills.is_empty() {
                add("no_fills", None, "这条记录没有任何成交,算不出盈亏".into());
            }
            for (id, kind, why) in fill_problems(&e.fills) {
                add(kind, Some(id), why);
            }
        } else if e.pnl_pct.is_none() {
            add("manual_no_pnl", None, "补记没填盈亏 %,不进任何统计".into());
        } else if !valid_day(&e.date) {
            add("manual_bad_date", None, format!("补记日期 {:?} 不是有效日期", e.date));
        }
        if !problems.is_empty() {
            cards.push(entry_card(e, problems));
        }
    }
    json!({
        "available": true,
        "problem_count": counts.values().sum::<usize>(),
        "counts": counts,
        "entries": cards,
        "non_stock": non_stock,
        "info": {"entries": entries.len(), "fills": fills_total, "fills_without_fee": fills_no_fee},
        "notes": [
            "交易日志只算 A 股股票:ETF、基金、债券、可转债、回购只计入持仓,不进交易日志的统计;下面只列出来,不算错误",
            "修复只动你点的那一笔;改完不能比改之前更糟,一条记录至少留一笔成交。成交只在本机,不进任何 AI",
            "没填费用的成交按「风险与纪律」里设置的费率估,不算问题",
        ],
    })
}

pub fn view(db: &Path) -> Result<Value> {
    let conn = open(db)?;
    let mut v = find_problems(&read_entries(&conn)?);
    v["generated_at"] = json!(Utc::now().to_rfc3339());
    Ok(v)
}

// ---------------------------------------------------------------- 修复

fn number(value: Option<&Value>, label: &str) -> Result<Option<f64>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n.as_f64().filter(|v| v.is_finite()).map(Some).ok_or_else(|| anyhow!("{label} 要是数字")),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok().filter(|v| v.is_finite()).map(Some).ok_or_else(|| anyhow!("{label} 要是数字")),
        _ => Err(anyhow!("{label} 要是数字")),
    }
}

fn find_fill(conn: &Connection, fill_id: i64) -> Result<(EntryRow, FillRow)> {
    let entry_id: i64 = conn.query_row("SELECT entry_id FROM journal_fills WHERE id = ?1", [fill_id], |r| r.get(0)).map_err(|_| anyhow!("没找到这笔成交,可能已经删了;请刷新后重试"))?;
    let entry = read_entries(conn)?.into_iter().find(|e| e.id == entry_id).ok_or_else(|| anyhow!("没找到这条记录"))?;
    let fill = entry.fills.iter().find(|f| f.id == fill_id).cloned().ok_or_else(|| anyhow!("没找到这笔成交"))?;
    Ok((entry, fill))
}

/// 改完之后记录的入场日 = 最早一笔成交;不合格的日期不参与(那一笔本来就是问题)
fn sync_entry_date(conn: &Connection, entry_id: i64) -> Result<()> {
    let fills = read_fills(conn, entry_id)?;
    if let Some(min) = fills.iter().filter(|f| valid_day(&f.date)).map(|f| f.date.clone()).min() {
        conn.execute("UPDATE journal_entries SET date = ?2, updated_at = ?3 WHERE id = ?1", params![entry_id, min, Utc::now().to_rfc3339()])?;
    }
    Ok(())
}

fn ensure_not_worse(before: &[FillRow], after: &[FillRow]) -> Result<()> {
    if fill_problems(after).len() > fill_problems(before).len() {
        return Err(anyhow!("这样改会让这条记录出现更多问题(卖出比持有多、日期无效),没有改"));
    }
    Ok(())
}

fn update_fill(db: &Path, body: &Value) -> Result<Value> {
    let fill_id = body.get("fill_id").and_then(Value::as_i64).ok_or_else(|| anyhow!("缺少成交 id"))?;
    let conn = open(db)?;
    let (entry, old) = find_fill(&conn, fill_id)?;
    if entry.kind != "episode" {
        return Err(anyhow!("补记没有成交明细"));
    }
    let date = match body.get("date") {
        None | Some(Value::Null) => old.date.clone(),
        Some(Value::String(s)) if valid_day(s.trim()) => s.trim().to_string(),
        Some(_) => return Err(anyhow!("日期格式应为 YYYY-MM-DD")),
    };
    let price = number(body.get("price"), "成交价")?.unwrap_or(old.price);
    let shares = number(body.get("shares"), "股数")?.unwrap_or(old.shares);
    if price <= 0.0 {
        return Err(anyhow!("成交价 要大于 0"));
    }
    if shares <= 0.0 {
        return Err(anyhow!("股数 要大于 0"));
    }
    let fee = if body.get("fee").is_some() { number(body.get("fee"), "费用")? } else { old.fee };
    if fee.is_some_and(|v| v < 0.0) {
        return Err(anyhow!("费用不能是负数"));
    }
    let mut after = entry.fills.clone();
    if let Some(f) = after.iter_mut().find(|f| f.id == fill_id) {
        f.date = date.clone();
        f.price = price;
        f.shares = shares;
        f.fee = fee;
    }
    ensure_not_worse(&entry.fills, &after)?;
    conn.execute("UPDATE journal_fills SET date = ?2, price = ?3, shares = ?4, fee = ?5 WHERE id = ?1", params![fill_id, date, price, shares, fee])?;
    sync_entry_date(&conn, entry.id)?;
    Ok(json!({"entry_id": entry.id, "fill_id": fill_id, "updated": true}))
}

fn delete_fill(db: &Path, body: &Value) -> Result<Value> {
    let fill_id = body.get("fill_id").and_then(Value::as_i64).ok_or_else(|| anyhow!("缺少成交 id"))?;
    let conn = open(db)?;
    let (entry, _) = find_fill(&conn, fill_id)?;
    if entry.fills.len() <= 1 {
        return Err(anyhow!("这是这条记录里最后一笔成交,删了就只剩空记录;要删整条请用「删除整条记录」"));
    }
    let after: Vec<FillRow> = entry.fills.iter().filter(|f| f.id != fill_id).cloned().collect();
    ensure_not_worse(&entry.fills, &after)?;
    conn.execute("DELETE FROM journal_fills WHERE id = ?1", [fill_id])?;
    sync_entry_date(&conn, entry.id)?;
    Ok(json!({"entry_id": entry.id, "fill_id": fill_id, "removed": true}))
}

fn delete_entry(db: &Path, body: &Value) -> Result<Value> {
    let id = body.get("entry_id").and_then(Value::as_i64).ok_or_else(|| anyhow!("缺少记录 id"))?;
    let conn = open(db)?;
    let symbol: String = conn.query_row("SELECT symbol FROM journal_entries WHERE id = ?1", [id], |r| r.get(0)).map_err(|_| anyhow!("没找到这条记录,可能已经删了"))?;
    // 不是 A 股的记录是持仓的一部分(持仓由成交推导),不在这里删:去「交易日志」里删
    if !is_journal_symbol(&symbol) {
        return Err(anyhow!("这条记录不是 A 股股票,它只计入持仓;要删请到「交易日志」页"));
    }
    conn.execute("DELETE FROM journal_entries WHERE id = ?1", [id])?;
    Ok(json!({"entry_id": id, "removed": true}))
}

fn set_manual_pnl(db: &Path, body: &Value) -> Result<Value> {
    let id = body.get("entry_id").and_then(Value::as_i64).ok_or_else(|| anyhow!("缺少记录 id"))?;
    let pnl = number(body.get("pnl_pct"), "盈亏 %")?.ok_or_else(|| anyhow!("盈亏 % 不能为空"))?;
    if !(-100.0..=10000.0).contains(&pnl) {
        return Err(anyhow!("盈亏 % 要在 -100 到 10000 之间"));
    }
    let conn = open(db)?;
    let kind: String = conn.query_row("SELECT kind FROM journal_entries WHERE id = ?1", [id], |r| r.get(0)).map_err(|_| anyhow!("没找到这条记录"))?;
    if kind != "manual" {
        return Err(anyhow!("只有没有成交明细的补记才能手填盈亏 %"));
    }
    conn.execute("UPDATE journal_entries SET pnl_pct = ?2, updated_at = ?3 WHERE id = ?1", params![id, pnl, Utc::now().to_rfc3339()])?;
    Ok(json!({"entry_id": id, "updated": true}))
}

/// `POST /mobile/journal/problems`:`action` = update_fill / delete_fill / delete_entry / set_manual_pnl;成功后连同新的视图一起返回
pub fn act(db: &Path, body: &Value) -> Result<Value> {
    let result = match body.get("action").and_then(Value::as_str).unwrap_or("") {
        "update_fill" => update_fill(db, body)?,
        "delete_fill" => delete_fill(db, body)?,
        "delete_entry" => delete_entry(db, body)?,
        "set_manual_pnl" => set_manual_pnl(db, body)?,
        other => return Err(anyhow!("不认识的动作 {other:?}(只有 update_fill / delete_fill / delete_entry / set_manual_pnl)")),
    };
    Ok(json!({"result": result, "view": view(db)?}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(id: i64, side: &str, date: &str, shares: f64) -> FillRow {
        FillRow { id, side: side.into(), date: date.into(), price: 10.0, shares, fee: Some(0.0), sort_order: id }
    }

    fn kinds(fills: &[FillRow]) -> Vec<(i64, &'static str)> {
        fill_problems(fills).into_iter().map(|(id, k, _)| (id, k)).collect()
    }

    #[test]
    fn replay_by_date_finds_sells_before_buys_and_oversells_and_bad_dates() {
        // 卖在买之前:录入顺序没问题(先买后卖),但按日期卖出 10-12 早于买入 10-14
        let early = vec![fill(1, "buy", "2026-10-14", 100.0), fill(2, "sell", "2026-10-12", 100.0)];
        assert_eq!(kinds(&early), vec![(2, "oversell")]);
        assert!(fill_problems(&early)[0].2.contains("卖在买之前"));
        // 卖超:持有 100 卖 150
        let over = vec![fill(1, "buy", "2026-10-12", 100.0), fill(2, "sell", "2026-10-13", 150.0)];
        assert_eq!(kinds(&over), vec![(2, "oversell")]);
        assert!(fill_problems(&over)[0].2.contains("当时只持有 100 股"));
        // 同一天先买后卖按录入顺序;正常的整轮没有问题
        let same_day = vec![fill(1, "buy", "2026-10-12", 100.0), fill(2, "sell", "2026-10-12", 100.0), fill(3, "buy", "2026-10-13", 50.0), fill(4, "sell", "2026-10-14", 50.0)];
        assert!(fill_problems(&same_day).is_empty());
        // 日期无效
        let bad = vec![fill(1, "buy", "2026-13-40", 100.0)];
        assert_eq!(kinds(&bad), vec![(1, "bad_date")]);
        // 卖超之后继续按持有量扣到 0,不会让后面的正常卖出被误报
        let chain = vec![fill(1, "buy", "2026-10-12", 100.0), fill(2, "sell", "2026-10-13", 150.0), fill(3, "buy", "2026-10-14", 100.0), fill(4, "sell", "2026-10-15", 100.0)];
        assert_eq!(kinds(&chain), vec![(2, "oversell")]);
    }

    fn entry(id: i64, kind: &str, symbol: &str, pnl: Option<f64>, fills: Vec<FillRow>) -> EntryRow {
        EntryRow { id, kind: kind.into(), symbol: symbol.into(), name: format!("N{id}"), date: "2026-10-12".into(), pnl_pct: pnl, fills }
    }

    #[test]
    fn find_problems_lists_each_class_and_keeps_non_stock_out_of_the_error_count() {
        let entries = vec![
            entry(1, "episode", "600001", None, vec![fill(1, "buy", "2026-10-12", 100.0), fill(2, "sell", "2026-10-13", 100.0)]),
            entry(2, "episode", "600002", None, vec![fill(3, "buy", "2026-10-14", 100.0), fill(4, "sell", "2026-10-12", 100.0)]),
            entry(3, "episode", "600003", None, vec![]),
            entry(4, "manual", "600004", None, vec![]),
            entry(5, "manual", "600005", Some(2.0), vec![]),
            entry(6, "episode", "510300", None, vec![fill(5, "buy", "2026-10-12", 100.0), fill(6, "sell", "2026-10-11", 100.0)]),
        ];
        let v = find_problems(&entries);
        assert_eq!(v["problem_count"], 3);
        assert_eq!((v["counts"]["oversell"].clone(), v["counts"]["no_fills"].clone(), v["counts"]["manual_no_pnl"].clone()), (json!(1), json!(1), json!(1)));
        let ids: Vec<i64> = v["entries"].as_array().unwrap().iter().map(|e| e["entry_id"].as_i64().unwrap()).collect();
        assert_eq!(ids, vec![2, 3, 4]);
        assert_eq!(v["entries"][0]["fills"].as_array().unwrap().len(), 2);
        assert_eq!(v["non_stock"][0]["symbol"], "510300", "ETF 只列出来,不算错误,也不检查它的成交顺序");
        assert_eq!(v["info"]["fills_without_fee"], 0);
        assert_eq!(find_problems(&[])["problem_count"], 0);
    }

    fn db(label: &str) -> (crate::AppCore, std::path::PathBuf) {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-jprob-{label}-{nonce}-{}", std::process::id()));
        (crate::AppCore::new(dir.clone()).unwrap(), dir)
    }

    /// 绕过录入校验造一条「卖在买之前」的记录:先按录入顺序合法地录入,再把买入日期改晚(旧数据 / 恢复的备份里可能有这种)
    fn broken_entry(db: &Path) -> (i64, i64, i64) {
        crate::journal::act(db, &json!({"action": "record", "symbol": "600519", "name": "贵州茅台",
            "fills": [{"type": "buy", "date": "2026-10-12", "price": 100, "shares": 100, "fee": 5}, {"type": "sell", "date": "2026-10-13", "price": 110, "shares": 100, "fee": 6}]})).unwrap();
        let conn = open(db).unwrap();
        let ids: Vec<i64> = conn.prepare("SELECT id FROM journal_fills ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().map(|x| x.unwrap()).collect();
        conn.execute("UPDATE journal_fills SET date = '2026-10-15' WHERE id = ?1", [ids[0]]).unwrap();
        (1, ids[0], ids[1])
    }

    #[test]
    fn update_fill_fixes_a_problem_keeps_the_entry_date_in_sync_and_never_makes_things_worse() {
        let (app, dir) = db("update");
        let path = app.db_path().to_path_buf();
        let (entry_id, buy, sell) = broken_entry(&path);
        let v = view(&path).unwrap();
        assert_eq!((v["problem_count"].clone(), v["entries"][0]["entry_id"].clone()), (json!(1), json!(entry_id)));
        // 改坏:把卖出再提前到 10-10 —— 问题没变多(还是 1 个),允许;但把股数改成 0 或日期乱写要拒绝
        for (body, needle) in [
            (json!({"action": "update_fill", "fill_id": sell, "shares": 0}), "股数 要大于 0"),
            (json!({"action": "update_fill", "fill_id": sell, "price": -1}), "成交价 要大于 0"),
            (json!({"action": "update_fill", "fill_id": sell, "date": "2026-2-30"}), "日期格式"),
            (json!({"action": "update_fill", "fill_id": sell, "fee": -3}), "费用不能是负数"),
            (json!({"action": "update_fill", "fill_id": 9999, "date": "2026-10-13"}), "没找到这笔成交"),
            (json!({"action": "nope"}), "不认识的动作"),
        ] {
            let e = act(&path, &body).unwrap_err().to_string();
            assert!(e.contains(needle), "{e} 应含 {needle}");
        }
        // 把买入日期改回 10-12:问题消失,入场日跟着变
        let fixed = act(&path, &json!({"action": "update_fill", "fill_id": buy, "date": "2026-10-12"})).unwrap();
        assert_eq!(fixed["view"]["problem_count"], 0);
        let conn = open(&path).unwrap();
        let date: String = conn.query_row("SELECT date FROM journal_entries WHERE id = ?1", [entry_id], |r| r.get(0)).unwrap();
        assert_eq!(date, "2026-10-12");
        // 现在状态是好的:再把卖出提前到买入之前会多出问题,必须拒绝,数据不动
        let e = act(&path, &json!({"action": "update_fill", "fill_id": sell, "date": "2026-10-10"})).unwrap_err().to_string();
        assert!(e.contains("更多问题"), "{e}");
        let still: String = conn.query_row("SELECT date FROM journal_fills WHERE id = ?1", [sell], |r| r.get(0)).unwrap();
        assert_eq!(still, "2026-10-13");
        // 修复后统计口径回到正常
        let risk = crate::journal_risk::view(&path).unwrap();
        assert_eq!(risk["risk"]["equity"]["trades"], 1);
        drop(conn);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_fill_keeps_one_fill_and_entry_delete_refuses_non_stock_records() {
        let (app, dir) = db("delete");
        let path = app.db_path().to_path_buf();
        let (entry_id, _buy, sell) = broken_entry(&path);
        // 删掉那笔多余的卖出:问题消失
        let r = act(&path, &json!({"action": "delete_fill", "fill_id": sell})).unwrap();
        assert_eq!(r["view"]["problem_count"], 0);
        // 只剩一笔成交了,不能再删
        let conn = open(&path).unwrap();
        let left: i64 = conn.query_row("SELECT id FROM journal_fills WHERE entry_id = ?1", [entry_id], |r| r.get(0)).unwrap();
        assert!(act(&path, &json!({"action": "delete_fill", "fill_id": left})).unwrap_err().to_string().contains("最后一笔成交"));
        // 删整条
        let gone = act(&path, &json!({"action": "delete_entry", "entry_id": entry_id})).unwrap();
        assert_eq!(gone["view"]["info"]["entries"], 0);
        assert!(act(&path, &json!({"action": "delete_entry", "entry_id": entry_id})).unwrap_err().to_string().contains("没找到"));
        // ETF 记录是持仓的一部分:这里不删
        crate::journal::act(&path, &json!({"action": "record", "symbol": "510300", "name": "沪深300ETF", "fills": [{"type": "buy", "date": "2026-10-12", "price": 4, "shares": 1000}]})).unwrap();
        let etf: i64 = conn.query_row("SELECT id FROM journal_entries WHERE symbol = '510300'", [], |r| r.get(0)).unwrap();
        assert!(act(&path, &json!({"action": "delete_entry", "entry_id": etf})).unwrap_err().to_string().contains("只计入持仓"));
        assert_eq!(view(&path).unwrap()["non_stock"][0]["symbol"], "510300");
        drop(conn);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn manual_entries_without_pnl_can_be_filled_in_and_only_manual_ones() {
        let (app, dir) = db("manual");
        let path = app.db_path().to_path_buf();
        crate::journal::act(&path, &json!({"action": "manual", "symbol": "600036", "name": "招商银行", "date": "2026-10-12"})).unwrap();
        assert_eq!(view(&path).unwrap()["counts"]["manual_no_pnl"], 1);
        assert!(act(&path, &json!({"action": "set_manual_pnl", "entry_id": 1, "pnl_pct": "abc"})).is_err());
        assert!(act(&path, &json!({"action": "set_manual_pnl", "entry_id": 1, "pnl_pct": -150})).unwrap_err().to_string().contains("-100"));
        assert!(act(&path, &json!({"action": "set_manual_pnl", "entry_id": 1})).unwrap_err().to_string().contains("不能为空"));
        let ok = act(&path, &json!({"action": "set_manual_pnl", "entry_id": 1, "pnl_pct": "3.5"})).unwrap();
        assert_eq!(ok["view"]["problem_count"], 0);
        crate::journal::act(&path, &json!({"action": "record", "symbol": "600519", "name": "x", "fills": [{"type": "buy", "date": "2026-10-12", "price": 1, "shares": 100}]})).unwrap();
        assert!(act(&path, &json!({"action": "set_manual_pnl", "entry_id": 2, "pnl_pct": 1})).unwrap_err().to_string().contains("补记"));
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 生成前端冻结夹具(全部是合成数据):
    /// VIBE_JOURNAL_PROBLEMS_FIXTURE=路径 cargo test --locked journal_problems::tests::dump_synthetic_fixture -- --ignored
    #[test]
    #[ignore]
    fn dump_synthetic_fixture() {
        let (app, dir) = db("fixture");
        let path = app.db_path().to_path_buf();
        let rec = |code: &str, name: &str, fills: Value| crate::journal::act(&path, &json!({"action": "record", "symbol": code, "name": name, "fills": fills})).unwrap();
        rec("600101", "样本A", json!([{"type": "buy", "date": "2026-10-12", "price": 10, "shares": 1000, "fee": 5}, {"type": "sell", "date": "2026-10-13", "price": 10.5, "shares": 1000}]));
        rec("600102", "样本B", json!([{"type": "buy", "date": "2026-10-12", "price": 20, "shares": 500}, {"type": "sell", "date": "2026-10-13", "price": 21, "shares": 500}]));
        rec("600103", "样本C", json!([{"type": "buy", "date": "2026-10-09", "price": 8, "shares": 2000}, {"type": "sell", "date": "2026-10-12", "price": 8.4, "shares": 1200}, {"type": "sell", "date": "2026-10-13", "price": 8.6, "shares": 800}]));
        rec("510300", "沪深300ETF样本", json!([{"type": "buy", "date": "2026-10-12", "price": 4, "shares": 5000}]));
        crate::journal::act(&path, &json!({"action": "manual", "symbol": "600104", "name": "样本补记", "date": "2026-10-10"})).unwrap();
        let conn = open(&path).unwrap();
        // 样本B:买入日期写晚了 → 卖在买之前;样本C:第二笔卖出多写了 300 股
        conn.execute("UPDATE journal_fills SET date = '2026-10-15' WHERE entry_id = 2 AND side = 'buy'", []).unwrap();
        conn.execute("UPDATE journal_fills SET shares = 1100 WHERE entry_id = 3 AND date = '2026-10-13'", []).unwrap();
        conn.execute("INSERT INTO journal_entries(kind, symbol, name, date, note, created_at, updated_at) VALUES ('episode', '600105', '样本空记录', '2026-10-11', '', 'a', 'a')", []).unwrap();
        let mut v = view(&path).unwrap();
        v["generated_at"] = json!("2026-10-17T02:00:00Z");
        if let Ok(out) = std::env::var("VIBE_JOURNAL_PROBLEMS_FIXTURE") {
            std::fs::write(out, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        }
        println!("problems {} counts {} non_stock {}", v["problem_count"], v["counts"], v["non_stock"]);
        drop(conn);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
