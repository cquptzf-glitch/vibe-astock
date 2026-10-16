use crate::{
    discovery::{self, DiscoveryError},
    stock_context,
    symbol::normalize_stock_symbol,
};
use anyhow::{anyhow, Result};
use chrono::{NaiveDate, Utc};
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub async fn fetch(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let page = discovery::integer(path, "page", 1, 1, 100)?;
    let kind = discovery::parameter(path, "kind").unwrap_or_else(|| "anomaly".into());
    let today = stock_context::today();
    match kind.as_str() {
        "anomaly" => {
            let params = vec![
                ("team", "h5".into()),
                ("product", "EastMoney".into()),
                ("client", "WAP".into()),
                ("version", "9001".into()),
                ("name", "WAP".into()),
                ("user", "123".into()),
                ("pageSize", "50".into()),
                ("pageNo", page.to_string()),
            ];
            let root = discovery::get_json(
                client,
                "https://dycalchis.eastmoney.com/price-anomaly/list",
                &params,
            )
            .await?;
            Ok(parse_anomaly(&root, page, today)?)
        }
        "monitor" => {
            let root = discovery::get_json(
                client,
                "https://mobappconfig.securities.eastmoney.com/emcfg/stock_monitor.json",
                &[],
            )
            .await?;
            Ok(parse_monitor(&root, today)?)
        }
        _ => Err(DiscoveryError::BadRequest(
            "kind 必须是 anomaly 或 monitor".into(),
        )),
    }
}

// Source rule descriptions are observations from the provider, not a local
// reconstruction of exchange rules or a prediction of a regulatory decision.
fn rule(code: i64) -> String {
    match code {
        1 => "主板连续10个交易日内4次出现同向异常波动",
        2 => "创业板连续10个交易日内3次出现同向异常波动",
        3 => "科创板连续10个交易日内3次出现同向异常波动",
        4 => "连续10个交易日内收盘涨跌幅偏离值累计达到+100%",
        5 => "连续10个交易日内收盘涨跌幅偏离值累计达到-50%",
        6 => "连续30个交易日内收盘涨跌幅偏离值累计达到+200%",
        7 => "连续30个交易日内收盘涨跌幅偏离值累计达到-70%",
        8 => "北交所连续10个交易日内3次出现同向异常波动",
        40 => "连续10个交易日内收盘涨跌幅偏离值累计达到+150%",
        50 => "连续10个交易日内收盘涨跌幅偏离值累计达到-60%",
        60 => "连续30个交易日内收盘涨跌幅偏离值累计达到+300%",
        70 => "连续30个交易日内收盘涨跌幅偏离值累计达到-75%",
        _ => return format!("来源未知规则码 {code}"),
    }
    .into()
}

fn parse_anomaly(root: &Value, page: usize, today: NaiveDate) -> Result<Value> {
    if root["result"].as_i64() != Some(0) {
        return Err(anyhow!("异常波动源拒绝请求"));
    }
    let raw_date = match &root["date"] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    };
    let date = NaiveDate::parse_from_str(&raw_date, "%Y%m%d")?;
    if date > today {
        return Err(anyhow!("异常波动日期在未来"));
    }
    let pages = root["pages"]
        .as_u64()
        .ok_or_else(|| anyhow!("异常波动页数缺失"))?;
    let raw = root["data"]
        .as_array()
        .ok_or_else(|| anyhow!("异常波动列表缺失"))?;
    let mut seen = BTreeSet::new();
    let mut items = Vec::new();
    let mut dropped = 0;
    for value in raw {
        let Some(key) = value["c"]
            .as_str()
            .and_then(|s| normalize_stock_symbol(s).ok())
        else {
            dropped += 1;
            continue;
        };
        // Board 6 is the provider's distinct BSE threshold family; shared SymbolKey owns routing.
        let mut rule_code = value["e"].as_i64().unwrap_or(-1);
        if value["s"] == 6 && (4..=7).contains(&rule_code) {
            rule_code *= 10;
        }
        let name = value["n"].as_str().unwrap_or("").trim();
        if name.is_empty() {
            dropped += 1;
            continue;
        }
        let is_today = value["o"]
            .as_i64()
            .filter(|v| [0, 1, 2].contains(v))
            .map(|v| v != 2);
        let identity = (key.digits.clone(), rule_code, is_today);
        if !seen.insert(identity) {
            continue;
        }
        items.push(json!({"symbol":format!("{}{}",key.market.as_str(),key.digits),"name":name,
            "change_pct":discovery::number(value.get("a")),"deviation_pct":discovery::number(value.get("x")),
            "days":value["d"].as_u64(),"rule_code":rule_code,"rule":rule(rule_code),"is_today":is_today}));
    }
    if !raw.is_empty() && items.is_empty() {
        return Err(anyhow!("异常波动无可用股票"));
    }
    Ok(
        json!({"status":if dropped>0{"partial"}else if items.is_empty(){"empty"}else{"ok"},
        "kind":"anomaly","date":date.to_string(),"stale":date!=today,"page":page,"pages":pages,
        "has_more":(page as u64)<pages,"items":items,"dropped":dropped,"source":"eastmoney",
        "note":"来源异常波动与偏离值名单，非盘中逐笔急拉急跌；规则文字沿用来源，不替代交易所公告。",
        "fetched_at":Utc::now().to_rfc3339()}),
    )
}

