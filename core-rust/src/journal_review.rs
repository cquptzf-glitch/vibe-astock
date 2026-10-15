use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

const PLAYBOOKS: [&str; 6] = ["打板", "低吸", "接力", "半路", "套利", "其它"];
const COMMISSION_RATE: f64 = 0.00025;
const COMMISSION_MIN: f64 = 5.0;
const STAMP_TAX_RATE: f64 = 0.0005;
const TRANSFER_FEE_RATE: f64 = 0.00001;

pub fn view(db_path: &Path) -> Result<Value> {
    let conn = Connection::open(db_path).context("open mobile journal review sqlite")?;
    let mut stmt = conn.prepare(
        "SELECT id, kind, symbol, name, date, playbook, as_planned,
                planned_stop, planned_target, pnl_pct
         FROM journal_entries
         ORDER BY date DESC, id DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Entry {
            id: row.get(0)?,
            kind: row.get(1)?,
            symbol: row.get(2)?,
            name: row.get(3)?,
            date: row.get(4)?,
            playbook: row.get(5)?,
            as_planned: row.get::<_, Option<i64>>(6)?.map(|value| value != 0),
            planned_stop: row.get(7)?,
            planned_target: row.get(8)?,
            pnl_pct: row.get(9)?,
        })
    })?;

    let mut overall = Stats::default();
    let mut recent = Stats::default();
    let mut recent_left = 10usize;
    let mut planned = Stats::default();
    let mut unplanned = Stats::default();
    let mut planning_unknown = 0usize;
    let mut playbook_stats: BTreeMap<String, Stats> = BTreeMap::new();
    let mut positions = Vec::new();
    let mut open_cost_total = 0.0f64;
    let mut planned_risk_total = 0.0f64;
    let mut stop_covered = 0usize;
    let mut without_stop = 0usize;

    for row in rows {
        let entry = row?;
        let metrics = if entry.kind == "episode" {
            episode_metrics(&conn, entry.id)?
        } else {
            EpisodeMetrics {
                pnl_pct: entry.pnl_pct,
                realized_pnl: None,
                closed: entry.pnl_pct.is_some(),
                open_shares: 0.0,
                avg_cost: None,
                open_cost: None,
            }
        };

        overall.observe(metrics.pnl_pct, metrics.realized_pnl);
        if metrics.pnl_pct.is_some() && recent_left > 0 {
            recent.observe(metrics.pnl_pct, metrics.realized_pnl);
            recent_left -= 1;
        }

        match entry.as_planned {
            Some(true) => planned.observe(metrics.pnl_pct, metrics.realized_pnl),
            Some(false) => unplanned.observe(metrics.pnl_pct, metrics.realized_pnl),
            None => planning_unknown += 1,
        }

        if let Some(playbook) = entry.playbook.as_deref() {
            playbook_stats
                .entry(playbook.to_string())
                .or_default()
                .observe(metrics.pnl_pct, metrics.realized_pnl);
        }

        if entry.kind == "episode" && !metrics.closed && metrics.open_shares > 0.0 {
            let open_cost = metrics.open_cost.unwrap_or(0.0);
            open_cost_total += open_cost;
            let (risk_amount, risk_pct) = match (metrics.avg_cost, entry.planned_stop) {
                (Some(avg_cost), Some(stop)) if avg_cost > 0.0 => {
                    let per_share = (avg_cost - stop).max(0.0);
                    stop_covered += 1;
                    let amount = per_share * metrics.open_shares;
                    planned_risk_total += amount;
                    (Some(round2(amount)), Some(round2(per_share / avg_cost * 100.0)))
                }
                (_, Some(_)) => {
                    stop_covered += 1;
                    (Some(0.0), Some(0.0))
                }
                _ => {
                    without_stop += 1;
                    (None, None)
                }
            };
            positions.push(json!({
                "id": entry.id,
                "symbol": entry.symbol,
                "name": entry.name,
                "date": entry.date,
                "open_shares": round2(metrics.open_shares),
                "avg_cost": metrics.avg_cost.map(round2),
                "open_cost": metrics.open_cost.map(round2),
                "planned_stop": entry.planned_stop,
                "planned_target": entry.planned_target,
                "risk_amount": risk_amount,
                "risk_pct": risk_pct
            }));
        }
    }

    positions.sort_by(|a, b| {
        let av = a.get("open_cost").and_then(Value::as_f64).unwrap_or(0.0);
        let bv = b.get("open_cost").and_then(Value::as_f64).unwrap_or(0.0);
        bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
    });

    let open_count = positions.len();
    let playbooks = PLAYBOOKS
        .iter()
        .filter_map(|name| playbook_stats.get(*name).map(|stats| stats.to_json(Some(name))))
        .collect::<Vec<_>>();

    Ok(json!({
        "available": true,
        "source": "mobile_sqlite",
        "generated_at": Utc::now().to_rfc3339(),
        "performance": overall.to_json(None),
        "recent_10": recent.to_json(None),
        "discipline": {
            "planned": planned.to_json(Some("按计划")),
            "unplanned": unplanned.to_json(Some("计划外")),
            "unknown": planning_unknown
        },
        "playbooks": playbooks,
        "risk": {
            "open_positions": open_count,
            "open_cost_total": if open_count > 0 { Some(round2(open_cost_total)) } else { None },
            "stop_covered": stop_covered,
            "stop_coverage": if open_count > 0 { Some(stop_covered as f64 / open_count as f64) } else { None },
            "planned_risk_total": if stop_covered > 0 { Some(round2(planned_risk_total)) } else { None },
            "without_stop": without_stop,
            "note": "未平仓成本按全部买入成交的加权均价估算；计划风险按 max(加权均价-计划止损价, 0) × 剩余股数估算，不含滑点与后续费用"
        },
        "positions": positions,
        "notes": [
            "胜率按盈利笔数 / (盈利笔数 + 亏损笔数) 计算，持平不进分母",
            "盈亏比 = 平均盈利% / 平均亏损%的绝对值",
            "盈利因子只使用有成交金额的已结算记录；仅记盈亏%的补记不参与金额口径"
        ]
    }))
}

