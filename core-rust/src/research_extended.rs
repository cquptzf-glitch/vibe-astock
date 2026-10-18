use crate::{discovery, research::ResearchError, symbol::normalize_stock_symbol};
use anyhow::{anyhow, Result};
use chrono::{Datelike, NaiveDate, Utc};
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// Small, unit-preserving series for controlled calculations. The full statements
/// remain available in the research panel; aliases never mix labels across periods.
pub(crate) fn calculation_series(data: &Value) -> Value {
    let specs = &[
        ("income", "revenue", &["营业总收入", "营业收入"][..]),
        (
            "income",
            "net_profit_parent",
            &[
                "归属于母公司所有者的净利润",
                "归属于母公司股东的净利润",
                "归母净利润",
            ][..],
        ),
        (
            "income",
            "net_profit_deducted",
            &["扣除非经常性损益后的净利润", "扣非净利润"][..],
        ),
        ("income", "operating_cost", &["营业成本"][..]),
        ("income", "operating_profit", &["营业利润"][..]),
        ("balance", "assets", &["资产总计", "资产总额"][..]),
        ("balance", "liabilities", &["负债合计"][..]),
        ("balance", "cash", &["货币资金"][..]),
        (
            "cashflow",
            "operating_cashflow",
            &["经营活动产生的现金流量净额"][..],
        ),
        (
            "cashflow",
            "investing_cashflow",
            &["投资活动产生的现金流量净额"][..],
        ),
        (
            "cashflow",
            "financing_cashflow",
            &["筹资活动产生的现金流量净额"][..],
        ),
    ];
    let mut series = serde_json::Map::new();
    let mut missing = Vec::new();
    for &(statement, key, labels) in specs {
        let Some(rows) = data["statements"][statement].as_array() else {
            missing.push(json!(key));
            continue;
        };
        let Some(label) = labels
            .iter()
            .find(|&&label| rows.iter().any(|r| r["values"].get(label).is_some()))
        else {
            missing.push(json!(key));
            continue;
        };
        let mut units = std::collections::BTreeSet::new();
        let mut no_unit = false;
        let mut values = Vec::new();
        let mut legacy_unit = false;
        for row in rows {
            let value = row["values"].get(*label).cloned().unwrap_or(Value::Null);
            if !value.is_null() {
                let mut unit = row["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|i| i["label"] == *label)
                    .and_then(|i| i["unit"].as_str())
                    .map(str::trim)
                    .filter(|u| !u.is_empty());
                // Only these two labels have an existing repository source contract
                // (fetch_financials.py LRB_FIELDS). Do not infer the remaining units.
                if unit.is_none()
                    && statement == "income"
                    && row["currency"] == "CNY"
                    && ["营业总收入", "归属于母公司所有者的净利润"].contains(label)
                {
                    unit = Some("元");
                    legacy_unit = true;
                }
                if let Some(unit) = unit {
                    units.insert(unit.to_owned());
                } else {
                    no_unit = true;
                }
            }
            values.push(json!({"period":row["period"],"value":value}));
        }
        if no_unit
            || units.len() != 1
            || units
                .iter()
                .next()
                .is_none_or(|u| crate::deterministic_calc::money_unit(u).is_err())
        {
            missing.push(json!(format!(
                "{key}: 源站缺少明确且一致的金额单位，未生成金额序列"
            )));
            continue;
        }
        series.insert(key.to_owned(),json!({"statement":statement,"source_label":label,"unit":units.first().unwrap(),"unit_source":if legacy_unit{"既有 fetch_financials.py LRB_FIELDS 源契约，并核对 rCurrency=CNY"}else{"源站科目单位"},"money":true,"cumulative":values}));
    }
    json!({"series":series,"missing":missing,"source":data["source"],"fetched_at":data["fetched_at"],"note":"科目来自同一来源和明确金额单位；不补缺失值，不拼接不同科目。利润/现金流为年初至今累计；资产负债表为期末时点，不可 quarterize。"})
}

pub(crate) fn parse_statement(root: &Value, limit: usize) -> Result<Vec<Value>> {
    let reports = root
        .pointer("/result/data/report_list")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("财报缺少 report_list"))?;
    let mut keys: Vec<&String> = reports.keys().collect();
    keys.sort_by(|a, b| b.cmp(a));
    let mut out = Vec::new();
    for k in keys.into_iter().take(limit) {
        let period = if k.len() == 8 && k.is_ascii() {
            format!("{}-{}-{}", &k[..4], &k[4..6], &k[6..])
        } else {
            k.to_string()
        };
        if NaiveDate::parse_from_str(&period, "%Y-%m-%d").is_err() {
            continue;
        }
        let Some(items) = reports[k]["data"].as_array() else {
            continue;
        };
        let mut values = Map::new();
        let mut raw = Vec::new();
        for item in items {
            let label = item["item_title"].as_str().unwrap_or("").trim();
            if label.is_empty() {
                continue;
            }
            let number = discovery::number(item.get("item_value")).or_else(|| {
                item["item_value"]
                    .as_str()
                    .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                    .filter(|v| v.is_finite())
            });
            values.insert(label.to_string(), json!(number));
            raw.push(json!({"label":label,"value":number,"raw":item["item_value"],"unit":item["item_unit"]}));
        }
        if !raw.is_empty() {
            out.push(json!({"period":period,"values":values,"items":raw,"currency":reports[k]["rCurrency"],"report_type":reports[k]["rType"],"publish_date":reports[k]["publish_date"]}));
        }
    }
    if out.is_empty() {
        return Err(anyhow!("财报没有可解析行"));
    }
    // Compare cumulative reports with the same month/day of the prior year.
    for i in 0..out.len() {
        let period = out[i]["period"].as_str().unwrap_or("");
        let prior = format!("{}{}", period[..4].parse::<i32>()? - 1, &period[4..]);
        let mut yoy = Map::new();
        if let Some(previous) = out.iter().find(|r| r["period"] == prior) {
            for (k, v) in out[i]["values"].as_object().unwrap() {
                let rate = match (v.as_f64(), previous["values"][k].as_f64()) {
                    (Some(a), Some(b)) if b != 0.0 => Some((a - b) / b.abs() * 100.0),
                    _ => None,
                };
                yoy.insert(k.clone(), json!(rate));
            }
        }
        out[i]["yoy_pct"] = json!(yoy);
    }
    Ok(out)
}

