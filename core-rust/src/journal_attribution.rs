//! 交易日志 · 看错了,还是没执行好(判断 vs 执行归因,移植自 orchestrator/src/finance/journal/attribution.ts,口径来自 Vibe AStock v2.0.0 `duanxian/attribution.py`)。
//!
//! 把「市场判断成没成立」和「自己赚没赚钱」交叉起来。只看一层会得出错的结论:看对了却亏钱和看错了才亏钱,要改的地方不同
//! (前者改执行,后者改读盘)。
//!
//! ```text
//!              自己赚钱      自己亏钱
//!   判对了      顺风顺水      执行问题
//!   判错了      运气          判断问题
//! ```
//! 「判错还赚钱」那格必须单独点出来,不能混进「总体盈利」里。
//!
//! 🔴 对齐哪一天:交易在 D 日入场,依据是 D-1 晚那份判断 ⇒ 交易的**入场日**对应条件的**被检验日**。
//!    不能按卖出日对齐:D 日买、D+2 日卖,按卖出日会去比 D+1 晚的判断 —— 那份判断没参与这笔决策。
//! 🔴 和桌面版 VA 原版不同:「判对没判对」来自短线复盘里写给下一场的**验证条件**对账结果(「明日验证」那一页,事后写的、暂定的不算):
//!    判定了的条件过半成立 = 判对,不到一半 = 判错,正好一半或一条都判不了的日子不进四格。只覆盖对账能回看的最近约 60 个交易日。
//! 🔴 手机端只有清仓后才算已实现盈亏,所以只有卖光了、有成交明细的交易才进四格;盈亏为 0 的日子不进任何一格。
//! ⛔ 这些数据只留本机,不进任何 AI。

use crate::discovery::{self, DiscoveryError};
use crate::emotion::{self, ChinaNow};
use crate::emotion_history;
use crate::journal_risk::{self, entry_day, Row};
use chrono::{NaiveDate, Utc};
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// 四格。顺序别改,页面按 key 取
pub(crate) const QUADRANTS: [(&str, &str); 4] = [
    ("right_win", "判对 + 赚钱"),
    ("right_lose", "判对 + 亏钱"),
    ("wrong_win", "判错 + 赚钱"),
    ("wrong_lose", "判错 + 亏钱"),
];
/// 少于这么多天,四格分布是噪声,不给倾向性描述
pub(crate) const ATTR_MIN_DAYS: usize = 8;
/// 只看最近这么多笔(桌面版同)
pub(crate) const ATTR_LIMIT: usize = 500;

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Verdict {
    Right,
    Wrong,
    Undecided,
}

#[derive(Clone, Debug)]
pub(crate) struct DayRead {
    pub set_date: String,
    pub decided: i64,
    pub hit: i64,
    pub verdict: Verdict,
}

/// 条件对账(`verify_view` 的 track.days)→ 每个被检验日判对 / 判错 / 判不了
pub(crate) fn reads_of(days: &[Value]) -> HashMap<String, DayRead> {
    let mut out = HashMap::new();
    for d in days {
        let (decided, hit) = (
            d["decided"].as_i64().unwrap_or(0),
            d["hit"].as_i64().unwrap_or(0),
        );
        let verdict = if decided > 0 && hit * 2 > decided {
            Verdict::Right
        } else if decided > 0 && hit * 2 < decided {
            Verdict::Wrong
        } else {
            Verdict::Undecided
        };
        if let Some(date) = d["date"].as_str() {
            out.insert(
                date.to_string(),
                DayRead {
                    set_date: d["set_date"].as_str().unwrap_or("").to_string(),
                    decided,
                    hit,
                    verdict,
                },
            );
        }
    }
    out
}

struct AttrDay {
    date: String,
    pnl: f64,
    trades: usize,
    unplanned: usize,
    read: DayRead,
}