fn episode_metrics(conn: &Connection, entry_id: i64) -> Result<EpisodeMetrics> {
    let mut stmt = conn.prepare(
        "SELECT side, price, shares, fee
         FROM journal_fills WHERE entry_id = ?1 ORDER BY sort_order ASC, id ASC",
    )?;
    let fills = stmt.query_map(params![entry_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, Option<f64>>(3)?,
        ))
    })?;

    let mut buy_amount = 0.0;
    let mut sell_amount = 0.0;
    let mut buy_shares = 0.0;
    let mut sell_shares = 0.0;
    let mut fees = 0.0;
    for fill in fills {
        let (side, price, shares, fee) = fill?;
        if side == "buy" {
            buy_amount += price * shares;
            buy_shares += shares;
        } else if side == "sell" {
            sell_amount += price * shares;
            sell_shares += shares;
        }
        fees += fee.unwrap_or_else(|| estimate_fee(&side, price, shares));
    }

    let open_shares = (buy_shares - sell_shares).max(0.0);
    let closed = buy_shares > 0.0 && (buy_shares - sell_shares).abs() < 0.000_001;
    let avg_cost = if buy_shares > 0.0 { Some(buy_amount / buy_shares) } else { None };
    let open_cost = avg_cost.map(|cost| cost * open_shares);
    let realized_pnl = if closed { Some(sell_amount - buy_amount - fees) } else { None };
    let pnl_pct = realized_pnl.and_then(|pnl| {
        if buy_amount > 0.0 { Some(pnl / buy_amount * 100.0) } else { None }
    });

    Ok(EpisodeMetrics {
        pnl_pct: pnl_pct.map(round2),
        realized_pnl: realized_pnl.map(round2),
        closed,
        open_shares,
        avg_cost,
        open_cost,
    })
}