pub async fn statements(client: &Client, path: &str) -> std::result::Result<Value, ResearchError> {
    let key = normalize_stock_symbol(&discovery::parameter(path, "symbol").unwrap_or_default())
        .map_err(|e| ResearchError::BadRequest(e.to_string()))?;
    let periods = discovery::integer(path, "periods", 12, 4, 20)
        .map_err(|e| ResearchError::BadRequest(format!("{e:?}")))?;
    let results=futures_util::future::join_all([("income","lrb"),("balance","fzb"),("cashflow","llb")].into_iter().map(|(name,code)| {
        let symbol=key.tencent.clone();
        async move {
            let r:Result<Vec<Value>>=async {
                let root=discovery::get_json(client,"https://quotes.sina.cn/cn/api/openapi.php/CompanyFinanceService.getFinanceReport2022",&[("paperCode",symbol),("source",code.into()),("type","0".into()),("page","1".into()),("num",periods.to_string())]).await?;
                parse_statement(&root,periods)
            }.await;
            (name,r)
        }
    })).await;
    let mut data = Map::new();
    let mut errors = Vec::new();
    for (name, result) in results {
        match result {
            Ok(rows) => {
                data.insert(name.into(), json!(rows));
            }
            Err(e) => errors.push(json!({"statement":name,"message":e.to_string()})),
        }
    }
    if data.is_empty() {
        return Err(ResearchError::Upstream(anyhow!(
            "财务三表均未取到:{errors:?}"
        )));
    }
    Ok(
        json!({"symbol":key.digits,"statements":data,"errors":errors,"status":if errors.is_empty(){"ok"}else{"partial"},"source":"新浪上市公司财报","fetched_at":Utc::now().to_rfc3339(),"note":"保留源站全部科目、原值及单位;同比只和上一年同报告期比较,缺比较期或基数为零不计算;同比=(本期-上期)/abs(上期)。利润及现金流为累计口径,不冒充单季度。"}),
    )
}

