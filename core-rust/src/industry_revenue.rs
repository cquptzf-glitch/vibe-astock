//! Taiwan supply-chain monthly revenue; amounts remain TWD and periods are explicit.
use crate::discovery;
use anyhow::{anyhow, Result};
use chrono::Datelike;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::BTreeMap;
const COMPANIES: [(&str, &str); 4] = [
    ("2383", "台光电子"),
    ("6274", "台燿"),
    ("2368", "金像电"),
    ("3081", "联亚光电"),
];
fn previous(y: i32, m: u32) -> (i32, u32) {
    if m == 1 {
        (y - 1, 12)
    } else {
        (y, m - 1)
    }
}
pub(crate) fn parse(root: &Value, code: &str, name: &str) -> Result<Value> {
    let today = crate::emotion::now_china().0;
    let rows = root["data"]
        .as_array()
        .ok_or_else(|| anyhow!("FinMind 缺少 data 数组"))?;
    let mut months: BTreeMap<(i32, u32), f64> = BTreeMap::new();
    for r in rows {
        if r["stock_id"].as_str().is_some_and(|s| s != code) {
            return Err(anyhow!("月营收代码不匹配"));
        }
        let y = r["revenue_year"]
            .as_i64()
            .or_else(|| r["revenue_year"].as_str()?.parse().ok())
            .filter(|y| (2000..=2100).contains(y))
            .ok_or_else(|| anyhow!("营收年份无效"))? as i32;
        let m = r["revenue_month"]
            .as_u64()
            .or_else(|| r["revenue_month"].as_str()?.parse().ok())
            .filter(|m| (1..=12).contains(m))
            .ok_or_else(|| anyhow!("营收月份无效"))? as u32;
        let v = discovery::number(r.get("revenue"))
            .filter(|v| *v > 0.0)
            .ok_or_else(|| anyhow!("营收数值无效"))?;
        if (y, m) > (today.year(), today.month()) {
            return Err(anyhow!("源站返回未来月份"));
        }
        if months.insert((y, m), v).is_some_and(|old| old != v) {
            return Err(anyhow!("同月存在冲突数值"));
        }
    }
    let (&(y, m), &current) = months
        .last_key_value()
        .ok_or_else(|| anyhow!("没有月营收数据"))?;
    let growth = |base: Option<&f64>| base.map(|b| (current / b - 1.0) * 100.0);
    let mom = growth(months.get(&previous(y, m)));
    let yoy = growth(months.get(&(y - 1, m)));
    let complete =
        (1..=m).all(|i| months.contains_key(&(y, i)) && months.contains_key(&(y - 1, i)));
    let cumulative = complete.then(|| {
        let a: f64 = (1..=m).map(|i| months[&(y, i)]).sum();
        let b: f64 = (1..=m).map(|i| months[&(y - 1, i)]).sum();
        (a / b - 1.0) * 100.0
    });
    let lag = (today.year() - y) * 12 + today.month() as i32 - m as i32;
    Ok(
        json!({"ticker":code,"name":name,"period":format!("{y}-{m:02}"),"revenue_twd":current,"mom_pct":mom,"yoy_pct":yoy,"cumulative_yoy_pct":cumulative,"stale":lag>2,"lag_months":lag,
        "series":months.into_iter().map(|((y,m),v)|json!([format!("{y}-{m:02}"),v])).collect::<Vec<_>>(),"missing":{"previous_month":mom.is_none(),"prior_year_month":yoy.is_none(),"cumulative_base":!complete}}),
    )
}
pub async fn fetch(client: &Client) -> Result<Value> {
    let start = format!("{}-01-01", crate::emotion::now_china().0.year() - 2);
    let mut companies = Vec::new();
    let mut errors = Vec::new();
    for (code, name) in COMPANIES {
        let result = discovery::get_json(
            client,
            "https://api.finmindtrade.com/api/v4/data",
            &[
                ("dataset", "TaiwanStockMonthRevenue".into()),
                ("data_id", code.into()),
                ("start_date", start.clone()),
            ],
        )
        .await
        .and_then(|v| parse(&v, code, name));
        match result {
            Ok(row) => companies.push(row),
            Err(e) => errors.push(json!({"ticker":code,"message":e.to_string()})),
        }
    }
    let a = companies.iter().find(|r| r["ticker"] == "2383");
    let b = companies.iter().find(|r| r["ticker"] == "2368");
    let difference = match (a, b) {
        (Some(a), Some(b)) if a["period"] == b["period"] => a["mom_pct"]
            .as_f64()
            .zip(b["mom_pct"].as_f64())
            .map(|(a, b)| a - b),
        _ => None,
    };
    Ok(
        json!({"status":if companies.is_empty(){"failed"}else if !errors.is_empty()||companies.iter().any(|c|c["stale"]==true){"partial"}else{"ok"},"companies":companies,"errors":errors,"ccl_pcb_mom_difference_pp":difference,"source":"FinMind TaiwanStockMonthRevenue","fetched_at":chrono::Utc::now().to_rfc3339(),"note":"台股月营收，金额为新台币；同比/环比与累计同比只用完整且同口径基数。超过两个月标陈旧。台光减金像电仅是同月环比差(百分点)，不能直接归因英伟达/ASIC 需求，也不能推出 A 股公司业绩。"}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_conflicts_and_incomplete_cumulative_bases_are_visible() {
        let mut root = json!({"data":[{"stock_id":"2383","revenue_year":2025,"revenue_month":9,"revenue":100},{"stock_id":"2383","revenue_year":2026,"revenue_month":8,"revenue":120},{"stock_id":"2383","revenue_year":2026,"revenue_month":9,"revenue":150}]});
        let out = parse(&root, "2383", "合成公司").unwrap();
        assert_eq!(out["mom_pct"], 25.0);
        assert_eq!(out["yoy_pct"], 50.0);
        assert!(out["cumulative_yoy_pct"].is_null());
        root["data"]
            .as_array_mut()
            .unwrap()
            .push(json!({"stock_id":"2383","revenue_year":2026,"revenue_month":9,"revenue":160}));
        assert!(parse(&root, "2383", "合成公司").is_err());
    }
}