fn estimate_fee(side: &str, price: f64, shares: f64) -> f64 {
    let amount = price * shares;
    let mut fee = (amount * COMMISSION_RATE).max(COMMISSION_MIN);
    fee += amount * TRANSFER_FEE_RATE;
    if side == "sell" {
        fee += amount * STAMP_TAX_RATE;
    }
    fee
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[derive(Debug)]
struct Entry {
    id: i64,
    kind: String,
    symbol: String,
    name: String,
    date: String,
    playbook: Option<String>,
    as_planned: Option<bool>,
    planned_stop: Option<f64>,
    planned_target: Option<f64>,
    pnl_pct: Option<f64>,
}

#[derive(Debug)]
struct EpisodeMetrics {
    pnl_pct: Option<f64>,
    realized_pnl: Option<f64>,
    closed: bool,
    open_shares: f64,
    avg_cost: Option<f64>,
    open_cost: Option<f64>,
}

#[derive(Clone, Debug, Default)]
struct Stats {
    trades: usize,
    scored: usize,
    wins: usize,
    losses: usize,
    flats: usize,
    sum_pct: f64,
    win_sum_pct: f64,
    loss_sum_pct: f64,
    best_pct: Option<f64>,
    worst_pct: Option<f64>,
    money_scored: usize,
    money_wins: f64,
    money_losses: f64,
}

impl Stats {
    fn observe(&mut self, pct: Option<f64>, pnl: Option<f64>) {
        self.trades += 1;
        if let Some(value) = pct {
            self.scored += 1;
            self.sum_pct += value;
            self.best_pct = Some(self.best_pct.map_or(value, |old| old.max(value)));
            self.worst_pct = Some(self.worst_pct.map_or(value, |old| old.min(value)));
            if value > 0.0 {
                self.wins += 1;
                self.win_sum_pct += value;
            } else if value < 0.0 {
                self.losses += 1;
                self.loss_sum_pct += value;
            } else {
                self.flats += 1;
            }
        }
        if let Some(value) = pnl {
            self.money_scored += 1;
            if value > 0.0 {
                self.money_wins += value;
            } else if value < 0.0 {
                self.money_losses += value;
            }
        }
    }

    fn to_json(&self, label: Option<&str>) -> Value {
        let directional = self.wins + self.losses;
        let avg_win = if self.wins > 0 { Some(self.win_sum_pct / self.wins as f64) } else { None };
        let avg_loss = if self.losses > 0 { Some(self.loss_sum_pct / self.losses as f64) } else { None };
        let payoff = match (avg_win, avg_loss) {
            (Some(win), Some(loss)) if loss < 0.0 => Some(win / loss.abs()),
            _ => None,
        };
        let profit_factor = if self.money_losses < 0.0 {
            Some(self.money_wins / self.money_losses.abs())
        } else {
            None
        };
        json!({
            "label": label,
            "trades": self.trades,
            "scored": self.scored,
            "wins": self.wins,
            "losses": self.losses,
            "flats": self.flats,
            "win_rate": if directional > 0 { Some(self.wins as f64 / directional as f64) } else { None },
            "avg_pct": if self.scored > 0 { Some(round2(self.sum_pct / self.scored as f64)) } else { None },
            "avg_win_pct": avg_win.map(round2),
            "avg_loss_pct": avg_loss.map(round2),
            "payoff_ratio": payoff.map(round2),
            "best_pct": self.best_pct.map(round2),
            "worst_pct": self.worst_pct.map(round2),
            "money_scored": self.money_scored,
            "net_pnl": if self.money_scored > 0 { Some(round2(self.money_wins + self.money_losses)) } else { None },
            "profit_factor": profit_factor.map(round2)
        })
    }
}
