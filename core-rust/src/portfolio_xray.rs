//! 我的持仓 · 持仓体检(移动版;口径对照 backtest/portfolio_xray.py 与 risk_xray.py,只做人民币 A 股与 ETF):
//! 集中度(单只、前三、HHI、等效只数、行业)、按现在的比例回看最近一年的波动 / 最大回撤 / 历史 VaR / 下行波动、
//! 每只的风险贡献、两两相关性。
//!
//! 🔴 只给数字和缺口,**不出综合分**、不评好坏;「超线」只对照用户自己在页面上设的提醒线(存在本机浏览器里)。
//! 🔴 权重 = 现在持有数量 × 最新收盘价的市值占比;组合收益 = 按「现在的比例」回看(权重固定),不是当时真实的持仓。
//! 🔴 价格:前复权日线(东财为主、腾讯兜底),最近约一年;对齐后共同交易日不足 60 根时,把历史最短的那只移进缺口再算,
//!    不用别的票去凑,不拿 0 补。北交所东财日线可能取不到,取不到的进缺口。
//! 🔴 行业取东财「所属行业」,取不到的单列一类「行业没取到」,不归进任何行业。
//! 🔴 个人持仓与市值只在本机,不进任何 AI。

use crate::{
    chart, discovery::DiscoveryError, journal_risk, journal_settings, research,
    symbol::normalize_stock_symbol,
};
use futures_util::{stream, StreamExt};
use reqwest::Client;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
};

const MIN_BARS: usize = 60;
const WINDOW_BARS: usize = 260;
const TRADING_DAYS: f64 = 252.0;
const MAX_POSITIONS: usize = 60;
const NO_INDUSTRY: &str = "行业没取到";

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
fn r4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

#[derive(Clone, Debug)]
pub(crate) struct Pos {
    pub symbol: String,
    pub name: String,
    pub shares: f64,
    pub industry: Option<String>,
    /// 前复权日收盘价,按日期升序;最后一根 = 最新价
    pub closes: Vec<(String, f64)>,
}

impl Pos {
    fn value(&self) -> f64 {
        self.shares * self.closes.last().map(|c| c.1).unwrap_or(0.0)
    }
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn cov(a: &[f64], b: &[f64]) -> f64 {
    let (ma, mb) = (mean(a), mean(b));
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - ma) * (y - mb))
        .sum::<f64>()
        / (a.len() as f64 - 1.0)
}

