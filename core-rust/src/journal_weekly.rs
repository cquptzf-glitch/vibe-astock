//! Calendar-week journal summary, computed locally from the same settlement as portfolio/risk.
use anyhow::{anyhow, Result};
use chrono::{Datelike, Duration, NaiveDate};
use serde_json::{json, Value};
use std::path::Path;

pub fn view(db: &Path, path: &str) -> Result<Value> {
    let today = crate::emotion::now_china().0;
    let end = crate::discovery::parameter(path, "date")
        .filter(|s| !s.is_empty())
        .map(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d"))
        .transpose()?
        .unwrap_or(today);
    if end > today {
        return Err(anyhow!("周报日期不能在未来"));
    }
    let start = (end - Duration::days(end.weekday().num_days_from_monday() as i64)).to_string();
    let end = end.to_string();
    let data = crate::journal_risk::analysis(db)?;
    let mut buys = 0;
    let mut sells = 0;
    let mut openings = 0;
    let mut splits = 0;
    let (mut buy_amount, mut sell_amount) = (0.0, 0.0);
    for row in &data.rows {
        for f in &row.e.fills {
            if f.date < start || f.date > end {
                continue;
            }
            match f.side.as_str() {
                "buy" => {
                    buys += 1;
                    buy_amount += f.price * f.shares;
                }
                "sell" => {
                    sells += 1;
                    sell_amount += f.price * f.shares;
                }
                "opening" => openings += 1,
                "split" => splits += 1,
                _ => {}
            }
        }
    }
    let closed: Vec<_> = data
        .rows
        .iter()
        .filter(|r| {
            r.closed
                && r.last_sell
                    .as_ref()
                    .is_some_and(|d| d >= &start && d <= &end)
        })
        .collect();
    let planned = closed
        .iter()
        .filter(|r| r.e.as_planned == Some(true))
        .count();
    let unplanned = closed
        .iter()
        .filter(|r| r.e.as_planned == Some(false))
        .count();
    Ok(
        json!({"start":start,"end":end,"buys":buys,"sells":sells,"openings":openings,"splits":splits,
        "buy_amount":buy_amount,"sell_amount":sell_amount,"closed":closed.len(),"planned":planned,"unplanned":unplanned,
        "performance":crate::journal_risk::bucket(&closed),
        "note":"本周一至所选日期；买卖金额不含费用，期初/送转不计成交额。盈亏与纪律仅统计本周清仓的 A 股轮次，按成本法结算；ETF 等不计入。"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn week_boundary_and_split_cash_flows() {
        let root = std::env::temp_dir().join(format!(
            "weekly-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let _app = crate::AppCore::new(root.clone()).unwrap();
        let db = _app.db_path().to_path_buf();
        crate::journal::act(
            &db,
            &json!({"action":"record","symbol":"600519","as_planned":true,"fills":[
            {"type":"opening","date":"2026-10-09","price":10,"shares":100},
            {"type":"split","date":"2026-10-12","price":2,"shares":1},
            {"type":"sell","date":"2026-10-14","price":6,"shares":200,"fee":0}]}),
        )
        .unwrap();
        let report = view(&db, "/?date=2026-10-16").unwrap();
        assert_eq!(report["start"], "2026-10-12");
        assert_eq!(report["buy_amount"], 0.0);
        assert_eq!(report["sell_amount"], 1200.0);
        assert_eq!(report["performance"]["net_pnl"], 200.0);
        assert_eq!(report["planned"], 1);
    }
}
