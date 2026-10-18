//! User-curated industry measurements, explicitly attached to research snapshots.
use anyhow::{anyhow, ensure, Result};
use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
pub(crate) const PREFIX: &str = "mobile_industry_fact:";
const METRICS: [&str; 8] = [
    "market_size",
    "capacity",
    "production",
    "yield",
    "market_share",
    "lead_time",
    "price",
    "other",
];
fn text(v: &Value, key: &str, max: usize) -> Result<String> {
    let s = v[key].as_str().unwrap_or("").trim();
    ensure!(
        !s.is_empty() && s.chars().count() <= max,
        "{key} 必填且最多 {max} 字"
    );
    Ok(s.into())
}
fn day(s: &str) -> Result<NaiveDate> {
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")?;
    ensure!(d.to_string() == s, "日期必须为 YYYY-MM-DD");
    Ok(d)
}
pub(crate) fn sector(key: &str) -> Result<Value> {
    let data: Value = serde_json::from_str(include_str!(
        "../../desktop/src/verticals/finance/data/sectors.json"
    ))?;
    data["sectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == key)
        .cloned()
        .ok_or_else(|| anyhow!("产业赛道不在目录内"))
}
fn normalize(v: &Value) -> Result<Value> {
    let key = text(v, "sector", 64)?;
    sector(&key)?;
    let metric = text(v, "metric", 40)?;
    ensure!(METRICS.contains(&metric.as_str()), "指标类型不在目录内");
    let entity = text(v, "entity", 120)?;
    let label = text(v, "label", 120)?;
    let unit = text(v, "unit", 40)?;
    let currency = text(v, "currency", 8)?;
    ensure!(
        ["CNY", "USD", "HKD", "TWD", "EUR", "none"].contains(&currency.as_str()),
        "币种不在目录内"
    );
    let value = v["value"]
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .ok_or_else(|| anyhow!("value 必须是非负有限数，不能用空或布尔代替"))?;
    if metric == "yield" || metric == "market_share" {
        ensure!(
            unit == "%" && value <= 100.,
            "良率和份额必须为 0–100 的百分数"
        );
    }
    let period = text(v, "period_end", 10)?;
    let published = text(v, "published_date", 10)?;
    let today = crate::emotion::now_china().0;
    ensure!(
        day(&period)? <= today && day(&published)? <= today,
        "已实现数据的统计期和发布日期不能在未来"
    );
    ensure!(
        published >= period,
        "发布日期不能早于实际观测期末，预测/目标不能作为已实现数据导入"
    );
    let source = text(v, "source_name", 120)?;
    let link = text(v, "source_url", 1500)?;
    let excerpt = text(v, "source_excerpt", 2000)?;
    let url = reqwest::Url::parse(&link)?;
    ensure!(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none(),
        "出处须为公开 HTTPS 链接，不能带用户凭据"
    );
    ensure!(
        !url.query_pairs().any(
            |(k, _)| ["token", "key", "api_key", "access_token", "password"]
                .contains(&k.to_ascii_lowercase().as_str())
        ),
        "出处链接不能带凭据参数"
    );
    Ok(
        json!({"sector":key,"metric":metric,"entity":entity,"label":label,"value":value,"unit":unit,"currency":currency,"period_end":period,"published_date":published,"source_name":source,"source_url":url.as_str(),"source_excerpt":excerpt,"verification":"user_curated","scope":"用户整理的来源数据；程序未独立核验原文、数值口径或来源真伪。不同口径不能直接比较。"}),
    )
}
pub fn catalog() -> Value {
    json!({"schema":1,"metrics":METRICS,"required":["sector","metric","entity","label","value","unit","currency","period_end","published_date","source_name","source_url","source_excerpt"],"rule":"仅保存有出处的已实现观测，预测/目标不得混为实际值；同比等运算由受控计算执行。"})
}
pub fn list(db: &Path, key: &str, asof: Option<&str>) -> Result<Value> {
    sector(key)?;
    let cutoff = asof
        .map(str::to_owned)
        .unwrap_or_else(|| crate::emotion::now_china().0.to_string());
    day(&cutoff)?;
    let c = Connection::open(db)?;
    let mut stmt=c.prepare("SELECT id,body FROM mobile_documents WHERE source=?1 AND status='active' ORDER BY id DESC LIMIT 2001")?;
    let rows = stmt.query_map([format!("{PREFIX}{key}")], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut records = Vec::new();
    let mut errors = Vec::new();
    for (index, row) in rows.enumerate() {
        ensure!(index < 2000, "产业资料记录超出读取预算");
        let (id, s) = row?;
        match serde_json::from_str::<Value>(&s)
            .map_err(anyhow::Error::from)
            .and_then(|v| normalize(&v))
        {
            Ok(mut v)
                if v["published_date"].as_str().unwrap() <= cutoff.as_str()
                    && v["period_end"].as_str().unwrap() <= cutoff.as_str() =>
            {
                v["id"] = json!(id);
                records.push(v)
            }
            Ok(_) => {}
            Err(e) => errors.push(json!({"id":id,"reason":e.to_string()})),
        }
    }
    ensure!(records.len() <= 2000, "产业资料记录超出读取预算");
    records.sort_by(|a, b| {
        b["period_end"]
            .as_str()
            .cmp(&a["period_end"].as_str())
            .then(a["entity"].as_str().cmp(&b["entity"].as_str()))
    });
    let mut grouped = std::collections::BTreeMap::<String, Vec<&Value>>::new();
    for r in &records {
        let k = json!([
            r["entity"],
            r["metric"],
            r["label"],
            r["unit"],
            r["currency"],
            r["period_end"]
        ])
        .to_string();
        grouped.entry(k).or_default().push(r);
    }
    let conflicts:Vec<Value>=grouped.values().filter(|rs|rs.iter().any(|r|r["value"]!=rs[0]["value"])).map(|rs|json!({"entity":rs[0]["entity"],"label":rs[0]["label"],"period_end":rs[0]["period_end"],"unit":rs[0]["unit"],"records":rs.iter().map(|r|json!({"id":r["id"],"value":r["value"],"source_url":r["source_url"]})).collect::<Vec<_>>(),"reason":"同一统计键出现不同数值；需核对原文、口径和更正版本，不自动取平均或覆盖。"})).collect();
    Ok(
        json!({"schema":1,"sector":key,"as_of":cutoff,"status":if errors.is_empty()&&conflicts.is_empty(){"ok"}else{"partial"},"records":records,"errors":errors,"conflicts":conflicts,"verification":"user_curated","note":"这些数值来自用户整理，出处和摘录保留，未由程序独立核验。仅在研究中明确开启后附带。"}),
    )
}
/// Keep a usable, explicit source snapshot within the research evidence budget.
/// Full observations stay in the library and backup; this is not report truncation.
pub(crate) fn research_snapshot(db: &Path, key: &str, asof: Option<&str>) -> Result<Value> {
    let full = list(db, key, asof)?;
    let rows = full["records"].as_array().unwrap();
    let mut selected = Vec::new();
    let mut shortened = 0;
    for original in rows {
        if selected.len() == 20 {
            break;
        }
        let mut row = original.clone();
        let quote = row["source_excerpt"].as_str().unwrap_or("");
        let excerpt = quote.chars().take(400).collect::<String>();
        let trimmed = excerpt != quote;
        row["source_excerpt"] = json!(excerpt);
        row["excerpt_truncated"] = json!(trimmed);
        selected.push(row);
        if serde_json::to_vec(&selected)?.len() > 28_000 {
            selected.pop();
            break;
        }
        if trimmed {
            shortened += 1;
        }
    }
    let omitted = rows.len() - selected.len();
    Ok(
        json!({"schema":1,"sector":key,"as_of":full["as_of"],"verification":"user_curated","status":if full["status"]=="partial"||omitted>0||shortened>0{"partial"}else{"ok"},"records":selected,"total_available":rows.len(),"included_count":rows.len()-omitted,"omitted_count":omitted,"excerpt_truncated_count":shortened,"conflict_groups":full["conflicts"].as_array().map_or(0,Vec::len),"invalid_records":full["errors"].as_array().map_or(0,Vec::len),"selection":"按统计期倒序选取至多二十条，受二十八KiB观测预算约束；摘录至多四百字并逐条标记裁减。原始数值、单位、日期、记录ID和出处URL保留，完整记录仍在资料库及业务备份。未附带记录不得当作不存在，用户整理内容未独立核验。"}),
    )
}

pub fn mutate(db: &Path, body: &Value) -> Result<Value> {
    if body["action"] == "delete" {
        let key = text(body, "sector", 64)?;
        sector(&key)?;
        let id = body["id"]
            .as_i64()
            .filter(|i| *i > 0)
            .ok_or_else(|| anyhow!("id 无效"))?;
        let changed=Connection::open(db)?.execute("UPDATE mobile_documents SET status='archived',updated_at=?1 WHERE id=?2 AND source=?3 AND status='active'",params![chrono::Utc::now().to_rfc3339(),id,format!("{PREFIX}{key}")])?;
        ensure!(changed == 1, "产业记录不存在或已移除");
        return Ok(json!({"removed":id}));
    }
    ensure!(
        body["action"] == "import" && body["schema"] == 1,
        "只允许 schema=1 的 import 或 delete"
    );
    ensure!(
        body.to_string().len() <= 32 * 1024 * 1024,
        "产业导入超过 32 MiB 预算"
    );
    let raw = body["records"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 2000)
        .ok_or_else(|| anyhow!("每次导入须为 1–2000 条记录"))?;
    let facts = raw.iter().map(normalize).collect::<Result<Vec<_>>>()?;
    let mut c = Connection::open(db)?;
    let tx = c.transaction()?;
    let (mut inserted, mut duplicates) = (0, 0);
    for fact in facts {
        let source = format!("{PREFIX}{}", fact["sector"].as_str().unwrap());
        let serialized = fact.to_string();
        let hash = format!("fact-{:x}", Sha256::digest(serialized.as_bytes()));
        let prior: Option<i64> = tx
            .query_row(
                "SELECT id FROM mobile_documents WHERE source=?1 AND title=?2 AND status='active'",
                params![source, hash],
                |r| r.get(0),
            )
            .optional()?;
        if prior.is_some() {
            duplicates += 1;
            continue;
        }
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM mobile_documents WHERE source=?1 AND status='active'",
            [&source],
            |r| r.get(0),
        )?;
        ensure!(count < 2000, "该赛道最多 2000 条活跃记录，请先导出整理");
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute("INSERT INTO mobile_documents(kind,category,title,body,status,source,created_at,updated_at) VALUES('note','产业资料',?1,?2,'active',?3,?4,?4)",params![hash,serialized,source,now])?;
        inserted += 1;
    }
    tx.commit()?;
    Ok(json!({"inserted":inserted,"duplicates":duplicates}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn research_library_attachment_requires_explicit_industry_opt_in() {
        let app = crate::AppCore::new(crate::test_http::temp_root("industry-opt-in")).unwrap();
        for (body, expected) in [
            (json!({"mode":"industry","sector":"hbm"}), false),
            (
                json!({"mode":"industry","sector":"hbm","industry_library":true}),
                true,
            ),
            (
                json!({"mode":"deep","symbol":"600519","industry_library":true}),
                false,
            ),
        ] {
            let task = crate::research_workflow::create(app.db_path(), &body).unwrap();
            assert_eq!(task["industry_library"], expected);
        }
    }
    #[test]
    fn research_snapshot_keeps_real_measurements_and_citations_when_excerpts_exceed_budget() {
        let app = crate::AppCore::new(crate::test_http::temp_root("industry-budget")).unwrap();
        let db = app.db_path();
        let facts = (0..30)
            .map(|i| {
                let mut f = fact();
                f["entity"] = json!(format!("合成{i}厂"));
                f["source_excerpt"] = json!("合成原文".repeat(400));
                f
            })
            .collect::<Vec<_>>();
        mutate(db, &json!({"action":"import","schema":1,"records":facts})).unwrap();
        let snap = research_snapshot(db, "hbm", None).unwrap();
        assert_eq!(snap["status"], "partial");
        assert_eq!(snap["total_available"], 30);
        assert!(snap["included_count"].as_u64().unwrap() > 0);
        assert!(snap["omitted_count"].as_u64().unwrap() > 0);
        assert!(serde_json::to_vec(&snap).unwrap().len() < 35_000);
        assert_eq!(snap["records"][0]["value"], 100.);
        assert_eq!(snap["records"][0]["unit"], "万片/年");
        assert_eq!(
            snap["records"][0]["source_url"],
            "https://example.com/public-report"
        );
        assert_eq!(snap["records"][0]["excerpt_truncated"], true);
        assert_eq!(
            list(db, "hbm", None).unwrap()["records"][0]["source_excerpt"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            1600
        );
    }
    fn fact() -> Value {
        json!({"sector":"hbm","metric":"capacity","entity":"合成测试厂","label":"年产能","value":100,"unit":"万片/年","currency":"none","period_end":"2026-06-30","published_date":"2026-07-01","source_name":"合成公告","source_url":"https://example.com/public-report","source_excerpt":"合成测试：年产能100万片。"})
    }
    #[test]
    fn library_import_is_atomic_deduplicated_backed_up_and_excluded_from_normal_chat() {
        let app = crate::AppCore::new(crate::test_http::temp_root("industry-library")).unwrap();
        let db = app.db_path();
        let import = json!({"action":"import","schema":1,"records":[fact(),fact()]});
        assert_eq!(
            mutate(db, &import).unwrap(),
            json!({"inserted":1,"duplicates":1})
        );
        let before = list(db, "hbm", None).unwrap();
        assert_eq!(before["records"].as_array().unwrap().len(), 1);
        assert_eq!(
            list(db, "hbm", Some("2026-06-30")).unwrap()["records"],
            json!([])
        );
        assert_eq!(
            crate::documents::list(db, "/?kind=note").unwrap()["count"],
            0
        );
        crate::documents::clear(db, &json!({"kind":"note","confirm":true})).unwrap();
        assert_eq!(list(db, "hbm", None).unwrap()["records"], before["records"]);
        let backup = crate::backup::export(db).unwrap();
        crate::backup::restore(
            db,
            &json!({"mode":"replace","confirm":true,"backup":backup}),
        )
        .unwrap();
        assert_eq!(list(db, "hbm", None).unwrap()["records"], before["records"]);
        let mut bad = fact();
        bad["value"] = json!(true);
        assert!(mutate(
            db,
            &json!({"action":"import","schema":1,"records":[fact(),bad]})
        )
        .is_err());
        assert!(mutate(
            db,
            &json!({"action":"delete","sector":"pcb","id":before["records"][0]["id"]})
        )
        .is_err());
        mutate(
            db,
            &json!({"action":"delete","sector":"hbm","id":before["records"][0]["id"]}),
        )
        .unwrap();
        assert_eq!(list(db, "hbm", None).unwrap()["records"], json!([]));
    }
    #[test]
    fn quotes_units_future_dates_and_invalid_sources_are_not_silently_accepted() {
        for (key, value) in [
            ("period_end", json!("2099-01-01")),
            ("source_url", json!("file:///secret")),
            ("source_url", json!("https://example.com/?token=secret")),
            ("sector", json!("unknown")),
            ("value", Value::Null),
        ] {
            let mut v = fact();
            v[key] = value;
            assert!(normalize(&v).is_err());
        }
        let mut v = fact();
        v["metric"] = json!("yield");
        assert!(normalize(&v).is_err());
        v["unit"] = json!("%");
        v["value"] = json!(101);
        assert!(normalize(&v).is_err());
    }
    #[test]
    fn conflicting_measurements_are_preserved_and_flagged_without_mixing_units() {
        let app = crate::AppCore::new(crate::test_http::temp_root("industry-conflict")).unwrap();
        let db = app.db_path();
        let mut changed = fact();
        changed["value"] = json!(120);
        let mut other = fact();
        other["value"] = json!(1);
        other["unit"] = json!("亿片/年");
        mutate(
            db,
            &json!({"action":"import","schema":1,"records":[fact(),changed,other]}),
        )
        .unwrap();
        let out = list(db, "hbm", None).unwrap();
        assert_eq!(out["records"].as_array().unwrap().len(), 3);
        assert_eq!(out["status"], "partial");
        assert_eq!(out["conflicts"].as_array().unwrap().len(), 1);
        assert_eq!(out["conflicts"][0]["records"].as_array().unwrap().len(), 2);
    }
}
