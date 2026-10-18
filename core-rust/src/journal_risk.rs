//! 交易日志 · 风险与纪律(移植自 orchestrator/src/finance/journal/{risk,stats,episodes}.ts,口径来自 Vibe AStock v2.0.0)。
//!
//! 权益曲线与回撤、近 10 / 20 / 50 笔(状态漂移)、纪律归因(按计划 / 计划外)、规矩逐条检查、在险资金、按打法 / 是否按计划 / 持有周期分组。
//!
//! ⚠️ 只对照**用户自己写下的规矩**和他自己的交易,不替他判断该不该交易、止损该放哪;不产出任何操作建议。
//! ⛔ 这些数据不进任何 AI(个人成交、持仓、金额只留在本机)。
//! 🔴 口径:
//!    - 一笔 = 手机上一条交易记录(episode)。移动加权平均成本;盈亏 % 的分母是不含费用的买入成本,金额是净额(扣买卖费用);
//!      成交没填费用的按「设置」里的费率估(默认值只是初值)。
//!    - 与桌面版的差别:桌面版按「卖出」就算已实现(含没卖完的已卖部分),手机端和既有的「交易统计」页一致 —— **清仓后**才计入,
//!      没卖完的交易只出现在「在险资金」里。补记(没有成交明细)只有 %,不进金额、不进权益曲线。
//!    - 权益曲线、滚动窗口按**最后卖出日**排;胜率不算持平(持平不进分母);全部持平给 null。
//!    - 单日亏损按每笔卖出发生的那天算,分母只能是用户填的账户规模,没填就不查、标出来。
//!    - 同时持仓按**代码**、用持有区间 [买入日, 卖光日) 判断:卖出当天不算,当日进出不占仓位。
//!    - 每条规矩都报「查了没有」:没查和查了没违反是两回事。没写计划止损的持仓,在险是「未知」不是 0。

use crate::journal_settings::{self, Fees, Rules, Settings};
use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
};

const EPS: f64 = 1e-8;
const UNTAGGED: &str = "未标注";
const PLAYBOOKS: [&str; 6] = ["打板", "低吸", "接力", "半路", "套利", "其它"];
const WINDOWS: [usize; 3] = [10, 20, 50];
const NOT_YET: &str = "还没有已平仓且填了成交明细的交易";

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn sum(vals: &[f64]) -> f64 {
    vals.iter().sum()
}

fn mean(vals: &[f64]) -> f64 {
    sum(vals) / vals.len() as f64
}

// ---------------------------------------------------------------- 一笔交易

#[derive(Clone, Debug)]
pub(crate) struct Fill {
    pub side: String,
    pub date: String,
    pub price: f64,
    pub shares: f64,
    pub fee: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Entry {
    pub id: i64,
    pub kind: String,
    pub symbol: String,
    pub name: String,
    pub date: String,
    pub created_at: String,
    pub playbook: Option<String>,
    pub as_planned: Option<bool>,
    pub planned_stop: Option<f64>,
    pub planned_target: Option<f64>,
    pub note: String,
    pub pnl_pct_manual: Option<f64>,
    pub fills: Vec<Fill>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Row {
    pub e: Entry,
    pub pnl_pct: Option<f64>,
    pub realized_pnl: Option<f64>,
    pub closed: bool,
    pub first_buy: Option<String>,
    pub last_sell: Option<String>,
    pub hold_days: Option<i64>,
    pub open_shares: f64,
    pub avg_cost: f64,
    /// 这一轮最多占用的资金(不含费用):每次买入后的持仓成本取最大值,不是买入总额(同桌面版 episode.amount)
    pub peak_cost: f64,
    /// 每笔卖出发生的那天记净盈亏(分两天减仓时,单日亏损规则要能分开算)。只有清仓的交易才有
    pub realized_by_date: BTreeMap<String, f64>,
    pub index: usize,
}

fn days_between(a: &str, b: &str) -> Option<i64> {
    let a = NaiveDate::parse_from_str(a, "%Y-%m-%d").ok()?;
    let b = NaiveDate::parse_from_str(b, "%Y-%m-%d").ok()?;
    Some((b - a).num_days())
}

/// 一笔买入还没卖掉的部分(先进先出用):剩余股数、买入价、还没结转的买入费用
struct Lot {
    shares: f64,
    price: f64,
    fee: f64,
}

/// 结算一条交易记录。成本法默认移动加权平均:卖出按卖出那一刻的均价结转,买入费用按卖出比例结转;
/// 设置里选了先进先出(`fees.fifo`)时,卖出从最早的买入开始结转,各笔买入费用跟着自己的股数走
pub(crate) fn settle(e: Entry, fees: &Fees, index: usize) -> Row {
    if e.kind == "manual" {
        return Row { pnl_pct: e.pnl_pct_manual, closed: e.pnl_pct_manual.is_some(), e, index, ..Row::default() };
    }
    let mut fills = e.fills.clone();
    fills.sort_by(|a, b| a.date.cmp(&b.date).then((a.side!="split").cmp(&(b.side!="split"))));
    let (mut pos, mut cost, mut pending_fee) = (0.0_f64, 0.0_f64, 0.0_f64);
    let (mut gross, mut r_cost, mut r_fee) = (0.0, 0.0, 0.0);
    let mut by_date: BTreeMap<String, f64> = BTreeMap::new();
    let (mut first_buy, mut last_sell): (Option<String>, Option<String>) = (None, None);
    let mut buy_amount = 0.0;
    let mut peak_cost = 0.0_f64;
    let mut lots: std::collections::VecDeque<Lot> = std::collections::VecDeque::new();
    for f in &fills {
        if f.side == "split" {
            pos *= f.price;
            for lot in &mut lots { lot.shares *= f.price; lot.price /= f.price; }
        } else if matches!(f.side.as_str(), "buy" | "opening") {
            let fee = f.fee.unwrap_or_else(|| fees.estimate("buy", f.price, f.shares));
            pos += f.shares;
            cost += f.price * f.shares;
            peak_cost = peak_cost.max(cost);
            pending_fee += fee;
            buy_amount += f.price * f.shares;
            first_buy.get_or_insert_with(|| f.date.clone());
            if fees.fifo {
                lots.push_back(Lot { shares: f.shares, price: f.price, fee });
            }
        } else if pos > EPS {
            let n = f.shares.min(pos);
            let sell_fee = f.fee.unwrap_or_else(|| fees.estimate("sell", f.price, f.shares));
            // 这一次卖出结转的成本、买入费用、价差
            let (leg_cost, leg_buy_fee, leg_gain) = if fees.fifo {
                let (mut left, mut c, mut bf) = (n, 0.0, 0.0);
                while left > EPS {
                    let Some(lot) = lots.front_mut() else { break };
                    let take = left.min(lot.shares);
                    let fee_part = lot.fee * (take / lot.shares);
                    c += lot.price * take;
                    bf += fee_part;
                    lot.fee -= fee_part;
                    lot.shares -= take;
                    left -= take;
                    if lot.shares <= EPS {
                        lots.pop_front();
                    }
                }
                (c, bf, f.price * n - c)
            } else {
                let unit = cost / pos;
                (unit * n, pending_fee * (n / pos), (f.price - unit) * n)
            };
            let leg_fee = leg_buy_fee + sell_fee;
            gross += leg_gain;
            r_cost += leg_cost;
            r_fee += leg_fee;
            *by_date.entry(f.date.clone()).or_default() += leg_gain - leg_fee;
            pending_fee -= leg_buy_fee;
            cost -= leg_cost;
            pos -= n;
            last_sell = Some(f.date.clone());
        }
    }
    let closed = last_sell.is_some() && pos <= EPS;
    let avg_cost = if pos > EPS { cost / pos } else if buy_amount > 0.0 { r_cost / fills.iter().filter(|f| f.side == "sell").map(|f| f.shares).sum::<f64>().max(EPS) } else { 0.0 };
    let mut row = Row { first_buy: first_buy.clone(), last_sell: last_sell.clone(), open_shares: pos.max(0.0), avg_cost, peak_cost, closed, index, ..Row::default() };
    if closed {
        let net = gross - r_fee;
        row.realized_pnl = Some(r2(net));
        row.pnl_pct = (r_cost > 0.0).then(|| r2(net / r_cost * 100.0));
        row.hold_days = first_buy.as_deref().zip(last_sell.as_deref()).and_then(|(a, b)| days_between(a, b));
        row.realized_by_date = by_date.into_iter().map(|(d, v)| (d, r2(v))).collect();
    }
    row.e = e;
    row
}

fn exit_day(r: &Row) -> String {
    r.last_sell.clone().unwrap_or_else(|| r.e.date.clone())
}

/// 有已实现盈亏的,按最后卖出日 → 录入时间排(稳定排序)
fn closed_by_exit(rows: &[Row]) -> Vec<&Row> {
    let mut closed: Vec<&Row> = rows.iter().filter(|r| r.realized_pnl.is_some()).collect();
    closed.sort_by(|a, b| exit_day(a).cmp(&exit_day(b)).then(a.e.created_at.cmp(&b.e.created_at)).then(a.index.cmp(&b.index)));
    closed
}

fn date_note(rows: &[&Row]) -> &'static str {
    if rows.iter().any(|r| r.last_sell.is_none()) { "缺平仓日期的记录按录入日排列" } else { "按平仓日排列" }
}

/// 胜率不算持平:全部持平给 None(那是「没有胜负可言」,不是「没赢过」)
fn win_rate(vals: &[f64]) -> Option<f64> {
    let wins = vals.iter().filter(|v| **v > 0.0).count();
    let decided = wins + vals.iter().filter(|v| **v < 0.0).count();
    (decided > 0).then(|| r3(wins as f64 / decided as f64))
}

// ---------------------------------------------------------------- 分组统计

pub(crate) fn bucket(rows: &[&Row]) -> Value {
    let pcts: Vec<f64> = rows.iter().filter_map(|r| r.pnl_pct).collect();
    let money: Vec<f64> = rows.iter().filter_map(|r| r.realized_pnl).collect();
    let mut out = json!({
        "count": rows.len(), "scored": pcts.len(), "money_scored": money.len(),
        "net_pnl": if money.is_empty() { Value::Null } else { json!(r2(sum(&money))) },
        "win_rate": null, "avg": null, "best": null, "worst": null,
    });
    if !pcts.is_empty() {
        out["win_rate"] = json!(win_rate(&pcts));
        out["avg"] = json!(r2(mean(&pcts)));
        out["best"] = json!(r2(pcts.iter().cloned().fold(f64::MIN, f64::max)));
        out["worst"] = json!(r2(pcts.iter().cloned().fold(f64::MAX, f64::min)));
    }
    out
}

fn planned_key(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "按计划",
        Some(false) => "计划外",
        None => UNTAGGED,
    }
}

