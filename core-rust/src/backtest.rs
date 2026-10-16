use crate::{quote, symbol::normalize_stock_symbol};
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
struct Bar {
    date: String,
    open: f64,
    close: f64,
}

pub async fn run(client: &Client, body: &Value) -> Result<Value> {
    let raw_symbol = body.get("symbol").and_then(Value::as_str).unwrap_or("");
    if raw_symbol.trim().is_empty() {
        return Err(anyhow!("symbol 不能为空"));
    }
    let symbol = normalize_stock_symbol(raw_symbol)?;

    let fast = body
        .get("fast")
        .and_then(Value::as_u64)
        .unwrap_or(10)
        .clamp(3, 60) as usize;
    let slow = body
        .get("slow")
        .and_then(Value::as_u64)
        .unwrap_or(30)
        .clamp(10, 200) as usize;
    if fast >= slow {
        return Err(anyhow!("fast 必须小于 slow"));
    }
    let bars = body
        .get("bars")
        .and_then(Value::as_u64)
        .unwrap_or(500)
        .clamp((slow + 5) as u64, 1000) as usize;
    let initial = body
        .get("initial_capital")
        .and_then(Value::as_f64)
        .unwrap_or(100_000.0);
    if !initial.is_finite() || initial <= 0.0 || initial > 100_000_000.0 {
        return Err(anyhow!("initial_capital 必须在 0 到 1 亿元之间"));
    }

    let envelope = quote::fetch_kline(client, &symbol, bars).await?;
    let raw_rows = envelope
        .pointer("/extra/rows")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("K 线返回缺少 rows"))?;
    let data = raw_rows.iter().filter_map(parse_bar).collect::<Vec<_>>();
    if data.len() < slow + 2 {
        return Err(anyhow!(
            "有效 K 线不足，至少需要 {} 根，实际 {}",
            slow + 2,
            data.len()
        ));
    }

    let closes: Vec<f64> = data.iter().map(|bar| bar.close).collect();
    let mut cash = initial;
    let mut shares = 0.0;
    let mut entry_price = None;
    let mut closed_trades = Vec::new();
    let mut equity = Vec::with_capacity(data.len() - slow);

    for i in slow..data.len() {
        let current = &data[i];
        if i > slow {
            let previous_fast = sma(&closes[..i], fast).unwrap_or(previous_close(&closes, i));
            let previous_slow = sma(&closes[..i], slow).unwrap_or(previous_close(&closes, i));
            let bullish =
                previous_close(&closes, i) > previous_fast && previous_fast > previous_slow;
            let bearish =
                previous_close(&closes, i) < previous_fast || previous_fast < previous_slow;

            if shares == 0.0 && bullish && current.open > 0.0 {
                shares = cash / current.open;
                cash = 0.0;
                entry_price = Some(current.open);
            } else if shares > 0.0 && bearish && current.open > 0.0 {
                let exit = current.open;
                let invested = entry_price.unwrap_or(exit);
                closed_trades.push((invested, exit, current.date.clone()));
                cash = shares * exit;
                shares = 0.0;
                entry_price = None;
            }
        }
        equity.push(json!([current.date.clone(), cash + shares * current.close]));
    }

    let final_close = data.last().map(|bar| bar.close).unwrap_or(0.0);
    let final_equity = cash + shares * final_close;
    let buy_hold = if data[slow].close > 0.0 {
        initial * final_close / data[slow].close
    } else {
        initial
    };

    let mut peak: f64 = initial;
    let mut max_drawdown: f64 = 0.0;
    for point in &equity {
        if let Some(value) = point.get(1).and_then(Value::as_f64) {
            peak = peak.max(value);
            if peak > 0.0 {
                max_drawdown = max_drawdown.min(value / peak - 1.0);
            }
        }
    }

    let wins = closed_trades
        .iter()
        .filter(|(entry, exit, _)| exit > entry)
        .count();
    let total_return = (final_equity / initial - 1.0) * 100.0;
    let buy_hold_return = (buy_hold / initial - 1.0) * 100.0;

    Ok(json!({
        "status": "ok",
        "symbol": symbol.digits,
        "market": symbol.market.as_str(),
        "strategy": {
            "name": "SMA 趋势交叉 Lite",
            "fast": fast,
            "slow": slow,
            "execution": "信号使用前一交易日收盘，下一交易日开盘成交",
            "fees": "未计手续费、印花税、滑点",
            "adjustment": "前复权日线"
        },
        "period": {
            "start": data.first().map(|bar| bar.date.clone()),
            "end": data.last().map(|bar| bar.date.clone()),
            "bars": data.len()
        },
        "metrics": {
            "initial_capital": initial,
            "final_equity": final_equity,
            "total_return_pct": total_return,
            "buy_hold_return_pct": buy_hold_return,
            "excess_return_pct": total_return - buy_hold_return,
            "max_drawdown_pct": max_drawdown * 100.0,
            "closed_trades": closed_trades.len(),
            "win_rate_pct": if closed_trades.is_empty() {
                Value::Null
            } else {
                json!(wins as f64 / closed_trades.len() as f64 * 100.0)
            },
            "open_position": shares > 0.0,
            "open_shares": if shares > 0.0 { Value::from(shares) } else { Value::Null },
            "open_entry_price": entry_price
        },
        "equity_curve": equity,
        "note": "这是移动端 Lite 回测，用于历史假设核对；不等同于桌面端完整 Agent 回测，也不包含真实交易成本与滑点。"
    }))
}

fn parse_bar(row: &Value) -> Option<Bar> {
    let items = row.as_array()?;
    let date = items.first()?.as_str()?.to_string();
    let open = items.get(1).and_then(number)?;
    let close = items.get(2).and_then(number)?;
    if date.is_empty() || open <= 0.0 || close <= 0.0 {
        return None;
    }
    Some(Bar { date, open, close })
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn previous_close(closes: &[f64], index: usize) -> f64 {
    closes
        .get(index.saturating_sub(1))
        .copied()
        .unwrap_or(0.0)
}

fn sma(values: &[f64], period: usize) -> Option<f64> {
    if values.len() < period || period == 0 {
        return None;
    }
    let tail = &values[values.len() - period..];
    Some(tail.iter().sum::<f64>() / period as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
