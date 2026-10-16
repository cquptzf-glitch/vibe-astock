use crate::discovery::{self, DiscoveryError};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use futures_util::future::join_all;
use reqwest::Client;
use roxmltree::Node;
use serde_json::{json, Value};
use std::collections::HashSet;

const CATALOG: &str = include_str!("../../datasources/rss_sources.json");

pub fn catalog() -> Result<Value> {
    let config: Value = serde_json::from_str(CATALOG)?;
    let sources: Vec<Value> = config["sources"]
        .as_array()
        .ok_or_else(|| anyhow!("RSS 源配置缺失"))?
        .iter()
        .filter(|source| {
            source["type"] == "rss"
                && source["url"]
                    .as_str()
                    .is_some_and(|url| url.starts_with("https://"))
        })
        .map(|source| json!({"name": source["name"], "industry": source["hint"]}))
        .collect();
    Ok(json!({"industries": config["industries"], "sources": sources, "batch_limit": 4}))
}

pub async fn fetch(client: &Client, path: &str) -> Result<Value, DiscoveryError> {
    let config: Value = serde_json::from_str(CATALOG).map_err(anyhow::Error::from)?;
    let industry = discovery::parameter(path, "industry").unwrap_or_else(|| "ai".into());
    if !config["industries"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["key"] == industry))
    {
        return Err(DiscoveryError::BadRequest("未知资讯行业".into()));
    }
    let days = discovery::integer(path, "days", 7, 1, 30)?;
    let source = discovery::parameter(path, "source").filter(|source| !source.is_empty());
    let pool: Vec<Value> = config["sources"]
        .as_array()
        .ok_or_else(|| anyhow!("RSS 源配置缺失"))?
        .iter()
        .filter(|row| {
            row["hint"] == industry
                && row["type"] == "rss"
                && row["url"]
                    .as_str()
                    .is_some_and(|url| url.starts_with("https://"))
                && source
                    .as_ref()
                    .is_none_or(|name| row["name"] == name.as_str())
        })
        .take(4)
        .cloned()
        .collect();
    if pool.is_empty() {
        return Err(DiscoveryError::BadRequest(
            "该行业没有匹配的 HTTPS RSS 源".into(),
        ));
    }
    let results = join_all(pool.iter().map(|source| async {
        let name = source["name"].as_str().unwrap_or_default().to_string();
        let result = async {
            let bytes =
                discovery::get_bytes(client, source["url"].as_str().unwrap_or_default(), &[])
                    .await?;
            let text = std::str::from_utf8(&bytes)?;
            parse_feed(text, 20)
        }
        .await;
        (name, result)
    }))
    .await;
    let redlines: Vec<String> = config["redline_keywords"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    Ok(aggregate(&industry, days, results, &redlines, Utc::now()))
}

fn child_text(node: Node<'_, '_>, names: &[&str]) -> String {
    for name in names {
        if let Some(child) = node
            .children()
            .find(|child| child.is_element() && child.tag_name().name() == *name)
        {
            let text: String = child
                .descendants()
                .filter(|part| part.is_text())
                .filter_map(|part| part.text())
                .collect();
            if !text.trim().is_empty() {
                return text.trim().to_string();
            }
        }
    }
    String::new()
}

fn strip_markup(text: &str) -> String {
    let mut result = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let tag_start = after.chars().next().is_some_and(|character| {
            character.is_ascii_alphabetic() || matches!(character, '/' | '!')
        });
        if let Some(end) = after.find('>').filter(|_| tag_start) {
            rest = &after[end + 1..];
        } else {
            result.push('<');
            rest = after;
        }
    }
    result.push_str(rest);
    result
}

pub(crate) fn safe_link(raw: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw.trim()).ok()?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    Some(url.to_string())
}

pub(crate) fn parse_feed(text: &str, limit: usize) -> Result<Vec<Value>> {
    let document = roxmltree::Document::parse(text.trim_start_matches('\u{feff}'))?;
    if !matches!(
        document.root_element().tag_name().name(),
        "rss" | "feed" | "RDF"
    ) {
        return Err(anyhow!("响应不是 RSS / Atom"));
    }
    let mut items = Vec::new();
    for entry in document
        .descendants()
        .filter(|node| node.is_element() && matches!(node.tag_name().name(), "item" | "entry"))
    {
        let title = html_escape::decode_html_entities(&child_text(entry, &["title"]))
            .trim()
            .chars()
            .take(500)
            .collect::<String>();
        if title.is_empty() {
            continue;
        }
        let alternate = entry
            .children()
            .find(|node| {
                node.is_element()
                    && node.tag_name().name() == "link"
                    && node.attribute("rel") == Some("alternate")
            })
            .and_then(|node| node.attribute("href"));
        let link_node = entry
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "link");
        let raw_link = alternate.map(str::to_string).unwrap_or_else(|| {
            link_node
                .and_then(|node| node.attribute("href"))
                .map(str::to_string)
                .unwrap_or_else(|| child_text(entry, &["link", "guid"]))
        });
        let raw_date = child_text(entry, &["pubDate", "published", "updated", "date"]);
        let published = DateTime::parse_from_rfc2822(&raw_date)
            .or_else(|_| DateTime::parse_from_rfc3339(&raw_date))
            .ok()
            .map(|date| date.with_timezone(&Utc).to_rfc3339());
        let summary = html_escape::decode_html_entities(&strip_markup(&child_text(
            entry,
            &["description", "summary", "content"],
        )))
        .chars()
        .take(300)
        .collect::<String>();
        items.push(json!({"title": title, "link": safe_link(&raw_link), "published": published, "summary": summary}));
        if items.len() >= limit {
            break;
        }
    }
    Ok(items)
}

