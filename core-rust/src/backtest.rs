//! 手机端 Lite 回测:日线、前复权、信号用前一交易日收盘、下一交易日开盘成交。
//!
//! 口径(和桌面版完整回测的差别写在 note 里):
//!  - 费用:默认按交易日志设置的默认费率(佣金、最低佣金、卖出印花税、过户费)计,`fees:"none"` 才不计;可传四项自定义。
//!  - 滑点:`slippage_bps` 默认 0(不替用户假设),买入价上浮、卖出价下浮。
//!  - 买入按 100 股整手,钱不够一手就不买;全仓进出,没有加减仓。
//!  - 没有涨跌停、停牌、T+1 之外的限制:一字涨停买不进、跌停卖不出这类情况**没有模拟**,结果会偏乐观,note 里写明。
//!  - 多标的是「等权、各自独立」:本金平均分,各跑各的,日期取各标的共有的交易日,不做再平衡。
//!  - 参数实验是固定的一小格组合(不是自动寻优),只给「参数一动结果怎么变」的稳健性参考;挑最好那格当结论就是过拟合。

use crate::{journal_settings::{Fees, DEFAULT_FEES}, quote, symbol::normalize_stock_symbol};
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::{json, Map, Value};
#[path = "backtest_advanced.rs"]
mod advanced;
#[path="backtest_rules.rs"]
mod rules;

const LOT: f64 = 100.0;
const SYMBOLS_MAX: usize = 5;

#[derive(Debug, Clone)]
struct Bar {
    date: String,
    open: f64,
    close: f64,
    tradable: bool,
    limit_override: Option<f64>,
}

impl Bar { fn limit(&self,fallback:Option<f64>)->Option<f64>{fallback?;match self.limit_override {Some(0.0)=>None,Some(v)=>Some(v),None=>fallback}} }

#[derive(Debug, Clone, PartialEq)]
enum Strategy {
    /// 前一收盘 > 快线 > 慢线进场;前一收盘跌破快线或快线跌破慢线出场
    SmaCross { fast: usize, slow: usize },
    /// 收盘突破前 entry 日最高收盘进场,跌破前 exit 日最低收盘出场
    Donchian { entry: usize, exit: usize },
    /// RSI(Wilder)低于 buy 进场,高于 sell 出场
    Rsi { period: usize, buy: f64, sell: f64 },
}