pub(crate) fn hold_key(r: &Row) -> String {
    match r.hold_days {
        None => (if r.e.kind == "manual" { "未填明细" } else { "还没卖" }).to_string(),
        Some(0) => "同日买卖(底仓待核)".into(),
        Some(1) => "隔日".into(),
        Some(d) => format!("持有{d}天"),
    }
}

fn hold_rank(key: &str) -> i64 {
    match key {
        "同日买卖(底仓待核)" => 0,
        "隔日" => 1,
        "还没卖" => 1_000_000,
        "未填明细" => 1_000_001,
        _ => key.chars().filter(char::is_ascii_digit).collect::<String>().parse().unwrap_or(1_000_002),
    }
}

fn group_by(rows: &[Row], key_of: impl Fn(&Row) -> String, order: impl Fn(&str) -> i64) -> Vec<Value> {
    let mut groups: BTreeMap<String, Vec<&Row>> = BTreeMap::new();
    for r in rows {
        groups.entry(key_of(r)).or_default().push(r);
    }
    let mut keys: Vec<String> = groups.keys().cloned().collect();
    keys.sort_by(|a, b| order(a).cmp(&order(b)).then(a.cmp(b)));
    keys.into_iter()
        .map(|k| {
            let mut b = bucket(&groups[&k]);
            b["key"] = json!(k);
            b
        })
        .collect()
}

pub(crate) fn stats(rows: &[Row]) -> Value {
    let all: Vec<&Row> = rows.iter().collect();
    let pb_rank = |k: &str| PLAYBOOKS.iter().position(|p| *p == k).unwrap_or(PLAYBOOKS.len()) as i64;
    let pl_rank = |k: &str| ["按计划", "计划外", UNTAGGED].iter().position(|p| *p == k).unwrap_or(3) as i64;
    json!({
        "overall": bucket(&all),
        "by_playbook": group_by(rows, |r| r.e.playbook.clone().unwrap_or_else(|| UNTAGGED.into()), pb_rank),
        "by_planned": group_by(rows, |r| planned_key(r.e.as_planned).into(), pl_rank),
        "by_hold": group_by(rows, hold_key, hold_rank),
    })
}

// ---------------------------------------------------------------- 权益曲线

pub(crate) fn equity_curve(rows: &[Row]) -> Value {
    let closed = closed_by_exit(rows);
    if closed.is_empty() {
        return json!({"available": false, "reason": NOT_YET});
    }
    let mut points = Vec::new();
    let (mut cum, mut peak, mut max_dd) = (0.0_f64, 0.0_f64, 0.0_f64);
    let (mut peak_date, mut max_dd_from, mut dd_start): (Option<String>, Option<String>, Option<String>) = (None, None, None);
    let (mut longest, mut under) = (0usize, 0usize);
    for r in &closed {
        let pnl = r.realized_pnl.unwrap_or(0.0);
        cum += pnl;
        let d = exit_day(r);
        if cum >= peak {
            peak = cum;
            peak_date = Some(d.clone());
            under = 0;
            dd_start = None;
        } else {
            under += 1;
            longest = longest.max(under);
            dd_start.get_or_insert_with(|| d.clone());
            let dd = peak - cum;
            if dd > max_dd {
                max_dd = dd;
                max_dd_from = dd_start.clone();
            }
        }
        points.push(json!({"date": d, "cum_pnl": r2(cum), "pnl": r2(pnl), "drawdown": r2(peak - cum)}));
    }
    let pnls: Vec<f64> = closed.iter().map(|r| r.realized_pnl.unwrap_or(0.0)).collect();
    let wins: Vec<f64> = pnls.iter().copied().filter(|v| *v > 0.0).collect();
    let losses: Vec<f64> = pnls.iter().copied().filter(|v| *v < 0.0).collect();
    let (mut streak, mut worst_streak) = (0usize, 0usize);
    for v in &pnls {
        streak = if *v < 0.0 { streak + 1 } else { 0 };
        worst_streak = worst_streak.max(streak);
    }
    let mut top = pnls.clone();
    top.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let top1 = top.first().copied().unwrap_or(0.0);
    json!({
        "available": true, "points": points, "date_note": date_note(&closed), "trades": closed.len(),
        "net_pnl": r2(cum), "peak": r2(peak), "peak_date": peak_date, "current_drawdown": r2(peak - cum),
        "max_drawdown": r2(max_dd), "max_drawdown_since": max_dd_from,
        "trades_since_peak": under, "longest_underwater": longest,
        "win_rate": if wins.is_empty() && losses.is_empty() { Value::Null } else { json!(r3(wins.len() as f64 / (wins.len() + losses.len()) as f64)) },
        "avg_win": if wins.is_empty() { Value::Null } else { json!(r2(mean(&wins))) },
        "avg_loss": if losses.is_empty() { Value::Null } else { json!(r2(mean(&losses))) },
        "payoff_ratio": if wins.is_empty() || losses.is_empty() { Value::Null } else { json!(r2(mean(&wins) / mean(&losses).abs())) },
        "profit_factor": if losses.is_empty() || sum(&losses) == 0.0 { Value::Null } else { json!(r2(sum(&wins) / sum(&losses).abs())) },
        "worst_losing_streak": worst_streak,
        "worst_trade": r2(pnls.iter().cloned().fold(f64::MAX, f64::min)),
        "net_without_best1": r2(cum - top1),
        "net_without_best3": r2(cum - sum(&top[..top.len().min(3)])),
        "best_trade_share": if !top.is_empty() && cum > 0.0 { json!(r3(top1 / cum)) } else { Value::Null },
    })
}