/// 历史 VaR:最差 (1−level) 分位的日收益,取正数表示亏多少(没有这么多样本时给 None)
fn var_cvar(returns: &[f64], level: f64) -> Option<(f64, f64)> {
    if returns.len() < 30 {
        return None;
    }
    let mut sorted = returns.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let k = (((1.0 - level) * sorted.len() as f64).floor() as usize).max(1);
    let tail = &sorted[..k];
    Some((-sorted[k - 1], -mean(tail)))
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

/// 纯计算。positions 里每只都已有日线;gaps 是调用方已经列出的缺口
pub(crate) fn compute(positions: &[Pos], mut gaps: Vec<Value>) -> Value {
    let mut keep: Vec<&Pos> = positions
        .iter()
        .filter(|p| p.shares > 0.0 && p.closes.last().is_some_and(|c| c.1 > 0.0))
        .collect();
    if keep.is_empty() {
        return json!({"ok": false, "reason": "没有能取到价格的在手持仓", "gaps": gaps});
    }
    // 对齐:共同交易日不足时,把历史最短的移进缺口再试
    let (dates, aligned) = loop {
        let mut common: Option<BTreeSet<&str>> = None;
        for p in &keep {
            let set: BTreeSet<&str> = p
                .closes
                .iter()
                .rev()
                .take(WINDOW_BARS)
                .map(|c| c.0.as_str())
                .collect();
            common = Some(match common {
                None => set,
                Some(c) => c.intersection(&set).copied().collect(),
            });
        }
        let dates: Vec<String> = common
            .unwrap_or_default()
            .into_iter()
            .map(String::from)
            .collect();
        if dates.len() > MIN_BARS || keep.len() < 2 {
            break (dates.clone(), keep.clone());
        }
        let shortest = keep
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| p.closes.len())
            .map(|(i, _)| i)
            .unwrap();
        let out = keep.remove(shortest);
        gaps.push(json!({"symbol": out.symbol, "why": format!("日线历史太短({} 根),和其它持仓对齐后不足 {MIN_BARS} 个共同交易日,这只没参与风险计算", out.closes.len())}));
    };
    let total: f64 = aligned.iter().map(|p| p.value()).sum();
    let weights: Vec<f64> = aligned.iter().map(|p| p.value() / total).collect();
    let mut by_weight: Vec<(&Pos, f64)> = aligned
        .iter()
        .copied()
        .zip(weights.iter().copied())
        .collect();
    by_weight.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let hhi: f64 = weights.iter().map(|w| w * w).sum();
    let mut industries: BTreeMap<String, f64> = BTreeMap::new();
    for (p, w) in &by_weight {
        *industries
            .entry(
                p.industry
                    .clone()
                    .unwrap_or_else(|| NO_INDUSTRY.to_string()),
            )
            .or_default() += w;
    }
    let mut industry_rows: Vec<(String, f64)> = industries.into_iter().collect();
    industry_rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let concentration = json!({
        "positions": aligned.len(), "total_value": r2(total), "top1_pct": r2(by_weight[0].1 * 100.0),
        "top3_pct": r2(by_weight.iter().take(3).map(|x| x.1).sum::<f64>() * 100.0), "hhi": r4(hhi), "effective_n": r2(1.0 / hhi),
        "top_industry_pct": industry_rows.iter().find(|r| r.0 != NO_INDUSTRY).map(|r| r2(r.1 * 100.0)),
        "industries": industry_rows.iter().map(|(n, w)| json!({"industry": n, "weight_pct": r2(w * 100.0), "known": n != NO_INDUSTRY})).collect::<Vec<_>>(),
        "weights": by_weight.iter().map(|(p, w)| json!({"symbol": p.symbol, "name": p.name, "value": r2(p.value()), "weight_pct": r2(w * 100.0), "industry": p.industry})).collect::<Vec<_>>(),
    });
    if dates.len() <= MIN_BARS {
        return json!({"ok": true, "concentration": concentration, "risk": null,
            "risk_reason": format!("共同交易日只有 {} 个,不足 {MIN_BARS} 个,波动、回撤、相关性算不了(集中度不受影响)", dates.len().saturating_sub(1)), "gaps": gaps, "note": NOTE});
    }
    // 日收益(按共同交易日,相邻两天;共同交易日之间若隔了没记录的天,按间隔收益计)
    let series: Vec<Vec<f64>> = aligned
        .iter()
        .map(|p| {
            let map: HashMap<&str, f64> = p.closes.iter().map(|c| (c.0.as_str(), c.1)).collect();
            dates
                .windows(2)
                .map(|w| map[w[1].as_str()] / map[w[0].as_str()] - 1.0)
                .collect()
        })
        .collect();
    let n = dates.len() - 1;
    let k = aligned.len();
    let portfolio: Vec<f64> = (0..n)
        .map(|t| (0..k).map(|i| weights[i] * series[i][t]).sum())
        .collect();
    let covm: Vec<Vec<f64>> = (0..k)
        .map(|i| (0..k).map(|j| cov(&series[i], &series[j])).collect())
        .collect();
    let var_p: f64 = (0..k)
        .map(|i| {
            (0..k)
                .map(|j| weights[i] * weights[j] * covm[i][j])
                .sum::<f64>()
        })
        .sum();
    let ann = TRADING_DAYS.sqrt();
    let vol = var_p.max(0.0).sqrt() * ann;
    let asset_vol: Vec<f64> = (0..k).map(|i| covm[i][i].max(0.0).sqrt() * ann).collect();
    let contrib: Vec<f64> = (0..k)
        .map(|i| {
            if var_p > 0.0 {
                weights[i] * (0..k).map(|j| covm[i][j] * weights[j]).sum::<f64>() / var_p
            } else {
                0.0
            }
        })
        .collect();
    let corr = |i: usize, j: usize| {
        let d = (covm[i][i] * covm[j][j]).sqrt();
        if d > 0.0 {
            Some(covm[i][j] / d)
        } else {
            None
        }
    };
    let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
    for i in 0..k {
        for j in i + 1..k {
            if let Some(c) = corr(i, j) {
                pairs.push((i, j, c));
            }
        }
    }
    pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let avg_corr = if pairs.is_empty() {
        None
    } else {
        Some(mean(&pairs.iter().map(|p| p.2).collect::<Vec<_>>()))
    };
    let weighted_vol: f64 = (0..k).map(|i| weights[i] * asset_vol[i]).sum();
    let downside =
        (portfolio.iter().map(|r| r.min(0.0).powi(2)).sum::<f64>() / n as f64).sqrt() * ann;
    let (best, worst) = (
        portfolio.iter().cloned().fold(f64::MIN, f64::max),
        portfolio.iter().cloned().fold(f64::MAX, f64::min),
    );
    let pct = |v: f64| r2(v * 100.0);
    let risk = json!({
        "window": {"from": dates.first(), "to": dates.last(), "returns": n},
        "annual_vol_pct": pct(vol), "max_drawdown_pct": pct(max_drawdown(&portfolio)), "downside_dev_pct": pct(downside),
        "var95_pct": var_cvar(&portfolio, 0.95).map(|v| pct(v.0)), "cvar95_pct": var_cvar(&portfolio, 0.95).map(|v| pct(v.1)),
        "var99_pct": var_cvar(&portfolio, 0.99).map(|v| pct(v.0)), "best_day_pct": pct(best), "worst_day_pct": pct(worst),
        "avg_pair_corr": avg_corr.map(r2), "diversification_ratio": if vol > 0.0 { Some(r2(weighted_vol / vol)) } else { None },
        "assets": by_weight.iter().map(|(p, _)| {
            let i = aligned.iter().position(|q| q.symbol == p.symbol).unwrap();
            json!({"symbol": p.symbol, "name": p.name, "weight_pct": pct(weights[i]), "vol_pct": pct(asset_vol[i]), "risk_share_pct": pct(contrib[i])})
        }).collect::<Vec<_>>(),
        "top_pairs": pairs.iter().take(5).map(|(i, j, c)| json!({"a": aligned[*i].symbol, "a_name": aligned[*i].name, "b": aligned[*j].symbol, "b_name": aligned[*j].name, "corr": r2(*c)})).collect::<Vec<_>>(),
    });
    json!({"ok": true, "concentration": concentration, "risk": risk, "risk_reason": null, "gaps": gaps, "note": NOTE})
}

