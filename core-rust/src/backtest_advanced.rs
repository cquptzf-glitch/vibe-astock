use super::*;
use std::collections::{BTreeMap, BTreeSet};

/// The portfolio calendar is a union. An absent bar carries the last known price
/// and cannot trade; dropping every suspended date would change other securities' signals.
pub(super) fn align_union(all: &[(crate::SymbolKey, Vec<Bar>)]) -> Result<Vec<Vec<Bar>>> {
    let start = all
        .iter()
        .filter_map(|(_, r)| r.first().map(|b| b.date.as_str()))
        .max()
        .ok_or_else(|| anyhow!("没有行情"))?;
    let end = all
        .iter()
        .filter_map(|(_, r)| r.last().map(|b| b.date.as_str()))
        .min()
        .unwrap_or(start);
    let dates: BTreeSet<&str> = all
        .iter()
        .flat_map(|(_, rows)| rows.iter().map(|b| b.date.as_str()))
        .filter(|d| *d >= start && *d <= end)
        .collect();
    all.iter()
        .map(|(_, rows)| {
            let by: BTreeMap<&str, &Bar> = rows.iter().map(|b| (b.date.as_str(), b)).collect();
            let mut previous = rows
                .iter()
                .filter(|r| r.date.as_str() <= start)
                .next_back()
                .map(|r| r.close);
            let mut out = Vec::new();
            for d in &dates {
                let bar = if let Some(b) = by.get(d) {
                    (*b).clone()
                } else {
                    let px = previous.ok_or_else(|| anyhow!("缺少起始价格"))?;
                    Bar {
                        date: (*d).to_string(),
                        open: px,
                        close: px,
                        tradable: false,
                        limit_override: None,
                    }
                };
                previous = Some(bar.close);
                out.push(bar);
            }
            Ok(out)
        })
        .collect()
}

pub(crate) fn sharpe_interval(curve: &[(String, f64)]) -> Value {
    let returns: Vec<f64> = curve
        .windows(2)
        .filter(|w| w[0].1 > 0.0)
        .map(|w| w[1].1 / w[0].1 - 1.0)
        .collect();
    if returns.len() < 60 {
        return json!({"available":false,"reason":"至少需要六十个日收益样本"});
    }
    let sharpe = |r: &[f64]| {
        let mean = r.iter().sum::<f64>() / r.len() as f64;
        let var = r.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (r.len() - 1) as f64;
        (var > 1e-20).then(|| mean / var.sqrt() * 252.0_f64.sqrt())
    };
    let mut state = 42_u64;
    let mut samples = Vec::new();
    let block = 10;
    let replications = 1000;
    for _ in 0..replications {
        let mut sample = Vec::with_capacity(returns.len());
        while sample.len() < returns.len() {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let begin = ((state >> 32) as usize) % returns.len();
            for j in 0..block {
                if sample.len() == returns.len() {
                    break;
                }
                sample.push(returns[(begin + j) % returns.len()]);
            }
        }
        if let Some(s) = sharpe(&sample) {
            samples.push(s);
        }
    }
    samples.sort_by(f64::total_cmp);
    if samples.len() < 100 {
        return json!({"available":false,"reason":"收益波动不足,重抽样夏普无法计算"});
    }
    json!({"available":true,"low":samples[samples.len()*25/1000],"high":samples[samples.len()*975/1000],"positive_share":samples.iter().filter(|v|**v>0.0).count()as f64/samples.len()as f64,"valid_samples":samples.len(),"replications":replications,"block_days":block,"seed":42,"confidence":0.95,"note":"循环分块重抽样,保留块内的日收益相关性;无风险利率按零计算,区间不代表未来收益。"})
}