impl Strategy {
    fn id(&self) -> &'static str {
        match self {
            Strategy::SmaCross { .. } => "sma_cross",
            Strategy::Donchian { .. } => "donchian",
            Strategy::Rsi { .. } => "rsi",
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Strategy::SmaCross { .. } => "SMA 趋势交叉 Lite",
            Strategy::Donchian { .. } => "唐奇安通道突破 Lite",
            Strategy::Rsi { .. } => "RSI 超卖反弹 Lite",
        }
    }

    fn params(&self) -> Value {
        match self {
            Strategy::SmaCross { fast, slow } => json!({"fast": fast, "slow": slow}),
            Strategy::Donchian { entry, exit } => json!({"entry": entry, "exit": exit}),
            Strategy::Rsi { period, buy, sell } => json!({"period": period, "buy": buy, "sell": sell}),
        }
    }

    fn label(&self) -> String {
        match self {
            Strategy::SmaCross { fast, slow } => format!("MA{fast} / MA{slow}"),
            Strategy::Donchian { entry, exit } => format!("突破 {entry} 日高 / 跌破 {exit} 日低"),
            Strategy::Rsi { period, buy, sell } => format!("RSI{period} < {buy} 买 / > {sell} 卖"),
        }
    }

    /// 至少要有这么多根历史才开始交易
    fn warmup(&self) -> usize {
        match self {
            Strategy::SmaCross { slow, .. } => *slow,
            Strategy::Donchian { entry, exit } => (*entry).max(*exit) + 1,
            Strategy::Rsi { period, .. } => period + 1,
        }
    }

    /// 第 i 根开盘时的决策(只看到 i-1 根及以前的收盘):(想进场, 想出场)
    fn decide(&self, closes: &[f64], rsi: &[f64], i: usize) -> (bool, bool) {
        let prev = closes[i - 1];
        match self {
            Strategy::SmaCross { fast, slow } => {
                let (Some(f), Some(s)) = (sma(&closes[..i], *fast), sma(&closes[..i], *slow)) else { return (false, false) };
                (prev > f && f > s, prev < f || f < s)
            }
            Strategy::Donchian { entry, exit } => {
                if i < entry + 1 || i < exit + 1 {
                    return (false, false);
                }
                let high = closes[i - 1 - entry..i - 1].iter().copied().fold(f64::MIN, f64::max);
                let low = closes[i - 1 - exit..i - 1].iter().copied().fold(f64::MAX, f64::min);
                (prev > high, prev < low)
            }
            Strategy::Rsi { buy, sell, .. } => {
                let r = rsi[i - 1];
                if r.is_nan() {
                    return (false, false);
                }
                (r < *buy, r > *sell)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Costs {
    fees: Option<Fees>,
    slippage_bps: f64,
    /// 每次进场用空仓现金的比例(%),默认 100 = 全仓;不是加减仓,进了就一次买够
    position_pct: f64,
}

impl Costs {
    fn fee(&self, side: &str, price: f64, shares: f64) -> f64 {
        self.fees.map(|f| f.estimate(side, price, shares)).unwrap_or(0.0)
    }
    fn buy_price(&self, open: f64) -> f64 {
        open * (1.0 + self.slippage_bps / 10_000.0)
    }
    fn sell_price(&self, open: f64) -> f64 {
        open * (1.0 - self.slippage_bps / 10_000.0)
    }
}

#[derive(Debug, Clone)]
struct Trade {
    entry_date: String,
    entry_price: f64,
    exit_date: String,
    exit_price: f64,
    shares: f64,
    /// 扣掉买卖费用后的盈亏(元)
    pnl: f64,
    fees: f64,
    reason: &'static str,
}

#[derive(Debug)]
struct Sim {
    equity: Vec<(String, f64)>,
    trades: Vec<Trade>,
    fees_paid: f64,
    slippage_cost: f64,
    open_shares: f64,
    open_entry: Option<f64>,
    skipped_cash: usize,
    /// 因开盘就在涨停价(买不进)/跌停价(卖不出)而没成交的次数
    blocked_buys: usize,
    blocked_sells: usize,
    start_close: f64,
    final_close: f64,
    /// 买入持有同期权益(同样的本金,从第一根交易日的收盘买入):逐日,用来对照
    bench: Vec<(String, f64)>,
}

/// 纯计算:一只股票的日线 + 策略 + 成本 → 逐日权益与成交。`stop_loss_pct`:持仓收盘跌到成本价下方这个比例,下一开盘出场
fn simulate(data: &[Bar], strategy: &Strategy, costs: Costs, initial: f64, stop_loss_pct: Option<f64>, limit_pct: Option<f64>) -> Result<Sim> {
    let warmup = strategy.warmup();
    if data.len() < warmup + 2 {
        return Err(anyhow!("有效 K 线不足,至少需要 {} 根,实际 {}", warmup + 2, data.len()));
    }
    let closes: Vec<f64> = data.iter().map(|b| b.close).collect();
    let rsi = match strategy {
        Strategy::Rsi { period, .. } => rsi_series(&closes, *period),
        _ => Vec::new(),
    };
    let mut cash = initial;
    let mut shares = 0.0_f64;
    let mut entry: Option<(String, f64, f64)> = None; // 日期、成交价、买入费用
    let mut trades = Vec::new();
    let mut equity = Vec::with_capacity(data.len() - warmup);
    let (mut fees_paid, mut slippage_cost, mut skipped_cash) = (0.0, 0.0, 0usize);
    let (mut blocked_buys, mut blocked_sells) = (0usize, 0usize);
    for i in warmup..data.len() {
        let bar = &data[i];
        if i > warmup && bar.tradable {
            let (want_in, mut want_out) = strategy.decide(&closes, &rsi, i);
            let mut reason = "信号";
            if let (Some(sl), Some((_, px, _))) = (stop_loss_pct, entry.as_ref()) {
                if closes[i - 1] <= px * (1.0 - sl / 100.0) {
                    want_out = true;
                    reason = "止损";
                }
            }
            // 涨跌停:开盘价贴着涨停价买不进、贴着跌停价卖不出(0.1% 容差吸收取整);信号保留,下一根开盘再试
            let gap = bar.open / data[i - 1].close - 1.0;
            let buy_blocked = bar.limit(limit_pct).is_some_and(|l| gap >= l - 0.001);
            let sell_blocked = bar.limit(limit_pct).is_some_and(|l| gap <= -l + 0.001);
            if shares == 0.0 && want_in && buy_blocked {
                blocked_buys += 1;
            } else if shares > 0.0 && want_out && sell_blocked {
                blocked_sells += 1;
            } else if shares == 0.0 && want_in {
                let px = costs.buy_price(bar.open);
                let budget = cash * costs.position_pct / 100.0;
                let mut lots = (budget / px / LOT).floor();
                while lots > 0.0 && lots * LOT * px + costs.fee("buy", px, lots * LOT) > budget {
                    lots -= 1.0;
                }
                if lots > 0.0 {
                    let n = lots * LOT;
                    let fee = costs.fee("buy", px, n);
                    cash -= n * px + fee;
                    shares = n;
                    fees_paid += fee;
                    slippage_cost += (px - bar.open) * n;
                    entry = Some((bar.date.clone(), px, fee));
                } else {
                    skipped_cash += 1;
                }
            } else if shares > 0.0 && want_out {
                let px = costs.sell_price(bar.open);
                let fee = costs.fee("sell", px, shares);
                let proceeds = shares * px - fee;
                let (entry_date, entry_px, buy_fee) = entry.take().unwrap_or((bar.date.clone(), px, 0.0));
                cash += proceeds;
                fees_paid += fee;
                slippage_cost += (bar.open - px) * shares;
                trades.push(Trade {
                    entry_date,
                    entry_price: entry_px,
                    exit_date: bar.date.clone(),
                    exit_price: px,
                    shares,
                    pnl: (px - entry_px) * shares - fee - buy_fee,
                    fees: fee + buy_fee,
                    reason,
                });
                shares = 0.0;
            }
        }
        equity.push((bar.date.clone(), cash + shares * bar.close));
    }
    Ok(Sim {
        equity,
        trades,
        fees_paid,
        slippage_cost,
        open_shares: shares,
        open_entry: entry.map(|(_, px, _)| px),
        skipped_cash,
        blocked_buys,
        blocked_sells,
        start_close: data[warmup].close,
        final_close: data[data.len() - 1].close,
        bench: data[warmup..].iter().map(|b| (b.date.clone(), initial * b.close / data[warmup].close)).collect(),
    })
}

struct Metrics {
    total_return_pct: f64,
    max_drawdown_pct: f64,
    annualized_pct: Option<f64>,
    sharpe: Option<f64>,
}

fn curve_metrics(initial: f64, curve: &[(String, f64)]) -> Metrics {
    let final_equity = curve.last().map(|p| p.1).unwrap_or(initial);
    let (mut peak, mut mdd) = (initial, 0.0_f64);
    for (_, v) in curve {
        peak = peak.max(*v);
        if peak > 0.0 {
            mdd = mdd.min(v / peak - 1.0);
        }
    }
    let rets: Vec<f64> = curve.windows(2).filter(|w| w[0].1 > 0.0).map(|w| w[1].1 / w[0].1 - 1.0).collect();
    let (annualized_pct, sharpe) = if rets.len() >= 30 {
        let years = rets.len() as f64 / 252.0;
        let ann = ((final_equity / initial).max(1e-9).powf(1.0 / years) - 1.0) * 100.0;
        let mean = rets.iter().sum::<f64>() / rets.len() as f64;
        let var = rets.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (rets.len() as f64 - 1.0);
        let sd = var.sqrt();
        (Some(ann), (sd > 1e-12).then(|| mean / sd * 252.0_f64.sqrt()))
    } else {
        (None, None)
    };
    Metrics { total_return_pct: (final_equity / initial - 1.0) * 100.0, max_drawdown_pct: mdd * 100.0, annualized_pct, sharpe }
}

fn trades_value(trades: &[Trade]) -> Vec<Value> {
    trades
        .iter()
        .map(|t| {
            json!({
                "entry_date": t.entry_date, "entry_price": r4(t.entry_price), "exit_date": t.exit_date, "exit_price": r4(t.exit_price),
                "shares": t.shares, "pnl": r2(t.pnl), "pnl_pct": r2(t.pnl / (t.entry_price * t.shares) * 100.0), "fees": r2(t.fees), "reason": t.reason,
            })
        })
        .collect()
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
fn r4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

fn win_rate(trades: &[Trade]) -> Value {
    if trades.is_empty() {
        Value::Null
    } else {
        json!(trades.iter().filter(|t| t.pnl > 0.0).count() as f64 / trades.len() as f64 * 100.0)
    }
}

// ------------------------------------------------------------ 参数解析

fn uint(body: &Value, key: &str, default: u64, lo: u64, hi: u64) -> Result<u64> {
    match body.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => {
            let n = v.as_u64().ok_or_else(|| anyhow!("{key} 必须是 {lo}–{hi} 的整数"))?;
            if n < lo || n > hi {
                return Err(anyhow!("{key} 必须在 {lo}–{hi} 之间"));
            }
            Ok(n)
        }
    }
}

fn num(body: &Value, key: &str, default: f64, lo: f64, hi: f64) -> Result<f64> {
    match body.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => {
            let n = v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| anyhow!("{key} 必须是 {lo}–{hi} 的数字"))?;
            if n < lo || n > hi {
                return Err(anyhow!("{key} 必须在 {lo}–{hi} 之间"));
            }
            Ok(n)
        }
    }
}

fn parse_strategy(body: &Value) -> Result<Strategy> {
    match body.get("strategy").and_then(Value::as_str).unwrap_or("sma_cross") {
        "sma_cross" => {
            let fast = uint(body, "fast", 10, 3, 60)? as usize;
            let slow = uint(body, "slow", 30, 10, 200)? as usize;
            if fast >= slow {
                return Err(anyhow!("fast 必须小于 slow"));
            }
            Ok(Strategy::SmaCross { fast, slow })
        }
        "donchian" => Ok(Strategy::Donchian { entry: uint(body, "entry", 20, 5, 120)? as usize, exit: uint(body, "exit", 10, 3, 60)? as usize }),
        "rsi" => {
            let buy = num(body, "buy", 30.0, 5.0, 45.0)?;
            let sell = num(body, "sell", 70.0, 55.0, 95.0)?;
            Ok(Strategy::Rsi { period: uint(body, "period", 14, 5, 30)? as usize, buy, sell })
        }
        other => Err(anyhow!("不认识的策略 {other:?}(可选 sma_cross / donchian / rsi)")),
    }
}

fn parse_costs(body: &Value) -> Result<(Costs, Value)> {
    let slippage_bps = num(body, "slippage_bps", 0.0, 0.0, 200.0)?;
    let position_pct = num(body, "position_pct", 100.0, 10.0, 100.0)?;
    let (fees, label): (Option<Fees>, Value) = match body.get("fees") {
        None | Some(Value::Null) => (Some(DEFAULT_FEES), json!("default")),
        Some(Value::String(s)) if s == "default" => (Some(DEFAULT_FEES), json!("default")),
        Some(Value::String(s)) if s == "none" => (None, json!("none")),
        Some(Value::Object(m)) => {
            let get = |k: &str| -> Result<f64> {
                m.get(k).and_then(Value::as_f64).filter(|v| v.is_finite() && *v >= 0.0).ok_or_else(|| anyhow!("fees 里的 {k} 必须是不小于 0 的数字(四项都要填:commission_rate / commission_min / stamp_tax_rate / transfer_fee_rate)"))
            };
            let f = Fees { commission_rate: get("commission_rate")?, commission_min: get("commission_min")?, stamp_tax_rate: get("stamp_tax_rate")?, transfer_fee_rate: get("transfer_fee_rate")?, fifo: false };
            for (k, v) in [("commission_rate", f.commission_rate), ("stamp_tax_rate", f.stamp_tax_rate), ("transfer_fee_rate", f.transfer_fee_rate)] {
                if v > 0.01 {
                    return Err(anyhow!("fees 里的 {k} 看起来不像费率({v}),万 2.5 应填 0.00025"));
                }
            }
            (Some(f), json!("custom"))
        }
        Some(_) => return Err(anyhow!("fees 只能是 \"default\"、\"none\" 或四项费率对象")),
    };
    let detail = match fees {
        Some(f) => json!({"mode": label, "commission_rate": f.commission_rate, "commission_min": f.commission_min, "stamp_tax_rate": f.stamp_tax_rate, "transfer_fee_rate": f.transfer_fee_rate, "slippage_bps": slippage_bps, "position_pct": position_pct}),
        None => json!({"mode": label, "slippage_bps": slippage_bps, "position_pct": position_pct}),
    };
    Ok((Costs { fees, slippage_bps, position_pct }, detail))
}

fn parse_symbols(body: &Value) -> Result<Vec<crate::symbol::SymbolKey>> {
    let mut raw: Vec<String> = Vec::new();
    if let Some(list) = body.get("symbols").and_then(Value::as_array) {
        for v in list {
            raw.push(v.as_str().ok_or_else(|| anyhow!("symbols 里要是代码字符串"))?.to_string());
        }
    }
    if let Some(one) = body.get("symbol").and_then(Value::as_str) {
        if !one.trim().is_empty() {
            raw.insert(0, one.to_string());
        }
    }
    if raw.is_empty() {
        return Err(anyhow!("symbol 不能为空"));
    }
    let mut out: Vec<crate::symbol::SymbolKey> = Vec::new();
    for r in raw {
        let key = normalize_stock_symbol(&r)?;
        if !out.iter().any(|k| k.digits == key.digits) {
            out.push(key);
        }
    }
    if out.len() > SYMBOLS_MAX {
        return Err(anyhow!("一次最多回测 {SYMBOLS_MAX} 只股票"));
    }
    Ok(out)
}

// ------------------------------------------------------------ 参数实验(固定小格)

fn experiment_grid(strategy: &Strategy) -> Vec<Strategy> {
    match strategy {
        Strategy::SmaCross { .. } => [5usize, 10, 20]
            .iter()
            .flat_map(|f| [30usize, 60, 120].iter().map(move |s| (*f, *s)))
            .map(|(fast, slow)| Strategy::SmaCross { fast, slow })
            .collect(),
        Strategy::Donchian { .. } => [10usize, 20, 55]
            .iter()
            .flat_map(|e| [5usize, 10, 20].iter().map(move |x| (*e, *x)))
            .map(|(entry, exit)| Strategy::Donchian { entry, exit })
            .collect(),
        Strategy::Rsi { period, .. } => [20.0, 25.0, 30.0]
            .iter()
            .flat_map(|b| [60.0, 70.0, 80.0].iter().map(move |s| (*b, *s)))
            .map(|(buy, sell)| Strategy::Rsi { period: *period, buy, sell })
            .collect(),
    }
}

fn experiment_value(data: &[Bar], base: &Strategy, costs: Costs, initial: f64, stop: Option<f64>, limit_pct: Option<f64>) -> Value {
    let rows: Vec<Value> = experiment_grid(base)
        .into_iter()
        .filter_map(|s| {
            let sim = simulate(data, &s, costs, initial, stop, limit_pct).ok()?;
            let m = curve_metrics(initial, &sim.equity);
            Some(json!({
                "label": s.label(), "params": s.params(), "is_current": s == *base,
                "total_return_pct": m.total_return_pct, "max_drawdown_pct": m.max_drawdown_pct,
                "closed_trades": sim.trades.len(), "win_rate_pct": win_rate(&sim.trades),
            }))
        })
        .collect();
    let positive = rows.iter().filter(|r| r["total_return_pct"].as_f64().unwrap_or(0.0) > 0.0).count();
    json!({
        "rows": rows,
        "positive_share": if rows.is_empty() { Value::Null } else { json!(positive as f64 / rows.len() as f64) },
        "note": "固定的一小格参数,不是自动寻优:看参数一动结果是不是跟着大幅变化。挑最好的一格当结论就是过拟合。",
    })
}

// ------------------------------------------------------------ 月度收益 / 回撤区间 / 样本内外对照

/// 逐月收益(%):月初基准 = 上月最后一天的权益(首月用本金);基准列是买入持有同期
fn monthly_table(curve: &[(String, f64)], bench: &[(String, f64)], initial: f64) -> Vec<Value> {
    fn by_month(curve: &[(String, f64)], initial: f64) -> Vec<(String, f64)> {
        let mut out: Vec<(String, f64)> = Vec::new();
        let (mut prev_end, mut cur_month, mut last) = (initial, String::new(), initial);
        for (date, value) in curve {
            let month = date.chars().take(7).collect::<String>();
            if month != cur_month {
                if !cur_month.is_empty() {
                    out.push((cur_month.clone(), (last / prev_end - 1.0) * 100.0));
                    prev_end = last;
                }
                cur_month = month;
            }
            last = *value;
        }
        if !cur_month.is_empty() {
            out.push((cur_month, (last / prev_end - 1.0) * 100.0));
        }
        out
    }
    let b: std::collections::HashMap<String, f64> = by_month(bench, bench.first().map(|p| p.1).unwrap_or(initial)).into_iter().collect();
    by_month(curve, initial).into_iter().map(|(m, r)| json!({"month": m, "return_pct": r2(r), "bench_pct": b.get(&m).map(|v| r2(*v))})).collect()
}

/// 最深的几段回撤:从前高到谷底到恢复(没恢复 recover_date 为空)。起点前高是本金,日期写「期初」
fn drawdown_episodes(curve: &[(String, f64)], initial: f64, top: usize) -> Vec<Value> {
    let (mut peak, mut peak_i): (f64, isize) = (initial, -1);
    let mut cur: Option<(isize, usize, f64, f64)> = None; // 前高下标、谷底下标、谷底值、前高值
    let mut eps: Vec<(isize, usize, f64, Option<usize>)> = Vec::new();
    for (i, (_, v)) in curve.iter().enumerate() {
        if *v >= peak {
            if let Some((pi, ti, tv, pv)) = cur.take() {
                eps.push((pi, ti, tv / pv - 1.0, Some(i)));
            }
            peak = *v;
            peak_i = i as isize;
        } else {
            match &mut cur {
                None => cur = Some((peak_i, i, *v, peak)),
                Some(c) if *v < c.2 => {
                    c.1 = i;
                    c.2 = *v;
                }
                _ => {}
            }
        }
    }
    if let Some((pi, ti, tv, pv)) = cur {
        eps.push((pi, ti, tv / pv - 1.0, None));
    }
    eps.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
    let date = |i: isize| if i < 0 { "期初".to_string() } else { curve[i as usize].0.clone() };
    eps.into_iter()
        .take(top)
        .map(|(pi, ti, depth, rec)| {
            let end = rec.unwrap_or(curve.len() - 1) as isize;
            json!({
                "peak_date": date(pi), "trough_date": curve[ti].0.clone(), "recover_date": rec.map(|i| curve[i].0.clone()),
                "depth_pct": r2(depth * 100.0), "bars_to_trough": ti as isize - pi, "bars_total": end - pi,
            })
        })
        .collect()
}

struct OosRow {
    label: String,
    params: Value,
    is_ret: f64,
    oos_ret: f64,
    is_dd: f64,
    oos_dd: f64,
    is_trades: usize,
    oos_trades: usize,
}

/// 样本内 / 样本外对照:一刀切成前 70% 与后 30%,在前段跑固定小格参数、按收益选最好的,再看它在后段(没参与选择)表现如何。
/// 只有一段样本外,不是严格的滚动检验;目的是看「样本内最好的那一格」到了样本外还站不站得住
fn optimize_value(data: &[Bar], base: &Strategy, costs: Costs, initial: f64, stop: Option<f64>, limit_pct: Option<f64>) -> Value {
    let grid = experiment_grid(base);
    let warm_max = grid.iter().map(|s| s.warmup()).max().unwrap_or(0);
    let n = data.len();
    if n < warm_max + 40 {
        return json!({"available": false, "reason": format!("K 线太短,不够切成样本内/外(至少需要 {} 根,现在 {})", warm_max + 40, n)});
    }
    let split = warm_max + ((n - warm_max) as f64 * 0.7) as usize;
    let mut rows: Vec<OosRow> = Vec::new();
    for s in &grid {
        let Ok(is_sim) = simulate(&data[..split], s, costs, initial, stop, limit_pct) else { continue };
        let Ok(oos_sim) = simulate(&data[split.saturating_sub(s.warmup())..], s, costs, initial, stop, limit_pct) else { continue };
        let (im, om) = (curve_metrics(initial, &is_sim.equity), curve_metrics(initial, &oos_sim.equity));
        rows.push(OosRow { label: s.label(), params: s.params(), is_ret: im.total_return_pct, oos_ret: om.total_return_pct, is_dd: im.max_drawdown_pct, oos_dd: om.max_drawdown_pct, is_trades: is_sim.trades.len(), oos_trades: oos_sim.trades.len() });
    }
    if rows.is_empty() {
        return json!({"available": false, "reason": "这段 K 线跑不了任何一组参数"});
    }
    // 样本内收益最高的一格;并列取排在前面的
    let mut best = 0usize;
    for (i, r) in rows.iter().enumerate() {
        if r.is_ret > rows[best].is_ret {
            best = i;
        }
    }
    let mut oos_desc: Vec<f64> = rows.iter().map(|r| r.oos_ret).collect();
    oos_desc.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let best_oos = rows[best].oos_ret;
    let oos_rank = oos_desc.iter().position(|v| *v <= best_oos).map(|i| i + 1).unwrap_or(rows.len());
    let median = {
        let m = oos_desc.len() / 2;
        if oos_desc.len() % 2 == 1 { oos_desc[m] } else { (oos_desc[m - 1] + oos_desc[m]) / 2.0 }
    };
    let positive = rows.iter().filter(|r| r.oos_ret > 0.0).count();
    json!({
        "available": true,
        "in_sample": {"start": data[0].date, "end": data[split - 1].date, "bars": split},
        "out_of_sample": {"start": data[split].date, "end": data[n - 1].date, "bars": n - split},
        "rows": rows.iter().enumerate().map(|(i, r)| json!({
            "label": r.label, "params": r.params, "is_return_pct": r2(r.is_ret), "oos_return_pct": r2(r.oos_ret), "is_drawdown_pct": r2(r.is_dd), "oos_drawdown_pct": r2(r.oos_dd),
            "is_trades": r.is_trades, "oos_trades": r.oos_trades, "best_in_sample": i == best,
        })).collect::<Vec<_>>(),
        "best_in_sample": {"label": rows[best].label, "is_return_pct": r2(rows[best].is_ret), "oos_return_pct": r2(best_oos), "oos_rank": oos_rank, "of": rows.len()},
        "median_oos_return_pct": r2(median),
        "oos_positive_share": positive as f64 / rows.len() as f64,
        "note": "样本内选出的最好一格,到样本外还站得住才有参考意义:样本外排名靠后或收益为负,说明那一格只是拟合了过去。只切了一段样本外,不是严格的滚动检验。",
    })
}

// ------------------------------------------------------------ 入口

pub async fn run(client: &Client, body: &Value) -> Result<Value> {
    let symbols = parse_symbols(body)?;
    let market_rules=rules::parse(body,&symbols)?;
    let (start_date,end_date)=rules::range(body)?;
    let strategy = parse_strategy(body)?;
    let (costs, costs_detail) = parse_costs(body)?;
    let bars = uint(body, "bars", 500, strategy.warmup() as u64 + 5, 4000)? as usize;
    let initial = num(body, "initial_capital", 100_000.0, f64::MIN_POSITIVE, 100_000_000.0)
        .map_err(|_| anyhow!("initial_capital 必须在 0 到 1 亿元之间"))?;
    let stop = match body.get("stop_loss_pct") {
        None | Some(Value::Null) => None,
        Some(_) => Some(num(body, "stop_loss_pct", 0.0, 1.0, 50.0)?),
    };
    let want_experiment = body.get("experiment").and_then(Value::as_bool).unwrap_or(false);
    let limit_check = body.get("limit_check").and_then(Value::as_bool).unwrap_or(true);
    let want_optimize = body.get("optimize").and_then(Value::as_bool).unwrap_or(false);
    let rebalance=uint(body,"rebalance_every",0,0,252)?;
    if rebalance>0&&symbols.len()<2{return Err(anyhow!("再平衡需要至少两只股票"));}
    if body["walk_forward"]==true&&symbols.len()>1{return Err(anyhow!("滚动样本外仅支持单只股票"));}
    if (want_experiment || want_optimize) && symbols.len() > 1 {
        return Err(anyhow!("参数实验和样本外检验只支持单只股票"));
    }

    let calendar=crate::chart::index_closes(client,"1.000300",4000).await;
    let mut provenance=Vec::new();
    let mut all: Vec<(crate::symbol::SymbolKey, Vec<Bar>)> = Vec::new();
    for key in &symbols {
        let envelope = quote::fetch_kline(client, key, if start_date.is_some()||end_date.is_some(){4000}else{bars}).await?;
        let raw_rows = envelope.pointer("/extra/rows").and_then(Value::as_array).ok_or_else(|| anyhow!("{} 的 K 线返回缺少 rows", key.digits))?;
        let mut data:Vec<Bar>=raw_rows.iter().filter_map(parse_bar).collect();
        if start_date.as_ref().is_some_and(|s|data.first().is_none_or(|b|&b.date>s)){return Err(anyhow!("{} 的行情没有覆盖请求起点，未把后来的样本冒充完整区间",key.digits));}
        provenance.push(json!({"symbol":key.digits,"source":envelope["primary_source"],"status":envelope["status"],"fetched_at":envelope["fetched_at"],"fallback_reason":envelope["extra"]["fallback_reason"]}));
        if data.len()!=raw_rows.len(){return Err(anyhow!("{} 存在无效 K 线，未静默跳过后计算",key.digits));}
        if let Ok(calendar)=&calendar{data=rules::complete_calendar(&data,calendar);}
        data.retain(|b|start_date.as_ref().is_none_or(|s|&b.date>=s)&&end_date.as_ref().is_none_or(|e|&b.date<=e));
        rules::apply(&key.digits,&mut data,&market_rules);
        all.push((key.clone(), data));
    }
    let mut result=build_result(&all, &strategy, costs, costs_detail, initial, stop, want_experiment, limit_check, want_optimize)?;
    advanced::enhance(client,body,&mut result,&all,&strategy,costs,initial,stop,limit_check).await?;
    result["data_sources"]=json!(provenance);
    result["requested_range"]=json!({"start":start_date,"end":end_date,"note":"取得区间含预热期；实际策略区间为 period，不把未取得的日期当成已经回测。"});
    result["market_rules"]=json!({"explicit_intervals":market_rules.len(),"note":"创业板按改革日期区分幅度；ST、上市初期与重新上市需提供带日期的 market_rules(0 表示无限制)。没有完整历史制度数据时仅按板块近似，不能用于精确成交可达性判断。"});
    result["suspension"]["calendar_available"]=json!(calendar.is_ok());
    result["suspension"]["calendar_error"]=json!(calendar.err().map(|e|e.to_string()));
    result["suspension"]["note"]=json!("零量及指数日历内缺失日禁止成交，最近已知收盘估值；日历缺失时仅按已有日线计算并披露缺口。起止区间以取得的真实行情为准。");
    Ok(result)
}

/// 取到行情后的全部计算(不碰网络,测试直接喂 K 线)
fn build_result(
    all: &[(crate::symbol::SymbolKey, Vec<Bar>)],
    strategy: &Strategy,
    costs: Costs,
    costs_detail: Value,
    initial: f64,
    stop: Option<f64>,
    want_experiment: bool,
    limit_check: bool,
    want_optimize: bool,
) -> Result<Value> {
    // 多只:只留各标的共有的交易日,各自独立跑
    let aligned: Vec<Vec<Bar>> = if all.len() == 1 {
        vec![all[0].1.clone()]
    } else {
        advanced::align_union(all)?
    };
    let share = initial / all.len() as f64;
    let mut sims = Vec::new();
    for ((key, _), data) in all.iter().zip(&aligned) {
        let sim = simulate(data, strategy, costs, share, stop, limit_check.then(|| board_limit(&key.digits))).map_err(|e| anyhow!("{}:{e}", key.digits))?;
        sims.push(sim);
    }

    // 组合权益 = 各只权益逐日相加(日期已对齐,各只 warmup 相同)
    let curve: Vec<(String, f64)> = (0..sims[0].equity.len()).map(|i| (sims[0].equity[i].0.clone(), sims.iter().map(|s| s.equity[i].1).sum())).collect();
    let metrics = curve_metrics(initial, &curve);
    let final_equity = curve.last().map(|p| p.1).unwrap_or(initial);
    let buy_hold: f64 = sims.iter().map(|s| share * s.final_close / s.start_close).sum();
    let buy_hold_return = (buy_hold / initial - 1.0) * 100.0;
    let trades_all: Vec<Trade> = sims.iter().flat_map(|s| s.trades.clone()).collect();
    let fees_paid: f64 = sims.iter().map(|s| s.fees_paid).sum();
    let slippage_cost: f64 = sims.iter().map(|s| s.slippage_cost).sum();
    let open_any = sims.iter().any(|s| s.open_shares > 0.0);
    let period_start = curve.first().map(|p| p.0.clone());
    let period_end = curve.last().map(|p| p.0.clone());

    let per_symbol: Vec<Value> = all
        .iter()
        .zip(&sims)
        .map(|((key, _), s)| {
            let m = curve_metrics(share, &s.equity);
            json!({
                "symbol": key.digits, "market": key.market.as_str(), "total_return_pct": m.total_return_pct, "max_drawdown_pct": m.max_drawdown_pct,
                "buy_hold_return_pct": (s.final_close / s.start_close - 1.0) * 100.0, "closed_trades": s.trades.len(), "win_rate_pct": win_rate(&s.trades),
                "open_position": s.open_shares > 0.0, "fees": r2(s.fees_paid), "skipped_for_cash": s.skipped_cash,
                "blocked_buys": s.blocked_buys, "blocked_sells": s.blocked_sells,
            })
        })
        .collect();

    let bench_curve: Vec<(String, f64)> = (0..sims[0].bench.len()).map(|i| (sims[0].bench[i].0.clone(), sims.iter().map(|s| s.bench[i].1).sum())).collect();
    let optimize = if want_optimize { Some(optimize_value(&aligned[0], strategy, costs, initial, stop, limit_check.then(|| board_limit(&all[0].0.digits)))) } else { None };
    let experiment = if want_experiment { Some(experiment_value(&aligned[0], strategy, costs, initial, stop, limit_check.then(|| board_limit(&all[0].0.digits)))) } else { None };
    let trade_rows: Vec<Value> = {
        let mut tagged: Vec<(String, Value)> = Vec::new();
        for ((key, _), s) in all.iter().zip(&sims) {
            for (t, mut v) in s.trades.iter().zip(trades_value(&s.trades)) {
                v["symbol"] = json!(key.digits);
                tagged.push((t.exit_date.clone(), v));
            }
        }
        tagged.sort_by(|a, b| a.0.cmp(&b.0));
        tagged.into_iter().map(|(_, v)| v).collect()
    };
    let fees_text = match (&costs.fees, costs.slippage_bps) {
        (None, s) if s == 0.0 => "未计手续费、印花税、滑点".to_string(),
        (None, s) => format!("未计手续费、印花税;滑点 {s} bp"),
        (Some(_), s) if s == 0.0 => "已计佣金、过户费与卖出印花税(费率见 costs);未计滑点".to_string(),
        (Some(_), s) => format!("已计佣金、过户费与卖出印花税(费率见 costs);滑点 {s} bp"),
    };
    let mut note = String::from("这是移动端 Lite 回测,用于历史假设核对;不等同于桌面端完整 Agent 回测。按 100 股整手、全仓进出");
    if limit_check {
        note.push_str(";涨跌停按板块幅度近似(开盘价贴着涨停价买不进、贴着跌停价卖不出,信号留到下一根再试;ST 股 5% 和新股上市初期不设限没单独识别),停牌没模拟,结果仍可能偏乐观");
    } else {
        note.push_str(";已关闭涨跌停限制:一字涨停也按开盘价买得进、跌停也卖得出,停牌没模拟,结果偏乐观");
    }
    if matches!(costs.fees, Some(f) if f == DEFAULT_FEES) {
        note.push_str(";费率用的是默认值(不是你的真实费率)");
    }
    let mut out = Map::new();
    out.insert("status".into(), json!("ok"));
    out.insert("symbol".into(), json!(all[0].0.digits));
    out.insert("market".into(), json!(all[0].0.market.as_str()));
    out.insert("symbols".into(), json!(all.iter().map(|(k, _)| k.digits.clone()).collect::<Vec<_>>()));
    out.insert(
        "strategy".into(),
        json!({
            "id": strategy.id(), "name": strategy.name(), "label": strategy.label(), "params": strategy.params(),
            "fast": strategy.params().get("fast"), "slow": strategy.params().get("slow"),
            "execution": "信号使用前一交易日收盘,下一交易日开盘成交", "fees": fees_text, "adjustment": "前复权日线",
            "stop_loss_pct": stop, "lot": 100, "limit_check": limit_check, "weighting": if all.len() > 1 { "等权、各自独立,不再平衡" } else { "单只全仓" },
        }),
    );
    out.insert("costs".into(), costs_detail);
    out.insert("period".into(), json!({"start": period_start, "end": period_end, "bars": aligned[0].len()}));
    out.insert(
        "metrics".into(),
        json!({
            "initial_capital": initial, "final_equity": final_equity, "total_return_pct": metrics.total_return_pct,
            "buy_hold_return_pct": buy_hold_return, "excess_return_pct": metrics.total_return_pct - buy_hold_return,
            "max_drawdown_pct": metrics.max_drawdown_pct, "annualized_return_pct": metrics.annualized_pct, "sharpe": metrics.sharpe,
            "closed_trades": trades_all.len(), "win_rate_pct": win_rate(&trades_all),
            "total_fees": r2(fees_paid), "slippage_cost": r2(slippage_cost),
            "blocked_buys": sims.iter().map(|s| s.blocked_buys).sum::<usize>(), "blocked_sells": sims.iter().map(|s| s.blocked_sells).sum::<usize>(),
            "open_position": open_any,
            "open_shares": if all.len() == 1 && sims[0].open_shares > 0.0 { json!(sims[0].open_shares) } else { Value::Null },
            "open_entry_price": if all.len() == 1 { json!(sims[0].open_entry) } else { Value::Null },
        }),
    );
    out.insert("per_symbol".into(), json!(per_symbol));
    out.insert("trades".into(), json!(trade_rows));
    out.insert("equity_curve".into(), json!(curve.iter().map(|(d, v)| json!([d, v])).collect::<Vec<_>>()));
    out.insert("monthly".into(), json!(monthly_table(&curve, &bench_curve, initial)));
    out.insert("drawdowns".into(), json!(drawdown_episodes(&curve, initial, 3)));
    if let Some(e) = experiment {
        out.insert("experiment".into(), e);
    }
    if let Some(o) = optimize {
        out.insert("optimize".into(), o);
    }
    out.insert("note".into(), json!(note));
    Ok(Value::Object(out))
}

/// 按板块给涨跌停幅度:科创板 / 创业板 20%,北交所 30%,其余 10%。ST(5%)和新股上市前几天不设限这两种没单独识别,note 里写明
fn board_limit(code: &str) -> f64 {
    if code.starts_with("688") || code.starts_with("689") || code.starts_with("300") || code.starts_with("301") {
        0.20
    } else if code.starts_with('8') || code.starts_with('4') || code.starts_with("920") {
        0.30
    } else {
        0.10
    }
}

fn parse_bar(row: &Value) -> Option<Bar> {
    let items = row.as_array()?;
    let date = items.first()?.as_str()?.to_string();
    let open = items.get(1).and_then(number)?;
    let close = items.get(2).and_then(number)?;
    if date.is_empty() || !open.is_finite() || !close.is_finite() || open <= 0.0 || close <= 0.0 {
        return None;
    }
    Some(Bar { date, open, close, tradable:items.get(5).and_then(number).is_none_or(|v|v>0.0),limit_override:None })
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn sma(values: &[f64], period: usize) -> Option<f64> {
    if values.len() < period || period == 0 {
        return None;
    }
    let tail = &values[values.len() - period..];
    Some(tail.iter().sum::<f64>() / period as f64)
}

/// Wilder RSI,下标 j 的值只用到 closes[..=j];前 period 根没有值(NaN)
fn rsi_series(closes: &[f64], period: usize) -> Vec<f64> {
    let mut out = vec![f64::NAN; closes.len()];
    if closes.len() <= period {
        return out;
    }
    let (mut gain, mut loss) = (0.0, 0.0);
    for j in 1..=period {
        let d = closes[j] - closes[j - 1];
        gain += d.max(0.0);
        loss += (-d).max(0.0);
    }
    let (mut avg_gain, mut avg_loss) = (gain / period as f64, loss / period as f64);
    let value = |g: f64, l: f64| if l == 0.0 { if g == 0.0 { 50.0 } else { 100.0 } } else { 100.0 - 100.0 / (1.0 + g / l) };
    out[period] = value(avg_gain, avg_loss);
    for j in period + 1..closes.len() {
        let d = closes[j] - closes[j - 1];
        avg_gain = (avg_gain * (period as f64 - 1.0) + d.max(0.0)) / period as f64;
        avg_loss = (avg_loss * (period as f64 - 1.0) + (-d).max(0.0)) / period as f64;
        out[j] = value(avg_gain, avg_loss);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_COST: Costs = Costs { fees: None, slippage_bps: 0.0, position_pct: 100.0 };

    fn bar(day: usize, open: f64, close: f64) -> Bar {
        Bar { date: format!("2026-01-{:02}", day + 1), open, close, tradable:true,limit_override:None }
    }

    /// 先横盘 40 根再单边上涨 40 根,开盘价 = 前一收盘
    fn rising() -> Vec<Bar> {
        let mut out = Vec::new();
        let mut last = 10.0;
        for i in 0..80 {
            let close = if i < 40 { 10.0 } else { 10.0 + (i - 39) as f64 * 0.2 };
            out.push(bar(i, last, close));
            last = close;
        }
        out
    }

    #[test]
    fn sma_uses_only_completed_values() {
        assert_eq!(sma(&[1.0, 2.0, 3.0], 2), Some(2.5));
        assert_eq!(sma(&[1.0], 2), None);
    }

    #[test]
    fn parse_bar_rejects_invalid_prices() {
        assert!(parse_bar(&json!(["2026-10-16", 10, 11, 12, 9, 100])).is_some());
        assert!(parse_bar(&json!(["2026-10-16", 0, 11, 12, 9, 100])).is_none());
    }

    #[test]
    fn rsi_matches_wilder_and_handles_flat_and_one_way_moves() {
        let up: Vec<f64> = (0..20).map(|i| 10.0 + i as f64).collect();
        let r = rsi_series(&up, 14);
        assert!(r[13].is_nan() && r[14] == 100.0 && r[19] == 100.0, "只涨不跌 = 100");
        assert!(rsi_series(&[10.0; 20], 14)[14] == 50.0, "不涨不跌取 50");
        // 教科书例子(Wilder):前 14 个涨跌平均后 RSI ≈ 70.5
        let closes = [44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03, 45.61, 46.28, 46.28];
        let v = rsi_series(&closes, 14)[14];
        assert!((v - 70.53).abs() < 0.1, "{v}");
    }

    #[test]
    fn trades_fill_at_next_open_in_lots_and_costs_reduce_the_result() {
        let data = rising();
        let s = Strategy::SmaCross { fast: 5, slow: 20 };
        let free = simulate(&data, &s, NO_COST, 100_000.0, None, None).unwrap();
        assert!(free.open_shares > 0.0 && free.open_shares % 100.0 == 0.0, "整手:{}", free.open_shares);
        assert_eq!(free.fees_paid, 0.0);
        let entry = free.open_entry.unwrap();
        assert!(entry > 10.0, "进场在上涨开始之后,成交价是当天开盘价");
        let costly = simulate(&data, &s, Costs { fees: Some(DEFAULT_FEES), slippage_bps: 10.0, position_pct: 100.0 }, 100_000.0, None, None).unwrap();
        assert!(costly.fees_paid > 0.0 && costly.slippage_cost > 0.0);
        assert!(costly.equity.last().unwrap().1 < free.equity.last().unwrap().1, "有成本一定比没成本差");
        assert!(costly.open_entry.unwrap() > entry, "买入滑点抬高成交价");
    }

    #[test]
    fn round_trip_pnl_matches_hand_calculation() {
        // 第 5 根(下标 4)收盘 11 突破前 3 根最高 10 → 下标 5 开盘 11 买;下标 7 收盘 8 跌破前 3 日最低 → 下标 8 开盘 8 卖
        let closes = [10.0, 10.0, 10.0, 10.0, 11.0, 12.0, 12.0, 8.0, 8.0, 8.0];
        let mut data = Vec::new();
        let mut last = 10.0;
        for (i, c) in closes.iter().enumerate() {
            data.push(bar(i, last, *c));
            last = *c;
        }
        let s = Strategy::Donchian { entry: 3, exit: 3 };
        let sim = simulate(&data, &s, Costs { fees: Some(DEFAULT_FEES), slippage_bps: 0.0, position_pct: 100.0 }, 100_000.0, None, None).unwrap();
        assert_eq!(sim.trades.len(), 1);
        let t = &sim.trades[0];
        assert_eq!((t.entry_date.as_str(), t.exit_date.as_str(), t.shares, t.entry_price, t.exit_price), ("2026-01-06", "2026-01-09", 9000.0, 11.0, 8.0));
        // 买入费 99000×0.025% = 24.75 + 过户 0.99 = 25.74;卖出费 72000×0.025% = 18 + 0.72 + 印花税 36 = 54.72
        assert!((t.fees - 80.46).abs() < 1e-6, "{}", t.fees);
        assert!((t.pnl - (-27_000.0 - 80.46)).abs() < 1e-6, "{}", t.pnl);
        // 平仓后现金 = 100000 - 99025.74 + 72000 - 54.72
        let last_equity = sim.equity.last().unwrap().1;
        assert!((last_equity - (100_000.0 - 27_080.46)).abs() < 1e-6, "{last_equity}");
        assert_eq!(sim.equity.len(), 6);
    }

    #[test]
    fn stop_loss_exits_next_open_and_is_labelled() {
        // 横盘 60 根,突破 11 进场,冲到 13 后缓慢回落到 10.2:不会触发通道出场(前 50 日最低是 10),但跌破成本 5% 就止损
        let mut closes = vec![10.0; 60];
        closes.extend([11.0, 12.0, 13.0, 12.0, 11.5, 11.0, 10.6, 10.3, 10.2, 10.2]);
        let mut data = Vec::new();
        let mut last = 10.0;
        for (i, c) in closes.iter().enumerate() {
            data.push(bar(i, last, *c));
            last = *c;
        }
        let s = Strategy::Donchian { entry: 5, exit: 50 };
        let with_stop = simulate(&data, &s, NO_COST, 100_000.0, Some(5.0), None).unwrap();
        assert_eq!(with_stop.trades.len(), 1);
        assert_eq!((with_stop.trades[0].reason, with_stop.trades[0].exit_price), ("止损", 10.3), "收盘 10.3 跌破 10.45 → 下一根开盘(= 10.3)出场");
        let no_stop = simulate(&data, &s, NO_COST, 100_000.0, None, None).unwrap();
        assert!(no_stop.trades.is_empty() && no_stop.open_shares > 0.0, "不止损就一直拿着");
        assert!(with_stop.equity.last().unwrap().1 > no_stop.equity.last().unwrap().1);
    }

    #[test]
    fn limit_up_blocks_the_buy_and_limit_down_blocks_the_sell_then_retry_next_open() {
        // 横盘 → 下标 60 收盘 11 突破;61 号开盘 12.1 = 涨停价,买不进;62 号开盘 12.0 买进
        // 63 号收盘 10.5 跌破前 3 日最低 11 → 64 号开盘 9.45 = 跌停价(10.5 × 0.9),卖不出;65 号开盘 9.4 卖出
        let mut closes = vec![10.0; 60];
        closes.extend([11.0, 12.1, 12.0, 10.5, 9.5, 9.4, 9.4]);
        let mut opens = closes.clone();
        for i in 1..closes.len() {
            opens[i] = closes[i - 1];
        }
        opens[61] = 12.1;
        opens[62] = 12.0;
        opens[64] = 9.45;
        opens[65] = 9.4;
        let data: Vec<Bar> = closes.iter().zip(&opens).enumerate().map(|(i, (c, o))| bar(i, *o, *c)).collect();
        let s = Strategy::Donchian { entry: 5, exit: 3 };
        let on = simulate(&data, &s, NO_COST, 100_000.0, None, Some(0.10)).unwrap();
        assert_eq!((on.blocked_buys, on.blocked_sells), (1, 1));
        assert_eq!(on.trades.len(), 1);
        let t = &on.trades[0];
        assert_eq!((t.entry_date.as_str(), t.entry_price, t.exit_date.as_str(), t.exit_price), ("2026-01-63", 12.0, "2026-01-66", 9.4));
        let off = simulate(&data, &s, NO_COST, 100_000.0, None, None).unwrap();
        assert_eq!((off.blocked_buys, off.blocked_sells), (0, 0));
        let t = &off.trades[0];
        assert_eq!((t.entry_price, t.exit_price), (12.1, 9.45), "不设限时一字涨停也买得进、跌停也卖得出(偏乐观)");
    }

    #[test]
    fn board_limits_follow_the_exchange_rules() {
        for (code, pct) in [("600519", 0.10), ("000001", 0.10), ("002594", 0.10), ("300750", 0.20), ("301000", 0.20), ("688981", 0.20), ("830799", 0.30), ("920575", 0.30)] {
            assert_eq!(board_limit(code), pct, "{code}");
        }
    }

    #[test]
    fn cash_below_one_lot_is_skipped_not_faked() {
        let data = rising();
        let s = Strategy::SmaCross { fast: 5, slow: 20 };
        let sim = simulate(&data, &s, NO_COST, 500.0, None, None).unwrap();
        assert!(sim.open_shares == 0.0 && sim.skipped_cash > 0, "500 元买不起一手 10 元以上的票");
        assert!(sim.equity.iter().all(|(_, v)| (*v - 500.0).abs() < 1e-9));
    }

    #[test]
    fn parameters_are_validated() {
        assert!(parse_strategy(&json!({"fast": 30, "slow": 20})).unwrap_err().to_string().contains("fast 必须小于 slow"));
        assert!(parse_strategy(&json!({"strategy": "martingale"})).unwrap_err().to_string().contains("不认识的策略"));
        assert!(parse_strategy(&json!({"strategy": "rsi", "buy": 60})).is_err());
        assert_eq!(parse_strategy(&json!({"strategy": "donchian"})).unwrap(), Strategy::Donchian { entry: 20, exit: 10 });
        assert!(parse_costs(&json!({"fees": {"commission_rate": 0.0003}})).is_err(), "自定义费率四项要齐");
        assert!(parse_costs(&json!({"fees": {"commission_rate": 2.5, "commission_min": 5, "stamp_tax_rate": 0.0005, "transfer_fee_rate": 0.00001}})).unwrap_err().to_string().contains("不像费率"));
        assert!(parse_costs(&json!({"slippage_bps": 500})).is_err());
        let (c, d) = parse_costs(&json!({})).unwrap();
        assert!(c.fees.is_some() && d["mode"] == "default", "默认计费用");
        assert!(parse_costs(&json!({"fees": "none"})).unwrap().0.fees.is_none());
        assert!(parse_symbols(&json!({})).unwrap_err().to_string().contains("不能为空"));
        assert!(parse_symbols(&json!({"symbols": ["600519", "000001", "300750", "688981", "002594", "601318"]})).unwrap_err().to_string().contains("最多"));
        assert_eq!(parse_symbols(&json!({"symbol": "600519", "symbols": ["600519", "000001"]})).unwrap().len(), 2, "重复代码只算一只");
    }

    fn key(code: &str) -> crate::symbol::SymbolKey {
        normalize_stock_symbol(code).unwrap()
    }

    #[test]
    fn result_has_costs_trades_and_aligned_portfolio() {
        let data = rising();
        let s = Strategy::SmaCross { fast: 5, slow: 20 };
        let single = build_result(&[(key("600519"), data.clone())], &s, Costs { fees: Some(DEFAULT_FEES), slippage_bps: 0.0, position_pct: 100.0 }, json!({"mode": "default"}), 100_000.0, None, true, false, false).unwrap();
        assert_eq!(single["strategy"]["id"], "sma_cross");
        assert!(single["metrics"]["total_fees"].as_f64().unwrap() > 0.0);
        assert!(single["note"].as_str().unwrap().contains("默认值"));
        assert_eq!(single["experiment"]["rows"].as_array().unwrap().len(), 6, "80 根 K 线跑不了 120 日慢线,那三格直接略过(不硬凑)");
        assert_eq!(single["experiment"]["rows"].as_array().unwrap().iter().filter(|r| r["is_current"] == true).count(), 0, "5/20 不在固定格里,没有标记当前");
        // 两只:第二只少一根头部 K 线 → 只留共有日期,权益逐日相加
        let two = build_result(&[(key("600519"), data.clone()), (key("000001"), data[1..].to_vec())], &s, NO_COST, json!({"mode": "none"}), 100_000.0, None, false, false, false).unwrap();
        assert_eq!(two["symbols"].as_array().unwrap().len(), 2);
        assert_eq!(two["period"]["bars"], 79);
        assert_eq!(two["per_symbol"].as_array().unwrap().len(), 2);
        let first = &two["equity_curve"][0];
        assert!((first[1].as_f64().unwrap() - 100_000.0).abs() < 1e-6, "进场前权益 = 本金");
        assert!(build_result(&[(key("600519"), data.clone()), (key("000001"), data)], &s, NO_COST, json!({}), 100_000.0, None, false, false, false).is_ok());
    }

    fn series(values: &[(&str, f64)]) -> Vec<(String, f64)> {
        values.iter().map(|(d, v)| (d.to_string(), *v)).collect()
    }

    #[test]
    fn monthly_table_chains_month_ends_and_pairs_the_benchmark() {
        let curve = series(&[("2026-01-02", 100_000.0), ("2026-01-30", 110_000.0), ("2026-02-02", 110_000.0), ("2026-02-27", 99_000.0)]);
        let bench = series(&[("2026-01-02", 100_000.0), ("2026-01-30", 105_000.0), ("2026-02-02", 105_000.0), ("2026-02-27", 107_100.0)]);
        let t = monthly_table(&curve, &bench, 100_000.0);
        assert_eq!(t.len(), 2);
        assert_eq!((t[0]["month"].as_str(), t[0]["return_pct"].as_f64(), t[0]["bench_pct"].as_f64()), (Some("2026-01"), Some(10.0), Some(5.0)));
        assert_eq!((t[1]["return_pct"].as_f64(), t[1]["bench_pct"].as_f64()), (Some(-10.0), Some(2.0)), "二月从一月最后一天的权益算起");
    }

    #[test]
    fn drawdown_episodes_list_depth_dates_and_unrecovered_tail() {
        let curve = series(&[("d0", 100.0), ("d1", 110.0), ("d2", 99.0), ("d3", 95.0), ("d4", 111.0), ("d5", 120.0), ("d6", 100.0)]);
        let eps = drawdown_episodes(&curve, 100.0, 3);
        assert_eq!(eps.len(), 2);
        assert_eq!((eps[0]["peak_date"].as_str(), eps[0]["trough_date"].as_str(), eps[0]["depth_pct"].as_f64()), (Some("d5"), Some("d6"), Some(-16.67)), "最深的一段在前");
        assert!(eps[0]["recover_date"].is_null(), "到最后还没恢复");
        assert_eq!((eps[0]["bars_to_trough"].as_i64(), eps[0]["bars_total"].as_i64()), (Some(1), Some(1)));
        assert_eq!((eps[1]["peak_date"].as_str(), eps[1]["trough_date"].as_str(), eps[1]["recover_date"].as_str(), eps[1]["depth_pct"].as_f64()), (Some("d1"), Some("d3"), Some("d4"), Some(-13.64)));
        assert_eq!((eps[1]["bars_to_trough"].as_i64(), eps[1]["bars_total"].as_i64()), (Some(2), Some(3)));
        // 一直创新高 = 没有回撤;从期初就亏 = 前高是本金
        assert!(drawdown_episodes(&series(&[("a", 101.0), ("b", 102.0)]), 100.0, 3).is_empty());
        let start = drawdown_episodes(&series(&[("a", 90.0), ("b", 100.0)]), 100.0, 3);
        assert_eq!((start[0]["peak_date"].as_str(), start[0]["depth_pct"].as_f64(), start[0]["recover_date"].as_str()), (Some("期初"), Some(-10.0), Some("b")));
    }

    #[test]
    fn position_pct_limits_how_much_cash_goes_in() {
        let data = rising();
        let s = Strategy::SmaCross { fast: 5, slow: 20 };
        let full = simulate(&data, &s, NO_COST, 100_000.0, None, None).unwrap();
        let half = simulate(&data, &s, Costs { fees: None, slippage_bps: 0.0, position_pct: 50.0 }, 100_000.0, None, None).unwrap();
        assert!(half.open_shares > 0.0 && half.open_shares % 100.0 == 0.0);
        assert!(half.open_shares <= full.open_shares / 2.0 + 100.0 && half.open_shares >= full.open_shares / 2.0 - 100.0, "{} vs {}", half.open_shares, full.open_shares);
        assert!(half.equity.last().unwrap().1 < full.equity.last().unwrap().1, "上涨行情里半仓赚得少");
        assert!(parse_costs(&json!({"position_pct": 5})).is_err() && parse_costs(&json!({"position_pct": 120})).is_err());
        assert_eq!(parse_costs(&json!({"position_pct": 60})).unwrap().0.position_pct, 60.0);
    }

    fn wavy(n: usize) -> Vec<Bar> {
        let mut out = Vec::new();
        let mut last = 10.0;
        for i in 0..n {
            let close = 10.0 + 0.01 * i as f64 + (i as f64 / 8.0).sin();
            out.push(bar(i, last, close));
            last = close;
        }
        out
    }

    #[test]
    fn optimize_splits_in_and_out_of_sample_and_flags_exactly_one_best() {
        let data = wavy(260);
        let s = Strategy::SmaCross { fast: 10, slow: 30 };
        let v = optimize_value(&data, &s, NO_COST, 100_000.0, None, None);
        assert_eq!(v["available"], true, "{v}");
        let (is_bars, oos_bars) = (v["in_sample"]["bars"].as_u64().unwrap(), v["out_of_sample"]["bars"].as_u64().unwrap());
        assert_eq!(is_bars + oos_bars, 260);
        assert!(oos_bars > 0 && (is_bars as f64) > 0.65 * 260.0 - 40.0);
        let rows = v["rows"].as_array().unwrap();
        assert!(!rows.is_empty() && rows.iter().filter(|r| r["best_in_sample"] == true).count() == 1);
        let best = &v["best_in_sample"];
        assert!(best["oos_rank"].as_u64().unwrap() >= 1 && best["oos_rank"].as_u64().unwrap() <= best["of"].as_u64().unwrap());
        let best_row = rows.iter().find(|r| r["best_in_sample"] == true).unwrap();
        assert!(rows.iter().all(|r| r["is_return_pct"].as_f64().unwrap() <= best_row["is_return_pct"].as_f64().unwrap()), "样本内收益最高的那一格才是 best");
        assert!(v["note"].as_str().unwrap().contains("拟合"));
        // 样本外第一天就是切分点之后
        assert_eq!(v["out_of_sample"]["start"].as_str().unwrap(), data[is_bars as usize].date);
        // 太短的 K 线明说不够切
        let short = optimize_value(&data[..100], &s, NO_COST, 100_000.0, None, None);
        assert_eq!(short["available"], false);
        assert!(short["reason"].as_str().unwrap().contains("不够切成样本内/外"));
    }

    #[test]
    fn result_carries_monthly_drawdowns_and_optional_oos_and_rejects_multi_symbol_oos() {
        let data = wavy(260);
        let s = Strategy::SmaCross { fast: 10, slow: 30 };
        let out = build_result(&[(key("600519"), data.clone())], &s, NO_COST, json!({}), 100_000.0, None, false, false, true).unwrap();
        assert!(out["monthly"].as_array().is_some_and(|m| !m.is_empty()) && out["drawdowns"].as_array().is_some());
        assert_eq!(out["optimize"]["available"], true);
        assert!(out["metrics"]["max_drawdown_pct"].as_f64().unwrap() <= 0.0);
        let plain = build_result(&[(key("600519"), data)], &s, NO_COST, json!({}), 100_000.0, None, false, false, false).unwrap();
        assert!(plain.get("optimize").is_none());
    }

    #[tokio::test]
    async fn multi_symbol_requests_for_experiment_or_oos_are_rejected_before_any_network_call() {
        let client = Client::new();
        for flag in ["optimize", "experiment"] {
            let err = run(&client, &json!({"symbols": ["600519", "000001"], flag: true})).await.unwrap_err().to_string();
            assert!(err.contains("只支持单只股票"), "{err}");
        }
    }

    #[test]
    fn short_history_is_rejected_per_symbol() {
        let s = Strategy::SmaCross { fast: 5, slow: 20 };
        let err = build_result(&[(key("600519"), rising()[..10].to_vec())], &s, NO_COST, json!({}), 100_000.0, None, false, false, false).unwrap_err().to_string();
        assert!(err.contains("600519") && err.contains("K 线不足"), "{err}");
    }

    /// 对真实东财日线跑一遍(联网,默认忽略):cargo test live_backtest_probe -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_backtest_probe() {
        let client = Client::builder().timeout(std::time::Duration::from_secs(20)).user_agent("Mozilla/5.0 VibeAStock-Mobile/test").build().unwrap();
        let body = json!({"symbols": ["600519", "000001"], "strategy": "donchian", "bars": 400, "slippage_bps": 5, "stop_loss_pct": 8});
        let v = run(&client, &body).await.unwrap();
        println!("{}", serde_json::to_string_pretty(&v["metrics"]).unwrap());
        assert!(v["metrics"]["total_fees"].as_f64().unwrap() >= 0.0);
        let one = run(&client, &json!({"symbol": "600519", "experiment": true})).await.unwrap();
        println!("{}", serde_json::to_string_pretty(&one["experiment"]["rows"]).unwrap());
        assert!(!one["experiment"]["rows"].as_array().unwrap().is_empty());
    }
}