// ---------------------------------------------------------------- 近 10 / 20 / 50 笔

fn window_stats(closed: &[&Row]) -> Value {
    let pnls: Vec<f64> = closed.iter().map(|r| r.realized_pnl.unwrap_or(0.0)).collect();
    if pnls.is_empty() {
        return json!({"trades": 0});
    }
    let wins: Vec<f64> = pnls.iter().copied().filter(|v| *v > 0.0).collect();
    let losses: Vec<f64> = pnls.iter().copied().filter(|v| *v < 0.0).collect();
    let (gross_win, gross_loss) = (sum(&wins), -sum(&losses));
    let planned = closed.iter().filter(|r| r.e.as_planned == Some(true)).count();
    let unplanned = closed.iter().filter(|r| r.e.as_planned == Some(false)).count();
    json!({
        "trades": pnls.len(), "net_pnl": r2(sum(&pnls)), "win_rate": win_rate(&pnls),
        "avg_win": if wins.is_empty() { Value::Null } else { json!(r2(gross_win / wins.len() as f64)) },
        "avg_loss": if losses.is_empty() { Value::Null } else { json!(r2(-gross_loss / losses.len() as f64)) },
        "payoff_ratio": if wins.is_empty() || losses.is_empty() { Value::Null } else { json!(r2((gross_win / wins.len() as f64) / (gross_loss / losses.len() as f64))) },
        "profit_factor": if gross_loss > 0.0 { json!(r2(gross_win / gross_loss)) } else { Value::Null },
        "execution_rate": if planned + unplanned > 0 { json!(r3(planned as f64 / (planned + unplanned) as f64)) } else { Value::Null },
        "date_note": date_note(closed), "date_from": exit_day(closed[0]), "date_to": exit_day(closed[closed.len() - 1]),
    })
}

pub(crate) fn rolling(rows: &[Row]) -> Value {
    let closed = closed_by_exit(rows);
    if closed.is_empty() {
        return json!({"available": false, "reason": NOT_YET});
    }
    let lifetime = window_stats(&closed);
    let mut windows = Map::new();
    // 样本不够时不拿全部冒充那个窗口:只有 12 笔却报「近 50 笔」,会让人以为打法在 50 笔的尺度上验证过
    for n in WINDOWS {
        let mut w = window_stats(&closed[closed.len().saturating_sub(n)..]);
        w["window"] = json!(n);
        w["enough"] = json!(closed.len() >= n);
        windows.insert(n.to_string(), w);
    }
    let mut out = json!({"available": true, "windows": windows, "lifetime": lifetime,
        "note": "10 笔看状态、20 笔看节奏、50 笔看打法是否还成立。终身统计会把最近的退化藏起来 —— 前面赚够了,最近一直亏,终身数字照样漂亮。"});
    let w10 = out["windows"]["10"].clone();
    if w10["enough"] == true {
        if let (Some(a), Some(b)) = (w10["win_rate"].as_f64(), out["lifetime"]["win_rate"].as_f64()) {
            out["win_rate_drift"] = json!(r3(a - b));
        }
        if let (Some(a), Some(b)) = (w10["profit_factor"].as_f64(), out["lifetime"]["profit_factor"].as_f64()) {
            out["profit_factor_drift"] = json!(r2(a - b));
        }
    }
    out
}

// ---------------------------------------------------------------- 纪律归因

pub(crate) fn discipline(rows: &[Row]) -> Value {
    let scored: Vec<&Row> = rows.iter().filter(|r| r.pnl_pct.is_some() || r.realized_pnl.is_some()).collect();
    if scored.is_empty() {
        return json!({"available": false, "reason": "还没有可统计的交易"});
    }
    let part = |rs: &[&Row]| {
        let pcts: Vec<f64> = rs.iter().filter_map(|r| r.pnl_pct).collect();
        let money: Vec<f64> = rs.iter().filter_map(|r| r.realized_pnl).collect();
        json!({"count": rs.len(), "win_rate": win_rate(&pcts), "avg_pct": if pcts.is_empty() { Value::Null } else { json!(r2(mean(&pcts))) },
            "net_pnl": if money.is_empty() { Value::Null } else { json!(r2(sum(&money))) }})
    };
    let planned: Vec<&Row> = scored.iter().copied().filter(|r| r.e.as_planned == Some(true)).collect();
    let unplanned: Vec<&Row> = scored.iter().copied().filter(|r| r.e.as_planned == Some(false)).collect();
    let untagged: Vec<&Row> = scored.iter().copied().filter(|r| r.e.as_planned.is_none()).collect();
    let all_money: Vec<f64> = scored.iter().filter_map(|r| r.realized_pnl).collect();
    let planned_money: Vec<f64> = planned.iter().filter_map(|r| r.realized_pnl).collect();
    json!({
        "available": true, "planned": part(&planned), "unplanned": part(&unplanned), "untagged": part(&untagged),
        "execution_rate": if planned.is_empty() && unplanned.is_empty() { Value::Null } else { json!(r3(planned.len() as f64 / (planned.len() + unplanned.len()) as f64)) },
        // 只做按计划的交易,账户会是什么样
        "what_if_only_planned": {
            "actual_net": if all_money.is_empty() { Value::Null } else { json!(r2(sum(&all_money))) },
            "planned_only_net": if planned_money.is_empty() { Value::Null } else { json!(r2(sum(&planned_money))) },
            "cost_of_indiscipline": if all_money.is_empty() || planned_money.is_empty() { Value::Null } else { json!(r2(sum(&all_money) - sum(&planned_money))) },
        },
        "note": "「计划外」要你自己在记录时标 —— 没标的归到未标注,不猜。这不是市场分析,是你自己的行为统计。",
    })
}

// ---------------------------------------------------------------- 规矩逐条检查

fn violation(date: &str, rule: &str, limit: f64, actual: f64, detail: String) -> Value {
    json!({"date": date, "rule": rule, "label": rule_label(rule), "limit": limit, "actual": actual, "detail": detail})
}

fn rule_label(rule: &str) -> &'static str {
    journal_settings::RULE_KEYS.iter().find(|(k, _, _)| *k == rule).map(|(_, l, _)| *l).unwrap_or("")
}

