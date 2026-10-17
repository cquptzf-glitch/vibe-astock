//! 我的持仓 · 历史回放(移动版;口径对照 backtest/scenario_replay.py,只做 A 股):按**现在的持仓比例**,把几段历史大跌期重放一遍:
//! 组合涨跌、期间最大回撤、最差单日,和同期沪深 300 并排,再列每只的涨跌和贡献。
//!
//! 🔴 这只是「按现在的比例回看那段行情」,不是当时的真实持仓,不预测下一次,不给任何操作提示。
//! 🔴 当时还没上市、那段时间没有价格的进缺口,**不当 0**,权重只在有价格的几只之间重新分;
//!    有价格的不到这一组市值的一半时**不给组合数字**,只列各只的涨跌 —— 样本不够就不下结论。
//! 🔴 价格是前复权日线(东财为主、腾讯兜底;分红送转不会被当成下跌),东财最多回到约 2015 年,更早的时期写「取不到」。
//!    ETF、可转债这一版取不到这么早的价格,进缺口。窗口 = 沪深 300 在这段时间的收盘最高点到最低点(与桌面版同一组预设)。
//! 🔴 个人持仓只在本机,不进任何 AI。

use crate::{chart, discovery::DiscoveryError, journal_risk, journal_settings, symbol::normalize_stock_symbol};
use futures_util::{stream, StreamExt};
use reqwest::Client;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const MAX_POSITIONS: usize = 60;
const HISTORY_BARS: usize = 4000;
/// 窗口起止附近最多允许差这么多个日历天(长假、停牌)
const EDGE_SLACK_DAYS: i64 = 7;
const MIN_COVERED: f64 = 0.5;

pub(crate) const SCENARIOS: [(&str, &str, &str, &str); 5] = [
    ("cn_2015", "2015 年 6–8 月大跌", "2015-06-08", "2015-08-26"),
    ("cn_2016", "2016 年初熔断", "2015-12-22", "2016-01-28"),
    ("cn_2018", "2018 年下跌", "2018-01-24", "2019-01-03"),
    ("cn_2020", "2020 年 3 月全球疫情冲击", "2020-03-05", "2020-03-23"),
    ("cn_2021_24", "2021–2024 年长期回撤", "2021-02-10", "2024-02-02"),
];

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[derive(Clone, Debug)]
pub(crate) struct Pos {
    pub symbol: String,
    pub name: String,
    /// 现在的市值(权重只用它的比例)
    pub value: f64,
    pub closes: BTreeMap<String, f64>,
}

fn days_between(a: &str, b: &str) -> i64 {
    match (chrono::NaiveDate::parse_from_str(a, "%Y-%m-%d"), chrono::NaiveDate::parse_from_str(b, "%Y-%m-%d")) {
        (Ok(a), Ok(b)) => (b - a).num_days(),
        _ => 0,
    }
}

/// 窗口两端的价格:起点取 ≥ start 的第一根(要在 slack 天内),终点取 ≤ end 的最后一根(同样)。取不到返回原因
fn edge_prices(closes: &BTreeMap<String, f64>, start: &str, end: &str) -> Result<(f64, f64), String> {
    let first = closes.range(start.to_string()..=end.to_string()).next();
    let last = closes.range(start.to_string()..=end.to_string()).next_back();
    match (first, last) {
        (Some((d0, c0)), Some((d1, c1))) if days_between(start, d0) <= EDGE_SLACK_DAYS && days_between(d1, end) <= EDGE_SLACK_DAYS && d0 != d1 => Ok((*c0, *c1)),
        (None, _) | (_, None) => Err("这段时间没有价格(当时可能还没上市,或数据源没有这么早的日线(东财取不到时腾讯只给最近约三年))".into()),
        (Some((d0, _)), Some((d1, _))) if d0 == d1 => Err("这段时间只有一天价格".into()),
        (Some((d0, _)), Some((d1, _))) => Err(format!("第一根价格 {d0}、最后一根 {d1} 离窗口 {start} ~ {end} 太远(上市晚于起点或中途停牌到结束)")),
    }
}

