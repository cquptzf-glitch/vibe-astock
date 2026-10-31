use crate::discovery;
use anyhow::{anyhow, Result};
use chrono::{NaiveDate, Utc};
use reqwest::Client;
use serde_json::{json, Value};
fn module(title: &str) -> Option<&'static str> {
    let t = title.to_lowercase();
    for (name, words) in [
        (
            "货币政策",
            &[
                "fed ",
                "federal reserve",
                "interest rate",
                "rate cut",
                "fomc",
            ] as &[&str],
        ),
        (
            "宏观经济",
            &[
                "inflation",
                "cpi",
                "pce",
                "gdp",
                "recession",
                "unemployment",
            ],
        ),
        (
            "AI科技",
            &[
                "artificial intelligence",
                "openai",
                "nvidia",
                "agi ",
                "b200",
                "h100",
            ],
        ),
        (
            "股指大宗",
            &["s&p 500", "nasdaq", "gold price", "oil price"],
        ),
        (
            "地缘政治",
            &["tariff", "ceasefire", "ukraine", "taiwan", "trade war"],
        ),
        (
            "政治选举",
            &[
                "presidential election",
                "midterm election",
                "prime minister",
            ],
        ),
    ] {
        if words.iter().any(|w| t.contains(w)) {
            return Some(name);
        }
    }
    None
}
fn array(v: &Value) -> Option<Vec<Value>> {
    v.as_array()
        .cloned()
        .or_else(|| v.as_str().and_then(|s| serde_json::from_str(s).ok()))
}
fn date(raw: Option<&Value>) -> Option<String> {
    let s = raw?.as_str()?.get(..10)?;
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    Some(s.into())
}
pub(crate) fn parse_poly(root: &Value) -> Result<Vec<Value>> {
    let rows = root
        .as_array()
        .ok_or_else(|| anyhow!("Polymarket 没有市场数组"))?;
    let today = crate::emotion::now_china().0;
    let end = (today + chrono::Duration::days(730)).to_string();
    let today = today.to_string();
    let mut out = Vec::new();
    for r in rows {
        let title = r["question"].as_str().unwrap_or("");
        let Some(group) = module(title) else { continue };
        let (Some(outcomes), Some(prices)) = (array(&r["outcomes"]), array(&r["outcomePrices"]))
        else {
            continue;
        };
        if outcomes.len() != prices.len() {
            continue;
        }
        let Some(i) = outcomes
            .iter()
            .position(|x| x.as_str().is_some_and(|s| s.eq_ignore_ascii_case("yes")))
        else {
            continue;
        };
        let Some(prob) = discovery::number(prices.get(i)).filter(|p| (0.0..=1.0).contains(p))
        else {
            continue;
        };
        let Some(close) = date(r.get("endDate")).filter(|s| s >= &today && s <= &end) else {
            continue;
        };
        let slug = r["slug"]
            .as_str()
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
        out.push(json!({"id":r["id"],"module":group,"venue":"Polymarket","title":title,"leg":"Yes","prob":prob,"price_type":"outcome_price","volume_24h":discovery::number(r.get("volume24hr")),"close":close,"url":slug.map(|s|format!("https://polymarket.com/event/{s}"))}));
    }
    Ok(out)
}
fn parse_kalshi(root: &Value) -> Result<Vec<Value>> {
    let events = root["events"]
        .as_array()
        .ok_or_else(|| anyhow!("Kalshi 没有 events 数组"))?;
    let today = crate::emotion::now_china().0.to_string();
    let mut out = Vec::new();
    for e in events {
        let title = e["title"].as_str().unwrap_or("");
        let Some(group) = module(title) else { continue };
        for m in e["markets"].as_array().into_iter().flatten() {
            let p = |d: &str, c: &str| {
                discovery::number(m.get(d))
                    .or_else(|| discovery::number(m.get(c)).map(|v| v / 100.0))
            };
            let (prob, price_type) = match (
                p("yes_bid_dollars", "yes_bid"),
                p("yes_ask_dollars", "yes_ask"),
            ) {
                (Some(b), Some(a)) if 0.0 <= b && b <= a && a <= 1.0 && a > 0.0 && b < 1.0 => {
                    ((a + b) / 2.0, "bid_ask_mid")
                }
                _ => match p("last_price_dollars", "last_price") {
                    Some(v) if (0.0..=1.0).contains(&v) => (v, "last_trade"),
                    _ => continue,
                },
            };
            let Some(close) = date(m.get("close_time")).filter(|s| s >= &today) else {
                continue;
            };
            out.push(json!({"id":m["ticker"],"module":group,"venue":"Kalshi","title":title,"leg":m["yes_sub_title"].as_str().unwrap_or("Yes"),"prob":prob,"price_type":price_type,"volume_24h":discovery::number(m.get("volume_24h_fp")),"close":close,"url":"https://kalshi.com/markets"}));
        }
    }
    Ok(out)
}

