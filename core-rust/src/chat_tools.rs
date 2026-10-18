use crate::{emotion, emotion_history, market_panel, quote, research, research_extended};
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::path::Path;
pub fn catalog() -> Value {
    json!([
        {"type":"function","function":{"name":"market_data","description":"读取公开市场或公司资料,只允许目录中的数据类型;不读取个人成交或任意文件。","parameters":{"type":"object","properties":{"kind":{"type":"string","enum":["quote","profile","statements","consensus","valuation","emotion","weekly","market"]},"symbol":{"type":"string","description":"个股资料必须为用户指定的股票代码"},"date":{"type":"string","description":"市场复盘日 YYYY-MM-DD,个股资料为当前快照"}},"required":["kind"],"additionalProperties":false}}},
        {"type":"function","function":{"name":"calculate","description":"确定性四则运算或百分比变化,不执行代码。","parameters":{"type":"object","properties":{"op":{"type":"string","enum":["sum","difference","product","ratio","pct_change"]},"left":{"type":"number"},"right":{"type":"number"}},"required":["op","left","right"],"additionalProperties":false}}},
        {"type":"function","function":{"name":"calculation_catalog","description":"读取财务、季度序列、技术指标及财报事件的真实计算函数目录与参数。没有文件访问。","parameters":{"type":"object","properties":{},"additionalProperties":false}}},
        {"type":"function","function":{"name":"financial_calculate","description":"执行 calculation_catalog 中的确定性函数。输入必须照抄本轮数据与单位；error 或 not_meaningful 不能写成数值结论，正文照抄 display。只计算，不访问网络或文件，不证明输入的业务来源。","parameters":{"type":"object","properties":{"function":{"type":"string"},"args":{"type":"object"}},"required":["function","args"],"additionalProperties":false}}}
    ])
}
pub(crate) fn calculate(args: &Value) -> Result<Value> {
    let a = args["left"]
        .as_f64()
        .filter(|v| v.is_finite() && v.abs() <= 1e15)
        .ok_or_else(|| anyhow!("left 不是有效有限数字"))?;
    let b = args["right"]
        .as_f64()
        .filter(|v| v.is_finite() && v.abs() <= 1e15)
        .ok_or_else(|| anyhow!("right 不是有效有限数字"))?;
    let op = args["op"].as_str().unwrap_or("");
    let result = match op {
        "sum" => a + b,
        "difference" => a - b,
        "product" => a * b,
        "ratio" if b != 0.0 => a / b,
        "pct_change" if b != 0.0 => (a - b) / b.abs() * 100.0,
        _ => return Err(anyhow!("不支持的算式或除数为零")),
    };
    if !result.is_finite() {
        return Err(anyhow!("算式结果不在有限数值范围"));
    }
    Ok(
        json!({"left":a,"right":b,"op":op,"value":result,"note":"只核对算术,输入数字的业务来源仍须核对"}),
    )
}
pub async fn execute(client: &Client, db: &Path, name: &str, args: &Value) -> Result<Value> {
    let object = args
        .as_object()
        .ok_or_else(|| anyhow!("工具参数须为对象"))?;
    let allowed: &[&str] = match name {
        "calculation_catalog" => &[],
        "financial_calculate" => &["function", "args"],
        "calculate" => &["op", "left", "right"],
        "market_data" => &["kind", "symbol", "date"],
        _ => return Err(anyhow!("工具不在目录内")),
    };
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(anyhow!("工具参数包含目录外字段"));
    }
    if name == "calculation_catalog" {
        return Ok(crate::deterministic_calc::catalog());
    }
    if name == "financial_calculate" {
        return Ok(crate::deterministic_calc::execute(
            args["function"].as_str().unwrap_or(""),
            &args["args"],
        ));
    }
    if name == "calculate" {
        return calculate(args);
    }
    let kind = args["kind"].as_str().unwrap_or("");
    let symbol = match kind {
        "quote" | "profile" | "statements" | "consensus" | "valuation" => Some(
            crate::normalize_stock_symbol(args["symbol"].as_str().unwrap_or(""))?,
        ),
        _ => None,
    };
    let mut path = format!(
        "/?symbol={}",
        symbol.as_ref().map(|s| s.digits.as_str()).unwrap_or("")
    );
    if let Some(date) = args["date"].as_str() {
        let day = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
        if day.to_string() != date {
            return Err(anyhow!("日期格式无效"));
        }
        if symbol.is_some() && date != crate::emotion::now_china().0.to_string() {
            return Err(anyhow!("个股接口只有当前快照,不能冒充历史资料"));
        }
        path.push_str(&format!("&date={date}"));
    }
    let result = match kind {
        "quote" => quote::fetch_quote(client, symbol.as_ref().unwrap()).await,
        "profile" => research::profile(client, &path)
            .await
            .map_err(|e| anyhow!("{e:?}")),
        "statements" => research_extended::statements(client, &path)
            .await
            .map_err(|e| anyhow!("{e:?}")),
        "consensus" => research_extended::consensus(client, &path)
            .await
            .map_err(|e| anyhow!("{e:?}")),
        "valuation" => research_extended::valuation(client, &path)
            .await
            .map_err(|e| anyhow!("{e:?}")),
        "emotion" => emotion::emotion(client, &path)
            .await
            .map_err(|e| anyhow!("{e:?}")),
        "weekly" => emotion_history::weekly(client, db, &path)
            .await
            .map_err(|e| anyhow!("{e:?}")),
        "market" => market_panel::fetch(client, &path)
            .await
            .map_err(|e| anyhow!("{e:?}")),
        _ => Err(anyhow!("未知数据类型")),
    }?;
    if result["status"] == "failed" {
        return Err(anyhow!("该数据工具没有取到有效资料：{}", result));
    }
    let packed = crate::research_workflow::compact(&result, 0);
    Ok(
        json!({"kind":kind,"data":packed,"truncated":packed!=result,"fetched_at":chrono::Utc::now().to_rfc3339()}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn financial_tool_reads_catalog_then_preserves_not_meaningful() {
        let app = crate::AppCore::new(crate::test_http::temp_root("financial-chat")).unwrap();
        let call = |id: &str, name: &str, args: Value| json!({"choices":[{"message":{"role":"assistant","tool_calls":[{"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}}]}}]});
        let (url, server) = crate::test_http::server(
            vec![
                call("catalog", "calculation_catalog", json!({})),
                call(
                    "pe",
                    "financial_calculate",
                    json!({"function":"pe_ttm_from_parts","args":{"total_market_cap":100,"cap_unit":"亿元","ttm_profit":0,"profit_unit":"亿元"}}),
                ),
                json!({"choices":[{"message":{"content":"合成输入的利润为零，市盈率无意义，未作为有效数值。"}}]}),
            ],
            0,
        );
        crate::test_http::config(app.db_path(), &url);
        let result = crate::ai_chat::send(
            &Client::new(),
            app.db_path(),
            &json!({"content":"核对合成样本的市盈率","tools_enabled":true}),
        )
        .await
        .unwrap();
        assert_eq!(
            result["tool_records"][0]["result"]["functions"]
                .as_array()
                .unwrap()
                .len(),
            24
        );
        assert_eq!(
            result["tool_records"][1]["result"]["status"],
            "not_meaningful"
        );
        assert!(result["tool_records"][1]["result"]["value"].is_null());
        server.join().unwrap();
    }
    #[tokio::test]
    async fn model_tool_round_trip_returns_actual_arithmetic_trace() {
        let app = crate::AppCore::new(std::env::temp_dir().join(format!(
            "tools-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        )))
        .unwrap();
        let first = json!({"choices":[{"message":{"role":"assistant","tool_calls":[{"id":"call_fixture","type":"function","function":{"name":"calculate","arguments":json!({"op":"sum","left":12,"right":10}).to_string()}}]}}]});
        let second =
            json!({"choices":[{"message":{"role":"assistant","content":"合成输入的和已复算。"}}]});
        let (url, server) = crate::test_http::server(vec![first, second], 0);
        crate::test_http::config(app.db_path(), &url);
        let out = crate::ai_chat::send(
            &Client::new(),
            app.db_path(),
            &json!({"content":"把合成输入相加","tools_enabled":true}),
        )
        .await
        .unwrap();
        assert_eq!(out["tool_records"][0]["result"]["value"], 22.0);
        assert_eq!(out["tool_records"][0]["ok"], true);
        let requests = server.join().unwrap();
        let messages = requests[1]["messages"].as_array().unwrap();
        let result = messages.iter().find(|m| m["role"] == "tool").unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(result["content"].as_str().unwrap()).unwrap()["value"],
            22.0
        );
    }
    #[tokio::test]
    async fn tool_budget_refuses_nine_calls_and_does_not_save_a_partial_answer() {
        let app = crate::AppCore::new(std::env::temp_dir().join(format!(
            "tools-budget-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        )))
        .unwrap();
        let calls:Vec<_>=(0..9).map(|i|json!({"id":format!("call_{i}"),"type":"function","function":{"name":"calculate","arguments":"{}"}})).collect();
        let (url, server) = crate::test_http::server(
            vec![json!({"choices":[{"message":{"tool_calls":calls}}]})],
            0,
        );
        crate::test_http::config(app.db_path(), &url);
        assert!(crate::ai_chat::send(
            &Client::new(),
            app.db_path(),
            &json!({"content":"预算测试","tools_enabled":true})
        )
        .await
        .is_err());
        server.join().unwrap();
        let conn = rusqlite::Connection::open(app.db_path()).unwrap();
        assert_eq!(
            conn.query_row::<i64, _, _>("SELECT COUNT(*) FROM mobile_ai_messages", [], |r| r
                .get(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn calculation_is_finite_and_zero_division_is_rejected() {
        assert_eq!(
            calculate(&json!({"op":"pct_change","left":12,"right":10})).unwrap()["value"],
            20.0
        );
        assert!(calculate(&json!({"op":"ratio","left":12,"right":0})).is_err());
        assert!(calculate(&json!({"op":"eval","left":1,"right":2})).is_err());
    }
    #[tokio::test]
    async fn unknown_tool_and_internal_fields_fail_before_network() {
        let c = Client::new();
        assert!(execute(&c, Path::new("unused"), "shell", &json!({}))
            .await
            .is_err());
        assert!(execute(
            &c,
            Path::new("unused"),
            "calculate",
            &json!({"op":"sum","left":1,"right":2,"__secure_api_key":"fake"})
        )
        .await
        .is_err());
    }
}