/// `rows` 要是桌面版顺序(入场日新的在前);当天盈亏按这个顺序逐笔累加再四舍五入(和桌面版的浮点顺序一样)。`reads` = 入场日 → 那天的判断对不对
pub(crate) fn compute_attribution(rows: &[Row], reads: &HashMap<String, DayRead>) -> Value {
    let mut by_day: BTreeMap<String, AttrDay> = BTreeMap::new();
    let (mut no_amount, mut no_read, mut undecided) = (0usize, 0usize, 0usize);
    for r in rows.iter().take(ATTR_LIMIT) {
        // 没有成交明细就没有金额:拿百分比按笔数平均会把 1 万的仓和 1 千的仓当成一样重
        let Some(pnl) = r.realized_pnl else {
            no_amount += 1;
            continue;
        };
        let day = entry_day(r);
        let Some(read) = reads.get(&day) else {
            no_read += 1;
            continue;
        };
        if read.verdict == Verdict::Undecided {
            undecided += 1;
            continue;
        }
        let b = by_day.entry(day.clone()).or_insert_with(|| AttrDay {
            date: day,
            pnl: 0.0,
            trades: 0,
            unplanned: 0,
            read: read.clone(),
        });
        b.pnl += pnl;
        b.trades += 1;
        if r.e.as_planned == Some(false) {
            b.unplanned += 1;
        }
    }
    let skipped = |v: &mut Value| {
        v["skipped_no_amount"] = json!(no_amount);
        v["skipped_no_read"] = json!(no_read);
        v["skipped_undecided"] = json!(undecided);
    };
    if by_day.is_empty() {
        let mut v = json!({"available": false, "reason": "没有能归因的交易日 —— 要有成交明细、已经卖光的交易,而且入场那天前一晚的复盘写了验证条件、已经对完账"});
        skipped(&mut v);
        return v;
    }
    let mut cells: BTreeMap<&str, Vec<Value>> =
        QUADRANTS.iter().map(|(k, _)| (*k, Vec::new())).collect();
    for (_, mut b) in by_day {
        b.pnl = r2(b.pnl);
        // 盈亏恰好为 0 的日子不进任何一格:硬塞进「亏钱」格会凭空造出执行问题(同战绩里「持平不进分母」)
        if b.pnl.abs() < 1e-6 {
            continue;
        }
        let right = b.read.verdict == Verdict::Right;
        let key = format!(
            "{}_{}",
            if right { "right" } else { "wrong" },
            if b.pnl > 0.0 { "win" } else { "lose" }
        );
        cells.get_mut(key.as_str()).expect("四格").push(json!({
            "date": b.date, "pnl": b.pnl, "trades": b.trades, "unplanned": b.unplanned, "set_date": b.read.set_date, "decided": b.read.decided, "hit": b.read.hit, "right": right,
        }));
    }
    let quadrants: serde_json::Map<String, Value> = QUADRANTS
        .iter()
        .map(|(k, _)| {
            let list = &cells[k];
            (k.to_string(), json!({"days": list.len(), "pnl": r2(list.iter().map(|d| d["pnl"].as_f64().unwrap_or(0.0)).sum()), "days_list": list.iter().map(|d| d["date"].clone()).collect::<Vec<_>>()}))
        })
        .collect();
    let counted: usize = cells.values().map(Vec::len).sum();
    let mut v = json!({
        "available": true,
        "days_counted": counted,
        "enough_samples": counted >= ATTR_MIN_DAYS,
        "min_days": ATTR_MIN_DAYS,
        "quadrant_labels": QUADRANTS.iter().map(|(k, l)| (k.to_string(), json!(l))).collect::<serde_json::Map<_, _>>(),
        "quadrants": quadrants,
        "cells": cells,
        "note": "按入场日对齐:那天的操作依据是前一晚写给下一场的验证条件。判定了的条件过半成立算判对、不到一半算判错,正好一半的日子不算;盈亏为 0 的日子不进任何一格。",
    });
    skipped(&mut v);
    v
}