pub(super) fn walk_forward(
    data: &[Bar],
    base: &Strategy,
    costs: Costs,
    initial: f64,
    stop: Option<f64>,
    limit: Option<f64>,
) -> Value {
    let grid = experiment_grid(base);
    let warmup = grid.iter().map(Strategy::warmup).max().unwrap_or(0);
    let train = 252;
    let test = 63;
    if data.len() < warmup + train + test {
        return json!({"available":false,"reason":format!("至少需要 {} 根日线(最大预热 + 252 日训练 + 63 日检验)",warmup+train+test)});
    }
    let mut folds = Vec::new();
    let mut capital = initial;
    for split in (warmup + train..data.len()).step_by(test) {
        let end = (split + test).min(data.len());
        if end - split < 20 {
            break;
        }
        let mut ranked: Vec<(&Strategy, f64)> = grid
            .iter()
            .filter_map(|s| {
                let begin = split - train - s.warmup();
                let sim = simulate(&data[begin..split], s, costs, initial, stop, limit).ok()?;
                Some((s, curve_metrics(initial, &sim.equity).total_return_pct))
            })
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        let Some((best, is_return)) = ranked.first() else {
            continue;
        };
        let before = capital;
        let Ok(sim) = simulate(
            &data[split - best.warmup()..end],
            best,
            costs,
            capital,
            stop,
            limit,
        ) else {
            continue;
        };
        capital = sim.equity.last().map(|p| p.1).unwrap_or(capital);
        // Explicitly close at the segment boundary, charging both fee and slippage.
        if sim.open_shares > 0.0 {
            let close = data[end - 1].close;
            let px = costs.sell_price(close);
            capital -= sim.open_shares * (close - px) + costs.fee("sell", px, sim.open_shares);
        }
        folds.push(json!({"train_start":data[split-train].date,"train_end":data[split-1].date,"test_start":data[split].date,"test_end":data[end-1].date,"selected":best.label(),"is_return_pct":is_return,"oos_return_pct":(capital/before-1.0)*100.0,"closed_trades":sim.trades.len(),"equity":capital}));
    }
    json!({"available":!folds.is_empty(),"folds":folds,"total_return_pct":(capital/initial-1.0)*100.0,"train_bars":train,"test_bars":test,"note":"每段仅用此前固定训练窗口挑选参数,后续检验段互不重叠;各段末按收盘计滑点和卖出费用平仓,下一段重新建仓。未在样本外挑参数,不宣称找到最优策略。"})
}

