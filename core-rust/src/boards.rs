use crate::{
    discovery::{self, DiscoveryError},
    symbol::normalize_stock_symbol,
};
use anyhow::{anyhow, Result};
use chrono::Utc;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashSet;

pub fn valid_code(raw: &str) -> bool {
    raw.len() == 6 && raw.starts_with("BK") && raw[2..].bytes().all(|byte| byte.is_ascii_digit())
}

pub async fn detail(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let code = discovery::parameter(path, "code")
        .unwrap_or_default()
        .to_ascii_uppercase();
    if !valid_code(&code) {
        return Err(DiscoveryError::BadRequest(
            "板块代码须为 BK 加四位数字".into(),
        ));
    }
    let page = discovery::integer(path, "page", 1, 1, 200)?;
    let sort = discovery::parameter(path, "sort").unwrap_or_else(|| "change".into());
    let field = match sort.as_str() {
        "change" => "f3",
        "net" => "f62",
        "turnover" => "f8",
        _ => {
            return Err(DiscoveryError::BadRequest(
                "sort 必须为 change / net / turnover".into(),
            ))
        }
    };
    let order = discovery::parameter(path, "order").unwrap_or_else(|| "desc".into());
    let po = match order.as_str() {
        "desc" => "1",
        "asc" => "0",
        _ => return Err(DiscoveryError::BadRequest("order 必须为 asc / desc".into())),
    };
    let quote_params = [
        ("secid", format!("90.{code}")),
        ("fltt", "2".into()),
        ("invt", "2".into()),
        ("fields", "f43,f57,f58,f86,f104,f105,f170,f48,f62".into()),
    ];
    let member_params = [
        ("fs", format!("b:{code}")),
        ("pn", page.to_string()),
        ("pz", "50".into()),
        ("po", po.into()),
        ("np", "1".into()),
        ("fltt", "2".into()),
        ("invt", "2".into()),
        ("fid", field.into()),
        ("fields", "f12,f13,f14,f2,f3,f6,f8,f62".into()),
    ];
    let history_params = [
        ("secid", format!("90.{code}")),
        ("klt", "101".into()),
        ("fqt", "0".into()),
        ("end", "20500101".into()),
        ("lmt", "90".into()),
        ("fields1", "f1,f2,f3,f4,f5,f6".into()),
        ("fields2", "f51,f52,f53,f54,f55,f56,f57".into()),
    ];
    let (quote, members, history) = futures_util::future::join3(
        async {
            parse_quote(
                &discovery::get_json(
                    client,
                    "https://push2.eastmoney.com/api/qt/stock/get",
                    &quote_params,
                )
                .await?,
                &code,
            )
        },
        async {
            parse_members(
                &discovery::get_json(
                    client,
                    "https://push2.eastmoney.com/api/qt/clist/get",
                    &member_params,
                )
                .await?,
                page,
            )
        },
        async {
            parse_history(
                &discovery::get_json(
                    client,
                    "https://push2his.eastmoney.com/api/qt/stock/kline/get",
                    &history_params,
                )
                .await?,
                &code,
            )
        },
    )
    .await;
    let mut errors = Vec::new();
    let quote = match quote {
        Ok(value) => value,
        Err(error) => {
            errors.push(discovery::failure("板块概览", &error));
            Value::Null
        }
    };
    let members = match members {
        Ok(value) => value,
        Err(error) => {
            errors.push(discovery::failure("成分股", &error));
            Value::Null
        }
    };
    let history = match history {
        Ok(value) => value,
        Err(error) => {
            errors.push(discovery::failure("板块历史", &error));
            Value::Null
        }
    };
    let status = if errors.len() == 3 {
        "failed"
    } else if !errors.is_empty() || members["dropped"].as_u64().unwrap_or(0) > 0 {
        "partial"
    } else {
        "ok"
    };
    Ok(
        json!({"status":status,"code":code,"quote":quote,"constituents":members,"history":history,
        "sort":sort,"order":order,"errors":errors,"source":"eastmoney","fetched_at":Utc::now().to_rfc3339(),
        "message":if status == "failed" {"板块数据源全部失败"} else {""}}),
    )
}

fn parse_quote(root: &Value, code: &str) -> Result<Value> {
    if root["rc"].as_i64().is_some_and(|rc| rc != 0) {
        return Err(anyhow!("板块概览业务错误"));
    }
    let data = root
        .get("data")
        .filter(|data| data.is_object())
        .ok_or_else(|| anyhow!("板块概览缺少 data"))?;
    if data["f57"] != code {
        return Err(anyhow!("返回的板块代码与请求不一致"));
    }
    let name = data["f58"]
        .as_str()
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| anyhow!("板块名称缺失"))?;
    Ok(
        json!({"name":name,"price":discovery::number(data.get("f43")),"change_pct":discovery::number(data.get("f170")),
        "amount_yuan":discovery::number(data.get("f48")),"main_net_yuan":discovery::number(data.get("f62")),
        "up_count":discovery::number(data.get("f104")),"down_count":discovery::number(data.get("f105")),
        "asof_ts":discovery::number(data.get("f86"))}),
    )
}

