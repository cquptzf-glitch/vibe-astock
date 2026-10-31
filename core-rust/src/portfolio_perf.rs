//! 我的持仓 · 收益表现(移动版;口径对照 backtest/portfolio_perf.py,只做人民币 A 股):
//!  - **时间加权收益(TWR)**:每天的涨跌连乘,存取钱的多少和时机不影响它 —— 看「选的这些票本身走得怎么样」;
//!  - **资金加权**:区间值(总取出 + 期末市值 ÷ 总投入 − 1)与年化内部收益率(XIRR,区间不满一年不年化)—— 看「我这些钱实际赚了多少」。
//!
//! 🔴 现金流口径:买入在当天开头投入(成交价 × 数量 + 费用),卖出与现金分红在当天收盘取出(卖出扣费用);
//!    没填费用的成交按「设置」里的费率估(估算对 ETF 等不收印花税的品种会偏高,页面写明)。
//! 🔴 价格:沪深腾讯、北交所东财**不复权**日线。已记录送转按股数换算;未知价格断崖列进提醒,
//!    那一段的市值和 TWR 可能算错;不去猜、不自动修正。
//! 🔴 取不到价格的整只不算,列进缺口(行情异常、持有超过约三年一次取不全、首笔买入日缺行情)。
//!    不拿成本价或 0 去凑市值。停牌的天按最近一个收盘价结转。
//! 🔴 只给数字,不评好坏、不和指数比输赢、不给操作提示。个人成交与收益只在本机,不进任何 AI。

use crate::{
    discovery::DiscoveryError, journal_excursion::is_bj, journal_risk, journal_settings,
    symbol::normalize_stock_symbol,
};
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
    Split,
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

pub(crate) fn flows_of(
    rows: &[journal_risk::Row],
    dividends: &[crate::portfolio::Dividend],
    fees: &journal_settings::Fees,
) -> Vec<Flow> {
    let mut out = Vec::new();
    for r in rows.iter().filter(|r| r.e.kind == "episode") {
        let mut ordered: Vec<_> = r.e.fills.iter().collect();
        ordered.sort_by(|a, b| {
            a.date
                .cmp(&b.date)
                .then((a.side != "split").cmp(&(b.side != "split")))
        });
        let mut position = 0.0;
        for f in ordered {
            let split_delta = position * (f.price - 1.0);
            if f.side == "split" {
                position *= f.price;
            } else if f.side == "sell" {
                position = (position - f.shares).max(0.0);
            } else {
                position += f.shares;
            }
            let buy = matches!(f.side.as_str(), "buy" | "opening");
            out.push(Flow {
                symbol: r.e.symbol.clone(),
                name: r.e.name.clone(),
                date: f.date.clone(),
                side: if f.side == "split" {
                    Side::Split
                } else if buy {
                    Side::Buy
                } else {
                    Side::Sell
                },
                shares: if f.side == "split" {
                    split_delta
                } else {
                    f.shares
                },
                price: f.price,
                fee: f
                    .fee
                    .unwrap_or_else(|| fees.estimate(&f.side, f.price, f.shares)),
                amount: 0.0,
                fee_estimated: f.fee.is_none(),
            });
        }
    }
    for d in dividends {
        out.push(Flow {
            symbol: d.symbol.clone(),
            name: String::new(),
            date: d.date.clone(),
            side: Side::Dividend,
            shares: 0.0,
            price: 0.0,
            fee: 0.0,
            amount: d.amount,
            fee_estimated: false,
        });
    }
    out
}

/// 年化内部收益率:Σ cf / (1+r)^(天数/365) = 0。现金流要有正有负,否则解不出(None)
pub(crate) fn xirr(cfs: &[(f64, f64)]) -> Option<f64> {
    if !cfs.iter().any(|c| c.1 > 0.0) || !cfs.iter().any(|c| c.1 < 0.0) {
        return None;
    }
    let f = |r: f64| {
        cfs.iter()
            .map(|(t, cf)| cf / (1.0 + r).powf(*t / 365.0))
            .sum::<f64>()
    };
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
        if (f(lo) < 0.0) == (fm < 0.0) {
            lo = mid
        } else {
            hi = mid
        }
        if (hi - lo).abs() < 1e-10 {
            break;
        }
    }
    Some((lo + hi) / 2.0)
}