pub(super) async fn enhance(
    client: &Client,
    body: &Value,
    out: &mut Value,
    all: &[(crate::SymbolKey, Vec<Bar>)],
    strategy: &Strategy,
    costs: Costs,
    initial: f64,
    stop: Option<f64>,
    limits: bool,
) -> Result<()> {
    let aligned = align_union(all)?;
    let mut curve = Vec::new();
    let mut free_curve = Vec::new();
    for (i, ((key, _), data)) in all.iter().zip(&aligned).enumerate() {
        let share = initial / all.len() as f64;
        let paid = simulate(
            data,
            strategy,
            costs,
            share,
            stop,
            limits.then(|| board_limit(&key.digits)),
        )?;
        let free = simulate(
            data,
            strategy,
            Costs {
                fees: None,
                slippage_bps: 0.0,
                ..costs
            },
            share,
            stop,
            limits.then(|| board_limit(&key.digits)),
        )?;
        if i == 0 {
            curve = paid.equity.clone();
            free_curve = free.equity.clone();
        } else {
            for (j, (_, v)) in paid.equity.iter().enumerate() {
                curve[j].1 += v;
            }
            for (j, (_, v)) in free.equity.iter().enumerate() {
                free_curve[j].1 += v;
            }
        }
    }
    let rebalance = uint(body, "rebalance_every", 0, 0, 252)? as usize;
    if rebalance > 0 {
        let paid = rebalance_portfolio(
            all, &aligned, strategy, costs, initial, stop, limits, rebalance,
        )?;
        let free = rebalance_portfolio(
            all,
            &aligned,
            strategy,
            Costs {
                fees: None,
                slippage_bps: 0.0,
                ..costs
            },
            initial,
            stop,
            limits,
            rebalance,
        )?;
        curve = paid.curve.clone();
        free_curve = free.curve;
        let metrics = curve_metrics(initial, &curve);
        out["metrics"]["final_equity"] = json!(curve.last().map(|p| p.1).unwrap_or(initial));
        out["metrics"]["total_return_pct"] = json!(metrics.total_return_pct);
        out["metrics"]["max_drawdown_pct"] = json!(metrics.max_drawdown_pct);
        out["metrics"]["annualized_return_pct"] = json!(metrics.annualized_pct);
        out["metrics"]["sharpe"] = json!(metrics.sharpe);
        out["metrics"]["excess_return_pct"] = json!(
            metrics.total_return_pct
                - out["metrics"]["buy_hold_return_pct"]
                    .as_f64()
                    .unwrap_or(0.0)
        );
        out["metrics"]["closed_trades"] = json!(paid.trades.len());
        out["metrics"]["win_rate_pct"] = win_rate(&paid.trades);
        out["metrics"]["total_fees"] = json!(paid.fees);
        out["metrics"]["slippage_cost"] = json!(paid.slippage);
        out["metrics"]["blocked_buys"] = json!(paid.blocked_buy);
        out["metrics"]["blocked_sells"] = json!(paid.blocked_sell);
        out["metrics"]["open_position"] = json!(paid.open);
        out["per_symbol"] = json!([]);
        out["trades"] = json!(paid.rows);
        out["strategy"]["weighting"] = json!(format!(
            "共享现金、每 {rebalance} 个交易日等权再平衡;信号变化时同步调整,空缺权重留现金"
        ));
        out["trade_unit"] = json!("sell_fill");
        let bench: Vec<(String, f64)> = aligned[0][strategy.warmup()..]
            .iter()
            .enumerate()
            .map(|(i, b)| {
                (
                    b.date.clone(),
                    aligned
                        .iter()
                        .map(|r| {
                            initial / all.len() as f64 * r[strategy.warmup() + i].close
                                / r[strategy.warmup()].close
                        })
                        .sum(),
                )
            })
            .collect();
        out["equity_curve"] = json!(curve);
        out["monthly"] = json!(monthly_table(&curve, &bench, initial));
        out["drawdowns"] = json!(drawdown_episodes(&curve, initial, 3));
        out["note"]=json!("组合按前一日信号、当日开盘再平衡,可加减仓;卖出先于买入,100股整手、共享现金、费用与滑点逐笔扣除。卖出明细和胜率按每笔卖出成交统计,部分减仓也算一笔,不等同完整交易轮次。零量/缺失日禁止成交;ST及新股历史制度仍需另行提供时点资料。");
    }
    out["sharpe_interval"] = sharpe_interval(&curve);
    out["cost_comparison"] = json!({"actual_return_pct":out["metrics"]["total_return_pct"],"no_cost_return_pct":curve_metrics(initial,&free_curve).total_return_pct,"note":"同一规则下分别重跑,无成本情形的整手数量/交易路径可能不同。"});
    out["suspension"] = json!({"untradable_bars":aligned.iter().map(|r|r.iter().filter(|b|!b.tradable).count()).sum::<usize>(),"note":"零成交量和多标的并集日历上的缺失日不可交易,以最近已知收盘价估值。单标的源站完全省略的停牌日还需指数日历才能补齐。"});
    if body["walk_forward"] == true {
        if all.len() != 1 {
            return Err(anyhow!("滚动样本外检验只支持单只股票"));
        }
        out["walk_forward"] = walk_forward(
            &all[0].1,
            strategy,
            costs,
            initial,
            stop,
            limits.then(|| board_limit(&all[0].0.digits)),
        );
    }
    if body["benchmark"] == true {
        let index = crate::chart::index_closes(client, "1.000300", 4000).await;
        out["benchmark"] = match index {
            Ok(rows) => {
                let by: BTreeMap<&str, f64> = rows.iter().map(|(d, p)| (d.as_str(), *p)).collect();
                let first = curve.first().and_then(|(d, _)| by.get(d.as_str()));
                let last = curve.last().and_then(|(d, _)| by.get(d.as_str()));
                match (first, last) {
                    (Some(a), Some(b)) if *a > 0.0 => {
                        json!({"available":true,"name":"沪深300价格指数","return_pct":(b/a-1.0)*100.0,"excess_pct":out["metrics"]["total_return_pct"].as_f64().unwrap_or(0.0)-(b/a-1.0)*100.0,"start":curve.first().map(|p|&p.0),"end":curve.last().map(|p|&p.0),"note":"严格匹配策略首末日;价格指数不含股息再投资。"})
                    }
                    _ => json!({"available":false,"reason":"指数没有覆盖策略区间的首末交易日"}),
                }
            }
            Err(e) => json!({"available":false,"reason":e.to_string()}),
        };
    }
    Ok(())
}

