//! Immutable calculation audit artifacts, backed up with existing local documents.
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;
pub(crate) const PREFIX: &str = "mobile_research_calculation:";
pub fn records(db: &Path, task_id: i64) -> Result<Vec<Value>> {
    let conn = Connection::open(db)?;
    let mut stmt =
        conn.prepare("SELECT id,body FROM mobile_documents WHERE source=?1 ORDER BY id")?;
    let rows = stmt.query_map([format!("{PREFIX}{task_id}")], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?;
    rows.map(|r| {
        let (id, body) = r?;
        let mut record: Value = serde_json::from_str(&body)?;
        crate::deterministic_calc::graph::verify_record(&record).map_err(anyhow::Error::msg)?;
        record["record_id"] = json!(id);
        Ok(record)
    })
    .collect()
}
pub fn store(db: &Path, task_id: i64, stage: &str, records: &mut [Value]) -> Result<()> {
    let mut conn = Connection::open(db)?;
    let tx = conn.transaction()?;
    let source = format!("{PREFIX}{task_id}");
    for record in records {
        crate::deterministic_calc::graph::verify_record(record).map_err(anyhow::Error::msg)?;
        let cid = record["calculation_id"]
            .as_str()
            .ok_or_else(|| anyhow!("计算记录缺少身份"))?
            .to_owned();
        let existing: Option<(i64, String)> = tx
            .query_row(
                "SELECT id,body FROM mobile_documents WHERE source=?1 AND title=?2",
                params![source, cid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let id = if let Some((id, text)) = existing {
            let previous: Value = serde_json::from_str(&text)?;
            for key in [
                "function",
                "inputs",
                "inputs_refs",
                "bindings",
                "output",
                "executed",
            ] {
                if previous[key] != record[key] {
                    return Err(anyhow!("计算身份和已有不可变产物不一致:{cid}"));
                }
            }
            id
        } else {
            let mut artifact = record.clone();
            artifact["task_id"] = json!(task_id);
            artifact["stage"] = json!(stage);
            let body = serde_json::to_string(&artifact)?;
            if body.chars().count() > 240_000 {
                return Err(anyhow!("计算产物超过二十四万字预算，没有截断审计"));
            }
            let now = chrono::Utc::now().to_rfc3339();
            tx.execute("INSERT INTO mobile_documents(kind,category,title,body,status,source,created_at,updated_at) VALUES('note','计算审计',?1,?2,'active',?3,?4,?4)",params![cid,body,source,now])?;
            tx.last_insert_rowid()
        };
        record["record_id"] = json!(id);
    }
    tx.commit()?;
    Ok(())
}
pub fn summaries(records: &[Value]) -> Value {
    json!(records.iter().map(|r|json!({"calculation_id":r["calculation_id"],"record_id":r["record_id"],"node_id":r["node_id"],"function":r["function"],"executed":r["executed"],"output":if r["output"]["status"]=="error"{r["output"].clone()}else{json!({"status":r["output"]["status"],"value":r["output"]["value"],"unit":r["output"]["unit"],"display":r["output"]["display"],"reason":r["output"]["reason"]})}})).collect::<Vec<_>>())
}
pub fn view(db: &Path, path: &str) -> Result<Value> {
    let id = crate::discovery::parameter(path, "task_id")
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|&i| i > 0)
        .ok_or_else(|| anyhow!("task_id 必须为正整数"))?;
    let exists:bool=Connection::open(db)?.query_row("SELECT EXISTS(SELECT 1 FROM mobile_documents WHERE id=?1 AND source='mobile_research_task')",[id],|r|r.get(0))?;
    if !exists {
        return Err(anyhow!("研究任务不存在"));
    }
    let records = records(db, id)?;
    if let Some(cid) = crate::discovery::parameter(path, "calculation_id") {
        return records
            .into_iter()
            .find(|r| r["calculation_id"] == cid)
            .ok_or_else(|| anyhow!("该任务没有这项计算"));
    }
    Ok(json!({"task_id":id,"records":summaries(&records)}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audit_survives_backup_and_is_excluded_from_library_and_chat() {
        let root = std::env::temp_dir().join(format!(
            "calc-audit-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let app = crate::AppCore::new(root).unwrap();
        let db = app.db_path();
        let task = crate::research_workflow::create(db, &json!({"mode":"emotion"})).unwrap();
        let id = task["id"].as_i64().unwrap();
        let e = json!([{"id":"E1","data":{"current":12,"base":10}}]);
        let graph = crate::deterministic_calc::graph::run(
            &e,
            &[],
            &json!([{"id":"growth","function":"growth_rate","args":{"current":{"evidence":"E1","path":"/current"},"base":{"evidence":"E1","path":"/base"}}}]),
        );
        let mut records = graph["records"].as_array().unwrap().clone();
        store(db, id, "合成阶段", &mut records).unwrap();
        store(db, id, "合成阶段", &mut records).unwrap();
        assert_eq!(super::records(db, id).unwrap().len(), 1);
        assert_eq!(
            crate::documents::list(db, "/?kind=note").unwrap()["count"],
            0
        );
        assert_eq!(
            crate::chat_documents::search(db, &json!({"query":"growth_rate"})).unwrap()["hits"],
            json!([])
        );
        crate::documents::clear(db, &json!({"kind":"note","confirm":true})).unwrap();
        assert_eq!(super::records(db, id).unwrap().len(), 1);
        let backup = crate::backup::export(db).unwrap();
        crate::backup::restore(
            db,
            &json!({"mode":"replace","confirm":true,"backup":backup}),
        )
        .unwrap();
        let restored = super::records(db, id).unwrap();
        assert_eq!(restored[0]["inputs"], json!({"current":12,"base":10}));
        assert_eq!(
            view(
                db,
                &format!(
                    "/?task_id={id}&calculation_id={}",
                    restored[0]["calculation_id"].as_str().unwrap()
                )
            )
            .unwrap()["output"]["status"],
            "ok"
        );
        let mut corrupted = restored[0].clone();
        corrupted["output"]["value"] = json!(999.);
        Connection::open(db)
            .unwrap()
            .execute(
                "UPDATE mobile_documents SET body=?1 WHERE id=?2",
                params![
                    corrupted.to_string(),
                    restored[0]["record_id"].as_i64().unwrap()
                ],
            )
            .unwrap();
        assert!(super::records(db, id)
            .unwrap_err()
            .to_string()
            .contains("重新计算不一致"));
        Connection::open(db)
            .unwrap()
            .execute(
                "UPDATE mobile_documents SET body='[]' WHERE id=?1",
                [restored[0]["record_id"].as_i64().unwrap()],
            )
            .unwrap();
        assert!(super::records(db, id).is_err());
    }
}