fn days_between(a: &str, b: &str) -> i64 {
    match (
        NaiveDate::parse_from_str(a, "%Y-%m-%d"),
        NaiveDate::parse_from_str(b, "%Y-%m-%d"),
    ) {
        (Ok(a), Ok(b)) => (b - a).num_days(),
        _ => 0,
    }
}

/// 纯计算:成交与分红 + 每只的收盘价序列 → 收益表现。closes 里没有的代码调用方已经列进 gaps。
pub(crate) fn compute(
    flows: &[Flow],
    closes: &HashMap<String, BTreeMap<String, f64>>,
    gaps: Vec<Value>,
) -> Value {
    let included: BTreeSet<&str> = flows
        .iter()
        .map(|f| f.symbol.as_str())
        .filter(|s| closes.contains_key(*s))
        .collect();
    let used: Vec<&Flow> = flows
        .iter()
        .filter(|f| included.contains(f.symbol.as_str()))
        .collect();
    if used.is_empty() {
        return json!({"ok": false, "reason": if gaps.is_empty() { "还没有可以算收益的成交".to_string() } else { "所有持仓都取不到价格,没法算".to_string() }, "gaps": gaps});
    }
    let first = used
        .iter()
        .map(|f| f.date.as_str())
        .min()
        .unwrap()
        .to_string();
    // 交易日:各只收盘价日期的并集(从第一笔成交起)
    let days: Vec<String> = included
        .iter()
        .flat_map(|s| closes[*s].keys().cloned())
        .filter(|d| d.as_str() >= first.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
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
        for f in todays.iter().filter(|f| f.side == Side::Split) {
            *shares.entry(f.symbol.as_str()).or_default() += f.shares;
        }
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
                    let multiplier = todays
                        .iter()
                        .find(|f| f.side == Side::Split && f.symbol == **s)
                        .map(|f| f.price)
                        .unwrap_or(1.0);
                    let ratio = c * multiplier / pc;
                    let jump_down = if is_bj(s) { 0.69 } else { JUMP_DOWN };
                    if (ratio < jump_down || ratio > JUMP_UP)
                        && shares.get(s).copied().unwrap_or(0.0) > EPS
                    {
                        warnings.push(format!("{s} 在 {day} 收盘价比 {pd} 变了 {:.0}%:可能是送转拆股没有记录(未记录或记录不完整),这一段的市值和时间加权收益可能算错", (ratio - 1.0) * 100.0));
                    }
                }
                prev_close_by_symbol.insert(s, (day.clone(), *c));
                last_close.insert(s, *c);
            }
        }
        let mut withdraw = 0.0;
        for f in todays
            .iter()
            .filter(|f| matches!(f.side, Side::Sell | Side::Dividend))
        {
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
        let mv: f64 = shares
            .iter()
            .filter(|(_, n)| **n > EPS)
            .map(|(s, n)| n * last_close.get(s).copied().unwrap_or(0.0))
            .sum();
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
    for s in &included {
        if shares.get(s).copied().unwrap_or(0.0) > EPS {
            if let Some(price_date) = closes[*s].keys().next_back().filter(|d| **d < last) {
                warnings.push(format!("{s} 最后价格为 {price_date}，之后至 {last} 按该收盘价结转；可能停牌或行情缺失，期末市值并非当日新报价"));
            }
        }
    }
    if !started || invested <= EPS {
        return json!({"ok": false, "reason": "区间内没有买入,算不出收益", "gaps": gaps});
    }
    cfs.push((last.clone(), last_day_mv));
    let t0 = cfs.first().map(|c| c.0.clone()).unwrap();
    let rel: Vec<(f64, f64)> = cfs
        .iter()
        .map(|(d, cf)| (days_between(&t0, d) as f64, *cf))
        .collect();
    let span = days_between(&first, &last);
    let irr = if span >= 365 { xirr(&rel) } else { None };
    let mw = (withdrawn + last_day_mv) / invested - 1.0;
    // 逐只:总投入、总取出、期末市值、盈亏
    let names: HashMap<&str, &str> = flows
        .iter()
        .filter(|f| !f.name.is_empty())
        .map(|f| (f.symbol.as_str(), f.name.as_str()))
        .collect();
    let mut symbols: Vec<Value> = per_symbol
        .iter()
        .map(|(s, (inv, out))| {
            let n = shares.get(s).copied().unwrap_or(0.0).max(0.0);
            let value = n * last_close.get(s).copied().unwrap_or(0.0);
            json!({"symbol": s, "name": names.get(s).copied().unwrap_or(""), "invested": r2(*inv), "withdrawn": r2(*out), "shares": r2(n), "value": r2(value), "pnl": r2(out + value - inv)})
        })
        .collect();
    symbols.sort_by(|a, b| {
        b["pnl"]
            .as_f64()
            .partial_cmp(&a["pnl"].as_f64())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // 序列最多 150 个点(含最后一个)
    let step = (series.len() as f64 / 150.0).ceil().max(1.0) as usize;
    let mut thin: Vec<Value> = series
        .iter()
        .enumerate()
        .filter(|(i, _)| i % step == 0)
        .map(|(_, (d, v))| json!({"date": d, "twr_pct": r2(*v)}))
        .collect();
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
        warnings
            .push("部分成交没填费用,按设置里的费率估算(对 ETF 等不收印花税的品种会偏高)".into());
    }
    json!({
        "ok": true, "from": first, "to": last, "span_days": span, "trading_days": days.len(),
        "twr_pct": r2((index - 1.0) * 100.0), "money_weighted_pct": r2(mw * 100.0),
        "xirr_pct": irr.map(|v| r2(v * 100.0)),
        "xirr_note": if span < 365 { Some(format!("区间 {span} 天不满一年,年化会把短期涨跌放大,不给年化数")) } else if irr.is_none() { Some("现金流解不出年化收益率".to_string()) } else { None },
        "invested": r2(invested), "withdrawn": r2(withdrawn), "final_value": r2(last_day_mv), "pnl": r2(withdrawn + last_day_mv - invested),
        "series": thin, "symbols": symbols, "gaps": gaps, "warnings": warnings,
        "note": "时间加权收益把每天的涨跌连乘,存取钱的多少和时机不影响它;资金加权看你这些钱实际赚了多少。买入在当天开头投入、卖出与分红在当天收盘取出;沪深使用腾讯、北交所使用东财不复权日线。已记录送转按股数换算,未知价格断崖列在提醒里;缺行情按最近收盘价结转,不能区分停牌和源站漏行。只给数字,不评好坏。",
    })
}

fn closes_from_response(
    root: &Value,
    symbol: &str,
    start: &str,
    end: &str,
) -> Result<BTreeMap<String, f64>> {
    let key = normalize_stock_symbol(symbol).map_err(|e| anyhow!("{e}"))?;
    let (bars, dropped, _) = if is_bj(symbol) {
        crate::chart::parse_em_kline(root, &key)?
    } else {
        crate::chart::parse_tencent_kline(root, &key, "day")?
    };
    if dropped > 0 {
        return Err(anyhow!(
            "日线有 {dropped} 行异常或重复，不能悄悄删行计算收益"
        ));
    }
    if bars
        .iter()
        .any(|b| NaiveDate::parse_from_str(&b.0, "%Y-%m-%d").is_err())
    {
        return Err(anyhow!("日线日期无效"));
    }
    let map: BTreeMap<String, f64> = bars
        .into_iter()
        .filter(|b| b.0.as_str() >= start && b.0.as_str() <= end)
        .map(|b| (b.0, b.2))
        .collect();
    if map.is_empty() {
        return Err(anyhow!("日 K 没有返回这段时间的数据"));
    }
    if map.keys().next().map(String::as_str) != Some(start) {
        return Err(anyhow!(
            "日线缺少第一笔买入日 {start} 的价格，不能用零市值开始计算；请核对成交日期和行情覆盖"
        ));
    }
    Ok(map)
}

async fn fetch_closes(
    client: &Client,
    symbol: &str,
    start: &str,
    end: &str,
) -> Result<BTreeMap<String, f64>> {
    let key = normalize_stock_symbol(symbol).map_err(|e| anyhow!("{e}"))?;
    let from = NaiveDate::parse_from_str(start, "%Y-%m-%d")?;
    let to = NaiveDate::parse_from_str(end, "%Y-%m-%d")?;
    let days = days_between(start, end);
    if !(0..=MAX_SPAN_DAYS).contains(&days) {
        return Err(anyhow!("持有区间无效或超过约三年,日 K 一次取不全"));
    }
    let count = ((days as f64 * 0.75) as i64 + 30).clamp(30, 800);
    let request = if is_bj(symbol) {
        client
            .get("https://push2his.eastmoney.com/api/qt/stock/kline/get")
            .query(&[
                ("secid", key.eastmoney),
                ("klt", "101".into()),
                ("fqt", "0".into()),
                ("beg", from.format("%Y%m%d").to_string()),
                ("end", to.format("%Y%m%d").to_string()),
                ("lmt", "1000".into()),
                ("fields1", "f1,f2,f3".into()),
                (
                    "fields2",
                    "f51,f52,f53,f54,f55,f56,f57,f58,f59,f60,f61".into(),
                ),
            ])
            .header("Referer", "https://quote.eastmoney.com/")
    } else {
        client
            .get("https://web.ifzq.gtimg.cn/appstock/app/fqkline/get")
            .query(&[(
                "param",
                format!("{},day,{},{},{count},", key.tencent, start, end),
            )])
    };
    let root: Value = request
        .timeout(Duration::from_secs(12))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    closes_from_response(&root, symbol, start, end)
}

pub async fn view(client: &Client, db: &Path) -> std::result::Result<Value, DiscoveryError> {
    let settings = journal_settings::load(db).map_err(DiscoveryError::Upstream)?;
    let entries = journal_risk::load_entries(db).map_err(DiscoveryError::Upstream)?;
    let rows = journal_risk::rows_of(entries, &settings.fees);
    let conn = rusqlite::Connection::open(db).map_err(|e| DiscoveryError::Upstream(e.into()))?;
    let dividends = crate::portfolio::read_dividends(&conn).map_err(DiscoveryError::Upstream)?;
    let flows = flows_of(&rows, &dividends, &settings.fees);
    if !flows.iter().any(|f| f.side == Side::Buy) {
        return Ok(
            json!({"ok": false, "reason": "还没有买入成交,算不出收益。先在交易日志里记成交,或从表格导入。", "gaps": []}),
        );
    }
    let today = crate::emotion::now_china().0.format("%Y-%m-%d").to_string();
    let mut first_by: BTreeMap<String, String> = BTreeMap::new();
    for f in flows.iter().filter(|f| f.side == Side::Buy) {
        let e = first_by
            .entry(f.symbol.clone())
            .or_insert_with(|| f.date.clone());
        if f.date < *e {
            *e = f.date.clone();
        }
    }
    if first_by.len() > MAX_SYMBOLS {
        return Ok(
            json!({"ok": false, "reason": format!("一次最多算 {MAX_SYMBOLS} 只,现在有 {} 只", first_by.len()), "gaps": []}),
        );
    }
    let mut gaps: Vec<Value> = Vec::new();
    let targets: Vec<(String, String)> = first_by.into_iter().collect();
    type PriceFetch = (String, String, Result<BTreeMap<String, f64>>);
    let fetched: Vec<PriceFetch> = stream::iter(targets)
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
    for (symbol, _first, result) in fetched {
        match result {
            Ok(map) => {
                closes.insert(symbol, map);
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
        Flow {
            symbol: symbol.into(),
            name: format!("N{symbol}"),
            date: date.into(),
            side,
            shares,
            price,
            fee: 0.0,
            amount: 0.0,
            fee_estimated: false,
        }
    }

    fn closes(symbol: &str, rows: &[(&str, f64)]) -> HashMap<String, BTreeMap<String, f64>> {
        HashMap::from([(
            symbol.to_string(),
            rows.iter().map(|(d, c)| (d.to_string(), *c)).collect(),
        )])
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
        let flows = vec![
            flow("600519", "2026-01-05", Side::Buy, 100.0, 10.0),
            flow("600519", "2026-01-07", Side::Buy, 100.0, 11.0),
        ];
        let c = closes(
            "600519",
            &[
                ("2026-01-05", 10.0),
                ("2026-01-06", 11.0),
                ("2026-01-07", 12.1),
            ],
        );
        let v = compute(&flows, &c, vec![]);
        assert_eq!(v["ok"], true);
        assert_eq!(
            v["twr_pct"],
            json!(21.0),
            "(1.1)×(1.1) − 1 = 21%,第二次加仓的金额不影响"
        );
        assert_eq!(
            (
                v["invested"].clone(),
                v["final_value"].clone(),
                v["pnl"].clone()
            ),
            (json!(2100.0), json!(2420.0), json!(320.0))
        );
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
        let dividend = Flow {
            amount: 20.0,
            side: Side::Dividend,
            shares: 0.0,
            price: 0.0,
            ..flow("600519", "2026-01-07", Side::Dividend, 0.0, 0.0)
        };
        let c = closes(
            "600519",
            &[
                ("2026-01-05", 10.0),
                ("2026-01-06", 11.0),
                ("2026-01-07", 11.5),
            ],
        );
        let v = compute(&[buy, sell, dividend], &c, vec![]);
        assert_eq!(
            (
                v["invested"].clone(),
                v["withdrawn"].clone(),
                v["final_value"].clone()
            ),
            (json!(1005.0), json!(1110.0), json!(0.0))
        );
        assert_eq!(
            v["pnl"],
            json!(105.0),
            "卖出 1100 − 10 + 分红 20 − 买入 1005"
        );
        assert_eq!(v["symbols"][0]["shares"], json!(0.0));
    }

    #[test]
    fn price_cliffs_without_a_split_record_are_warned_and_missing_symbols_stay_in_the_gaps() {
        let flows = vec![
            flow("600519", "2026-01-05", Side::Buy, 100.0, 10.0),
            flow("000001", "2026-01-05", Side::Buy, 100.0, 10.0),
        ];
        let c = closes(
            "600519",
            &[
                ("2026-01-05", 10.0),
                ("2026-01-06", 5.0),
                ("2026-01-07", 5.1),
            ],
        );
        let v = compute(
            &flows,
            &c,
            vec![json!({"symbol": "000001", "why": "取不到价格"})],
        );
        assert_eq!(v["ok"], true);
        assert!(v["warnings"].as_array().unwrap().iter().any(|w| w
            .as_str()
            .unwrap()
            .contains("600519 在 2026-01-06")
            && w.as_str().unwrap().contains("送转")));
        assert_eq!(v["gaps"][0]["symbol"], "000001");
        assert!(
            v["symbols"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s["symbol"] != "000001"),
            "没价格的整只不算"
        );
        let none = compute(
            &flows,
            &HashMap::new(),
            vec![json!({"symbol": "x", "why": "y"})],
        );
        assert_eq!(none["ok"], false);
        assert!(none["reason"].as_str().unwrap().contains("取不到价格"));
    }

    #[test]
    fn beijing_prices_contribute_to_returns_and_ordinary_drops_are_not_split_warnings() {
        let f = [flow("920575", "2026-01-05", Side::Buy, 100.0, 10.0)];
        let v = compute(
            &f,
            &closes("920575", &[("2026-01-05", 10.0), ("2026-01-06", 7.0)]),
            vec![],
        );
        assert_eq!(v["twr_pct"], -30.0);
        assert_eq!(v["pnl"], -300.0);
        assert!(v["warnings"].as_array().unwrap().is_empty());
        let cliff = compute(
            &f,
            &closes("920575", &[("2026-01-05", 10.0), ("2026-01-06", 5.0)]),
            vec![],
        );
        assert_eq!(cliff["warnings"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn carried_prices_are_disclosed_only_for_still_held_positions() {
        let mut c = closes("920575", &[("2026-01-05", 10.0), ("2026-01-06", 11.0)]);
        c.extend(closes(
            "600519",
            &[("2026-01-05", 10.0), ("2026-01-07", 10.0)],
        ));
        let mut f = vec![
            flow("920575", "2026-01-05", Side::Buy, 100.0, 10.0),
            flow("600519", "2026-01-05", Side::Buy, 100.0, 10.0),
        ];
        let v = compute(&f, &c, vec![]);
        assert_eq!(v["final_value"], 2100.0);
        assert!(v["warnings"][0]
            .as_str()
            .unwrap()
            .contains("最后价格为 2026-01-06"));
        f.push(flow("920575", "2026-01-06", Side::Sell, 100.0, 11.0));
        assert!(compute(&f, &c, vec![])["warnings"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn provider_rows_require_identity_valid_dates_and_no_silent_drops() {
        let em = json!({"data":{"code":"920575","market":0,"klines":["2026-01-05,10,10,11,9,100,10000", "2026-01-06,7,7,8,6,100,7000"]}});
        let parsed = closes_from_response(&em, "920575", "2026-01-05", "2026-01-06").unwrap();
        assert_eq!(parsed["2026-01-06"], 7.0);
        let mut wrong = em.clone();
        wrong["data"]["code"] = json!("600519");
        assert!(closes_from_response(&wrong, "920575", "2026-01-05", "2026-01-06").is_err());
        let mut bad = em.clone();
        bad["data"]["klines"][1] = json!("2026-01-06,7,7,6,8,100,7000");
        assert!(closes_from_response(&bad, "920575", "2026-01-05", "2026-01-06").is_err());
        let mut date = em.clone();
        date["data"]["klines"][1] = json!("2026-02-30,7,7,8,6,100,7000");
        assert!(closes_from_response(&date, "920575", "2026-01-05", "2026-03-01").is_err());
        let tx = json!({"code":0,"data":{"sh600519":{"day":[["2026-01-05","10","10","11","9","100"],["2026-01-05","10","10","11","9","100"]]}}});
        assert!(closes_from_response(&tx, "600519", "2026-01-05", "2026-01-06").is_err());
        assert!(closes_from_response(&em, "920575", "2025-01-01", "2025-01-02").is_err());
        assert!(
            closes_from_response(&em, "920575", "2026-01-04", "2026-01-06")
                .unwrap_err()
                .to_string()
                .contains("第一笔买入日")
        );
    }

    #[tokio::test]
    #[ignore = "requires public Eastmoney HTTPS"]
    async fn live_beijing_performance_prices() {
        let c = fetch_closes(&Client::new(), "920575", "2026-10-08", "2026-10-16")
            .await
            .unwrap();
        assert_eq!(c.keys().next().map(String::as_str), Some("2026-10-08"));
        assert_eq!(c.keys().next_back().map(String::as_str), Some("2026-10-16"));
        assert!(c.len() >= 5);
    }

    #[test]
    fn a_year_or_more_gives_annualised_return_and_the_series_is_thinned() {
        let mut rows: Vec<(String, f64)> = Vec::new();
        let start = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        for i in 0..420 {
            let d = start + chrono::Duration::days(i);
            rows.push((
                d.format("%Y-%m-%d").to_string(),
                10.0 * (1.0 + 0.1 * i as f64 / 365.0),
            ));
        }
        let borrowed: Vec<(&str, f64)> = rows.iter().map(|(d, c)| (d.as_str(), *c)).collect();
        let v = compute(
            &[flow("600519", "2025-01-01", Side::Buy, 100.0, 10.0)],
            &closes("600519", &borrowed),
            vec![],
        );
        let irr = v["xirr_pct"].as_f64().unwrap();
        assert!(irr > 9.0 && irr < 11.5, "线性涨 10%/年,年化约 10%:{irr}");
        assert!(v["series"].as_array().unwrap().len() <= 151);
        assert_eq!(
            v["series"].as_array().unwrap().last().unwrap()["date"],
            v["to"]
        );
    }

    /// 合成数据夹具(不是任何真实持仓):VIBE_PERF_FIXTURE=路径 cargo test portfolio_perf::tests::dump_synthetic_fixture -- --ignored
    #[test]
    #[ignore]
    fn dump_synthetic_fixture() {
        let Ok(out) = std::env::var("VIBE_PERF_FIXTURE") else {
            return;
        };
        let start = NaiveDate::from_ymd_opt(2025, 3, 3).unwrap();
        let mut a: Vec<(String, f64)> = Vec::new();
        let mut b: Vec<(String, f64)> = Vec::new();
        let mut d = start;
        let mut i = 0;
        while a.len() < 300 {
            if d.format("%u").to_string().parse::<u32>().unwrap() <= 5 {
                a.push((
                    d.format("%Y-%m-%d").to_string(),
                    100.0 * (1.0 + 0.0012 * i as f64 + if i % 3 == 0 { 0.012 } else { -0.006 }),
                ));
                b.push((
                    d.format("%Y-%m-%d").to_string(),
                    20.0 * (1.0 - 0.0003 * i as f64 + if i % 2 == 0 { 0.02 } else { -0.01 }),
                ));
                i += 1;
            }
            d += chrono::Duration::days(1);
        }
        let ar: Vec<(&str, f64)> = a.iter().map(|(d, c)| (d.as_str(), *c)).collect();
        let br: Vec<(&str, f64)> = b.iter().map(|(d, c)| (d.as_str(), *c)).collect();
        let mut c = closes("600519", &ar);
        c.extend(closes("000001", &br));
        let flows = vec![
            Flow {
                name: "样本白酒".into(),
                ..flow("600519", &a[0].0, Side::Buy, 100.0, a[0].1)
            },
            Flow {
                name: "样本银行".into(),
                ..flow("000001", &b[10].0, Side::Buy, 1000.0, b[10].1)
            },
            Flow {
                name: "样本白酒".into(),
                ..flow("600519", &a[120].0, Side::Buy, 50.0, a[120].1)
            },
            Flow {
                name: "样本银行".into(),
                ..flow("000001", &b[200].0, Side::Sell, 400.0, b[200].1)
            },
            Flow {
                name: "样本白酒".into(),
                amount: 300.0,
                ..flow("600519", &a[150].0, Side::Dividend, 0.0, 0.0)
            },
        ];
        let mut flows = flows;
        let bj: Vec<(&str, f64)> = a
            .iter()
            .enumerate()
            .map(|(i, (d, _))| (d.as_str(), 10.0 * (1.0 + i as f64 * 0.0003)))
            .collect();
        c.extend(closes("920575", &bj));
        flows.push(Flow {
            name: "样本北交所".into(),
            ..flow("920575", &a[0].0, Side::Buy, 100.0, 10.0)
        });
        let v = compute(
            &flows,
            &c,
            vec![json!({"symbol": "688001", "why": "行情源未返回价格,这只没算"})],
        );
        std::fs::write(out, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }
}