pub(crate) fn parse_consensus(html: &str) -> Result<Vec<Value>> {
    let doc = Html::parse_document(html);
    let table = Selector::parse("table").unwrap();
    let tr = Selector::parse("tr").unwrap();
    let cell = Selector::parse("th,td").unwrap();
    let mut out = Vec::new();
    for t in doc.select(&table) {
        let rows: Vec<Vec<String>> = t
            .select(&tr)
            .map(|r| {
                r.select(&cell)
                    .map(|c| {
                        c.text()
                            .collect::<String>()
                            .split_whitespace()
                            .collect::<String>()
                    })
                    .collect()
            })
            .collect();
        let Some((index, head)) = rows.iter().enumerate().find(|(_, r)| {
            r.iter().any(|v| v.contains("预测机构数")) && r.iter().any(|v| v == "均值")
        }) else {
            continue;
        };
        let pos = |name: &str| head.iter().position(|x| x == name);
        let (Some(y), Some(n), Some(mean), Some(min), Some(max)) = (
            pos("年度"),
            pos("预测机构数"),
            pos("均值"),
            pos("最小值"),
            pos("最大值"),
        ) else {
            continue;
        };
        for row in rows.iter().skip(index + 1) {
            let year = row
                .get(y)
                .and_then(|x| x.get(..4))
                .and_then(|s| s.parse::<i32>().ok());
            let num = |i| {
                row.get(i)
                    .and_then(|s: &String| s.replace(',', "").parse::<f64>().ok())
                    .filter(|v| v.is_finite())
            };
            if let (Some(year), Some(count), Some(avg), Some(low), Some(high)) =
                (year, num(n), num(mean), num(min), num(max))
            {
                if !(2000..=2100).contains(&year)
                    || count < 1.0
                    || count.fract() != 0.0
                    || low > avg
                    || avg > high
                {
                    continue;
                }
                out.push(json!({"year":year,"institutions":count as u64,"mean":avg,"min":low,"max":high,"thin_sample":count<3.0}));
            }
        }
    }
    out.sort_by_key(|r| r["year"].as_i64());
    out.dedup_by(|a, b| a["year"] == b["year"]);
    if out.is_empty() {
        return Err(anyhow!(
            "同花顺没有完整的年度/机构数/均值/最小/最大值表;不把逐篇研报平均值冒充一致预期"
        ));
    }
    Ok(out)
}

pub async fn consensus(client: &Client, path: &str) -> std::result::Result<Value, ResearchError> {
    let key = normalize_stock_symbol(&discovery::parameter(path, "symbol").unwrap_or_default())
        .map_err(|e| ResearchError::BadRequest(e.to_string()))?;
    let url = format!("https://basic.10jqka.com.cn/new/{}/worth.html", key.digits);
    let bytes = discovery::get_bytes(client, &url, &[]).await?;
    let (text, _, _) = encoding_rs::GBK.decode(&bytes);
    let rows = parse_consensus(&text)?;
    let year = crate::emotion::now_china().0.year();
    let missing: Vec<i32> = (year..=year + 2)
        .filter(|y| !rows.iter().any(|r| r["year"].as_i64() == Some(*y as i64)))
        .collect();
    Ok(
        json!({"symbol":key.digits,"rows":rows,"missing_years":missing,"status":if missing.is_empty(){"ok"}else{"partial"},"source":"同花顺机构一致预期","source_url":url,"fetched_at":Utc::now().to_rfc3339(),"note":"直接读取源站汇总,不对逐篇研报做平均来替代共识;机构样本少于三家时标记,缺失年度不补值。"}),
    )
}

pub(crate) fn valuation_series(root: &Value) -> Result<Value> {
    let body = root
        .pointer("/Result/0/DisplayData/resultData/tplData/result/chartInfo/0/body")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("估值源没有历史序列"))?;
    let mut by = BTreeMap::new();
    for point in body {
        let Some(date) = point
            .get(0)
            .and_then(Value::as_str)
            .and_then(|s| s.get(..10))
        else {
            continue;
        };
        let Ok(day) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
            continue;
        };
        if day.weekday().number_from_monday() > 5 {
            continue;
        }
        let Some(value) = discovery::number(point.get(1)) else {
            continue;
        };
        by.insert(date.to_string(), value);
    }
    let rows: Vec<Value> = by.iter().map(|(d, v)| json!([d, v])).collect();
    let positive: Vec<f64> = by.values().copied().filter(|v| *v > 0.0).collect();
    let Some((date, latest)) = by.last_key_value() else {
        return Err(anyhow!("估值序列为空"));
    };
    let pct = if *latest > 0.0 && positive.len() >= 20 {
        Some(
            positive.iter().filter(|v| **v <= *latest).count() as f64 / positive.len() as f64
                * 100.0,
        )
    } else {
        None
    };
    Ok(
        json!({"rows":rows,"latest":latest,"date":date,"positive_samples":positive.len(),"percentile_pct":pct,"min":positive.iter().copied().reduce(f64::min),"max":positive.iter().copied().reduce(f64::max)}),
    )
}