struct PortfolioSim {
    curve: Vec<(String, f64)>,
    trades: Vec<Trade>,
    rows: Vec<Value>,
    fees: f64,
    slippage: f64,
    blocked_buy: usize,
    blocked_sell: usize,
    open: bool,
}
#[derive(Default, Clone)]
struct Position {
    shares: f64,
    avg: f64,
    buy_fee: f64,
    date: String,
}
fn rebalance_portfolio(
    all: &[(crate::SymbolKey, Vec<Bar>)],
    data: &[Vec<Bar>],
    strategy: &Strategy,
    costs: Costs,
    initial: f64,
    stop: Option<f64>,
    limits: bool,
    every: usize,
) -> Result<PortfolioSim> {
    if all.len() < 2 || every == 0 {
        return Err(anyhow!("再平衡模式需要至少两只股票与正的间隔"));
    }
    let n = all.len();
    let warmup = strategy.warmup();
    let len = data[0].len();
    if len < warmup + 2 {
        return Err(anyhow!("再平衡日线不足"));
    }
    let closes: Vec<Vec<f64>> = data
        .iter()
        .map(|r| r.iter().map(|b| b.close).collect())
        .collect();
    let rsi: Vec<Vec<f64>> = closes
        .iter()
        .map(|c| {
            if let Strategy::Rsi { period, .. } = strategy {
                rsi_series(c, *period)
            } else {
                vec![]
            }
        })
        .collect();
    let mut positions = vec![Position::default(); n];
    let mut active = vec![false; n];
    let mut cash = initial;
    let mut result = PortfolioSim {
        curve: vec![],
        trades: vec![],
        rows: vec![],
        fees: 0.0,
        slippage: 0.0,
        blocked_buy: 0,
        blocked_sell: 0,
        open: false,
    };
    for i in warmup..len {
        let mut changed = false;
        let mut reasons = vec!["再平衡减仓"; n];
        if i > warmup {
            for k in 0..n {
                let (buy, sell) = strategy.decide(&closes[k], &rsi[k], i);
                let stopped = stop.is_some_and(|sl| {
                    positions[k].shares > 0.0
                        && closes[k][i - 1] <= positions[k].avg * (1.0 - sl / 100.0)
                });
                let next = if stopped || sell {
                    false
                } else if buy {
                    true
                } else {
                    active[k]
                };
                if active[k] != next {
                    changed = true;
                    active[k] = next;
                }
                if stopped {
                    reasons[k] = "止损";
                } else if sell {
                    reasons[k] = "信号";
                }
            }
            if changed || (i - warmup) % every == 0 {
                let equity = cash
                    + (0..n)
                        .map(|k| positions[k].shares * data[k][i].open)
                        .sum::<f64>();
                let target: Vec<f64> = (0..n)
                    .map(|k| {
                        if active[k] {
                            (equity * costs.position_pct / 100.0 / n as f64 / data[k][i].open / LOT)
                                .floor()
                                * LOT
                        } else {
                            0.0
                        }
                    })
                    .collect();
                // Every signal is known before the open. Sell first to free shared cash.
                for k in 0..n {
                    let bar = &data[k][i];
                    if !bar.tradable {
                        continue;
                    }
                    let reduce = (positions[k].shares - target[k]).max(0.0);
                    if reduce == 0.0 {
                        continue;
                    }
                    let gap = bar.open / data[k][i - 1].close - 1.0;
                    if bar
                        .limit(limits.then(|| board_limit(&all[k].0.digits)))
                        .is_some_and(|l| gap <= -l + 0.001)
                    {
                        result.blocked_sell += 1;
                        continue;
                    }
                    let p = &mut positions[k];
                    let px = costs.sell_price(bar.open);
                    let fee = costs.fee("sell", px, reduce);
                    let buy_fee = p.buy_fee * reduce / p.shares;
                    let t = Trade {
                        entry_date: p.date.clone(),
                        entry_price: p.avg,
                        exit_date: bar.date.clone(),
                        exit_price: px,
                        shares: reduce,
                        pnl: (px - p.avg) * reduce - buy_fee - fee,
                        fees: buy_fee + fee,
                        reason: reasons[k],
                    };
                    let mut row = trades_value(std::slice::from_ref(&t)).remove(0);
                    row["symbol"] = json!(all[k].0.digits);
                    result.rows.push(row);
                    result.trades.push(t);
                    p.shares -= reduce;
                    p.buy_fee -= buy_fee;
                    cash += reduce * px - fee;
                    result.fees += fee;
                    result.slippage += (bar.open - px) * reduce;
                }
                for k in 0..n {
                    let bar = &data[k][i];
                    if !bar.tradable {
                        continue;
                    }
                    let want = (target[k] - positions[k].shares).max(0.0);
                    if want == 0.0 {
                        continue;
                    }
                    let gap = bar.open / data[k][i - 1].close - 1.0;
                    if bar
                        .limit(limits.then(|| board_limit(&all[k].0.digits)))
                        .is_some_and(|l| gap >= l - 0.001)
                    {
                        result.blocked_buy += 1;
                        continue;
                    }
                    let px = costs.buy_price(bar.open);
                    let mut buy = want.min((cash / px / LOT).floor() * LOT);
                    while buy > 0.0 && buy * px + costs.fee("buy", px, buy) > cash {
                        buy -= LOT;
                    }
                    if buy <= 0.0 {
                        continue;
                    }
                    let p = &mut positions[k];
                    let fee = costs.fee("buy", px, buy);
                    p.avg = (p.avg * p.shares + px * buy) / (p.shares + buy);
                    if p.shares == 0.0 {
                        p.date = bar.date.clone();
                    }
                    p.shares += buy;
                    p.buy_fee += fee;
                    cash -= buy * px + fee;
                    result.fees += fee;
                    result.slippage += (px - bar.open) * buy;
                }
            }
        }
        if cash < -1e-6 {
            return Err(anyhow!("组合现金对账失败"));
        }
        result.curve.push((
            data[0][i].date.clone(),
            cash + (0..n)
                .map(|k| positions[k].shares * data[k][i].close)
                .sum::<f64>(),
        ));
    }
    result.open = positions.iter().any(|p| p.shares > 0.0);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rebalance_uses_shared_cash_and_changes_lot_positions_without_overspending() {
        let make = |slope: f64| {
            (0..100)
                .map(|i| Bar {
                    date: format!("d{i:03}"),
                    open: 20.0 + i as f64 * slope,
                    close: 20.0 + i as f64 * slope,
                    tradable: true,
                    limit_override: None,
                })
                .collect::<Vec<_>>()
        };
        let all = vec![
            (crate::normalize_stock_symbol("600001").unwrap(), make(0.5)),
            (crate::normalize_stock_symbol("000001").unwrap(), make(0.02)),
        ];
        let data = align_union(&all).unwrap();
        let strategy = Strategy::SmaCross { fast: 3, slow: 10 };
        let costs = Costs {
            fees: Some(DEFAULT_FEES),
            slippage_bps: 10.0,
            position_pct: 100.0,
        };
        let result =
            rebalance_portfolio(&all, &data, &strategy, costs, 100000.0, None, false, 5).unwrap();
        assert!(!result.trades.is_empty());
        assert!(result
            .trades
            .iter()
            .all(|t| t.shares % 100.0 == 0.0 && t.fees > 0.0));
        assert!(result.fees > 0.0 && result.slippage > 0.0);
        assert_eq!(result.curve[0].1, 100000.0);
        assert!(result.curve.iter().all(|(_, v)| v.is_finite() && *v > 0.0));
        let free = rebalance_portfolio(
            &all,
            &data,
            &strategy,
            Costs {
                fees: None,
                slippage_bps: 0.0,
                ..costs
            },
            100000.0,
            None,
            false,
            5,
        )
        .unwrap();
        assert!(free.curve.last().unwrap().1 > result.curve.last().unwrap().1);
    }
    #[test]
    fn suspension_carries_price_but_cannot_fill_a_buy() {
        let mut data: Vec<Bar> = (0..15)
            .map(|i| Bar {
                date: format!("d{i:03}"),
                open: 10.0 + i as f64,
                close: 10.0 + i as f64,
                tradable: false,
                limit_override: None,
            })
            .collect();
        let s = Strategy::SmaCross { fast: 3, slow: 10 };
        let c = Costs {
            fees: None,
            slippage_bps: 0.0,
            position_pct: 100.0,
        };
        assert_eq!(
            simulate(&data, &s, c, 10000.0, None, None)
                .unwrap()
                .open_shares,
            0.0
        );
        data[14].tradable = true;
        assert!(
            simulate(&data, &s, c, 10000.0, None, None)
                .unwrap()
                .open_shares
                > 0.0
        );
    }
    #[test]
    fn confidence_and_rolling_selection_are_deterministic_and_no_future_leaks() {
        let curve: Vec<(String, f64)> = (0..150)
            .map(|i| {
                (
                    format!("{i}"),
                    10000.0 + (i as f64 * 10.0) + (i % 7) as f64 * 5.0,
                )
            })
            .collect();
        assert_eq!(sharpe_interval(&curve), sharpe_interval(&curve));
        assert_eq!(sharpe_interval(&curve)["available"], true);
        let data: Vec<Bar> = (0..570)
            .map(|i| Bar {
                date: format!("d{i:03}"),
                open: 20.0 + i as f64 * 0.01 + (i % 20) as f64 * 0.1,
                close: 20.0 + i as f64 * 0.01 + (i % 20) as f64 * 0.1,
                tradable: true,
                limit_override: None,
            })
            .collect();
        let s = Strategy::SmaCross { fast: 10, slow: 30 };
        let c = Costs {
            fees: None,
            slippage_bps: 0.0,
            position_pct: 100.0,
        };
        let a = walk_forward(&data, &s, c, 100000.0, None, None);
        let mut changed = data.clone();
        for b in changed.iter_mut().skip(440) {
            b.close *= 2.0;
            b.open *= 2.0;
        }
        let b = walk_forward(&changed, &s, c, 100000.0, None, None);
        assert_eq!(a["folds"][0], b["folds"][0]);
        assert_eq!(a["available"], true);
    }
}
