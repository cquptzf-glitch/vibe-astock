use crate::discovery::{self, DiscoveryError};
use anyhow::{anyhow, Result};
use chrono::Utc;
use futures_util::future::join_all;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const BASE: &str =
    "https://500.farm/vastai/grafana.v2/api/datasources/proxy/uid/EdgV2xcnz/api/v1/query_range";
const GPUS: [&str; 3] = ["B200", "H100 SXM", "A100 SXM4"];

pub async fn gpu_rent(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let days = discovery::integer(path, "days", 90, 7, 365)?;
    let now = Utc::now().timestamp();
    let results = join_all(GPUS.iter().map(|gpu| async move {
        let params = [
            ("query", format!("quantile(0.5, vastai_v2_ondemand_price_median_dollars{{gpu_name=\"{gpu}\", rented=\"no\"}})")),
            ("start", (now - days as i64 * 86400).to_string()),
            ("end", now.to_string()),
            ("step", "86400".to_string()),
        ];
        let result = async {
            let root = discovery::get_json(client, BASE, &params).await?;
            parse_history(gpu, &root, now)
        }.await;
        (gpu.to_string(), result)
    })).await;
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for (gpu, result) in results {
        match result {
            Ok(item) => items.push(item),
            Err(error) => {
                errors.push(discovery::failure(&gpu, &error));
                items.push(json!({"gpu":gpu,"status":"failed","points":[],"latest":null,"message":"该型号数据源暂不可用或响应格式变化"}));
            }
        }
    }
    let status = if errors.len() == GPUS.len() {
        "failed"
    } else if !errors.is_empty() || items.iter().any(|item| item["status"] == "partial") {
        "partial"
    } else {
        "ok"
    };
    Ok(
        json!({"status":status,"items":items,"errors":errors,"history_days":days,
        "source":"500.farm / Vast.ai","source_url":"https://500.farm/",
        "method":"可租挂单按机型档位分组统计后聚合的中位，含平台费；每日采样，非逐张挂单精确中位。",
        "guard":"现货卡片取曲线最新点；B200 的 3 美元仅是设备折旧参考线，不含电力、机房和运维，不能解释为完整保本线。其他型号不套用此参考线。",
        "scope":"移动端现货历史版；暂不包含 Kalshi 月均远期市场。",
        "fetched_at":Utc::now().to_rfc3339(),"message":if status == "failed" {"GPU 租金数据源全部失败"} else {""}}),
    )
}

fn parse_history(gpu: &str, root: &Value, now: i64) -> Result<Value> {
    if root["status"] != "success" || root["data"]["resultType"] != "matrix" {
        return Err(anyhow!("GPU 统计源响应不是成功 matrix"));
    }
    let results = root["data"]["result"]
        .as_array()
        .ok_or_else(|| anyhow!("GPU 统计源缺少 result"))?;
    if results.is_empty() {
        return Ok(
            json!({"gpu":gpu,"status":"unavailable","points":[],"latest":null,"message":"统计站暂无该型号序列；不等于租金为零。"}),
        );
    }
    if results.len() != 1 {
        return Err(anyhow!("聚合 GPU 查询返回多条序列"));
    }
    let values = results[0]["values"]
        .as_array()
        .ok_or_else(|| anyhow!("GPU 序列缺少 values"))?;
    let mut points = BTreeMap::new();
    let mut dropped = 0;
    for pair in values {
        let timestamp = discovery::number(pair.get(0));
        let price = discovery::number(pair.get(1));
        let (Some(timestamp), Some(price)) = (timestamp, price) else {
            dropped += 1;
            continue;
        };
        if timestamp.fract() != 0.0
            || timestamp < 0.0
            || timestamp > (now + 300) as f64
            || price <= 0.0
        {
            dropped += 1;
            continue;
        }
        let timestamp = timestamp as i64;
        let price = (price * 100.0).round() / 100.0;
        if !price.is_finite() || price <= 0.0 {
            dropped += 1;
            continue;
        }
        if let Some(previous) = points.insert(timestamp, price) {
            if previous != price {
                return Err(anyhow!("同一 GPU 采样时间有冲突数值"));
            }
        }
    }
    let Some((&last_ts, &latest)) = points.last_key_value() else {
        return Err(anyhow!("GPU 序列没有可解析的有效点"));
    };
    let previous = points
        .range(..=last_ts - 7 * 86400)
        .next_back()
        .filter(|(timestamp, _)| last_ts - **timestamp <= 10 * 86400)
        .map(|(_, price)| *price);
    let change_7d = previous
        .filter(|value| *value > 0.0)
        .map(|price| (latest / price - 1.0) * 100.0);
    let stale = now - last_ts > 3 * 86400;
    let status = if stale || dropped > 0 {
        "partial"
    } else {
        "ok"
    };
    let rows: Vec<Value> = points
        .iter()
        .map(|(timestamp, value)| json!([timestamp, value]))
        .collect();
    Ok(
        json!({"gpu":gpu,"status":status,"points":rows,"latest":latest,"asof_ts":last_ts,
        "change_7d_pct":change_7d,"stale":stale,"dropped":dropped,
        "depreciation_reference_usd":if gpu == "B200" {Some(3.0)} else {None},
        "message":if stale {"最新采样已超过 3 天，请勿作为当前报价"} else if dropped > 0 {"部分无效采样已丢弃"} else {""}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_history_and_spot_are_identical_and_seconds_are_preserved() {
        let root: Value =
            serde_json::from_str(include_str!("../tests/fixtures/gpu_history.json")).unwrap();
        let result = parse_history("B200", &root, 1791590400).unwrap();
        assert_eq!(
            result["points"],
            json!([[1790812800_i64, 6.0], [1791504000_i64, 6.4]])
        );
        assert_eq!(result["latest"], 6.4);
        assert_eq!(result["asof_ts"], 1791504000_i64);
        assert_eq!(result["dropped"], 1);
        assert!((result["change_7d_pct"].as_f64().unwrap() - 6.6666666667).abs() < 0.0001);
        assert!(parse_history("H100 SXM", &root, 1791590400).unwrap()
            ["depreciation_reference_usd"]
            .is_null());
    }
    #[test]
    fn empty_failed_stale_and_conflicting_samples_are_distinct() {
        let empty = json!({"status":"success","data":{"resultType":"matrix","result":[]}});
        assert_eq!(
            parse_history("B200", &empty, 1000000).unwrap()["status"],
            "unavailable"
        );
        assert!(parse_history("B200", &json!({"status":"error"}), 1000000).is_err());
        let invalid = json!({"status":"success","data":{"resultType":"matrix","result":[{"values":[[1,"NaN"],[2,"Inf"]]}]}});
        assert!(parse_history("B200", &invalid, 1000000).is_err());
        let conflict = json!({"status":"success","data":{"resultType":"matrix","result":[{"values":[[1,"2"],[1,"3"]]}]}});
        assert!(parse_history("B200", &conflict, 1000000).is_err());
        let old = json!({"status":"success","data":{"resultType":"matrix","result":[{"values":[[1,"2"]]}]}});
        assert_eq!(parse_history("B200", &old, 1000000).unwrap()["stale"], true);
    }
}