const NOTE: &str = "按现在的持仓比例(数量 × 最新收盘价)回看最近约一年的前复权日线:波动、最大回撤、历史 VaR 都是这段历史里已经发生的数字,不是预测;行业取东财所属行业。只给数字和缺口,不出综合分、不评好坏;超线只对照你自己设的提醒线。";

async fn industry_of(client: &Client, symbol: &str) -> Option<String> {
    let v = research::profile(client, &format!("/mobile/research/profile?symbol={symbol}"))
        .await
        .ok()?;
    v["industry"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "-")
        .map(String::from)
}

pub async fn view(client: &Client, db: &Path) -> std::result::Result<Value, DiscoveryError> {
    let settings = journal_settings::load(db).map_err(DiscoveryError::Upstream)?;
    let rows = journal_risk::rows_of(
        journal_risk::load_entries(db).map_err(DiscoveryError::Upstream)?,
        &settings.fees,
    );
    // 现在的持仓:每只票所有未清仓的轮合计
    let mut held: BTreeMap<String, (String, f64)> = BTreeMap::new();
    for r in rows
        .iter()
        .filter(|r| r.e.kind == "episode" && !r.closed && r.open_shares > 1e-8)
    {
        let e = held
            .entry(r.e.symbol.clone())
            .or_insert_with(|| (r.e.name.clone(), 0.0));
        e.1 += r.open_shares;
        if e.0.is_empty() {
            e.0 = r.e.name.clone();
        }
    }
    if held.is_empty() {
        return Ok(
            json!({"ok": false, "reason": "现在没有在手持仓(交易日志里没有未清仓的交易)", "gaps": []}),
        );
    }
    if held.len() > MAX_POSITIONS {
        return Ok(
            json!({"ok": false, "reason": format!("一次最多体检 {MAX_POSITIONS} 只,现在有 {} 只", held.len()), "gaps": []}),
        );
    }
    type PositionFetch = (
        String,
        String,
        f64,
        Option<String>,
        Result<Vec<(String, f64)>, String>,
    );
    let fetched: Vec<PositionFetch> = stream::iter(held)
        .map(|(symbol, (name, shares))| {
            let client = client.clone();
            async move {
                let result = match normalize_stock_symbol(&symbol) {
                    Ok(key) => chart::day_closes(&client, &key, true, WINDOW_BARS + 20)
                        .await
                        .map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                };
                let industry = if result.is_ok() {
                    industry_of(&client, &symbol).await
                } else {
                    None
                };
                (symbol, name, shares, industry, result)
            }
        })
        .buffer_unordered(4)
        .collect()
        .await;
    let (mut positions, mut gaps) = (Vec::new(), Vec::new());
    for (symbol, name, shares, industry, result) in fetched {
        match result {
            Ok(closes) if !closes.is_empty() => positions.push(Pos {
                symbol,
                name,
                shares,
                industry,
                closes,
            }),
            Ok(_) => gaps.push(json!({"symbol": symbol, "why": "没有取到日线"})),
            Err(e) => gaps.push(json!({"symbol": symbol, "why": format!("取不到日线:{e}")})),
        }
    }
    gaps.sort_by(|a, b| a["symbol"].as_str().cmp(&b["symbol"].as_str()));
    Ok(compute(&positions, gaps))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(symbol: &str, shares: f64, industry: Option<&str>, closes: &[f64]) -> Pos {
        let start = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        Pos {
            symbol: symbol.into(),
            name: format!("N{symbol}"),
            shares,
            industry: industry.map(String::from),
            closes: closes
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    (
                        (start + chrono::Duration::days(i as i64))
                            .format("%Y-%m-%d")
                            .to_string(),
                        *c,
                    )
                })
                .collect(),
        }
    }

    fn walk(n: usize, base: f64, step: f64, wiggle: f64) -> Vec<f64> {
        (0..n)
            .map(|i| base * (1.0 + step * i as f64 + if i % 2 == 0 { wiggle } else { -wiggle }))
            .collect()
    }

    #[test]
    fn concentration_uses_market_value_weights_and_keeps_unknown_industry_apart() {
        let a = pos(
            "600519",
            100.0,
            Some("白酒"),
            &walk(120, 100.0, 0.001, 0.01),
        ); // ≈ 100×112
        let b = pos(
            "000001",
            1000.0,
            Some("银行"),
            &walk(120, 10.0, 0.0005, 0.005),
        ); // ≈ 1000×10.6
        let c = pos("300308", 100.0, None, &walk(120, 50.0, 0.002, 0.02));
        let v = compute(&[a, b, c], vec![]);
        assert_eq!(v["ok"], true);
        let con = &v["concentration"];
        assert_eq!(con["positions"], 3);
        let w: Vec<f64> = con["weights"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["weight_pct"].as_f64().unwrap())
            .collect();
        assert!((w.iter().sum::<f64>() - 100.0).abs() < 0.05, "{w:?}");
        assert!(w[0] >= w[1] && w[1] >= w[2], "从大到小");
        assert_eq!(con["top1_pct"].as_f64().unwrap(), w[0]);
        let ind = con["industries"].as_array().unwrap();
        assert!(
            ind.iter()
                .any(|i| i["industry"] == NO_INDUSTRY && i["known"] == false),
            "没取到行业的单列"
        );
        assert!(con["top_industry_pct"].as_f64().unwrap() <= w[0] + 0.01 + 100.0);
        assert!(
            con["effective_n"].as_f64().unwrap() > 1.0
                && con["effective_n"].as_f64().unwrap() <= 3.0
        );
    }

    #[test]
    fn risk_numbers_match_a_two_asset_closed_form() {
        // 两只完全同步的票 → 相关性 1,组合波动 = 单只波动,分散化比 = 1,风险贡献 = 权重
        let base = walk(120, 10.0, 0.001, 0.02);
        let a = pos("600519", 100.0, Some("A"), &base);
        let b = pos("000001", 100.0, Some("B"), &base);
        let v = compute(&[a, b], vec![]);
        let r = &v["risk"];
        assert_eq!(r["avg_pair_corr"], json!(1.0));
        assert_eq!(r["diversification_ratio"], json!(1.0));
        let assets = r["assets"].as_array().unwrap();
        assert!((assets[0]["risk_share_pct"].as_f64().unwrap() - 50.0).abs() < 0.01);
        assert_eq!(assets[0]["vol_pct"], r["annual_vol_pct"]);
        assert!(r["max_drawdown_pct"].as_f64().unwrap() <= 0.0);
        assert!(
            r["var95_pct"].as_f64().unwrap() > 0.0
                && r["cvar95_pct"].as_f64().unwrap() >= r["var95_pct"].as_f64().unwrap()
        );
        assert_eq!(r["window"]["returns"], 119);
    }

    #[test]
    fn opposite_moves_diversify_and_short_history_moves_to_the_gaps() {
        let up_down: Vec<f64> = (0..120)
            .map(|i| 10.0 * if i % 2 == 0 { 1.02 } else { 0.98 })
            .collect();
        let down_up: Vec<f64> = (0..120)
            .map(|i| 10.0 * if i % 2 == 0 { 0.98 } else { 1.02 })
            .collect();
        let v = compute(
            &[
                pos("600519", 100.0, None, &up_down),
                pos("000001", 100.0, None, &down_up),
            ],
            vec![],
        );
        assert_eq!(v["risk"]["avg_pair_corr"], json!(-1.0));
        assert!(
            v["risk"]["annual_vol_pct"].as_f64().unwrap() < 3.0,
            "完全反向、近似等权(最后一天价格不同权重差 2%):组合几乎不波动,单只年化波动约 60%"
        );
        assert!(v["risk"]["diversification_ratio"].as_f64().unwrap() > 5.0);
        // 第三只历史只有 20 根:对齐后不足,被移进缺口,其余照算
        let short = pos("300308", 100.0, None, &walk(20, 50.0, 0.001, 0.01));
        let with_short = compute(
            &[
                pos("600519", 100.0, None, &up_down),
                pos("000001", 100.0, None, &down_up),
                short,
            ],
            vec![],
        );
        assert_eq!(with_short["gaps"][0]["symbol"], "300308");
        assert!(with_short["gaps"][0]["why"]
            .as_str()
            .unwrap()
            .contains("历史太短"));
        assert_eq!(
            with_short["concentration"]["positions"], 2,
            "没参与风险计算的不进集中度"
        );
    }

    #[test]
    fn too_little_common_history_still_gives_concentration_but_no_risk_and_empty_input_is_explained(
    ) {
        let a = pos("600519", 100.0, None, &walk(30, 10.0, 0.001, 0.01));
        let v = compute(&[a], vec![]);
        assert_eq!(v["ok"], true);
        assert!(v["risk"].is_null());
        assert!(v["risk_reason"].as_str().unwrap().contains("不足"));
        assert_eq!(v["concentration"]["positions"], 1);
        let none = compute(&[], vec![json!({"symbol": "x", "why": "y"})]);
        assert_eq!(none["ok"], false);
        assert_eq!(none["gaps"][0]["symbol"], "x");
    }

    #[test]
    fn var_cvar_and_drawdown_helpers() {
        let mut r: Vec<f64> = (0..100).map(|i| (i as f64 - 50.0) / 1000.0).collect(); // -5% … +4.9%
        let (var, cvar) = var_cvar(&r, 0.95).unwrap();
        assert!(
            (var - 0.046).abs() < 1e-9 && (cvar - 0.048).abs() < 1e-9,
            "最差 5% = 5 个样本:第 5 差的 4.6% 是 VaR,5 个的平均 4.8% 是 CVaR;{var} {cvar}"
        );
        r.truncate(10);
        assert!(var_cvar(&r, 0.95).is_none(), "样本太少不算");
        assert!((max_drawdown(&[0.1, -0.2, 0.05]) - (0.88 / 1.1 - 1.0)).abs() < 1e-12);
    }

    /// 合成数据夹具(不是任何真实持仓):VIBE_XRAY_FIXTURE=路径 cargo test portfolio_xray::tests::dump_synthetic_fixture -- --ignored
    #[test]
    #[ignore]
    fn dump_synthetic_fixture() {
        let Ok(out) = std::env::var("VIBE_XRAY_FIXTURE") else {
            return;
        };
        let a = pos(
            "600519",
            100.0,
            Some("白酒"),
            &walk(280, 100.0, 0.0006, 0.012),
        );
        let b = pos(
            "000001",
            1000.0,
            Some("银行"),
            &walk(280, 10.0, 0.0002, 0.006),
        );
        let c = pos("300308", 200.0, None, &walk(280, 50.0, 0.0012, 0.025));
        let short = pos(
            "688001",
            100.0,
            Some("半导体"),
            &walk(20, 30.0, 0.001, 0.01),
        );
        let mut names = [a, b, c, short];
        names[0].name = "样本白酒".into();
        names[1].name = "样本银行".into();
        names[2].name = "样本光模块".into();
        names[3].name = "样本新股".into();
        let v = compute(
            &names,
            vec![json!({"symbol": "920575", "why": "取不到日线:北交所东财日线没有返回"})],
        );
        std::fs::write(out, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }
}