fn day_param(path: &str, today: &str) -> Result<Option<String>, DiscoveryError> {
    let Some(raw) = discovery::parameter(path, "date").filter(|v| !v.trim().is_empty()) else {
        return Ok(None);
    };
    let day = raw.trim();
    if day.len() != 10 || NaiveDate::parse_from_str(day, "%Y-%m-%d").is_err() || day > today {
        return Err(DiscoveryError::BadRequest(
            "date 要写成 YYYY-MM-DD,而且不能晚于今天".into(),
        ));
    }
    Ok(Some(day.to_string()))
}

/// 最近一个交易日(今天开盘了算今天):交易日历取上证指数日线日期,今天不是交易日就是之前最近的一个
fn latest_trading_day(calendar: &[String], today: &str) -> String {
    calendar
        .iter()
        .rfind(|d| d.as_str() <= today)
        .cloned()
        .unwrap_or_else(|| today.to_string())
}

/// 纯部分:给定读数窗口和验证条件,算出整页(便于测试)
pub(crate) fn build_view(
    rows: &[Row],
    date: &str,
    series: &[emotion_history::DayRecord],
    items: &[emotion_history::Item],
    next: Option<&str>,
    now: ChinaNow,
) -> Value {
    let v = emotion_history::verify_view(date, series, items, next, now);
    let days: Vec<Value> = v["track"]["days"].as_array().cloned().unwrap_or_default();
    let reads = reads_of(&days);
    let window = json!({"from": v["track"]["from"], "to": v["track"]["to"]});
    if !reads.values().any(|r| r.verdict != Verdict::Undecided) {
        return json!({"available": false, "window": window, "reason": "最近还没有对完账的验证条件(「明日验证」里写给下一场的条件)—— 没有「判对没判对」,就没法做归因"});
    }
    let mut out = compute_attribution(rows, &reads);
    out["window"] = window;
    out
}