fn aggregate(
    industry: &str,
    days: usize,
    results: Vec<(String, Result<Vec<Value>>)>,
    redlines: &[String],
    now: DateTime<Utc>,
) -> Value {
    let tried = results.len();
    let mut failures = Vec::new();
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut ok = 0;
    let mut future_items = 0;
    for (source, result) in results {
        match result {
            Err(error) => failures.push(discovery::failure(&source, &error)),
            Ok(rows) => {
                ok += 1;
                for mut item in rows {
                    let date = item["published"]
                        .as_str()
                        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok());
                    if let Some(date) = date {
                        if date < now - Duration::days(days as i64) {
                            continue;
                        }
                        if date > now + Duration::minutes(5) {
                            future_items += 1;
                            continue;
                        }
                    }
                    let dedup = item["link"]
                        .as_str()
                        .unwrap_or_else(|| item["title"].as_str().unwrap_or_default())
                        .to_string();
                    if !seen.insert(dedup) {
                        continue;
                    }
                    let text = format!(
                        "{} {}",
                        item["title"].as_str().unwrap_or_default(),
                        item["summary"].as_str().unwrap_or_default()
                    );
                    let hits: Vec<&String> = redlines
                        .iter()
                        .filter(|keyword| text.contains(keyword.as_str()))
                        .collect();
                    item["redline"] = json!(hits);
                    item["source"] = json!(source);
                    item["industry"] = json!(industry);
                    items.push(item);
                }
            }
        }
    }
    items.sort_by(|left, right| right["published"].as_str().cmp(&left["published"].as_str()));
    let status = if ok == 0 {
        "failed"
    } else if failures.is_empty() {
        "ok"
    } else {
        "partial"
    };
    json!({"status": status, "industry": industry, "recent_days": days, "sources_tried": tried, "sources_ok": ok,
        "source_limit": 4, "items": items, "failures": failures, "future_items_dropped": future_items, "fetched_at": now.to_rfc3339(),
        "message": if ok == 0 { "全部资讯源获取失败，请重试或切换来源" } else { "" }})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_atom_entities_and_alternate_link() {
        let rows = parse_feed(include_str!("../tests/fixtures/news_atom.xml"), 20).unwrap();
        assert_eq!(rows[0]["title"], "‘Pure insanity’: OpenAI’s latest drop");
        assert_eq!(rows[0]["summary"], "R&D — 3 < 5");
        assert_eq!(rows[0]["link"], "https://v.com/1");
    }
    #[test]
    fn legacy_rss_double_escape() {
        let rows = parse_feed(include_str!("../tests/fixtures/news_rss.xml"), 20).unwrap();
        assert_eq!(rows[0]["title"], "Nvidia’s racks ship to AT&T");
        assert_eq!(rows[0]["published"], "2026-10-09T12:00:00+00:00");
        assert!(parse_feed("<html>denied</html>", 20).is_err());
        assert!(parse_feed("<rss><channel/></rss>", 20).unwrap().is_empty());
        assert_eq!(safe_link("javascript:alert(1)"), None);
        assert_eq!(strip_markup("成本 < 5，利润 > 1"), "成本 < 5，利润 > 1");
        assert_eq!(safe_link("https://user:password@example.com"), None);
    }
    #[test]
    fn partial_empty_failed_dedup_and_time_filter_are_distinct() {
        let now = DateTime::parse_from_rfc3339("2026-10-10T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let rows = parse_feed(include_str!("../tests/fixtures/news_rss.xml"), 20).unwrap();
        let result = aggregate(
            "ai",
            7,
            vec![
                ("a".into(), Ok(rows.clone())),
                ("b".into(), Ok(rows)),
                ("c".into(), Err(anyhow!("timeout"))),
            ],
            &[],
            now,
        );
        assert_eq!(result["status"], "partial");
        assert_eq!(result["items"].as_array().unwrap().len(), 1);
        assert_eq!(result["sources_ok"], 2);
        assert_eq!(
            aggregate("ai", 7, vec![("a".into(), Ok(vec![]))], &[], now)["status"],
            "ok"
        );
        assert_eq!(
            aggregate("ai", 7, vec![("a".into(), Err(anyhow!("down")))], &[], now)["status"],
            "failed"
        );
        let future = vec![json!({"title":"future","link":null,"published":"2026-10-11T00:00:00Z"})];
        let result = aggregate("ai", 7, vec![("a".into(), Ok(future))], &[], now);
        assert_eq!(result["future_items_dropped"], 1);
        assert_eq!(result["items"].as_array().unwrap().len(), 0);
        assert!(catalog().unwrap()["sources"].as_array().unwrap().len() > 30);
    }
}
