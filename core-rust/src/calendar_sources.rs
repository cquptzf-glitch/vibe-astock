use crate::{calendar, discovery, documents, research, storage};
use anyhow::{anyhow, Result};
use chrono::{Datelike, NaiveDate, Utc};
use futures_util::{stream, StreamExt};
use reqwest::Client;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

pub(crate) fn parse_month(root: &Value, year: i32, month: u32) -> Result<Value> {
    let start = NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(|| anyhow!("月份无效"))?;
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .ok_or_else(|| anyhow!("月份无效"))?;
    let expected = (next - start).num_days() as usize;
    let rows = root["data"]
        .as_array()
        .filter(|r| !r.is_empty())
        .ok_or_else(|| anyhow!("交易所尚未公布本月日历,不能推断为全月休市"))?;
    let mut days = BTreeMap::new();
    for row in rows {
        let raw = row["jyrq"].as_str().unwrap_or("");
        let day = raw
            .get(..10)
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .ok_or_else(|| anyhow!("日历日期格式变化"))?;
        let flag = row["jybz"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| row["jybz"].to_string());
        if day.year() != year || day.month() != month || !matches!(flag.as_str(), "0" | "1") {
            return Err(anyhow!("日历月份或开市标志不匹配"));
        }
        if days.insert(day.to_string(), flag == "1").is_some() {
            return Err(anyhow!("日历有重复日期"));
        }
    }
    if days.len() != expected {
        return Err(anyhow!(
            "本月日历缺日:实际 {} / 应有 {expected}",
            days.len()
        ));
    }
    Ok(
        json!({"month":format!("{year}-{month:02}"),"days":days.into_iter().map(|(date,is_open)|json!({"date":date,"is_open":is_open})).collect::<Vec<_>>(),"source":"深交所官方完整月历","fetched_at":Utc::now().to_rfc3339(),"note":"来自深交所月历,不拿工作日或参考股票K线推断休市。"}),
    )
}

pub async fn month(client: &Client, path: &str) -> Result<Value> {
    let today = crate::emotion::now_china().0;
    let raw =
        discovery::parameter(path, "month").unwrap_or_else(|| today.format("%Y-%m").to_string());
    let start = NaiveDate::parse_from_str(&format!("{raw}-01"), "%Y-%m-%d")?;
    if start.format("%Y-%m").to_string() != raw {
        return Err(anyhow!("月份格式须为 YYYY-MM"));
    }
    let root = discovery::get_json(
        client,
        "https://www.szse.cn/api/report/exchange/onepersistenthour/monthList",
        &[("month", format!("{}-{}", start.year(), start.month()))],
    )
    .await?;
    parse_month(&root, start.year(), start.month())
}

pub(crate) fn appointments(root: &Value, symbol: &str) -> Result<Vec<Value>> {
    if root["success"] != true || root.get("result").is_none() {
        return Err(anyhow!("预约披露接口返回失败或格式变化"));
    }
    if root["result"].is_null() {
        return Ok(vec![]);
    }
    let rows = root
        .pointer("/result/data")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("预约披露缺少 data"))?;
    let mut out = Vec::new();
    for r in rows {
        if r["SECURITY_CODE"] != symbol {
            continue;
        }
        let valid = |key: &str| {
            r[key]
                .as_str()
                .and_then(|s| s.get(..10))
                .filter(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok())
                .map(str::to_string)
        };
        let Some(period) = valid("REPORT_DATE") else {
            continue;
        };
        let actual = valid("ACTUAL_PUBLISH_DATE");
        let date = actual.clone().or_else(|| valid("APPOINT_PUBLISH_DATE"));
        let Some(date) = date else { continue };
        out.push(json!({"date":date,"kind":"company","title":format!("{symbol} {period} 定期报告{}",if actual.is_some(){"已披露"}else{"预约披露"}),"detail":"预约日期可能变化,以公司公告为准。","symbol":symbol,"status":if actual.is_some(){"done"}else{"open"},"source":format!("eastmoney:appointment:{symbol}:{period}")}));
    }
    if !rows.is_empty() && out.is_empty() {
        return Err(anyhow!("披露表有数据但日期/股票均无法匹配"));
    }
    Ok(out)
}