/// `GET /mobile/journal/attribution?date=`
pub async fn view(client: &Client, db: &Path, path: &str) -> Result<Value, DiscoveryError> {
    let now = emotion::now_china();
    let today = now.0.format("%Y-%m-%d").to_string();
    let requested = day_param(path, &today)?;
    let a = journal_risk::analysis(db).map_err(DiscoveryError::Upstream)?;
    let mut rows = a.rows;
    journal_risk::va_order(&mut rows);
    let calendar = emotion::trade_dates(client)
        .await
        .map_err(DiscoveryError::Upstream)?;
    let date = requested.unwrap_or_else(|| latest_trading_day(&calendar, &today));
    let (series, gaps) = emotion_history::series(
        client,
        db,
        &calendar,
        &date,
        emotion_history::BASELINE_DAYS,
        false,
        now,
    )
    .await
    .map_err(DiscoveryError::Upstream)?;
    let items = emotion_history::load_items(db).map_err(DiscoveryError::Upstream)?;
    let next = emotion_history::next_session(&calendar, &date);
    let mut out = build_view(&rows, &date, &series, &items, next.as_deref(), now);
    out["gaps"] = json!(gaps);
    out["date"] = json!(date);
    out["source"] =
        json!("明日验证的对账结果(东财涨停 / 炸板 / 跌停 / 昨日涨停池,已收盘的日子存本机)");
    out["generated_at"] = json!(Utc::now().to_rfc3339());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal_risk::{Entry, Fill};
    use crate::journal_settings::Fees;

    const ZERO: Fees = Fees {
        commission_rate: 0.0,
        commission_min: 0.0,
        stamp_tax_rate: 0.0,
        transfer_fee_rate: 0.0,
        fifo: false,
    };

    /// 买入 100 股 @ 10,卖出 @ 10 + pnl/100(盈亏 = pnl 元)
    fn trade(id: i64, day: &str, pnl: f64, planned: Option<bool>) -> Row {
        let e = Entry {
            id,
            kind: "episode".into(),
            symbol: format!("6000{id:02}"),
            date: day.into(),
            created_at: format!("{day}T09:{id:02}:00Z"),
            as_planned: planned,
            fills: vec![
                Fill {
                    side: "buy".into(),
                    date: day.into(),
                    price: 10.0,
                    shares: 100.0,
                    fee: Some(0.0),
                },
                Fill {
                    side: "sell".into(),
                    date: day.into(),
                    price: 10.0 + pnl / 100.0,
                    shares: 100.0,
                    fee: Some(0.0),
                },
            ],
            ..Entry::default()
        };
        journal_risk::settle(e, &ZERO, id as usize)
    }

    fn day(date: &str, set: &str, decided: i64, hit: i64) -> Value {
        json!({"date": date, "set_date": set, "decided": decided, "hit": hit, "total": decided, "excluded": 0})
    }

    fn ordered(mut rows: Vec<Row>) -> Vec<Row> {
        journal_risk::va_order(&mut rows);
        rows
    }

    #[test]
    fn verdict_needs_a_strict_majority_and_ties_or_nothing_decided_are_left_out() {
        let r = reads_of(&[
            day("2026-10-12", "2026-10-09", 3, 2),
            day("2026-10-13", "2026-10-12", 3, 1),
            day("2026-10-14", "2026-10-13", 2, 1),
            day("2026-10-15", "2026-10-14", 0, 0),
        ]);
        assert_eq!(
            (
                r["2026-10-12"].verdict.clone(),
                r["2026-10-13"].verdict.clone(),
                r["2026-10-14"].verdict.clone(),
                r["2026-10-15"].verdict.clone()
            ),
            (
                Verdict::Right,
                Verdict::Wrong,
                Verdict::Undecided,
                Verdict::Undecided
            )
        );
        assert_eq!(r["2026-10-12"].set_date, "2026-10-09");
    }

    #[test]
    fn four_cells_align_by_entry_day_and_show_luck_separately() {
        let reads = reads_of(&[
            day("2026-10-12", "2026-10-09", 3, 3),
            day("2026-10-13", "2026-10-12", 3, 0),
            day("2026-10-14", "2026-10-13", 2, 1),
            day("2026-10-15", "2026-10-14", 4, 3),
            day("2026-10-16", "2026-10-15", 4, 0),
        ]);
        let rows = ordered(vec![
            trade(1, "2026-10-12", 300.0, Some(true)),  // 判对 + 赚钱
            trade(2, "2026-10-13", 150.0, Some(false)), // 判错 + 赚钱(运气),计划外
            trade(3, "2026-10-13", 50.0, Some(true)),   // 同一天:累加成 200
            trade(4, "2026-10-14", -80.0, None),        // 一半对一半错:不进四格
            trade(5, "2026-10-15", -120.0, Some(true)), // 判对 + 亏钱(执行问题)
            trade(6, "2026-10-16", -60.0, Some(false)), // 判错 + 亏钱
            trade(7, "2026-10-17", 40.0, None),         // 那天前一晚没有读数
        ]);
        let v = compute_attribution(&rows, &reads);
        assert_eq!(v["available"], true);
        let q = &v["quadrants"];
        assert_eq!(
            (
                q["right_win"]["days"].clone(),
                q["right_win"]["pnl"].clone()
            ),
            (json!(1), json!(300.0))
        );
        assert_eq!(
            (
                q["wrong_win"]["days"].clone(),
                q["wrong_win"]["pnl"].clone(),
                q["wrong_win"]["days_list"].clone()
            ),
            (json!(1), json!(200.0), json!(["2026-10-13"]))
        );
        assert_eq!(
            (
                q["right_lose"]["pnl"].clone(),
                q["wrong_lose"]["pnl"].clone()
            ),
            (json!(-120.0), json!(-60.0))
        );
        assert_eq!(
            (
                v["skipped_undecided"].clone(),
                v["skipped_no_read"].clone(),
                v["skipped_no_amount"].clone()
            ),
            (json!(1), json!(1), json!(0))
        );
        assert_eq!(v["cells"]["wrong_win"][0]["trades"], 2);
        assert_eq!(v["cells"]["wrong_win"][0]["unplanned"], 1);
        assert_eq!(
            (
                v["days_counted"].clone(),
                v["enough_samples"].clone(),
                v["min_days"].clone()
            ),
            (json!(4), json!(false), json!(8))
        );
        assert_eq!(v["quadrant_labels"]["wrong_win"], "判错 + 赚钱");
    }

    #[test]
    fn zero_pnl_days_and_trades_without_amounts_are_not_forced_into_a_cell() {
        let reads = reads_of(&[
            day("2026-10-12", "2026-10-09", 3, 3),
            day("2026-10-13", "2026-10-12", 3, 3),
        ]);
        let manual = journal_risk::settle(
            Entry {
                id: 9,
                kind: "manual".into(),
                symbol: "600009".into(),
                date: "2026-10-12".into(),
                pnl_pct_manual: Some(5.0),
                ..Entry::default()
            },
            &ZERO,
            9,
        );
        let open = journal_risk::settle(
            Entry {
                id: 8,
                kind: "episode".into(),
                symbol: "600008".into(),
                date: "2026-10-12".into(),
                fills: vec![Fill {
                    side: "buy".into(),
                    date: "2026-10-12".into(),
                    price: 10.0,
                    shares: 100.0,
                    fee: Some(0.0),
                }],
                ..Entry::default()
            },
            &ZERO,
            8,
        );
        // 10-13 两笔一盈一亏正好抵消:当天 0,不进任何一格
        let rows = ordered(vec![
            trade(1, "2026-10-13", 70.0, None),
            trade(2, "2026-10-13", -70.0, None),
            manual,
            open,
        ]);
        let v = compute_attribution(&rows, &reads);
        assert_eq!(
            (
                v["available"].clone(),
                v["days_counted"].clone(),
                v["enough_samples"].clone()
            ),
            (json!(true), json!(0), json!(false)),
            "只剩 0 盈亏的日子:四格全空,不硬塞"
        );
        let rows = ordered(vec![
            trade(1, "2026-10-13", 70.0, None),
            trade(2, "2026-10-13", -70.0, None),
            trade(3, "2026-10-12", 10.0, None),
        ]);
        let v = compute_attribution(&rows, &reads);
        assert_eq!(v["days_counted"], 1, "0 盈亏的那天不进格");
        // 没有成交明细的补记、还没卖完的:算不出金额
        let both = ordered(vec![
            journal_risk::settle(
                Entry {
                    id: 9,
                    kind: "manual".into(),
                    symbol: "600009".into(),
                    date: "2026-10-12".into(),
                    pnl_pct_manual: Some(5.0),
                    ..Entry::default()
                },
                &ZERO,
                9,
            ),
            trade(3, "2026-10-12", 10.0, None),
        ]);
        assert_eq!(compute_attribution(&both, &reads)["skipped_no_amount"], 1);
        let none = compute_attribution(&[], &reads);
        assert_eq!(none["available"], false);
        assert!(none["reason"].as_str().unwrap().contains("验证条件"));
    }

    #[test]
    fn page_is_unavailable_until_some_condition_has_been_reconciled() {
        let rows = ordered(vec![trade(1, "2026-10-13", 70.0, None)]);
        let series: Vec<emotion_history::DayRecord> = Vec::new();
        let v = build_view(
            &rows,
            "2026-10-16",
            &series,
            &[],
            None,
            (NaiveDate::from_ymd_opt(2026, 10, 17).unwrap(), 1200),
        );
        assert_eq!(v["available"], false);
        assert!(v["reason"].as_str().unwrap().contains("没有「判对没判对」"));
    }

    #[test]
    fn latest_trading_day_and_date_parameter_rules() {
        let cal: Vec<String> = ["2026-10-14", "2026-10-15", "2026-10-16"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            latest_trading_day(&cal, "2026-10-17"),
            "2026-10-16",
            "周末取最近一个交易日"
        );
        assert_eq!(latest_trading_day(&cal, "2026-10-15"), "2026-10-15");
        assert!(
            day_param("/x?date=2026-10-20", "2026-10-17").is_err(),
            "未来日期拒绝"
        );
        assert!(day_param("/x?date=2026-02-30", "2026-10-17").is_err());
        assert_eq!(
            day_param("/x?date=2026-10-16", "2026-10-17").unwrap(),
            Some("2026-10-16".into())
        );
        assert_eq!(day_param("/x", "2026-10-17").unwrap(), None);
    }

    /// 生成前端冻结夹具(全部是合成数据,不含任何真实成交与真实复盘):
    /// VIBE_JOURNAL_ATTR_FIXTURE=路径 cargo test --locked journal_attribution::tests::dump_synthetic_fixture -- --ignored
    #[test]
    #[ignore]
    fn dump_synthetic_fixture() {
        // (入场日, 前一晚, 判定条数, 成立条数, 当天各笔盈亏)
        let spec: [(&str, &str, i64, i64, &[f64]); 13] = [
            ("2026-09-21", "2026-09-18", 3, 3, &[420.0, 180.0]),
            ("2026-09-22", "2026-09-21", 3, 1, &[-260.0]),
            ("2026-09-23", "2026-09-22", 4, 3, &[-150.0, 60.0]),
            ("2026-09-24", "2026-09-23", 3, 0, &[310.0]),
            ("2026-09-25", "2026-09-24", 3, 2, &[240.0]),
            ("2026-09-28", "2026-09-25", 2, 1, &[-90.0]),
            ("2026-09-29", "2026-09-28", 3, 3, &[130.0, 95.0]),
            ("2026-10-09", "2026-09-29", 4, 1, &[-340.0, -120.0]),
            ("2026-10-12", "2026-10-09", 3, 2, &[-75.0]),
            ("2026-10-13", "2026-10-12", 3, 0, &[-210.0]),
            ("2026-10-14", "2026-10-13", 3, 2, &[350.0]),
            ("2026-10-15", "2026-10-14", 4, 1, &[180.0, -40.0]),
            ("2026-10-16", "2026-10-15", 3, 3, &[0.0]),
        ];
        let days: Vec<Value> = spec
            .iter()
            .map(|(d, set, decided, hit, _)| day(d, set, *decided, *hit))
            .collect();
        let reads = reads_of(&days);
        let mut rows = Vec::new();
        let mut id = 0;
        for (d, _, _, _, pnls) in spec {
            for (k, pnl) in pnls.iter().enumerate() {
                id += 1;
                rows.push(trade(
                    id,
                    d,
                    *pnl,
                    if k == 1 || (id % 4 == 0) {
                        Some(false)
                    } else {
                        Some(true)
                    },
                ));
            }
        }
        // 一笔入场那天前一晚没写条件、一笔补记
        id += 1;
        rows.push(trade(id, "2026-10-17", 55.0, None));
        let manual = journal_risk::settle(
            Entry {
                id: id + 1,
                kind: "manual".into(),
                symbol: "600099".into(),
                date: "2026-09-18".into(),
                pnl_pct_manual: Some(3.0),
                ..Entry::default()
            },
            &ZERO,
            99,
        );
        rows.push(manual);
        let rows = ordered(rows);
        let mut v = compute_attribution(&rows, &reads);
        v["window"] = json!({"from": "2026-09-18", "to": "2026-10-16"});
        v["gaps"] = json!([
            "2 个交易日没取到读数(东财池子只留约一个月,更早又没缓存过的日子取不到),不进统计"
        ]);
        v["date"] = json!("2026-10-16");
        v["source"] =
            json!("明日验证的对账结果(东财涨停 / 炸板 / 跌停 / 昨日涨停池,已收盘的日子存本机)");
        v["generated_at"] = json!("2026-10-17T02:00:00Z");
        if let Ok(out) = std::env::var("VIBE_JOURNAL_ATTR_FIXTURE") {
            std::fs::write(out, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        }
        println!(
            "days {} quadrants {} skipped {} {} {}",
            v["days_counted"],
            v["quadrants"],
            v["skipped_no_amount"],
            v["skipped_no_read"],
            v["skipped_undecided"]
        );
    }
}