fn max_drawdown(returns: &[f64]) -> f64 {
    let (mut level, mut peak, mut worst) = (1.0_f64, 1.0_f64, 0.0_f64);
    for r in returns {
        level *= 1.0 + r;
        peak = peak.max(level);
        worst = worst.min(level / peak - 1.0);
    }
    worst
}

fn daily_returns(closes: &BTreeMap<String, f64>, start: &str, end: &str, dates: Option<&BTreeSet<&str>>) -> Vec<(String, f64)> {
    let mut prev: Option<f64> = None;
    let mut out = Vec::new();
    for (d, c) in closes.range(start.to_string()..=end.to_string()).filter(|(d, _)| dates.map_or(true, |s| s.contains(d.as_str()))) {
        if let Some(p) = prev {
            out.push((d.clone(), c / p - 1.0));
        }
        prev = Some(*c);
    }
    out
}

/// 一段时期:组合 / 指数 / 每只
pub(crate) fn scenario_view(id: &str, label: &str, start: &str, end: &str, positions: &[Pos], index: &BTreeMap<String, f64>) -> Value {
    let total: f64 = positions.iter().map(|p| p.value).sum();
    let mut covered: Vec<(&Pos, f64, f64)> = Vec::new(); // (pos, 区间涨跌, 现在的权重)
    let mut gaps: Vec<Value> = Vec::new();
    for p in positions {
        match edge_prices(&p.closes, start, end) {
            Ok((c0, c1)) => covered.push((p, c1 / c0 - 1.0, p.value / total)),
            Err(why) => gaps.push(json!({"symbol": p.symbol, "why": why})),
        }
    }
    let covered_share: f64 = covered.iter().map(|c| c.2).sum();
    let index_view = edge_prices(index, start, end).ok().map(|(c0, c1)| {
        let rets = daily_returns(index, start, end, None);
        let values: Vec<f64> = rets.iter().map(|r| r.1).collect();
        json!({"return_pct": r2((c1 / c0 - 1.0) * 100.0), "max_drawdown_pct": r2(max_drawdown(&values) * 100.0), "worst_day_pct": r2(values.iter().cloned().fold(f64::MAX, f64::min) * 100.0), "label": "沪深 300"})
    });
    let mut stocks: Vec<Value> = covered.iter().map(|(p, ret, w)| {
        let renorm = w / covered_share;
        json!({"symbol": p.symbol, "name": p.name, "weight_pct": r2(renorm * 100.0), "return_pct": r2(ret * 100.0), "contribution_pct": r2(renorm * ret * 100.0)})
    }).collect();
    stocks.sort_by(|a, b| a["contribution_pct"].as_f64().partial_cmp(&b["contribution_pct"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
    let mut view = json!({"id": id, "label": label, "start": start, "end": end, "covered_pct": r2(covered_share * 100.0), "stocks": stocks, "gaps": gaps, "index": index_view, "portfolio": Value::Null, "portfolio_reason": Value::Null});
    if covered.is_empty() || covered_share < MIN_COVERED {
        view["portfolio_reason"] = json!(format!("这段时间有价格的只占现在持仓市值的 {:.0}%,不到一半,不给组合数字(样本不够就不下结论),只列各只的涨跌", covered_share * 100.0));
        return view;
    }
    // 组合日收益:只用所有有价格的票都有价格的交易日(相邻两天),权重按有价格的几只重新分
    let mut common: Option<BTreeSet<&str>> = None;
    for (p, _, _) in &covered {
        let set: BTreeSet<&str> = p.closes.range(start.to_string()..=end.to_string()).map(|(d, _)| d.as_str()).collect();
        common = Some(match common { None => set, Some(c) => c.intersection(&set).copied().collect() });
    }
    let common = common.unwrap_or_default();
    let series: Vec<Vec<(String, f64)>> = covered.iter().map(|(p, _, _)| daily_returns(&p.closes, start, end, Some(&common))).collect();
    let n = series.iter().map(|s| s.len()).min().unwrap_or(0);
    let port: Vec<f64> = (0..n).map(|t| covered.iter().zip(&series).map(|((_, _, w), s)| w / covered_share * s[t].1).sum()).collect();
    let total_return: f64 = covered.iter().map(|(_, ret, w)| w / covered_share * ret).sum();
    view["portfolio"] = json!({
        "return_pct": r2(total_return * 100.0), "max_drawdown_pct": r2(max_drawdown(&port) * 100.0),
        "worst_day_pct": port.iter().cloned().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |x| x.min(v)))).map(|v| r2(v * 100.0)), "days": n,
    });
    view
}