pub async fn sync(client: &Client, db: &Path, body: &Value) -> Result<Value> {
    let explicit = body["symbol"].as_str().filter(|s| !s.trim().is_empty());
    let symbols: Vec<String> = if let Some(s) = explicit {
        vec![crate::normalize_stock_symbol(s)?.digits]
    } else {
        storage::list(db)?["items"]
            .as_array()
            .into_iter()
            .flatten()
            .take(20)
            .filter_map(|r| r["symbol"].as_str().map(str::to_string))
            .collect()
    };
    if symbols.is_empty() {
        return Err(anyhow!("请填写股票代码或先添加自选"));
    }
    let results = stream::iter(symbols.into_iter().map(|symbol| async move {
        let filter = format!("(SECURITY_CODE=\"{symbol}\")");
        let params = [
            ("reportName", "RPT_PUBLIC_BS_APPOIN".into()),
            ("columns", "ALL".into()),
            ("filter", filter),
            ("sortColumns", "REPORT_DATE".into()),
            ("sortTypes", "-1".into()),
            ("pageSize", "12".into()),
            ("pageNumber", "1".into()),
        ];
        let path = format!("/?symbol={symbol}&limit=10");
        let (a, b) = tokio::join!(
            discovery::get_json(
                client,
                "https://datacenter-web.eastmoney.com/api/data/v1/get",
                &params
            ),
            research::announcements(client, &path)
        );
        (symbol, a, b)
    }))
    .buffer_unordered(4)
    .collect::<Vec<_>>()
    .await;
    let mut events = Vec::new();
    let mut gaps = Vec::new();
    for (symbol, a, b) in results {
        match a.and_then(|v| appointments(&v, &symbol)) {
            Ok(rows) => events.extend(rows),
            Err(e) => {
                gaps.push(json!({"source":format!("{symbol}预约披露"),"message":e.to_string()}))
            }
        }
        match b {
            Ok(data) => {
                for a in data["items"].as_array().into_iter().flatten() {
                    if let (Some(date), Some(code), Some(title)) = (
                        a["date"].as_str().and_then(|s| s.get(..10)),
                        a["art_code"].as_str(),
                        a["title"].as_str(),
                    ) {
                        if NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok() {
                            events.push(json!({"date":date,"kind":"company","title":title,"detail":a["pdf_url"].as_str().unwrap_or(""),"symbol":symbol,"source":format!("eastmoney:announcement:{code}"),"status":"open"}));
                        }
                    }
                }
            }
            Err(e) => {
                gaps.push(json!({"source":format!("{symbol}公告"),"message":format!("{e:?}")}))
            }
        }
    }
    let conn = rusqlite::Connection::open(db)?;
    let mut saved = Vec::new();
    let mut unique = HashSet::new();
    for mut event in events {
        let source = event["source"].as_str().unwrap_or("").to_string();
        if !unique.insert(source.clone()) {
            continue;
        }
        let existing: Option<(i64, String)> = conn
            .query_row(
                "SELECT id,status FROM mobile_calendar_events WHERE source=?1 LIMIT 1",
                [source],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        if let Some((id, status)) = existing {
            event["id"] = json!(id);
            if event["status"] != "done" {
                event["status"] = json!(status);
            }
        }
        saved.push(calendar::save(db, &event)?);
    }
    Ok(
        json!({"count":saved.len(),"items":saved,"gaps":gaps,"status":if gaps.is_empty(){"ok"}else{"partial"},"note":"同源公告/同股票报告期幂等更新,保留已处理状态;一次最多同步前二十只自选,也可逐只指定。"}),
    )
}

pub fn set_reminder(db: &Path, body: &Value) -> Result<Value> {
    use rusqlite::OptionalExtension;
    let id = body["id"]
        .as_i64()
        .filter(|id| *id > 0)
        .ok_or_else(|| anyhow!("需要有效事件编号"))?;
    let hour = body["hour"].as_u64().unwrap_or(9);
    if hour > 23 {
        return Err(anyhow!("提醒小时须为 0–23"));
    }
    let conn = rusqlite::Connection::open(db)?;
    let title: String = conn.query_row(
        "SELECT title FROM mobile_calendar_events WHERE id=?1",
        [id],
        |r| r.get(0),
    )?;
    let source = format!("calendar_reminder:{id}");
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM mobile_documents WHERE source=?1",
            [&source],
            |r| r.get(0),
        )
        .optional()?;
    documents::save(
        db,
        &json!({"id":existing,"kind":"note","title":format!("事件提醒:{title}"),"category":"事件提醒设置","source":source,"body":json!({"event_id":id,"hour":hour,"enabled":body["enabled"].as_bool().unwrap_or(true)}).to_string()}),
    )?;
    Ok(json!({"id":id,"enabled":body["enabled"].as_bool().unwrap_or(true),"hour":hour}))
}