pub async fn valuation(client: &Client, path: &str) -> std::result::Result<Value, ResearchError> {
    let key = normalize_stock_symbol(&discovery::parameter(path, "symbol").unwrap_or_default())
        .map_err(|e| ResearchError::BadRequest(e.to_string()))?;
    let mut series = Map::new();
    let mut errors = Vec::new();
    for (name, indicator) in [("pe_ttm", "市盈率(TTM)"), ("pb", "市净率")] {
        let result: Result<Value> = async {
            let root = discovery::get_json(
                client,
                "https://gushitong.baidu.com/opendata",
                &[
                    ("openapi", "1".into()),
                    ("dspName", "iphone".into()),
                    ("tn", "tangram".into()),
                    ("client", "app".into()),
                    ("query", indicator.into()),
                    ("code", key.digits.clone()),
                    ("resource_id", "51171".into()),
                    ("market", "ab".into()),
                    ("tag", indicator.into()),
                    ("chart_select", "近三年".into()),
                    ("skip_industry", "1".into()),
                    ("finClientType", "pc".into()),
                ],
            )
            .await?;
            valuation_series(&root)
        }
        .await;
        match result {
            Ok(data) => {
                series.insert(name.into(), data);
            }
            Err(e) => errors.push(json!({"metric":name,"message":e.to_string()})),
        }
    }
    if series.is_empty() {
        return Err(ResearchError::Upstream(anyhow!(
            "估值历史均未取到:{errors:?}"
        )));
    }
    Ok(
        json!({"symbol":key.digits,"series":series,"errors":errors,"source":"百度股市通","fetched_at":Utc::now().to_rfc3339(),"note":"近三年历史,实际范围以首末日期为准。分位只含正值,至少二十个有效样本;非正估值不计算分位。排除周末,源站若沿用节假日前值仍可能重复,不等同严格交易日估值数据库。"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "live public financial / OHLC calculation acceptance; no model or private data"]
    async fn live_calculation_inputs_probe() {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("Mozilla/5.0 VibeAStock-validation")
            .build()
            .unwrap();
        let data = statements(&client, "/?symbol=600519").await.unwrap();
        let series = calculation_series(&data);
        let source = &series["series"]["net_profit_parent"];
        assert_eq!(source["unit"], "元", "{series}");
        let quarter = crate::deterministic_calc::execute(
            "quarterize",
            &json!({"cumulative":source["cumulative"],"unit":source["unit"],"money":true}),
        );
        assert_eq!(quarter["status"], "ok", "{quarter}");
        let end = source["cumulative"][0]["period"].clone();
        let args = json!({"single_quarters":quarter["details"]["series"],"unit":source["unit"],"money":true,"end_period":end});
        for f in ["ttm_sum", "ttm_yoy"] {
            let result = crate::deterministic_calc::execute(f, &args);
            assert_eq!(result["status"], "ok", "{result}");
            eprintln!("live {f}: {}", result["display"]);
        }
        let symbol = crate::normalize_stock_symbol("600519").unwrap();
        let raw = crate::quote::fetch_kline(&client, &symbol, 180)
            .await
            .unwrap();
        let rows: Vec<_> = raw["extra"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| json!({"date":r[0],"open":r[1],"close":r[2],"high":r[3],"low":r[4]}))
            .collect();
        let result =
            crate::deterministic_calc::execute("technical_indicators", &json!({"klines":rows}));
        assert_eq!(result["status"], "ok", "{result}");
        eprintln!(
            "live indicators: points={} date={} source={}",
            result["details"]["points"], result["details"]["date"], raw["primary_source"]
        );
    }
    #[test]
    fn calculation_series_requires_known_units_and_never_splices_labels_or_currencies() {
        let root = json!({"result":{"data":{"report_list":{"20260630":{"rCurrency":"CNY","data":[{"item_title":"营业总收入","item_value":"1200"},{"item_title":"归属于母公司所有者的净利润","item_value":"120"},{"item_title":"营业成本","item_value":"900"}]},"20260331":{"rCurrency":"CNY","data":[{"item_title":"营业总收入","item_value":"500"},{"item_title":"归属于母公司所有者的净利润","item_value":"50"}]}}}}});
        let rows = parse_statement(&root, 12).unwrap();
        let mut data = json!({"statements":{"income":rows},"source":"synthetic"});
        let series = calculation_series(&data);
        assert_eq!(series["series"]["revenue"]["unit"], "元");
        assert_eq!(
            series["series"]["net_profit_parent"]["cumulative"][0]["value"],
            120.
        );
        assert!(series["series"].get("operating_cost").is_none());
        data["statements"]["income"][0]["currency"] = json!("USD");
        assert!(calculation_series(&data)["series"].get("revenue").is_none());
    }
    #[tokio::test]
    #[ignore = "real public provider connectivity, no model or personal data"]
    async fn live_completion_sources_probe() {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(4))
            .timeout(std::time::Duration::from_secs(12))
            .user_agent("Mozilla/5.0 VibeAStock-dev-validation")
            .build()
            .unwrap();
        let path = "/?symbol=600519";
        let (a, b, c, d, e) = tokio::join!(
            statements(&client, path),
            consensus(&client, path),
            valuation(&client, path),
            crate::calendar_sources::month(&client, "/"),
            crate::industry_signals::gpu_rent(&client, "/?days=7")
        );
        let results = vec![
            ("statements", a.map_err(|e| format!("{e:?}"))),
            ("consensus", b.map_err(|e| format!("{e:?}"))),
            ("valuation", c.map_err(|e| format!("{e:?}"))),
            ("official_calendar", d.map_err(|e| e.to_string())),
            ("gpu_and_forward", e.map_err(|e| format!("{e:?}"))),
        ];
        let mut failed = Vec::new();
        for (name, result) in results {
            match result {
                Ok(v) => {
                    eprintln!(
                        "{name}: status={} sources={} errors={} forward={}",
                        v["status"], v["source"], v["errors"], v["forward"]
                    );
                    if v["status"] == "failed"
                        || v["status"] == "partial"
                        || v["forward"]["status"] == "failed"
                    {
                        failed.push(name);
                    }
                }
                Err(error) => {
                    eprintln!("{name}: FAILED {error}");
                    failed.push(name);
                }
            }
        }
        assert!(failed.is_empty(), "real providers not verified: {failed:?}");
    }
    #[test]
    fn statements_yoy_uses_same_reporting_period_and_preserves_missing() {
        let root = json!({"result":{"data":{"report_list":{"20260630":{"data":[{"item_title":"营业总收入","item_value":"1,200"},{"item_title":"缺失","item_value":"--"}]},"20260331":{"data":[{"item_title":"营业总收入","item_value":"400"}]},"20250630":{"data":[{"item_title":"营业总收入","item_value":"1000"}]}}}}});
        let rows = parse_statement(&root, 8).unwrap();
        assert_eq!(rows[0]["yoy_pct"]["营业总收入"], 20.0);
        assert!(rows[0]["values"]["缺失"].is_null());
        assert!(rows[1]["yoy_pct"]["营业总收入"].is_null());
    }
    #[test]
    fn consensus_requires_real_summary_and_rejects_bad_ranges() {
        let table="<table><tr><th>年度</th><th>预测机构数</th><th>最小值</th><th>均值</th><th>最大值</th></tr><tr><td>2026年</td><td>3</td><td>1</td><td>2</td><td>4</td></tr></table>";
        let rows = parse_consensus(table).unwrap();
        assert_eq!(rows[0]["mean"], 2.0);
        assert_eq!(rows[0]["institutions"], 3);
        assert!(parse_consensus(&table.replace("<td>2</td>", "<td>5</td>")).is_err());
        assert!(parse_consensus("<p>没有覆盖</p>").is_err());
    }
}