fn parse_monitor(root: &Value, today: NaiveDate) -> Result<Value> {
    let raw = root
        .as_array()
        .ok_or_else(|| anyhow!("重点监控列表格式错误"))?;
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    let mut dropped = 0;
    for value in raw {
        let Some(key) = value["STKCODE"]
            .as_str()
            .and_then(|s| normalize_stock_symbol(s).ok())
        else {
            dropped += 1;
            continue;
        };
        let expected = match key.market.as_str() {
            "SH" => "1",
            "SZ" => "0",
            _ => "B",
        };
        if value["MARKET"].as_str() != Some(expected) {
            dropped += 1;
            continue;
        }
        let start = value["VALIDATESTARTDATE"]
            .as_str()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        let end = value["VALIDATEENDDATE"]
            .as_str()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        let (Some(start), Some(end)) = (start, end) else {
            dropped += 1;
            continue;
        };
        let name = value["STKNAME"].as_str().unwrap_or("").trim();
        if start > end || name.is_empty() {
            dropped += 1;
            continue;
        }
        if end < today {
            continue;
        }
        if !seen.insert((key.digits.clone(), start, end)) {
            continue;
        }
        items.push(json!({"symbol":format!("{}{}",key.market.as_str(),key.digits),"name":name,
            "start":start.to_string(),"end":end.to_string(),"state":if start>today{"upcoming"}else{"active"}}));
    }
    if !raw.is_empty() && items.is_empty() && dropped == raw.len() {
        return Err(anyhow!("重点监控无可用记录"));
    }
    items.sort_by(|a, b| {
        a["start"]
            .as_str()
            .cmp(&b["start"].as_str())
            .then(a["symbol"].as_str().cmp(&b["symbol"].as_str()))
    });
    Ok(
        json!({"status":if dropped>0{"partial"}else if items.is_empty(){"empty"}else{"ok"},
        "kind":"monitor","date":today.to_string(),"page":1,"pages":1,"has_more":false,"stale":false,
        "items":items,"dropped":dropped,"source":"eastmoney",
        "note":"来源未提供名单更新时间；窗口状态按设备北京时间计算，展示生效中及即将生效窗口。已过期记录不展示，未获取到记录不等于个股无风险。",
        "fetched_at":Utc::now().to_rfc3339()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 16).unwrap()
    }
    #[test]
    fn public_anomaly_fixture_matches_legacy_rules_and_history_mark() {
        let root =
            serde_json::from_str(include_str!("../tests/fixtures/market_anomaly.json")).unwrap();
        let data = parse_anomaly(&root, 1, day()).unwrap();
        assert_eq!(data["items"][0]["symbol"], "SZ300006");
        assert_eq!(data["items"][0]["is_today"], false);
        assert_eq!(data["items"][0]["rule_code"], 4);
        assert_eq!(data["items"][0]["deviation_pct"], 94.17);
        assert_eq!(data["has_more"], true);
    }
    #[test]
    fn bse_and_missing_fields_keep_identity_and_unknowns() {
        let root = json!({"result":0,"date":"20261016","pages":1,"data":[
            {"c":"920575","n":"样本","s":6,"e":4,"a":"-","x":null},
            {"c":"600519","n":"样本二","s":1,"e":999,"o":0},
            {"c":"invalid","n":"bad"}]});
        let data = parse_anomaly(&root, 1, day()).unwrap();
        assert_eq!(data["items"][0]["symbol"], "BJ920575");
        assert_eq!(data["items"][0]["rule_code"], 40);
        assert!(data["items"][0]["change_pct"].is_null());
        assert!(data["items"][0]["is_today"].is_null());
        assert_eq!(data["status"], "partial");
        assert_eq!(data["dropped"], 1);
        assert_eq!(data["items"][1]["rule"], "来源未知规则码 999");
        assert!(parse_anomaly(&json!({"result":403}), 1, day()).is_err());
    }
    #[test]
    fn monitor_future_window_is_upcoming_not_active_and_expired_is_hidden() {
        let root: Value =
            serde_json::from_str(include_str!("../tests/fixtures/market_monitor.json")).unwrap();
        let data = parse_monitor(&root, day()).unwrap();
        assert_eq!(data["items"].as_array().unwrap().len(), 3);
        assert_eq!(data["items"][0]["state"], "upcoming");
        let active = parse_monitor(&root, NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()).unwrap();
        assert_eq!(active["items"][0]["state"], "active");
        assert_eq!(
            parse_monitor(&root, NaiveDate::from_ymd_opt(2026, 10, 24).unwrap()).unwrap()["status"],
            "empty"
        );
        let mut bad = root;
        bad[0]["MARKET"] = json!("1");
        assert_eq!(parse_monitor(&bad, day()).unwrap()["dropped"], 1);
    }
}