fn parse_members(root: &Value, page: usize) -> Result<Value> {
    let rows = discovery::provider_rows(root)?;
    let total = root["data"]["total"]
        .as_u64()
        .ok_or_else(|| anyhow!("成分股缺少 total"))? as usize;
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut dropped = 0;
    let (mut up, mut down, mut flat, mut unknown) = (0, 0, 0, 0);
    for row in rows {
        let raw = row["f12"].as_str().unwrap_or_default();
        let Ok(key) = normalize_stock_symbol(raw) else {
            dropped += 1;
            continue;
        };
        let expected_market = if key.market.as_str() == "SH" { 1 } else { 0 };
        if row["f13"].as_i64() != Some(expected_market)
            || row["f14"].as_str().is_none_or(|name| name.is_empty())
        {
            dropped += 1;
            continue;
        }
        if !seen.insert(key.normalized.clone()) {
            continue;
        }
        let change = discovery::number(row.get("f3"));
        match change {
            Some(value) if value > 0.0 => up += 1,
            Some(value) if value < 0.0 => down += 1,
            Some(_) => flat += 1,
            None => unknown += 1,
        }
        items.push(json!({"symbol":key.normalized,"name":row["f14"],"price":discovery::number(row.get("f2")),
            "change_pct":change,"amount_yuan":discovery::number(row.get("f6")),
            "turnover_pct":discovery::number(row.get("f8")),"main_net_yuan":discovery::number(row.get("f62"))}));
    }
    if total > 0 && items.is_empty() && page == 1 {
        return Err(anyhow!("成分股列表非空但无法解析出股票"));
    }
    Ok(
        json!({"items":items,"total":total,"page":page,"page_size":50,"has_more":page*50 < total,"dropped":dropped,
        "page_breadth":{"up":up,"down":down,"flat":flat,"unknown":unknown}}),
    )
}

fn parse_history(root: &Value, code: &str) -> Result<Value> {
    if root["rc"].as_i64().is_some_and(|rc| rc != 0) {
        return Err(anyhow!("板块历史业务错误"));
    }
    if root["data"]["code"] != code {
        return Err(anyhow!("返回的历史板块代码不一致"));
    }
    let raw = root["data"]["klines"]
        .as_array()
        .ok_or_else(|| anyhow!("板块历史缺少 klines"))?;
    let mut rows = Vec::new();
    let mut last_date = None;
    for value in raw {
        let columns: Vec<&str> = value
            .as_str()
            .ok_or_else(|| anyhow!("历史行格式错误"))?
            .split(',')
            .collect();
        if columns.len() < 7 {
            return Err(anyhow!("历史行字段不足"));
        }
        let date = chrono::NaiveDate::parse_from_str(columns[0], "%Y-%m-%d")?;
        if last_date.is_some_and(|previous| date <= previous) {
            return Err(anyhow!("历史日期重复或非升序"));
        }
        let numbers: Vec<f64> = columns[1..7]
            .iter()
            .map(|raw| raw.parse::<f64>())
            .collect::<std::result::Result<_, _>>()?;
        if numbers.iter().any(|value| !value.is_finite())
            || numbers[..4].iter().any(|value| *value <= 0.0)
        {
            return Err(anyhow!("板块历史价格无效"));
        }
        last_date = Some(date);
        rows.push(json!({"date":columns[0],"open":numbers[0],"close":numbers[1],"high":numbers[2],"low":numbers[3],"amount_yuan":numbers[5]}));
    }
    Ok(json!({"rows":rows,"adjustment":"none","period":"daily","limit":90}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn board_codes_and_provider_identity_are_validated() {
        assert!(valid_code("BK0475"));
        for code in ["BK0475&fs=x", "SH600519", "BK12", "bk0475", "BK中01"] {
            assert!(!valid_code(code));
        }
        let quote = json!({"rc":0,"data":{"f57":"BK0475","f58":"银行Ⅱ","f43":4379.33,"f170":0.32,"f104":"-"}});
        assert_eq!(parse_quote(&quote, "BK0475").unwrap()["price"], 4379.33);
        assert!(parse_quote(&quote, "BK0001").is_err());
        assert!(parse_quote(&quote, "BK0475").unwrap()["up_count"].is_null());
    }
    #[test]
    fn constituent_fixture_uses_symbol_key_and_keeps_missing_values() {
        let root: Value =
            serde_json::from_str(include_str!("../tests/fixtures/board_members.json")).unwrap();
        let result = parse_members(&root, 1).unwrap();
        assert_eq!(result["total"], 42);
        assert_eq!(result["items"][0]["symbol"], "SZ002807");
        assert_eq!(result["items"][1]["symbol"], "SH603323");
        assert_eq!(result["page_breadth"]["up"], 2);
        let empty = json!({"rc":0,"data":{"total":0,"diff":null}});
        assert_eq!(parse_members(&empty, 1).unwrap()["items"], json!([]));
        assert!(parse_members(&json!({"rc":1}), 1).is_err());
        let bj = json!({"data":{"total":1,"diff":[{"f12":"430047","f13":0,"f14":"样本","f2":"-","f3":"-"}]}});
        assert_eq!(
            parse_members(&bj, 1).unwrap()["items"][0]["symbol"],
            "BJ430047"
        );
        assert!(parse_members(&bj, 1).unwrap()["items"][0]["price"].is_null());
    }
    #[test]
    fn history_is_unadjusted_and_bad_order_is_rejected() {
        let root = json!({"data":{"code":"BK0475","klines":["2026-10-09,100,101,102,99,10,1000"]}});
        assert_eq!(
            parse_history(&root, "BK0475").unwrap()["rows"][0]["close"],
            101.0
        );
        assert_eq!(
            parse_history(&root, "BK0475").unwrap()["adjustment"],
            "none"
        );
        let bad = json!({"data":{"code":"BK0475","klines":["2026-10-09,100,101,102,99,10,1000","2026-10-08,100,101,102,99,10,1000"]}});
        assert!(parse_history(&bad, "BK0475").is_err());
    }
}