async fn scan(client: &Client, poly: bool) -> (Vec<Value>, Vec<Value>) {
    use futures_util::{stream, StreamExt};
    let name = if poly { "Polymarket" } else { "Kalshi" };
    let mut queries: Vec<(String, Vec<(&str, String)>)> = Vec::new();
    let mut gaps = Vec::new();
    if poly {
        for offset in [0, 100, 200] {
            queries.push((
                "https://gamma-api.polymarket.com/markets".into(),
                vec![
                    ("active", "true".into()),
                    ("closed", "false".into()),
                    ("limit", "100".into()),
                    ("offset", offset.to_string()),
                    ("order", "volume24hr".into()),
                    ("ascending", "false".into()),
                ],
            ));
        }
        match discovery::get_json(
            client,
            "https://gamma-api.polymarket.com/tags",
            &[("limit", "500".into())],
        )
        .await
        {
            Ok(tags) => match tags.as_array() {
                Some(tags) => {
                    let selected: Vec<_> = tags
                        .iter()
                        .filter(|t| {
                            let text = format!(
                                "{} {}",
                                t["label"].as_str().unwrap_or(""),
                                t["slug"].as_str().unwrap_or("")
                            )
                            .to_lowercase();
                            [
                                "econom",
                                "inflation",
                                "fed",
                                "interest",
                                "gdp",
                                "recession",
                                "employment",
                            ]
                            .iter()
                            .any(|w| text.contains(w))
                        })
                        .filter_map(|t| {
                            t["id"]
                                .as_str()
                                .map(str::to_owned)
                                .or_else(|| t["id"].as_u64().map(|v| v.to_string()))
                        })
                        .collect();
                    if tags.len() >= 500 || selected.len() > 8 {
                        gaps.push(json!({"source":name,"message":"标签发现或定向查询达到上限，不能视为全量覆盖"}));
                    }
                    for id in selected.into_iter().take(8) {
                        queries.push((
                            "https://gamma-api.polymarket.com/markets".into(),
                            vec![
                                ("tag_id", id),
                                ("active", "true".into()),
                                ("closed", "false".into()),
                                ("limit", "100".into()),
                            ],
                        ));
                    }
                }
                None => {
                    gaps.push(json!({"source":name,"message":"标签响应结构变化，未执行定向查询"}))
                }
            },
            Err(e) => gaps.push(json!({"source":name,"message":format!("宏观标签发现失败：{e}")})),
        }
    } else {
        queries.push((
            "https://api.elections.kalshi.com/trade-api/v2/events".into(),
            vec![
                ("status", "open".into()),
                ("with_nested_markets", "true".into()),
                ("limit", "200".into()),
            ],
        ));
        for category in ["Economics", "Financials"] {
            match discovery::get_json(
                client,
                "https://api.elections.kalshi.com/trade-api/v2/series",
                &[("category", category.into()), ("limit", "1000".into())],
            )
            .await
            {
                Ok(root) => match root["series"].as_array() {
                    Some(rows) => {
                        let selected: Vec<_> = rows
                            .iter()
                            .filter(|r| module(r["title"].as_str().unwrap_or("")).is_some())
                            .filter_map(|r| r["ticker"].as_str())
                            .collect();
                        if rows.len() >= 1000 || selected.len() > 6 {
                            gaps.push(json!({"source":name,"message":format!("{category} 系列发现或定向查询达到上限")}));
                        }
                        for ticker in selected.into_iter().take(6) {
                            queries.push((
                                "https://api.elections.kalshi.com/trade-api/v2/events".into(),
                                vec![
                                    ("series_ticker", ticker.into()),
                                    ("status", "open".into()),
                                    ("with_nested_markets", "true".into()),
                                    ("limit", "100".into()),
                                ],
                            ));
                        }
                    }
                    None => gaps
                        .push(json!({"source":name,"message":"系列响应结构变化，未执行定向查询"})),
                },
                Err(e) => gaps
                    .push(json!({"source":name,"message":format!("{category} 系列发现失败：{e}")})),
            }
        }
    }
    // Build owned futures before awaiting: Tauri commands require a Send future
    // for every caller lifetime, including on Android.
    async fn query(
        client: Client,
        url: String,
        args: Vec<(&'static str, String)>,
    ) -> Result<Value> {
        discovery::get_json(&client, &url, &args).await
    }
    let jobs: Vec<_> = queries
        .into_iter()
        .map(|(url, args)| query(client.clone(), url, args))
        .collect();
    let results = stream::iter(jobs)
        .buffer_unordered(4)
        .collect::<Vec<_>>()
        .await;
    let mut items = Vec::new();
    for result in results {
        match result.and_then(|v| {
            if !poly && v["cursor"].as_str().is_some_and(|s| !s.is_empty()) {
                gaps.push(
                    json!({"source":name,"message":"开放事件返回后续游标，本次窗口未穷尽所有事件"}),
                );
            }
            if poly {
                parse_poly(&v)
            } else {
                parse_kalshi(&v)
            }
        }) {
            Ok(rows) => items.extend(rows),
            Err(e) => gaps.push(json!({"source":name,"message":e.to_string()})),
        }
    }
    (items, gaps)
}
pub async fn fetch(client: &Client) -> Result<Value> {
    let (poly, kalshi) = tokio::join!(
        tokio::time::timeout(std::time::Duration::from_secs(75), scan(client, true)),
        tokio::time::timeout(std::time::Duration::from_secs(75), scan(client, false))
    );
    let mut items = Vec::new();
    let mut failures = Vec::new();
    for (name, result) in [("Polymarket", poly), ("Kalshi", kalshi)] {
        match result {
            Ok((rows, gaps)) => {
                items.extend(rows);
                failures.extend(gaps);
            }
            Err(_) => failures
                .push(json!({"source":name,"message":"取数超过本轮七十五秒预算，请稍后重试"})),
        }
    }
    let mut seen = std::collections::HashSet::new();
    items.retain(|r| seen.insert(format!("{}:{}", r["venue"], r["id"])));
    items.sort_by(|a, b| {
        discovery::number(b.get("volume_24h"))
            .unwrap_or(-1.0)
            .total_cmp(&discovery::number(a.get("volume_24h")).unwrap_or(-1.0))
    });
    let matched = items.len();
    items.truncate(100);
    Ok(
        json!({"status":if items.is_empty()&&!failures.is_empty(){"failed"}else{"partial"},"items":items,"matched":matched,"failures":failures,"sampled_at":Utc::now().to_rfc3339(),"scope":"Polymarket 前三页开放市场及最多八个现查宏观标签；Kalshi 开放事件及 Economics/Financials 每类最多六个现查相关系列。按合约去重，最多显示一百条；有界扫描，不代表全量。","note":"这是当前市场定价概率，不是事件事实或本软件预测。记录报价类型、结算日期与24小时成交量；低流动性/最后成交可能陈旧，不作为买卖信号。"}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn yes_outcome_is_selected_by_label_not_position() {
        let root = json!([{"id":"test","question":"Will GDP fall?","outcomes":"[\"No\",\"Yes\"]","outcomePrices":"[\"0.8\",\"0.2\"]","endDate":"2027-01-01","volume24hr":100}]);
        let rows = parse_poly(&root).unwrap();
        assert_eq!(rows[0]["prob"], 0.2);
        assert_eq!(rows[0]["module"], "宏观经济");
        let mut bad = root;
        bad[0]["outcomes"] = json!(["Unknown"]);
        assert!(parse_poly(&bad).unwrap().is_empty());
    }
}
