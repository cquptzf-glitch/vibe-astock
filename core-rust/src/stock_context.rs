use crate::{
    discovery::{self, DiscoveryError},
    symbol::{normalize_stock_symbol, SymbolKey},
};
use anyhow::{anyhow, Result};
use chrono::{FixedOffset, NaiveDate, NaiveDateTime, Utc};
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(crate) fn symbol(path: &str) -> Result<SymbolKey, DiscoveryError> {
    normalize_stock_symbol(&discovery::parameter(path, "symbol").unwrap_or_default())
        .map_err(|e| DiscoveryError::BadRequest(e.to_string()))
}

pub(crate) fn today() -> NaiveDate {
    Utc::now()
        .with_timezone(&FixedOffset::east_opt(28800).unwrap())
        .date_naive()
}

pub(crate) fn provider_data<'a>(root: &'a Value, key: &SymbolKey) -> Result<&'a Value> {
    if root
        .get("rc")
        .and_then(Value::as_i64)
        .is_some_and(|v| v != 0)
    {
        return Err(anyhow!("行情源业务错误"));
    }
    let data = root
        .get("data")
        .filter(|v| v.is_object())
        .ok_or_else(|| anyhow!("缺少数据"))?;
    if data.get("code").and_then(Value::as_str) != Some(key.digits.as_str()) {
        return Err(anyhow!("行情源股票代码不匹配"));
    }
    if let Some(market) = data.get("market").and_then(Value::as_i64) {
        let expected = key.eastmoney.split('.').next().unwrap_or("");
        if market.to_string() != expected {
            return Err(anyhow!("行情源市场不匹配"));
        }
    }
    Ok(data)
}

pub async fn fund_flow(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let key = symbol(path)?;
    let mode = discovery::parameter(path, "mode").unwrap_or_else(|| "daily".into());
    if !["daily", "minute"].contains(&mode.as_str()) {
        return Err(DiscoveryError::BadRequest(
            "mode 必须是 daily 或 minute".into(),
        ));
    }
    let limit = discovery::integer(path, "limit", 60, 5, 120)?;
    let minute = mode == "minute";
    let url = if minute {
        "https://push2.eastmoney.com/api/qt/stock/fflow/kline/get"
    } else {
        "https://push2his.eastmoney.com/api/qt/stock/fflow/daykline/get"
    };
    let params = vec![
        ("secid", key.eastmoney.clone()),
        ("klt", if minute { "1" } else { "101" }.into()),
        ("fields1", "f1,f2,f3,f7".into()),
        ("fields2", "f51,f52,f53,f54,f55,f56,f57".into()),
        ("lmt", limit.to_string()),
    ];
    let root = discovery::get_json(client, url, &params).await?;
    Ok(parse_flow(&root, &key, minute, limit, today())?)
}

fn parse_flow(
    root: &Value,
    key: &SymbolKey,
    minute: bool,
    limit: usize,
    today: NaiveDate,
) -> Result<Value> {
    let data = provider_data(root, key)?;
    let raw = data
        .get("klines")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("缺少资金流序列"))?;
    let mut ordered = BTreeMap::new();
    for value in raw {
        let line = value.as_str().ok_or_else(|| anyhow!("资金流行格式错误"))?;
        let fields: Vec<_> = line.split(',').collect();
        if fields.len() < 6 {
            return Err(anyhow!("资金流字段不足"));
        }
        let stamp = fields[0];
        let day = if minute {
            NaiveDateTime::parse_from_str(stamp, "%Y-%m-%d %H:%M").map(|v| v.date())?
        } else {
            NaiveDate::parse_from_str(stamp, "%Y-%m-%d")?
        };
        if day > today {
            return Err(anyhow!("资金流包含未来日期"));
        }
        let values = fields[1..6]
            .iter()
            .map(|s| discovery::number(Some(&json!(s))))
            .collect::<Vec<_>>();
        let row = json!({"time": stamp, "date": day.to_string(), "main_net": values[0],
            "small_net": values[1], "mid_net": values[2], "large_net": values[3], "super_net": values[4]});
        if let Some(previous) = ordered.insert(stamp.to_string(), row.clone()) {
            if previous != row {
                return Err(anyhow!("资金流时间戳冲突"));
            }
        }
    }
    let mut rows = ordered.into_values().collect::<Vec<_>>();
    if minute && rows.first().map(|v| &v["date"]) != rows.last().map(|v| &v["date"]) {
        return Err(anyhow!("分钟资金流跨越多个日期，不能解释为同日累计"));
    }
    if !minute && rows.len() > limit {
        rows.drain(..rows.len() - limit);
    }
    let latest = rows.last().cloned().unwrap_or(Value::Null);
    let missing = rows.iter().any(|v| {
        ["main_net", "small_net", "mid_net", "large_net", "super_net"]
            .iter()
            .any(|f| v[f].is_null())
    });
    let asof_date = latest["date"].as_str();
    let stale = asof_date.is_some_and(|v| v != today.to_string());
    Ok(
        json!({"status": if rows.is_empty() { "empty" } else if missing { "partial" } else { "ok" },
        "symbol": key.digits, "market": key.market.as_str(), "mode": if minute { "minute" } else { "daily" },
        "rows": rows, "latest": latest, "count": rows.len(), "stale": stale, "source": "eastmoney", "unit": "元",
        "method": if minute { "各分钟点为当日累计净额，不可逐分钟相加" } else { "逐交易日净额，不是逐笔成交或个人资金" },
        "fetched_at": Utc::now().to_rfc3339()}),
    )
}

