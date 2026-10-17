//! 我的持仓 · 收益表现(移动版;口径对照 backtest/portfolio_perf.py,只做人民币 A 股):
//!  - **时间加权收益(TWR)**:每天的涨跌连乘,存取钱的多少和时机不影响它 —— 看「选的这些票本身走得怎么样」;
//!  - **资金加权**:区间值(总取出 + 期末市值 ÷ 总投入 − 1)与年化内部收益率(XIRR,区间不满一年不年化)—— 看「我这些钱实际赚了多少」。
//!
//! 🔴 现金流口径:买入在当天开头投入(成交价 × 数量 + 费用),卖出与现金分红在当天收盘取出(卖出扣费用);
//!    没填费用的成交按「设置」里的费率估(估算对 ETF 等不收印花税的品种会偏高,页面写明)。
//! 🔴 价格:腾讯**不复权**日线。手机端没有「送转拆股」流水,价格出现单日断崖(收盘 < 前一天 72% 或 > 145%)而当天前后
//!    又没法确认时,列进提醒 —— 那一段的市值可能算错,TWR 不可信;不去猜、不自动修正。
//! 🔴 取不到价格的整只不算,列进缺口(北交所腾讯日线取不到、持有超过约三年一次取不全、第一根价格晚于第一笔成交太多)。
//!    不拿成本价或 0 去凑市值。停牌的天按最近一个收盘价结转。
//! 🔴 只给数字,不评好坏、不和指数比输赢、不给操作提示。个人成交与收益只在本机,不进任何 AI。

use crate::{discovery::DiscoveryError, journal_excursion::is_bj, journal_risk, journal_settings, symbol::normalize_stock_symbol};
use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use futures_util::{stream, StreamExt};
use reqwest::Client;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
    time::Duration,
};

const EPS: f64 = 1e-8;
const JUMP_DOWN: f64 = 0.72;
const JUMP_UP: f64 = 1.45;
const MAX_SYMBOLS: usize = 60;
const MAX_SPAN_DAYS: i64 = 1100;
/// 第一根价格比第一笔成交晚超过这么多天,当作取价被窗口截断
const LATE_START_DAYS: i64 = 20;

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
fn r4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Side {
    Buy,
    Sell,
    Dividend,
}

#[derive(Clone, Debug)]
pub(crate) struct Flow {
    pub symbol: String,
    pub name: String,
    pub date: String,
    pub side: Side,
    pub shares: f64,
    pub price: f64,
    /// 买卖的费用;分红为 0
    pub fee: f64,
    /// 分红到手金额(买卖为 0)
    pub amount: f64,
    pub fee_estimated: bool,
}

pub(crate) fn flows_of(rows: &[journal_risk::Row], dividends: &[crate::portfolio::Dividend], fees: &journal_settings::Fees) -> Vec<Flow> {
    let mut out = Vec::new();
    for r in rows.iter().filter(|r| r.e.kind == "episode") {
        for f in &r.e.fills {
            let buy = f.side == "buy";
            out.push(Flow {
                symbol: r.e.symbol.clone(), name: r.e.name.clone(), date: f.date.clone(), side: if buy { Side::Buy } else { Side::Sell },
                shares: f.shares, price: f.price, fee: f.fee.unwrap_or_else(|| fees.estimate(&f.side, f.price, f.shares)), amount: 0.0, fee_estimated: f.fee.is_none(),
            });
        }
    }
    for d in dividends {
        out.push(Flow { symbol: d.symbol.clone(), name: String::new(), date: d.date.clone(), side: Side::Dividend, shares: 0.0, price: 0.0, fee: 0.0, amount: d.amount, fee_estimated: false });
    }
    out
}

/// 年化内部收益率:Σ cf / (1+r)^(天数/365) = 0。现金流要有正有负,否则解不出(None)
pub(crate) fn xirr(cfs: &[(f64, f64)]) -> Option<f64> {
    if !cfs.iter().any(|c| c.1 > 0.0) || !cfs.iter().any(|c| c.1 < 0.0) {
        return None;
    }
    let f = |r: f64| cfs.iter().map(|(t, cf)| cf / (1.0 + r).powf(*t / 365.0)).sum::<f64>();
    let (mut lo, mut hi) = (-0.9999_f64, 1000.0_f64);
    let (flo, fhi) = (f(lo), f(hi));
    if !flo.is_finite() || !fhi.is_finite() || flo * fhi > 0.0 {
        return None;
    }
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        let fm = f(mid);
        if !fm.is_finite() {
            return None;
        }
        if (f(lo) < 0.0) == (fm < 0.0) { lo = mid } else { hi = mid }
        if (hi - lo).abs() < 1e-10 {
            break;
        }
    }
    Some((lo + hi) / 2.0)
}