pub(crate) fn compute(positions: &[Pos], index: &BTreeMap<String, f64>, gaps: Vec<Value>) -> Value {
    json!({
        "ok": true, "gaps": gaps, "index_available": !index.is_empty(),
        "scenarios": SCENARIOS.iter().map(|(id, label, start, end)| scenario_view(id, label, start, end, positions, index)).collect::<Vec<_>>(),
        "note": "按现在的持仓比例回看历史大跌期:区间涨跌 = 期末收盘 ÷ 期初收盘 − 1;最大回撤、最差单日用有价格的几只的共同交易日算,权重在有价格的几只之间重新分;当时没价格的进缺口、不当 0;有价格的不到一半市值就不给组合数字。窗口是沪深 300 在这段时间的收盘最高点到最低点。价格是前复权日线。只是回看,不是当时的真实持仓,不预测下一次,不给操作提示。",
    })
}

pub async fn view(client: &Client, db: &Path) -> std::result::Result<Value, DiscoveryError> {
    let settings = journal_settings::load(db).map_err(DiscoveryError::Upstream)?;
    let rows = journal_risk::rows_of(journal_risk::load_entries(db).map_err(DiscoveryError::Upstream)?, &settings.fees);
    let mut held: BTreeMap<String, (String, f64)> = BTreeMap::new();
    for r in rows.iter().filter(|r| r.e.kind == "episode" && !r.closed && r.open_shares > 1e-8) {
        let e = held.entry(r.e.symbol.clone()).or_insert_with(|| (r.e.name.clone(), 0.0));
        e.1 += r.open_shares;
        if e.0.is_empty() {
            e.0 = r.e.name.clone();
        }
    }
    if held.is_empty() {
        return Ok(json!({"ok": false, "reason": "现在没有在手持仓(交易日志里没有未清仓的交易)", "gaps": []}));
    }
    if held.len() > MAX_POSITIONS {
        return Ok(json!({"ok": false, "reason": format!("一次最多回放 {MAX_POSITIONS} 只,现在有 {} 只", held.len()), "gaps": []}));
    }
    let index_task = chart::index_closes(client, "1.000300", HISTORY_BARS);
    let fetched = stream::iter(held)
        .map(|(symbol, (name, shares))| {
            let client = client.clone();
            async move {
                let result = match normalize_stock_symbol(&symbol) {
                    Ok(key) => chart::day_closes(&client, &key, true, HISTORY_BARS).await.map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                };
                (symbol, name, shares, result)
            }
        })
        .buffer_unordered(4)
        .collect::<Vec<_>>();
    let (index_result, fetched) = futures_util::future::join(index_task, fetched).await;
    let index: BTreeMap<String, f64> = index_result.map(|v| v.into_iter().collect()).unwrap_or_default();
    let (mut positions, mut gaps): (Vec<Pos>, Vec<Value>) = (Vec::new(), Vec::new());
    for (symbol, name, shares, result) in fetched {
        match result {
            Ok(closes) if !closes.is_empty() => {
                let latest = closes.last().map(|c| c.1).unwrap_or(0.0);
                positions.push(Pos { symbol, name, value: shares * latest, closes: closes.into_iter().collect() });
            }
            Ok(_) => gaps.push(json!({"symbol": symbol, "why": "没有取到日线"})),
            Err(e) => gaps.push(json!({"symbol": symbol, "why": format!("取不到日线:{e}")})),
        }
    }
    gaps.sort_by(|a, b| a["symbol"].as_str().cmp(&b["symbol"].as_str()));
    if positions.is_empty() {
        return Ok(json!({"ok": false, "reason": "所有在手持仓都取不到日线", "gaps": gaps}));
    }
    Ok(compute(&positions, &index, gaps))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(rows: &[(&str, f64)]) -> BTreeMap<String, f64> {
        rows.iter().map(|(d, c)| (d.to_string(), *c)).collect()
    }

    fn pos(symbol: &str, value: f64, rows: &[(&str, f64)]) -> Pos {
        Pos { symbol: symbol.into(), name: format!("N{symbol}"), value, closes: series(rows) }
    }

    const START: &str = "2020-03-05";
    const END: &str = "2020-03-23";

    fn window(f: impl Fn(usize) -> f64) -> Vec<(String, f64)> {
        // 2020-03-05 … 2020-03-23 的工作日
        let mut rows = Vec::new();
        let mut d = chrono::NaiveDate::from_ymd_opt(2020, 3, 5).unwrap();
        let mut i = 0;
        while d <= chrono::NaiveDate::from_ymd_opt(2020, 3, 23).unwrap() {
            if d.format("%u").to_string() != "6" && d.format("%u").to_string() != "7" {
                rows.push((d.format("%Y-%m-%d").to_string(), f(i)));
                i += 1;
            }
            d += chrono::Duration::days(1);
        }
        rows
    }

    fn borrowed(rows: &[(String, f64)]) -> Vec<(&str, f64)> {
        rows.iter().map(|(d, c)| (d.as_str(), *c)).collect()
    }

    #[test]
    fn edge_prices_need_bars_near_both_ends_and_say_why_when_missing() {
        let ok = series(&[("2020-03-05", 10.0), ("2020-03-13", 9.0), ("2020-03-23", 8.0)]);
        assert_eq!(edge_prices(&ok, START, END), Ok((10.0, 8.0)));
        let late_listing = series(&[("2020-03-18", 10.0), ("2020-03-23", 8.0)]);
        assert!(edge_prices(&late_listing, START, END).unwrap_err().contains("离窗口"), "上市晚于起点超过 7 天不凑");
        assert!(edge_prices(&series(&[("2019-01-02", 10.0)]), START, END).unwrap_err().contains("没有价格"));
        assert!(edge_prices(&series(&[("2020-03-10", 10.0)]), START, END).unwrap_err().contains("只有一天"));
        let suspended = series(&[("2020-03-05", 10.0), ("2020-03-06", 9.0)]);
        assert!(edge_prices(&suspended, START, END).unwrap_err().contains("离窗口"), "到结束还在停牌的不算");
    }

    #[test]
    fn portfolio_return_is_the_weighted_mean_of_returns_with_weights_renormalised_over_covered_stocks() {
        let a = window(|i| 10.0 - 0.1 * i as f64); // 下跌
        let b = window(|i| 20.0 + 0.2 * i as f64); // 上涨
        let ra = pos("600519", 6000.0, &borrowed(&a));
        let rb = pos("000001", 4000.0, &borrowed(&b));
        let ret_a = a.last().unwrap().1 / a[0].1 - 1.0;
        let ret_b = b.last().unwrap().1 / b[0].1 - 1.0;
        let v = scenario_view("cn_2020", "x", START, END, &[ra, rb], &BTreeMap::new());
        let want = (0.6 * ret_a + 0.4 * ret_b) * 100.0;
        assert!((v["portfolio"]["return_pct"].as_f64().unwrap() - want).abs() < 0.01, "{v}");
        assert_eq!(v["covered_pct"], json!(100.0));
        let stocks = v["stocks"].as_array().unwrap();
        assert_eq!(stocks[0]["symbol"], "600519", "贡献最差的在前");
        assert!(v["portfolio"]["max_drawdown_pct"].as_f64().unwrap() <= 0.0);
        assert!(v["index"].is_null(), "没有指数数据就不编");
    }

    #[test]
    fn missing_history_goes_to_gaps_and_less_than_half_coverage_gives_no_portfolio_number() {
        let a = window(|i| 10.0 - 0.1 * i as f64);
        let covered = pos("600519", 3000.0, &borrowed(&a));
        let newer = pos("688001", 7000.0, &[("2024-01-02", 30.0), ("2024-02-01", 31.0)]);
        let v = scenario_view("cn_2020", "x", START, END, &[covered.clone(), newer.clone()], &BTreeMap::new());
        assert!(v["portfolio"].is_null());
        assert!(v["portfolio_reason"].as_str().unwrap().contains("不到一半"), "{v}");
        assert_eq!(v["covered_pct"], json!(30.0));
        assert_eq!(v["gaps"][0]["symbol"], "688001");
        assert_eq!(v["stocks"].as_array().unwrap().len(), 1, "有价格的照样列涨跌");
        // 有价格的占 ≥ 一半就给,权重在有价格的几只之间重分(这里只剩一只 = 100%)
        let heavy = pos("600519", 6000.0, &borrowed(&a));
        let v2 = scenario_view("cn_2020", "x", START, END, &[heavy, pos("688001", 4000.0, &[("2024-01-02", 30.0), ("2024-02-01", 31.0)])], &BTreeMap::new());
        assert_eq!(v2["portfolio"]["return_pct"], v2["stocks"][0]["return_pct"]);
        assert_eq!(v2["stocks"][0]["weight_pct"], json!(100.0));
    }

    #[test]
    fn index_comparison_uses_the_same_window_and_all_five_scenarios_are_listed() {
        let a = window(|i| 10.0 - 0.1 * i as f64);
        let idx = window(|i| 4000.0 * (1.0 - 0.01 * i as f64));
        let v = compute(&[pos("600519", 1000.0, &borrowed(&a))], &series(&borrowed(&idx)), vec![]);
        assert_eq!(v["scenarios"].as_array().unwrap().len(), 5);
        let cn2020 = v["scenarios"].as_array().unwrap().iter().find(|s| s["id"] == "cn_2020").unwrap();
        let idx_ret = (idx.last().unwrap().1 / idx[0].1 - 1.0) * 100.0;
        assert!((cn2020["index"]["return_pct"].as_f64().unwrap() - idx_ret).abs() < 0.01);
        assert_eq!(cn2020["index"]["label"], "沪深 300");
        let cn2015 = v["scenarios"].as_array().unwrap().iter().find(|s| s["id"] == "cn_2015").unwrap();
        assert!(cn2015["portfolio"].is_null() && cn2015["index"].is_null(), "没有那段价格的时期不编数字");
        assert!(v["note"].as_str().unwrap().contains("不是当时的真实持仓"));
    }

    /// 合成数据夹具(不是任何真实持仓或行情):VIBE_REPLAY_FIXTURE=路径 cargo test portfolio_replay::tests::dump_synthetic_fixture -- --ignored
    #[test]
    #[ignore]
    fn dump_synthetic_fixture() {
        let Ok(out) = std::env::var("VIBE_REPLAY_FIXTURE") else { return };
        let build = |from: &str, base: f64, amp: f64, drift: f64| -> BTreeMap<String, f64> {
            let mut d = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d").unwrap();
            let end = chrono::NaiveDate::from_ymd_opt(2025, 1, 31).unwrap();
            let (mut i, mut map) = (0.0_f64, BTreeMap::new());
            while d <= end {
                if !matches!(d.format("%u").to_string().as_str(), "6" | "7") {
                    map.insert(d.format("%Y-%m-%d").to_string(), base * (1.0 + drift * (i / 250.0).sin() + amp * if (i as i64) % 3 == 0 { 1.0 } else { -0.5 }));
                    i += 1.0;
                }
                d += chrono::Duration::days(1);
            }
            map
        };
        let mut a = pos("600519", 6000.0, &[]);
        a.name = "样本白酒".into();
        a.closes = build("2014-01-02", 100.0, 0.01, 0.25);
        let mut b = pos("000001", 3000.0, &[]);
        b.name = "样本银行".into();
        b.closes = build("2014-01-02", 10.0, 0.006, 0.2);
        let mut c = pos("688001", 1000.0, &[]);
        c.name = "样本新股".into();
        c.closes = build("2022-06-01", 30.0, 0.02, 0.3);
        let index = build("2014-01-02", 4000.0, 0.004, 0.18);
        let v = compute(&[a, b, c], &index, vec![json!({"symbol": "510300", "why": "取不到日线:ETF 这一版取不到这么早的价格"})]);
        std::fs::write(out, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }
}