pub async fn stock_boards(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let key = symbol(path)?;
    let params = vec![
        ("secid", key.eastmoney.clone()),
        ("spt", "3".into()),
        ("pi", "0".into()),
        ("pz", "200".into()),
        ("po", "1".into()),
        ("fltt", "2".into()),
        ("invt", "2".into()),
        ("fields", "f12,f14,f3,f128".into()),
    ];
    let root = discovery::get_json(
        client,
        "https://push2.eastmoney.com/api/qt/slist/get",
        &params,
    )
    .await?;
    Ok(parse_boards(&root, &key)?)
}

fn parse_boards(root: &Value, key: &SymbolKey) -> Result<Value> {
    let raw = discovery::provider_rows(root)?;
    let total = root
        .pointer("/data/total")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("板块数量缺失"))?;
    let mut boards = BTreeMap::new();
    let mut dropped = 0;
    for value in raw {
        let code = value["f12"].as_str().unwrap_or("");
        let name = value["f14"].as_str().unwrap_or("").trim();
        if code.len() != 6
            || !code.starts_with("BK")
            || !code[2..].bytes().all(|b| b.is_ascii_digit())
            || name.is_empty()
        {
            dropped += 1;
            continue;
        }
        boards.insert(
            code.to_string(),
            json!({"code":code,"name":name,
            "change_pct":discovery::number(value.get("f3"))}),
        );
    }
    if total > 0 && boards.is_empty() {
        return Err(anyhow!("所属板块无可用记录"));
    }
    let incomplete = (boards.len() as u64) < total || dropped > 0;
    Ok(
        json!({"status": if incomplete { "partial" } else if boards.is_empty() { "empty" } else { "ok" },
        "symbol":key.digits,"market":key.market.as_str(),"items":boards.into_values().collect::<Vec<_>>(),
        "total":total,"dropped":dropped,"truncated":incomplete,"source":"eastmoney",
        "note":"来源含行业、概念、地区及指数分组；展示本次返回的关联，不代表完整分类体系。",
        "fetched_at":Utc::now().to_rfc3339()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key() -> SymbolKey {
        normalize_stock_symbol("600519").unwrap()
    }
    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 16).unwrap()
    }
    #[test]
    fn public_daily_fixture_preserves_units_and_latest() {
        let root =
            serde_json::from_str(include_str!("../tests/fixtures/market_daily_flow.json")).unwrap();
        let value = parse_flow(&root, &key(), false, 60, day()).unwrap();
        assert_eq!(value["count"], 5);
        assert_eq!(value["latest"]["main_net"], 468661168.0);
        assert_eq!(value["latest"]["super_net"], 43617040.0);
        assert_eq!(value["unit"], "元");
    }
    #[test]
    fn minute_is_cumulative_not_sum_and_old_dates_remain_visible() {
        let root =
            serde_json::from_str(include_str!("../tests/fixtures/market_minute.json")).unwrap();
        let value = parse_flow(&root, &key(), true, 60, day().succ_opt().unwrap()).unwrap();
        assert_eq!(value["latest"]["main_net"], 468661172.0);
        assert_eq!(value["count"], 3);
        assert_eq!(value["stale"], true);
    }
    #[test]
    fn missing_latest_does_not_fall_back_or_become_zero() {
        // Freeze legacy test_audit_round3_sources.py::test_b3_fund_flow_latest_dash_is_missing_not_zero.
        let root = json!({"data":{"code":"600519","klines":["2026-10-08,100,1,2,3,4,5","2026-10-09,-,0,NaN,Inf,-,0"]}});
        let value = parse_flow(&root, &key(), false, 60, day()).unwrap();
        assert!(value["latest"]["main_net"].is_null());
        assert_eq!(value["latest"]["small_net"], 0.0);
        assert_eq!(value["status"], "partial");
    }
    #[test]
    fn rejects_wrong_identity_conflicts_future_and_cross_day_minutes() {
        for data in [
            json!({"code":"000001","klines":[]}),
            json!({"code":"600519","market":0,"klines":[]}),
            json!({"code":"600519","klines":["2026-10-09,1,2,3,4,5","2026-10-09,9,2,3,4,5"]}),
            json!({"code":"600519","klines":["2099-10-09,1,2,3,4,5"]}),
        ] {
            assert!(parse_flow(&json!({"data":data}), &key(), false, 60, day()).is_err());
        }
        let data = json!({"code":"600519","klines":["2026-10-08 15:00,1,2,3,4,5","2026-10-09 09:31,1,2,3,4,5"]});
        assert!(parse_flow(&json!({"data":data}), &key(), true, 60, day()).is_err());
        assert_eq!(
            parse_flow(
                &json!({"data":{"code":"600519","klines":[]}}),
                &key(),
                false,
                60,
                day()
            )
            .unwrap()["status"],
            "empty"
        );
        assert!(parse_flow(
            &json!({"rc":1,"data":{"code":"600519","klines":[]}}),
            &key(),
            false,
            60,
            day()
        )
        .is_err());
    }
    #[test]
    fn reverse_boards_fixture_preserves_missing_and_warns_about_truncation() {
        let mut root: Value =
            serde_json::from_str(include_str!("../tests/fixtures/market_boards.json")).unwrap();
        let value = parse_boards(&root, &key()).unwrap();
        assert_eq!(value["items"].as_array().unwrap().len(), 30);
        assert!(value["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["code"] == "BK1686" && v["change_pct"].is_null()));
        root["data"]["total"] = json!(100);
        assert_eq!(parse_boards(&root, &key()).unwrap()["status"], "partial");
        assert!(parse_boards(
            &json!({"data":{"total":1,"diff":[{"f12":"javascript:evil","f14":"bad"}]}}),
            &key()
        )
        .is_err());
    }
}