pub(crate) fn violations(rows: &[Row], rules: &Rules, rules_default: bool, equity_base: Option<f64>) -> Value {
    if rows.is_empty() {
        return json!({"available": false, "reason": "还没有交易记录"});
    }
    let mut checked: BTreeMap<&str, String> = BTreeMap::new();
    let mut found: Vec<Value> = Vec::new();

    // 单日最大亏损:按每笔卖出发生的那天汇总净盈亏
    let mut day_pnl: BTreeMap<String, f64> = BTreeMap::new();
    for r in rows {
        for (d, v) in &r.realized_by_date {
            *day_pnl.entry(d.clone()).or_default() += v;
        }
    }
    if day_pnl.is_empty() {
        checked.insert("max_loss_per_day_pct", "unavailable:没有带成交明细的已平仓交易,算不出单日盈亏".into());
    } else if let Some(base) = equity_base {
        checked.insert("max_loss_per_day_pct", "checked".into());
        let cap = rules.max_loss_per_day_pct.abs();
        for (day, pnl) in &day_pnl {
            let pct = pnl / base * 100.0;
            if pct < -cap {
                found.push(violation(day, "max_loss_per_day_pct", -cap, r2(pct), format!("{day} 当日净亏 {} 元,占账户 {}%", r2(*pnl), r2(pct))));
            }
        }
    } else {
        checked.insert("max_loss_per_day_pct", "unavailable:没填账户规模,单日亏损占比没有分母".into());
    }

    // 最多同时持仓:按代码、[买入日, 卖光日)
    let mut spans: Vec<(String, String, Option<String>)> = Vec::new();
    let mut undated = 0;
    for r in rows {
        match &r.first_buy {
            Some(start) if r.e.kind == "episode" => spans.push((r.e.symbol.clone(), start.clone(), if r.closed { r.last_sell.clone() } else { None })),
            _ => undated += 1,
        }
    }
    if spans.is_empty() {
        checked.insert("max_positions", "unavailable:没有可用的建仓/平仓日期".into());
    } else {
        checked.insert("max_positions", "checked".into());
        let cap = rules.max_positions.trunc();
        let days: BTreeSet<&String> = spans.iter().flat_map(|(_, s, e)| std::iter::once(s).chain(e.iter())).collect();
        for day in days {
            let holding: BTreeSet<&String> = spans.iter().filter(|(_, s, e)| s <= day && e.as_ref().map_or(true, |e| day < e)).map(|(c, _, _)| c).collect();
            if holding.len() as f64 > cap {
                found.push(violation(day, "max_positions", cap, holding.len() as f64, format!("{day} 同时持有 {} 只", holding.len())));
            }
        }
    }
    if undated > 0 {
        checked.insert("max_positions", "unavailable:有没有成交明细的补记(缺买入、卖出日期),未能完整核对历史持仓上限;已确认的超限仍展示,结果可能漏计".into());
    }

    let mut by_day: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
    for r in rows {
        by_day.entry(r.e.date.as_str()).or_default().push(r);
    }
    for (day, rs) in &by_day {
        if rs.len() as f64 > rules.max_trades_per_day {
            found.push(violation(day, "max_trades_per_day", rules.max_trades_per_day, rs.len() as f64, format!("{day} 开仓 {} 笔", rs.len())));
        }
        for r in rs {
            if let Some(p) = r.pnl_pct {
                let cap = rules.max_loss_per_trade_pct.abs();
                if p < -cap {
                    let who = if r.e.name.is_empty() { &r.e.symbol } else { &r.e.name };
                    found.push(violation(day, "max_loss_per_trade_pct", -cap, p, format!("{who} 亏 {p}%")));
                }
            }
        }
        let tagged: Vec<&&Row> = rs.iter().filter(|r| r.e.as_planned.is_some()).collect();
        if !tagged.is_empty() {
            let un = tagged.iter().filter(|r| r.e.as_planned == Some(false)).count();
            let ratio = un as f64 / tagged.len() as f64;
            if ratio > rules.max_unplanned_ratio {
                found.push(violation(day, "max_unplanned_ratio", rules.max_unplanned_ratio, r3(ratio), format!("{day} 计划外 {un}/{} 笔", tagged.len())));
            }
        }
    }

    // 连亏到自己的阈值之后还继续做的那些笔:只陈述历史,不下「该停手」的结论
    let mut scored: Vec<&Row> = rows.iter().filter(|r| r.pnl_pct.is_some()).collect();
    scored.sort_by(|a, b| a.e.date.cmp(&b.e.date).then(a.e.created_at.cmp(&b.e.created_at)).then(a.index.cmp(&b.index)));
    let limit = rules.pause_after_losses.trunc() as usize;
    let (mut after, mut streak) = (Vec::new(), 0usize);
    for r in &scored {
        let v = r.pnl_pct.unwrap_or(0.0);
        if streak >= limit {
            after.push(v);
        }
        streak = if v < 0.0 { streak + 1 } else { 0 };
    }
    checked.insert("max_trades_per_day", "checked".into());
    checked.insert("max_loss_per_trade_pct", "checked".into());
    checked.insert("max_unplanned_ratio", if rows.iter().any(|r| r.e.as_planned.is_some()) { "checked".into() } else { "unavailable:还没有标注过「按计划 / 计划外」的交易".into() });
    checked.insert("pause_after_losses", "checked".into());
    let unchecked: Vec<&&str> = checked.iter().filter(|(_, v)| v.starts_with("unavailable")).map(|(k, _)| k).collect();
    json!({
        "available": true, "rules": rules.to_value(), "is_default_rules": rules_default,
        // 每条规矩查了没有:界面要把「没查」和「查了没违反」分开显示
        "rule_status": checked, "unchecked": unchecked, "violations": found, "violation_count": found.len(),
        "after_loss_streak": {"threshold": limit, "trades": after.len(), "avg_pct": if after.is_empty() { Value::Null } else { json!(r2(mean(&after))) }, "win_rate": win_rate(&after)},
    })
}

// ---------------------------------------------------------------- 在险资金