pub fn notification_rules(db: &Path) -> Result<Vec<Value>> {
    let conn = rusqlite::Connection::open(db)?;
    // Standalone reminder contract tests intentionally create only their own table.
    let tables:i64=conn.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('mobile_documents','mobile_calendar_events')",[],|r|r.get(0))?;
    if tables < 2 {
        return Ok(vec![]);
    }
    let mut stmt=conn.prepare("SELECT e.id,e.date,e.title,e.detail,d.body FROM mobile_calendar_events e JOIN mobile_documents d ON d.source='calendar_reminder:'||e.id WHERE e.status='open'")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut rules = Vec::new();
    for row in rows {
        let (id, date, title, detail, setting) = row?;
        let config: Value = serde_json::from_str(&setting)?;
        if config["enabled"] != true {
            continue;
        }
        rules.push(json!({"id":1_000_000_000_i64+id,"kind":"calendar_event","date":date,"name":title,"detail":detail.chars().take(200).collect::<String>(),"review_hour":config["hour"],"enabled":true,"route":"/mobile/discovery/calendar"}));
    }
    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar_reminder_round_trips_through_backup_and_stops_when_event_is_done() {
        let app = crate::AppCore::new(std::env::temp_dir().join(format!(
            "calendar-reminder-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        )))
        .unwrap();
        let db = app.db_path();
        let event = crate::calendar::save(
            db,
            &json!({"date":"2026-10-20","kind":"company","title":"合成财报事件"}),
        )
        .unwrap();
        let id = event["id"].as_i64().unwrap();
        set_reminder(db, &json!({"id":id,"hour":9,"enabled":true})).unwrap();
        let rules = notification_rules(db).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["date"], "2026-10-20");
        assert_eq!(rules[0]["review_hour"], 9);
        let backup = crate::backup::export(db).unwrap();
        let other = crate::AppCore::new(std::env::temp_dir().join(format!(
            "calendar-restored-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        )))
        .unwrap();
        crate::backup::restore(
            other.db_path(),
            &json!({"backup":backup,"mode":"replace","confirm":true}),
        )
        .unwrap();
        assert_eq!(notification_rules(other.db_path()).unwrap(), rules);
        crate::calendar::save(db,&json!({"id":id,"date":"2026-10-20","kind":"company","title":"合成财报事件","status":"done"})).unwrap();
        assert!(notification_rules(db).unwrap().is_empty());
    }
    #[test]
    fn complete_calendar_refuses_missing_or_wrong_days() {
        let rows: Vec<Value> = (1..=28)
            .map(|d| json!({"jyrq":format!("2026-02-{d:02}"),"jybz":if d==1{"0"}else{"1"}}))
            .collect();
        let mut root = json!({"data":rows});
        assert_eq!(
            parse_month(&root, 2026, 2).unwrap()["days"][0]["is_open"],
            false
        );
        root["data"].as_array_mut().unwrap().pop();
        assert!(parse_month(&root, 2026, 2).is_err());
        assert!(parse_month(&json!({"data":[]}), 2026, 2).is_err());
    }
    #[test]
    fn appointment_status_and_identity_are_not_guessed() {
        let root = json!({"success":true,"result":{"data":[{"SECURITY_CODE":"600519","REPORT_DATE":"2026-06-30","APPOINT_PUBLISH_DATE":"2026-08-20","ACTUAL_PUBLISH_DATE":"2026-08-19"}]}});
        let rows = appointments(&root, "600519").unwrap();
        assert_eq!(rows[0]["date"], "2026-08-19");
        assert_eq!(rows[0]["status"], "done");
        assert!(appointments(&root, "000001").is_err());
        assert!(appointments(&json!({"success":false}), "600519").is_err());
        assert!(
            appointments(&json!({"success":true,"result":null}), "600519")
                .unwrap()
                .is_empty()
        );
    }
}