fn days_between(a: &str, b: &str) -> i64 {
    match (NaiveDate::parse_from_str(a, "%Y-%m-%d"), NaiveDate::parse_from_str(b, "%Y-%m-%d")) {
        (Ok(a), Ok(b)) => (b - a).num_days(),
        _ => 0,
    }
}

/// 纯计算:成交与分红 + 每只的收盘价序列 → 收益表现。closes 里没有的代码调用方已经列进 gaps。
pub(crate) fn compute(flows: &[Flow], closes: &HashMap<String, BTreeMap<String, f64>>, gaps: Vec<Value>) -> Value {
    let included: BTreeSet<&str> = flows.iter().map(|f| f.symbol.as_str()).filter(|s| closes.contains_key(*s)).collect();
    let used: Vec<&Flow> = flows.iter().filter(|f| included.contains(f.symbol.as_str())).collect();
    if used.is_empty() {
        return json!({"ok": false, "reason": if gaps.is_empty() { "还没有可以算收益的成交".to_string() } else { "所有持仓都取不到价格,没法算".to_string() }, "gaps": gaps});
    }
    let first = used.iter().map(|f| f.date.as_str()).min().unwrap().to_string();
    // 交易日:各只收盘价日期的并集(从第一笔成交起)
    let days: Vec<String> = included.iter().flat_map(|s| closes[*s].keys().cloned()).filter(|d| d.as_str() >= first.as_str()).collect::<BTreeSet<_>>().into_iter().collect();
    if days.len() < 2 {
        return json!({"ok": false, "reason": "第一笔成交以来的交易日不足 2 天,算不出收益", "gaps": gaps});
    }
    // 把落在非交易日的流水挪到下一个交易日
    let day_of = |d: &str| days.iter().find(|x| x.as_str() >= d).cloned();
    let mut by_day: BTreeMap<String, Vec<&Flow>> = BTreeMap::new();
    let mut dropped = 0usize;
    for f in &used {
        match day_of(&f.date) {
            Some(d) => by_day.entry(d).or_default().push(*f),
            None => dropped += 1,
        }
    }
    let mut shares: BTreeMap<&str, f64> = BTreeMap::new();
    let mut last_close: HashMap<&str, f64> = HashMap::new();
    let (mut invested, mut withdrawn) = (0.0_f64, 0.0_f64);
    let mut per_symbol: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    let mut cfs: Vec<(String, f64)> = Vec::new();
    let mut prev_mv: Option<f64> = None;
    let (mut index, mut series): (f64, Vec<(String, f64)>) = (1.0, Vec::new());
    let mut warnings: Vec<String> = Vec::new();
    let mut started = false;
    let mut last_day_mv = 0.0;
    let mut prev_close_by_symbol: HashMap<&str, (String, f64)> = HashMap::new();
    for day in &days {
        let todays = by_day.get(day).cloned().unwrap_or_default();
        let mut invest = 0.0;
        for f in todays.iter().filter(|f| f.side == Side::Buy) {
            invest += f.price * f.shares + f.fee;
            *shares.entry(f.symbol.as_str()).or_default() += f.shares;
            per_symbol.entry(f.symbol.as_str()).or_default().0 += f.price * f.shares + f.fee;
        }
        // 当天收盘价(没有就结转最近一个)
        for s in &included {
            if let Some(c) = closes[*s].get(day) {
                if let Some((pd, pc)) = prev_close_by_symbol.get(s) {
                    let ratio = c / pc;
                    if (ratio < JUMP_DOWN || ratio > JUMP_UP) && shares.get(s).copied().unwrap_or(0.0) > EPS {
                        warnings.push(format!("{s} 在 {day} 收盘价比 {pd} 变了 {:.0}%:可能是送转拆股没有记录(手机端没有送转流水),这一段的市值和时间加权收益可能算错", (ratio - 1.0) * 100.0));
                    }
                }
                prev_close_by_symbol.insert(s, (day.clone(), *c));
                last_close.insert(s, *c);
            }
        }
        let mut withdraw = 0.0;
        for f in todays.iter().filter(|f| f.side != Side::Buy) {
            if f.side == Side::Sell {
                let got = f.price * f.shares - f.fee;
                withdraw += got;
                *shares.entry(f.symbol.as_str()).or_default() -= f.shares;
                per_symbol.entry(f.symbol.as_str()).or_default().1 += got;
            } else {
                withdraw += f.amount;
                per_symbol.entry(f.symbol.as_str()).or_default().1 += f.amount;
            }
        }
        let mv: f64 = shares.iter().filter(|(_, n)| **n > EPS).map(|(s, n)| n * last_close.get(s).copied().unwrap_or(0.0)).sum();
        invested += invest;
        withdrawn += withdraw;
        if invest > 0.0 {
            cfs.push((day.clone(), -invest));
        }
        if withdraw > 0.0 {
            cfs.push((day.clone(), withdraw));
        }
        let denom = prev_mv.unwrap_or(0.0) + invest;
        if denom > EPS {
            index *= (mv + withdraw) / denom;
            started = true;
        }
        if started {
            series.push((day.clone(), r4(index - 1.0) * 100.0));
        }
        prev_mv = Some(mv);
        last_day_mv = mv;
    }
    let last = days.last().unwrap().clone();
    if !started || invested <= EPS {
        return json!({"ok": false, "reason": "区间内没有买入,算不出收益", "gaps": gaps});
    }
    cfs.push((last.clone(), last_day_mv));
    let t0 = cfs.first().map(|c| c.0.clone()).unwrap();
    let rel: Vec<(f64, f64)> = cfs.iter().map(|(d, cf)| (days_between(&t0, d) as f64, *cf)).collect();
    let span = days_between(&first, &last);
    let irr = if span >= 365 { xirr(&rel) } else { None };
    let mw = (withdrawn + last_day_mv) / invested - 1.0;
    // 逐只:总投入、总取出、期末市值、盈亏
    let names: HashMap<&str, &str> = flows.iter().filter(|f| !f.name.is_empty()).map(|f| (f.symbol.as_str(), f.name.as_str())).collect();
    let mut symbols: Vec<Value> = per_symbol
        .iter()
        .map(|(s, (inv, out))| {
            let n = shares.get(s).copied().unwrap_or(0.0).max(0.0);
            let value = n * last_close.get(s).copied().unwrap_or(0.0);
            json!({"symbol": s, "name": names.get(s).copied().unwrap_or(""), "invested": r2(*inv), "withdrawn": r2(*out), "shares": r2(n), "value": r2(value), "pnl": r2(out + value - inv)})
        })
        .collect();
    symbols.sort_by(|a, b| b["pnl"].as_f64().partial_cmp(&a["pnl"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
    // 序列最多 150 个点(含最后一个)
    let step = (series.len() as f64 / 150.0).ceil().max(1.0) as usize;
    let mut thin: Vec<Value> = series.iter().enumerate().filter(|(i, _)| i % step == 0).map(|(_, (d, v))| json!({"date": d, "twr_pct": r2(*v)})).collect();
    if let Some((d, v)) = series.last() {
        if thin.last().and_then(|x| x["date"].as_str()) != Some(d.as_str()) {
            thin.push(json!({"date": d, "twr_pct": r2(*v)}));
        }
    }
    warnings.sort();
    warnings.dedup();
    if dropped > 0 {
        warnings.push(format!("{dropped} 笔流水晚于最后一个交易日,没有计入"));
    }
    if used.iter().any(|f| f.fee_estimated) {
        warnings.push("部分成交没填费用,按设置里的费率估算(对 ETF 等不收印花税的品种会偏高)".into());
    }
    json!({
        "ok": true, "from": first, "to": last, "span_days": span, "trading_days": days.len(),
        "twr_pct": r2((index - 1.0) * 100.0), "money_weighted_pct": r2(mw * 100.0),
        "xirr_pct": irr.map(|v| r2(v * 100.0)),
        "xirr_note": if span < 365 { Some(format!("区间 {span} 天不满一年,年化会把短期涨跌放大,不给年化数")) } else if irr.is_none() { Some("现金流解不出年化收益率".to_string()) } else { None },
        "invested": r2(invested), "withdrawn": r2(withdrawn), "final_value": r2(last_day_mv), "pnl": r2(withdrawn + last_day_mv - invested),
        "series": thin, "symbols": symbols, "gaps": gaps, "warnings": warnings,
        "note": "时间加权收益把每天的涨跌连乘,存取钱的多少和时机不影响它;资金加权看你这些钱实际赚了多少。买入在当天开头投入、卖出与分红在当天收盘取出;价格是腾讯不复权日线,手机端没有送转流水,价格断崖会列在提醒里。只给数字,不评好坏。",
    })
}

async fn fetch_closes(client: &Client, symbol: &str, start: &str, end: &str) -> Result<BTreeMap<String, f64>> {
    let key = normalize_stock_symbol(symbol).map_err(|e| anyhow!("{e}"))?;
    let days = days_between(start, end);
    if days > MAX_SPAN_DAYS {
        return Err(anyhow!("持有超过约三年,腾讯日 K 一次取不全"));
    }
    let count = ((days as f64 * 0.75) as i64 + 30).clamp(30, 800);
    let root: Value = client
        .get("https://web.ifzq.gtimg.cn/appstock/app/fqkline/get")
        .query(&[("param", format!("{},day,{},{},{count},", key.tencent, start, end))])
        .timeout(Duration::from_secs(12))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let (bars, _, _) = crate::chart::parse_tencent_kline(&root, &key, "day")?;
    let map: BTreeMap<String, f64> = bars.into_iter().filter(|b| b.0.len() == 10 && b.0.as_str() >= start && b.0.as_str() <= end).map(|b| (b.0, b.2)).collect();
    if map.is_empty() {
        return Err(anyhow!("腾讯日 K 没有返回这段时间的数据"));
    }
    Ok(map)
}

pub async fn view(client: &Client, db: &Path) -> std::result::Result<Value, DiscoveryError> {
    let settings = journal_settings::load(db).map_err(DiscoveryError::Upstream)?;
    let entries = journal_risk::load_entries(db).map_err(DiscoveryError::Upstream)?;
    let rows = journal_risk::rows_of(entries, &settings.fees);
    let conn = rusqlite::Connection::open(db).map_err(|e| DiscoveryError::Upstream(e.into()))?;
    let dividends = crate::portfolio::read_dividends(&conn).map_err(DiscoveryError::Upstream)?;
    let flows = flows_of(&rows, &dividends, &settings.fees);
    if !flows.iter().any(|f| f.side == Side::Buy) {
        return Ok(json!({"ok": false, "reason": "还没有买入成交,算不出收益。先在交易日志里记成交,或从表格导入。", "gaps": []}));
    }
    let today = crate::emotion::now_china().0.format("%Y-%m-%d").to_string();
    let mut first_by: BTreeMap<String, String> = BTreeMap::new();
    for f in &flows {
        let e = first_by.entry(f.symbol.clone()).or_insert_with(|| f.date.clone());
        if f.date < *e {
            *e = f.date.clone();
        }
    }
    if first_by.len() > MAX_SYMBOLS {
        return Ok(json!({"ok": false, "reason": format!("一次最多算 {MAX_SYMBOLS} 只,现在有 {} 只", first_by.len()), "gaps": []}));
    }
    let mut gaps: Vec<Value> = Vec::new();
    let mut targets: Vec<(String, String)> = Vec::new();
    for (symbol, first) in &first_by {
        if is_bj(symbol) {
            gaps.push(json!({"symbol": symbol, "why": "北交所腾讯日线取不到,这只没算"}));
        } else {
            targets.push((symbol.clone(), first.clone()));
        }
    }
    let fetched: Vec<(String, String, Result<BTreeMap<String, f64>>)> = stream::iter(targets)
        .map(|(symbol, first)| {
            let client = client.clone();
            let today = today.clone();
            async move {
                let r = fetch_closes(&client, &symbol, &first, &today).await;
                (symbol, first, r)
            }
        })
        .buffer_unordered(4)
        .collect()
        .await;
    let mut closes: HashMap<String, BTreeMap<String, f64>> = HashMap::new();
    for (symbol, first, result) in fetched {
        match result {
            Ok(map) => {
                let start = map.keys().next().cloned().unwrap_or_default();
                if days_between(&first, &start) > LATE_START_DAYS {
                    gaps.push(json!({"symbol": symbol, "why": format!("第一根价格({start})比第一笔成交({first})晚了 {} 天,取价被截断,这只没算", days_between(&first, &start))}));
                } else {
                    closes.insert(symbol, map);
                }
            }
            Err(e) => gaps.push(json!({"symbol": symbol, "why": format!("取不到价格:{e}")})),
        }
    }
    gaps.sort_by(|a, b| a["symbol"].as_str().cmp(&b["symbol"].as_str()));
    Ok(compute(&flows, &closes, gaps))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(symbol: &str, date: &str, side: Side, shares: f64, price: f64) -> Flow {
        Flow { symbol: symbol.into(), name: format!("N{symbol}"), date: date.into(), side, shares, price, fee: 0.0, amount: 0.0, fee_estimated: false }
    }

    fn closes(symbol: &str, rows: &[(&str, f64)]) -> HashMap<String, BTreeMap<String, f64>> {
        HashMap::from([(symbol.to_string(), rows.iter().map(|(d, c)| (d.to_string(), *c)).collect())])
    }

    #[test]
    fn xirr_matches_a_known_one_year_return_and_needs_both_signs() {
        let r = xirr(&[(0.0, -100.0), (365.0, 110.0)]).unwrap();
        assert!((r - 0.10).abs() < 1e-6, "{r}");
        let two = xirr(&[(0.0, -100.0), (182.5, -100.0), (365.0, 231.0)]).unwrap();
        assert!(two > 0.0 && two < 0.35, "{two}");
        assert!(xirr(&[(0.0, -100.0), (30.0, -5.0)]).is_none());
        assert!(xirr(&[(0.0, 100.0)]).is_none());
    }

    #[test]
    fn twr_chains_daily_returns_and_ignores_the_size_of_later_deposits() {
        // 第 1 天开头买 100 股@10,收盘 10;第 2 天收盘 11(+10%);第 3 天开头再买 100 股@11,收盘 12.1(+10%)
        let flows = vec![flow("600519", "2026-01-05", Side::Buy, 100.0, 10.0), flow("600519", "2026-01-07", Side::Buy, 100.0, 11.0)];
        let c = closes("600519", &[("2026-01-05", 10.0), ("2026-01-06", 11.0), ("2026-01-07", 12.1)]);
        let v = compute(&flows, &c, vec![]);
        assert_eq!(v["ok"], true);
        assert_eq!(v["twr_pct"], json!(21.0), "(1.1)×(1.1) − 1 = 21%,第二次加仓的金额不影响");
        assert_eq!((v["invested"].clone(), v["final_value"].clone(), v["pnl"].clone()), (json!(2100.0), json!(2420.0), json!(320.0)));
        assert_eq!(v["money_weighted_pct"], json!(15.24), "2420/2100 − 1");
        assert!(v["xirr_pct"].is_null() && v["xirr_note"].as_str().unwrap().contains("不满一年"));
        assert_eq!(v["symbols"][0]["pnl"], json!(320.0));
        assert_eq!(v["from"], "2026-01-05");
        assert_eq!(v["trading_days"], 3);
    }

    #[test]
    fn sells_and_dividends_are_withdrawn_at_the_close_and_fees_reduce_proceeds() {
        let mut sell = flow("600519", "2026-01-06", Side::Sell, 100.0, 11.0);
        sell.fee = 10.0;
        let mut buy = flow("600519", "2026-01-05", Side::Buy, 100.0, 10.0);
        buy.fee = 5.0;
        let dividend = Flow { amount: 20.0, side: Side::Dividend, shares: 0.0, price: 0.0, ..flow("600519", "2026-01-07", Side::Dividend, 0.0, 0.0) };
        let c = closes("600519", &[("2026-01-05", 10.0), ("2026-01-06", 11.0), ("2026-01-07", 11.5)]);
        let v = compute(&[buy, sell, dividend], &c, vec![]);
        assert_eq!((v["invested"].clone(), v["withdrawn"].clone(), v["final_value"].clone()), (json!(1005.0), json!(1110.0), json!(0.0)));
        assert_eq!(v["pnl"], json!(105.0), "卖出 1100 − 10 + 分红 20 − 买入 1005");
        assert_eq!(v["symbols"][0]["shares"], json!(0.0));
    }

    #[test]
    fn price_cliffs_without_a_split_record_are_warned_and_missing_symbols_stay_in_the_gaps() {
        let flows = vec![flow("600519", "2026-01-05", Side::Buy, 100.0, 10.0), flow("000001", "2026-01-05", Side::Buy, 100.0, 10.0)];
        let c = closes("600519", &[("2026-01-05", 10.0), ("2026-01-06", 5.0), ("2026-01-07", 5.1)]);
        let v = compute(&flows, &c, vec![json!({"symbol": "000001", "why": "取不到价格"})]);
        assert_eq!(v["ok"], true);
        assert!(v["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("600519 在 2026-01-06") && w.as_str().unwrap().contains("送转")));
        assert_eq!(v["gaps"][0]["symbol"], "000001");
        assert!(v["symbols"].as_array().unwrap().iter().all(|s| s["symbol"] != "000001"), "没价格的整只不算");
        let none = compute(&flows, &HashMap::new(), vec![json!({"symbol": "x", "why": "y"})]);
        assert_eq!(none["ok"], false);
        assert!(none["reason"].as_str().unwrap().contains("取不到价格"));
    }

    #[test]
    fn a_year_or_more_gives_annualised_return_and_the_series_is_thinned() {
        let mut rows: Vec<(String, f64)> = Vec::new();
        let start = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        for i in 0..420 {
            let d = start + chrono::Duration::days(i);
            rows.push((d.format("%Y-%m-%d").to_string(), 10.0 * (1.0 + 0.1 * i as f64 / 365.0)));
        }
        let borrowed: Vec<(&str, f64)> = rows.iter().map(|(d, c)| (d.as_str(), *c)).collect();
        let v = compute(&[flow("600519", "2025-01-01", Side::Buy, 100.0, 10.0)], &closes("600519", &borrowed), vec![]);
        let irr = v["xirr_pct"].as_f64().unwrap();
        assert!(irr > 9.0 && irr < 11.5, "线性涨 10%/年,年化约 10%:{irr}");
        assert!(v["series"].as_array().unwrap().len() <= 151);
        assert_eq!(v["series"].as_array().unwrap().last().unwrap()["date"], v["to"]);
    }

    /// 合成数据夹具(不是任何真实持仓):VIBE_PERF_FIXTURE=路径 cargo test portfolio_perf::tests::dump_synthetic_fixture -- --ignored
    #[test]
    #[ignore]
    fn dump_synthetic_fixture() {
        let Ok(out) = std::env::var("VIBE_PERF_FIXTURE") else { return };
        let start = NaiveDate::from_ymd_opt(2025, 3, 3).unwrap();
        let mut a: Vec<(String, f64)> = Vec::new();
        let mut b: Vec<(String, f64)> = Vec::new();
        let mut d = start;
        let mut i = 0;
        while a.len() < 300 {
            if d.format("%u").to_string().parse::<u32>().unwrap() <= 5 {
                a.push((d.format("%Y-%m-%d").to_string(), 100.0 * (1.0 + 0.0012 * i as f64 + if i % 3 == 0 { 0.012 } else { -0.006 })));
                b.push((d.format("%Y-%m-%d").to_string(), 20.0 * (1.0 - 0.0003 * i as f64 + if i % 2 == 0 { 0.02 } else { -0.01 })));
                i += 1;
            }
            d += chrono::Duration::days(1);
        }
        let ar: Vec<(&str, f64)> = a.iter().map(|(d, c)| (d.as_str(), *c)).collect();
        let br: Vec<(&str, f64)> = b.iter().map(|(d, c)| (d.as_str(), *c)).collect();
        let mut c = closes("600519", &ar);
        c.extend(closes("000001", &br));
        let flows = vec![
            Flow { name: "样本白酒".into(), ..flow("600519", &a[0].0, Side::Buy, 100.0, a[0].1) },
            Flow { name: "样本银行".into(), ..flow("000001", &b[10].0, Side::Buy, 1000.0, b[10].1) },
            Flow { name: "样本白酒".into(), ..flow("600519", &a[120].0, Side::Buy, 50.0, a[120].1) },
            Flow { name: "样本银行".into(), ..flow("000001", &b[200].0, Side::Sell, 400.0, b[200].1) },
            Flow { name: "样本白酒".into(), amount: 300.0, ..flow("600519", &a[150].0, Side::Dividend, 0.0, 0.0) },
        ];
        let v = compute(&flows, &c, vec![json!({"symbol": "920575", "why": "北交所腾讯日线取不到,这只没算"})]);
        std::fs::write(out, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }
}