pub(crate) fn money0(v: f64) -> String {
    let n = v.round() as i64;
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

pub(crate) fn at_risk(rows: &[Row], rules: &Rules, rules_default: bool, equity_base: Option<f64>) -> Value {
    let mut pos: Vec<Value> = Vec::new();
    let mut raw: Vec<(String, String, f64, f64, bool, f64)> = Vec::new();
    for r in rows {
        if r.e.kind != "episode" || r.closed || r.open_shares <= 0.0 || r.avg_cost <= 0.0 {
            continue;
        }
        let (shares, cost) = (r.open_shares, r.avg_cost);
        let (at_risk, at_risk_pct) = match r.e.planned_stop {
            Some(stop) => (Some(r2(((cost - stop) * shares).max(0.0))), Some(r2((1.0 - stop / cost).max(0.0) * 100.0))),
            None => (None, None),
        };
        let date = r.first_buy.clone().unwrap_or_else(|| r.e.date.clone());
        raw.push((date.clone(), r.e.symbol.clone(), at_risk.unwrap_or(0.0), r2(cost * shares), at_risk.is_some(), 0.0));
        pos.push(json!({
            "key": format!("mobile:{}", r.e.id), "symbol": r.e.symbol, "name": r.e.name, "date": date, "playbook": r.e.playbook,
            "shares": r2(shares), "avg_cost": r2(cost), "capital": r2(cost * shares),
            "planned_stop": r.e.planned_stop, "planned_target": r.e.planned_target,
            "at_risk": at_risk, "at_risk_pct": at_risk_pct, "bounded": at_risk.is_some(),
        }));
    }
    if pos.is_empty() {
        return json!({"available": false, "reason": "当前没有还没卖完的持仓"});
    }
    pos.sort_by(|a, b| a["date"].as_str().cmp(&b["date"].as_str()));
    let bounded: Vec<&Value> = pos.iter().filter(|p| p["bounded"] == true).collect();
    let unbounded: Vec<&Value> = pos.iter().filter(|p| p["bounded"] == false).collect();
    let total_risk = r2(bounded.iter().filter_map(|p| p["at_risk"].as_f64()).sum());
    let total_capital = r2(pos.iter().filter_map(|p| p["capital"].as_f64()).sum());
    let unbounded_capital = r2(unbounded.iter().filter_map(|p| p["capital"].as_f64()).sum());
    let codes: BTreeSet<&str> = pos.iter().filter_map(|p| p["symbol"].as_str()).collect();
    let mut out = json!({
        "available": true, "positions": pos, "position_count": pos.len(), "total_capital": total_capital,
        // 只加有边界的;没写止损的不能当 0 混进来
        "total_at_risk": total_risk, "bounded_count": bounded.len(), "unbounded_count": unbounded.len(), "unbounded_capital": unbounded_capital,
        "equity_base": equity_base,
        "rules": {"max_loss_per_trade_pct": rules.max_loss_per_trade_pct, "max_positions": rules.max_positions, "is_default": rules_default},
        "at_risk_of_equity_pct": null, "capital_of_equity_pct": null, "over_per_trade_limit": null, "equity_base_hint": null,
        "over_position_limit": null, "unbounded_note": null,
    });
    if let Some(base) = equity_base {
        out["at_risk_of_equity_pct"] = json!(r2(total_risk / base * 100.0));
        out["capital_of_equity_pct"] = json!(r2(total_capital / base * 100.0));
        out["over_per_trade_limit"] = json!(bounded.iter().filter(|p| p["at_risk"].as_f64().unwrap_or(0.0) / base * 100.0 > rules.max_loss_per_trade_pct + 1e-9)
            .map(|p| json!({"name": p["name"], "code": p["symbol"], "pct_of_equity": r2(p["at_risk"].as_f64().unwrap_or(0.0) / base * 100.0)})).collect::<Vec<_>>());
    } else {
        out["equity_base_hint"] = json!("填了账户规模才能给占比 —— 绝对金额说明不了这个风险相对你的账户算大还是小。");
    }
    if codes.len() as f64 > rules.max_positions.trunc() {
        out["over_position_limit"] = json!({"actual": codes.len(), "limit": rules.max_positions.trunc()});
    }
    if !unbounded.is_empty() {
        out["unbounded_note"] = json!(format!("另有 {} 笔没写计划止损、占用本金 {} 元,最坏情况无从估计 —— 它们没算进上面的总在险,所以那个数是下限。", unbounded.len(), money0(unbounded_capital)));
    }
    out
}

// ---------------------------------------------------------------- 读取与入口

pub(crate) fn load_entries(db: &Path) -> Result<Vec<Entry>> {
    let conn = Connection::open(db).context("open mobile journal risk sqlite")?;
    let mut stmt = conn.prepare(
        "SELECT id, kind, symbol, name, date, playbook, as_planned, planned_stop, planned_target, pnl_pct, created_at, note
         FROM journal_entries ORDER BY date DESC, id DESC",
    )?;
    let mut entries: Vec<Entry> = stmt
        .query_map([], |r| {
            Ok(Entry {
                id: r.get(0)?, kind: r.get(1)?, symbol: r.get(2)?, name: r.get(3)?, date: r.get(4)?,
                playbook: r.get(5)?, as_planned: r.get::<_, Option<i64>>(6)?.map(|v| v != 0),
                planned_stop: r.get(7)?, planned_target: r.get(8)?, pnl_pct_manual: r.get(9)?, created_at: r.get(10)?, note: r.get(11)?, fills: Vec::new(),
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut fill_stmt = conn.prepare("SELECT entry_id, side, date, price, shares, fee FROM journal_fills ORDER BY entry_id, sort_order, id")?;
    let mut by_entry: HashMap<i64, Vec<Fill>> = HashMap::new();
    for row in fill_stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, Fill { side: r.get(1)?, date: r.get(2)?, price: r.get(3)?, shares: r.get(4)?, fee: r.get(5)? })))? {
        let (id, fill) = row?;
        by_entry.entry(id).or_default().push(fill);
    }
    for e in &mut entries {
        e.fills = by_entry.remove(&e.id).unwrap_or_default();
    }
    Ok(entries)
}

pub(crate) fn rows_of(entries: Vec<Entry>, fees: &Fees) -> Vec<Row> {
    entries.into_iter().enumerate().map(|(i, e)| settle(e, fees, i)).collect()
}

/// 能进交易日志的代码:A 股股票(六位数字,不是基金 / ETF / 债券 / 可转债 / 回购)。
/// 1、5 开头 = 基金 ETF LOF 债券可转债深市回购,沪市 01、02 开头 = 国债,204 开头 = 沪市国债逆回购(同桌面版 episodes.ts)。
/// 🔴 交易日志只算 A 股股票:ETF、债券混进来会算进股票的胜率和纪律统计,还会按股票扣印花税
pub(crate) fn is_journal_symbol(symbol: &str) -> bool {
    symbol.len() == 6 && symbol.bytes().all(|b| b.is_ascii_digit()) && !(symbol.starts_with('1') || symbol.starts_with('5') || symbol.starts_with("01") || symbol.starts_with("02") || symbol.starts_with("204"))
}

/// 一笔交易的入场日:一轮 = 第一笔买入那天,补记 = 用户填的日期(同桌面版 JournalRow.date)
pub(crate) fn entry_day(r: &Row) -> String {
    if r.e.kind == "episode" { r.first_buy.clone().unwrap_or_else(|| r.e.date.clone()) } else { r.e.date.clone() }
}

/// 桌面版列表顺序:入场日新的在前,同一天后录的在前,再按 id(分析页的「最近 N 笔」都按这个顺序截取)
pub(crate) fn va_order(rows: &mut [Row]) {
    rows.sort_by(|a, b| entry_day(b).cmp(&entry_day(a)).then(b.e.created_at.cmp(&a.e.created_at)).then(a.e.id.cmp(&b.e.id)));
}

/// 分析页共用的读取:设置 + 只含 A 股股票的交易。不是 A 股的记录(ETF 等)单独返回,由「问题记录」页列出。
pub(crate) struct Analysis {
    pub settings: Settings,
    pub rows: Vec<Row>,
    pub excluded: Vec<Entry>,
}

pub(crate) fn analysis(db: &Path) -> Result<Analysis> {
    let settings = journal_settings::load(db)?;
    let (kept, excluded): (Vec<Entry>, Vec<Entry>) = load_entries(db)?.into_iter().partition(|e| is_journal_symbol(&e.symbol));
    Ok(Analysis { rows: rows_of(kept, &settings.fees), settings, excluded })
}

pub(crate) fn build(rows: &[Row], settings: &Settings) -> Value {
    json!({
        "available": true,
        "source": "mobile_sqlite",
        "generated_at": Utc::now().to_rfc3339(),
        "settings": journal_settings::settings_value(settings),
        "playbooks": PLAYBOOKS,
        "stats": stats(rows),
        "risk": {
            "equity": equity_curve(rows),
            "rolling": rolling(rows),
            "discipline": discipline(rows),
            "violations": violations(rows, &settings.rules, settings.rules_default, settings.equity_base),
        },
        "at_risk": at_risk(rows, &settings.rules, settings.rules_default, settings.equity_base),
        "notes": [
            "只有清仓的交易才计入已实现盈亏(与「交易统计」一致);没卖完的交易只出现在在险资金里",
            "成交没填费用的按「设置」里的费率估算,默认费率只是初值,不是你的真实费率",
            "个人成交、持仓、金额只留在本机,不进任何 AI",
        ],
    })
}

pub fn view(db: &Path) -> Result<Value> {
    let a = analysis(db)?;
    let mut v = build(&a.rows, &a.settings);
    v["excluded_non_stock"] = json!(a.excluded.len());
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(side: &str, date: &str, price: f64, shares: f64, fee: Option<f64>) -> Fill {
        Fill { side: side.into(), date: date.into(), price, shares, fee }
    }

    fn episode(id: i64, symbol: &str, date: &str, fills: Vec<Fill>) -> Entry {
        Entry { id, kind: "episode".into(), symbol: symbol.into(), name: format!("N{symbol}"), date: date.into(), created_at: format!("2026-10-{:02}T09:00:00Z", id), fills, ..Entry::default() }
    }

    const ZERO_FEES: Fees = Fees { commission_rate: 0.0, commission_min: 0.0, stamp_tax_rate: 0.0, transfer_fee_rate: 0.0, fifo: false };

    fn row(e: Entry) -> Row {
        let index = e.id as usize;
        settle(e, &ZERO_FEES, index)
    }

    #[test]
    fn only_a_share_stocks_belong_in_the_journal() {
        for ok in ["600519", "000001", "300750", "688981", "002594", "830799", "920575", "301000"] {
            assert!(is_journal_symbol(ok), "{ok}");
        }
        for no in ["510300", "159915", "113050", "127045", "010107", "020001", "204001", "12345", "6005190", "60051a", ""] {
            assert!(!is_journal_symbol(no), "{no}");
        }
    }

    #[test]
    fn fifo_consumes_oldest_lots_and_moves_their_buy_fee() {
        const FIFO: Fees = Fees { commission_rate: 0.0, commission_min: 0.0, stamp_tax_rate: 0.0, transfer_fee_rate: 0.0, fifo: true };
        let fills = vec![
            fill("buy", "2026-10-12", 10.0, 100.0, Some(2.0)), fill("buy", "2026-10-13", 12.0, 100.0, Some(2.0)),
            fill("sell", "2026-10-14", 13.0, 100.0, Some(1.0)), fill("sell", "2026-10-15", 9.0, 100.0, Some(1.0)),
        ];
        let r = settle(episode(1, "600001", "2026-10-12", fills.clone()), &FIFO, 0);
        assert!(r.closed);
        assert_eq!(r.realized_pnl, Some(-6.0), "总盈亏两种成本法一样:买 2200 + 费 4,卖 2200 + 费 2,差 -6");
        let by: Vec<(String, f64)> = r.realized_by_date.clone().into_iter().collect();
        assert_eq!(by, vec![("2026-10-14".to_string(), 297.0), ("2026-10-15".to_string(), -303.0)], "先卖掉的是 10 元那一笔,费用跟着自己的股数走");
        let avg = settle(episode(1, "600001", "2026-10-12", fills), &ZERO_FEES, 0);
        let by_avg: Vec<(String, f64)> = avg.realized_by_date.into_iter().collect();
        assert_eq!(by_avg, vec![("2026-10-14".to_string(), 197.0), ("2026-10-15".to_string(), -203.0)], "移动加权平均按均价 11 结转");
    }

    #[test]
    fn fifo_open_position_keeps_the_newest_lot_cost() {
        const FIFO: Fees = Fees { commission_rate: 0.0, commission_min: 0.0, stamp_tax_rate: 0.0, transfer_fee_rate: 0.0, fifo: true };
        let fills = vec![fill("buy", "2026-10-12", 10.0, 100.0, Some(0.0)), fill("buy", "2026-10-13", 12.0, 100.0, Some(0.0)), fill("sell", "2026-10-14", 13.0, 150.0, Some(0.0))];
        let r = settle(episode(1, "600001", "2026-10-12", fills.clone()), &FIFO, 0);
        assert!(!r.closed && r.realized_pnl.is_none());
        assert_eq!((r.open_shares, r.avg_cost), (50.0, 12.0), "先进先出:剩下的 50 股都是 12 元买的");
        let avg = settle(episode(1, "600001", "2026-10-12", fills), &ZERO_FEES, 0);
        assert_eq!((avg.open_shares, avg.avg_cost), (50.0, 11.0), "移动加权平均:剩下的按均价 11");
    }

    #[test]
    fn settle_uses_moving_average_and_splits_pnl_by_sell_day() {
        let e = episode(1, "600001", "2026-10-12", vec![
            fill("buy", "2026-10-12", 10.0, 100.0, Some(2.0)), fill("buy", "2026-10-13", 12.0, 100.0, Some(2.0)),
            fill("sell", "2026-10-14", 13.0, 100.0, Some(1.0)), fill("sell", "2026-10-15", 9.0, 100.0, Some(1.0)),
        ]);
        let r = settle(e, &ZERO_FEES, 0);
        assert!(r.closed);
        // 均价 11:第一次卖 +200、第二次卖 -200,买入费用各结转一半;净额 = 0 - 4 - 2 = -6
        assert_eq!(r.realized_pnl, Some(-6.0));
        assert_eq!(r.realized_by_date["2026-10-14"], 200.0 - 2.0 - 1.0);
        assert_eq!(r.realized_by_date["2026-10-15"], -200.0 - 2.0 - 1.0);
        assert_eq!(r.hold_days, Some(3));
        assert_eq!(r.pnl_pct, Some(r2(-6.0 / 2200.0 * 100.0)));
        let open = settle(episode(2, "600002", "2026-10-12", vec![fill("buy", "2026-10-12", 10.0, 100.0, Some(0.0)), fill("sell", "2026-10-13", 11.0, 40.0, Some(0.0))]), &ZERO_FEES, 1);
        assert!(!open.closed && open.realized_pnl.is_none(), "没卖完不算已实现");
        assert_eq!((open.open_shares, open.avg_cost), (60.0, 10.0));
    }

    fn closed(id: i64, date: &str, sell: &str, pnl_per_share: f64) -> Row {
        row(episode(id, &format!("60000{id}"), date, vec![fill("buy", date, 10.0, 100.0, Some(0.0)), fill("sell", sell, 10.0 + pnl_per_share, 100.0, Some(0.0))]))
    }

    #[test]
    fn equity_curve_tracks_peak_drawdown_and_luck() {
        let rows: Vec<Row> = vec![closed(1, "2026-10-12", "2026-10-13", 1.0), closed(2, "2026-10-13", "2026-10-14", -0.5), closed(3, "2026-10-14", "2026-10-15", -1.0), closed(4, "2026-10-15", "2026-10-16", 0.2)];
        let v = equity_curve(&rows);
        // 累计:+100,+50,-50,-30;峰值 100,最大回撤 150,现回撤 130
        assert_eq!((v["net_pnl"].clone(), v["peak"].clone(), v["max_drawdown"].clone(), v["current_drawdown"].clone()), (json!(-30.0), json!(100.0), json!(150.0), json!(130.0)));
        assert_eq!((v["trades_since_peak"].clone(), v["longest_underwater"].clone(), v["worst_losing_streak"].clone(), v["max_drawdown_since"].clone()), (json!(3), json!(3), json!(2), json!("2026-10-14")));
        assert_eq!((v["win_rate"].clone(), v["payoff_ratio"].clone(), v["net_without_best1"].clone()), (json!(0.5), json!(0.8), json!(-130.0)));
        assert_eq!(v["best_trade_share"], Value::Null, "累计为负不给「靠一笔」的占比");
        assert_eq!(equity_curve(&[])["available"], false);
    }

    #[test]
    fn rolling_does_not_pass_off_short_history_as_a_window() {
        let rows: Vec<Row> = (1..=12).map(|i| closed(i, &format!("2026-09-{:02}", i + 1), &format!("2026-09-{:02}", i + 2), if i > 8 { -1.0 } else { 1.0 })).collect();
        let v = rolling(&rows);
        assert_eq!((v["windows"]["10"]["enough"].clone(), v["windows"]["20"]["enough"].clone(), v["windows"]["50"]["enough"].clone()), (json!(true), json!(false), json!(false)));
        assert_eq!(v["windows"]["20"]["trades"], 12, "不够 20 笔时给的是全部,但标 enough=false");
        // 近 10 笔:第 3~8 笔赚、第 9~12 笔亏 → 胜率 6/10;终身 8/12
        assert_eq!(v["windows"]["10"]["win_rate"], 0.6);
        assert_eq!(v["win_rate_drift"], r3(0.6 - 8.0 / 12.0));
    }

    #[test]
    fn discipline_splits_planned_unplanned_and_untagged() {
        let mut a = closed(1, "2026-10-12", "2026-10-13", 1.0);
        a.e.as_planned = Some(true);
        let mut b = closed(2, "2026-10-13", "2026-10-14", -2.0);
        b.e.as_planned = Some(false);
        let c = closed(3, "2026-10-14", "2026-10-15", 0.5);
        let v = discipline(&[a, b, c]);
        assert_eq!((v["planned"]["count"].clone(), v["unplanned"]["win_rate"].clone(), v["untagged"]["count"].clone()), (json!(1), json!(0.0), json!(1)));
        assert_eq!(v["execution_rate"], 0.5);
        assert_eq!((v["what_if_only_planned"]["actual_net"].clone(), v["what_if_only_planned"]["planned_only_net"].clone(), v["what_if_only_planned"]["cost_of_indiscipline"].clone()), (json!(-50.0), json!(100.0), json!(-150.0)));
    }

    fn rules() -> Rules {
        journal_settings::settings_of(None).rules
    }

    #[test]
    fn violations_report_what_was_checked_and_what_was_not() {
        // 单日亏损:10-14 一天净亏 300,账户 10000 → -3%,没超默认 8%;改成账户 3000 → -10% 超限
        let rows = vec![closed(1, "2026-10-12", "2026-10-14", -3.0)];
        let none = violations(&rows, &rules(), true, None);
        assert!(none["rule_status"]["max_loss_per_day_pct"].as_str().unwrap().starts_with("unavailable:没填账户规模"));
        assert!(none["unchecked"].as_array().unwrap().contains(&json!("max_loss_per_day_pct")));
        assert!(none["unchecked"].as_array().unwrap().contains(&json!("max_unplanned_ratio")));
        let ok = violations(&rows, &rules(), true, Some(10000.0));
        assert_eq!(ok["rule_status"]["max_loss_per_day_pct"], "checked");
        assert!(ok["violations"].as_array().unwrap().iter().all(|v| v["rule"] != "max_loss_per_day_pct"));
        let over = violations(&rows, &rules(), true, Some(3000.0));
        let hit = over["violations"].as_array().unwrap().iter().find(|v| v["rule"] == "max_loss_per_day_pct").unwrap();
        assert_eq!((hit["date"].clone(), hit["actual"].clone(), hit["limit"].clone()), (json!("2026-10-14"), json!(-10.0), json!(-8.0)));
        // 单笔 -30% 超过默认 5%
        assert!(over["violations"].as_array().unwrap().iter().any(|v| v["rule"] == "max_loss_per_trade_pct" && v["actual"] == -30.0));
        // 补记没有成交明细:持仓上限不能完整核对,要说明
        let manual = Row { e: Entry { id: 9, kind: "manual".into(), symbol: "600009".into(), date: "2026-10-10".into(), pnl_pct_manual: Some(1.0), ..Entry::default() }, pnl_pct: Some(1.0), closed: true, index: 9, ..Row::default() };
        let mixed = violations(&[rows[0].clone(), manual], &rules(), true, Some(10000.0));
        assert!(mixed["rule_status"]["max_positions"].as_str().unwrap().contains("补记"));
    }

    #[test]
    fn position_limit_uses_half_open_holding_intervals_by_code() {
        // 3 只在 10-13 同时持有(第四只 10-13 才买入,第一只 10-13 卖光 → 当天不占仓位)
        let mk = |id: i64, buy: &str, sell: &str| closed_span(id, buy, sell);
        let rows = vec![mk(1, "2026-10-12", "2026-10-13"), mk(2, "2026-10-12", "2026-10-16"), mk(3, "2026-10-12", "2026-10-16"), mk(4, "2026-10-13", "2026-10-16")];
        let mut tight = rules();
        tight.max_positions = 3.0;
        let v = violations(&rows, &tight, false, None);
        assert_eq!(v["violations"].as_array().unwrap().iter().filter(|x| x["rule"] == "max_positions").count(), 0, "卖出当天不算、当日进出不占仓位");
        tight.max_positions = 2.0;
        let v = violations(&rows, &tight, false, None);
        let days: Vec<&str> = v["violations"].as_array().unwrap().iter().filter(|x| x["rule"] == "max_positions").map(|x| x["date"].as_str().unwrap()).collect();
        // 10-12 同时 3 只;10-13 第一只卖光(当天不占)、第四只买入,仍是 3;10-16 全部卖光
        assert_eq!(days, vec!["2026-10-12", "2026-10-13"]);
    }

    fn closed_span(id: i64, buy: &str, sell: &str) -> Row {
        row(episode(id, &format!("60010{id}"), buy, vec![fill("buy", buy, 10.0, 100.0, Some(0.0)), fill("sell", sell, 10.0, 100.0, Some(0.0))]))
    }

    #[test]
    fn after_loss_streak_describes_history_only() {
        let rows: Vec<Row> = [-1.0, -1.0, -1.0, 2.0, -1.0].iter().enumerate().map(|(i, p)| closed(i as i64 + 1, &format!("2026-10-{:02}", i + 10), &format!("2026-10-{:02}", i + 11), *p)).collect();
        let v = violations(&rows, &rules(), true, None);
        // 默认连亏 3 笔后停手:第 4 笔(+2.0)是连亏后的第一笔,第 5 笔前连亏被打断
        assert_eq!((v["after_loss_streak"]["threshold"].clone(), v["after_loss_streak"]["trades"].clone(), v["after_loss_streak"]["win_rate"].clone()), (json!(3), json!(1), json!(1.0)));
    }

    #[test]
    fn grouping_orders_hold_periods_and_excludes_flat_from_win_rate() {
        let mut a = closed(1, "2026-10-12", "2026-10-12", 1.0);
        a.e.playbook = Some("打板".into());
        let b = closed(2, "2026-10-12", "2026-10-13", -1.0);
        let c = closed(3, "2026-10-10", "2026-10-15", 0.0);
        let open = row(episode(4, "600004", "2026-10-16", vec![fill("buy", "2026-10-16", 10.0, 100.0, Some(0.0))]));
        let s = stats(&[a, b, c, open]);
        let hold: Vec<&str> = s["by_hold"].as_array().unwrap().iter().map(|g| g["key"].as_str().unwrap()).collect();
        assert_eq!(hold, vec!["同日买卖(底仓待核)", "隔日", "持有5天", "还没卖"]);
        assert_eq!(s["overall"]["count"], 4);
        assert_eq!(s["overall"]["scored"], 3);
        assert_eq!(s["overall"]["win_rate"], 0.5, "持平不进分母:1 赢 1 亏");
        let flat_only = bucket(&[&closed(5, "2026-10-10", "2026-10-11", 0.0)]);
        assert_eq!(flat_only["win_rate"], Value::Null, "全部持平不是「没赢过」");
        assert_eq!(s["by_playbook"][0]["key"], "打板");
        assert_eq!(s["by_playbook"][1]["key"], UNTAGGED);
    }

    #[test]
    fn at_risk_floors_at_zero_and_reports_unbounded_separately() {
        let mut locked = row(episode(1, "600001", "2026-10-12", vec![fill("buy", "2026-10-12", 10.0, 100.0, Some(0.0))]));
        locked.e.planned_stop = Some(11.0);
        let mut bounded = row(episode(2, "600002", "2026-10-12", vec![fill("buy", "2026-10-12", 20.0, 100.0, Some(0.0))]));
        bounded.e.planned_stop = Some(18.0);
        let unbounded = row(episode(3, "600003", "2026-10-12", vec![fill("buy", "2026-10-12", 30.0, 100.0, Some(0.0))]));
        let v = at_risk(&[locked, bounded, unbounded], &rules(), true, Some(50000.0));
        assert_eq!((v["total_at_risk"].clone(), v["bounded_count"].clone(), v["unbounded_count"].clone(), v["total_capital"].clone()), (json!(200.0), json!(2), json!(1), json!(6000.0)));
        assert_eq!(v["positions"][0]["at_risk"], 0.0, "止损高于成本 = 已锁定盈利,在险是 0 不是负数");
        assert!(v["unbounded_note"].as_str().unwrap().contains("3,000 元") && v["unbounded_note"].as_str().unwrap().contains("下限"));
        assert_eq!(v["at_risk_of_equity_pct"], 0.4);
        assert_eq!(v["over_position_limit"], Value::Null);
        let no_base = at_risk(&[row(episode(1, "600001", "2026-10-12", vec![fill("buy", "2026-10-12", 10.0, 100.0, Some(0.0))]))], &rules(), true, None);
        assert!(no_base["equity_base_hint"].as_str().unwrap().contains("账户规模"));
        assert_eq!(at_risk(&[closed(1, "2026-10-12", "2026-10-13", 1.0)], &rules(), true, None)["available"], false);
    }

    #[test]
    fn view_runs_against_a_real_database() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-jrisk-{nonce}-{}", std::process::id()));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let db = app.db_path().to_path_buf();
        let empty = view(&db).unwrap();
        assert_eq!(empty["risk"]["equity"]["available"], false);
        assert_eq!(empty["settings"]["fees_default"], true);
        crate::journal::act(&db, &json!({"action": "record", "symbol": "600519", "name": "贵州茅台", "playbook": "低吸", "as_planned": true,
            "fills": [{"type": "buy", "date": "2026-10-12", "price": 100, "shares": 100, "fee": 5}, {"type": "sell", "date": "2026-10-14", "price": 110, "shares": 100, "fee": 6}]})).unwrap();
        let v = view(&db).unwrap();
        assert_eq!(v["risk"]["equity"]["trades"], 1);
        assert_eq!(v["risk"]["equity"]["net_pnl"], 989.0);
        assert_eq!(v["stats"]["by_hold"][0]["key"], "持有2天");
        // 与「交易统计」页同一口径
        let review = crate::journal_review::view(&db).unwrap();
        assert_eq!(review["performance"]["net_pnl"], 989.0);
        // 改费率后,没填费用的成交按新费率估
        crate::journal::act(&db, &json!({"action": "record", "symbol": "000001", "name": "平安银行",
            "fills": [{"type": "buy", "date": "2026-10-12", "price": 10, "shares": 1000}, {"type": "sell", "date": "2026-10-13", "price": 10, "shares": 1000}]})).unwrap();
        let pnl_on = |v: &Value, day: &str| v["risk"]["equity"]["points"].as_array().unwrap().iter().find(|p| p["date"] == day).map(|p| p["pnl"].as_f64().unwrap());
        let default_fee = pnl_on(&view(&db).unwrap(), "2026-10-13").unwrap();
        journal_settings::save(&db, &json!({"fees": {"commission_rate": 0, "commission_min": 0, "stamp_tax_rate": 0, "transfer_fee_rate": 0}})).unwrap();
        let zero_fee = pnl_on(&view(&db).unwrap(), "2026-10-13").unwrap();
        assert!(default_fee < -10.0, "默认费率下平价买卖亏手续费(两边佣金最低各 5 元 + 印花税 + 过户费): {default_fee}");
        assert_eq!(zero_fee, 0.0, "费率清零后没填费用的成交不再估费");
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 生成前端冻结夹具(全部是合成数据,不含任何真实成交):
    /// VIBE_JOURNAL_RISK_FIXTURE=路径 cargo test --locked journal_risk::tests::dump_synthetic_fixture -- --ignored
    #[test]
    #[ignore]
    fn dump_synthetic_fixture() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("vibe-jrisk-fixture-{nonce}"));
        let app = crate::AppCore::new(dir.clone()).unwrap();
        let db = app.db_path().to_path_buf();
        journal_settings::save(&db, &json!({"rules": {"max_positions": 2, "max_loss_per_trade_pct": 4}, "equity_base": 100000})).unwrap();
        // (代码, 名称, 买入日, 卖出日, 买价, 卖价, 股数, 打法, 是否按计划, 止损)
        let trades: [(&str, &str, &str, &str, f64, f64, f64, &str, Option<bool>); 13] = [
            ("600001", "样本一", "2026-09-21", "2026-09-22", 10.0, 10.8, 1000.0, "打板", Some(true)),
            ("600002", "样本二", "2026-09-22", "2026-09-24", 20.0, 21.5, 500.0, "低吸", Some(true)),
            ("600003", "样本三", "2026-09-23", "2026-09-23", 8.0, 7.7, 2000.0, "打板", Some(false)),
            ("600004", "样本四", "2026-09-25", "2026-09-28", 15.0, 16.2, 800.0, "接力", Some(true)),
            ("600005", "样本五", "2026-09-28", "2026-09-29", 12.0, 11.0, 1500.0, "接力", Some(false)),
            ("600006", "样本六", "2026-09-29", "2026-10-09", 30.0, 31.0, 300.0, "半路", None),
            ("600007", "样本七", "2026-10-09", "2026-10-12", 9.0, 8.4, 2500.0, "打板", Some(false)),
            ("600008", "样本八", "2026-10-09", "2026-10-12", 18.0, 17.0, 1000.0, "打板", Some(true)),
            ("600009", "样本九", "2026-10-12", "2026-10-13", 25.0, 25.0, 400.0, "低吸", Some(true)),
            ("600010", "样本十", "2026-10-13", "2026-10-14", 11.0, 12.1, 1200.0, "接力", Some(true)),
            ("600011", "样本十一", "2026-10-14", "2026-10-15", 14.0, 13.0, 1400.0, "打板", Some(false)),
            ("600012", "样本十二", "2026-10-15", "2026-10-16", 6.0, 5.4, 4000.0, "半路", None),
            ("600013", "样本十三", "2026-10-15", "2026-10-16", 40.0, 42.0, 200.0, "低吸", Some(true)),
        ];
        for (code, name, buy, sell, bp, sp, shares, playbook, planned) in trades {
            let mut body = json!({"action": "record", "symbol": code, "name": name, "playbook": playbook,
                "fills": [{"type": "buy", "date": buy, "price": bp, "shares": shares}, {"type": "sell", "date": sell, "price": sp, "shares": shares}]});
            if let Some(p) = planned {
                body["as_planned"] = json!(p);
            }
            crate::journal::act(&db, &body).unwrap();
        }
        // 两笔没卖完:一笔写了止损,一笔没写;再来一笔没有成交明细的补记
        crate::journal::act(&db, &json!({"action": "record", "symbol": "600014", "name": "样本十四", "playbook": "低吸", "as_planned": true, "planned_stop": 18.5, "planned_target": 24,
            "fills": [{"type": "buy", "date": "2026-10-16", "price": 20, "shares": 1000}]})).unwrap();
        crate::journal::act(&db, &json!({"action": "record", "symbol": "600015", "name": "样本十五",
            "fills": [{"type": "buy", "date": "2026-10-16", "price": 9.5, "shares": 3000}]})).unwrap();
        crate::journal::act(&db, &json!({"action": "manual", "symbol": "600016", "name": "样本十六", "date": "2026-09-18", "pnl_pct": 3.2, "playbook": "打板", "as_planned": true})).unwrap();
        let v = view(&db).unwrap();
        if let Ok(out) = std::env::var("VIBE_JOURNAL_RISK_FIXTURE") {
            let mut v = v.clone();
            v["generated_at"] = json!("2026-10-17T02:00:00Z");
            std::fs::write(out, serde_json::to_string_pretty(&v).unwrap()).unwrap();
        }
        println!("trades {} net {} maxdd {} violations {}", v["risk"]["equity"]["trades"], v["risk"]["equity"]["net_pnl"], v["risk"]["equity"]["max_drawdown"], v["risk"]["violations"]["violation_count"]);
        drop(app);
        let _ = std::fs::remove_dir_all(dir);
    }
}
