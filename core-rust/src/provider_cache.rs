//! Public-provider response cache. Business ledgers and credentials are never cached here.
use crate::{MobileApiRequest, MobileApiResponse};
use serde_json::{json, Value};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};
const MAX_BYTES: usize = 3 * 1024 * 1024;
fn dir(db: &Path) -> PathBuf {
    db.parent().unwrap_or(Path::new(".")).join("provider-cache")
}
fn prefs(db: &Path) -> PathBuf {
    dir(db).join("preferences.json")
}
pub fn settings(db: &Path, body: Option<&Value>) -> anyhow::Result<Value> {
    std::fs::create_dir_all(dir(db))?;
    if let Some(body) = body {
        let offline = body["offline"]
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("offline 必须为布尔值"))?;
        std::fs::write(prefs(db), json!({"offline":offline}).to_string())?;
    }
    let offline = std::fs::read(prefs(db))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .is_some_and(|v| v["offline"] == true);
    let files = std::fs::read_dir(dir(db))?
        .filter_map(Result::ok)
        .filter(|e| {
            e.path()
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with("response-"))
        })
        .count();
    Ok(
        json!({"offline":offline,"cached_responses":files,"scope":"仅公开数据;离线时行情是历史快照,不用于实时异动判断"}),
    )
}
pub fn offline(db: &Path) -> bool {
    std::fs::read(prefs(db))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .is_some_and(|v| v["offline"] == true)
}
pub fn key(request: &MobileApiRequest) -> Option<(String, i64)> {
    if request.method.to_uppercase() != "GET" {
        return None;
    }
    let url = reqwest::Url::parse(&format!("https://mobile.invalid{}", request.path)).ok()?;
    let path = url.path();
    if !matches!(
        path,
        "/mobile/market-overview"
            | "/mobile/rotation"
            | "/mobile/industry/gpu-rent"
            | "/mobile/industry/revenue"
            | "/mobile/news"
            | "/mobile/boards/detail"
            | "/mobile/research/profile"
            | "/mobile/research/announcements"
            | "/mobile/research/financials"
            | "/mobile/research/statements"
            | "/mobile/research/consensus"
            | "/mobile/research/valuation"
            | "/mobile/review/market"
            | "/mobile/shortline/emotion"
            | "/mobile/shortline/weekly"
            | "/mobile/calendar/month"
            | "/mobile/stock/kline"
            | "/mobile/stock/intraday"
            | "/mobile/stock/snapshot"
            | "/mobile/stock/fund-flow"
            | "/mobile/stock/boards"
            | "/mobile/stock/observations"
    ) {
        return None;
    }
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| !matches!(k.as_ref(), "refresh" | "offline"))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    pairs.sort();
    let query = reqwest::Url::parse_with_params("https://cache.invalid/", pairs).ok()?;
    let ttl = if path == "/mobile/calendar/month" {
        86400
    } else if path.contains("research/") || path == "/mobile/industry/revenue" {
        3600
    } else if path.contains("kline") || path.contains("weekly") {
        300
    } else {
        5
    };
    Some((format!("{path}?{}", query.query().unwrap_or("")), ttl))
}
fn filename(db: &Path, key: &str) -> PathBuf {
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    dir(db).join(format!("response-{:016x}.json", h.finish()))
}
pub fn read(db: &Path, key: &str, ttl: i64) -> Option<(Value, bool, String)> {
    let file = filename(db, key);
    if std::fs::metadata(&file).ok()?.len() > MAX_BYTES as u64 {
        return None;
    }
    let v: Value = serde_json::from_slice(&std::fs::read(file).ok()?).ok()?;
    if v["key"] != key {
        return None;
    }
    let timestamp = v["timestamp"].as_i64()?;
    let now = chrono::Utc::now().timestamp();
    let date = chrono::DateTime::from_timestamp(timestamp, 0)?.to_rfc3339();
    Some((
        v["body"].clone(),
        now - timestamp > ttl || timestamp > now,
        date,
    ))
}
pub fn write(db: &Path, key: &str, body: &Value) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(
        &json!({"key":key,"timestamp":chrono::Utc::now().timestamp(),"body":body}),
    )?;
    if bytes.len() > MAX_BYTES {
        return Ok(());
    }
    std::fs::create_dir_all(dir(db))?;
    std::fs::write(filename(db, key), bytes)?;
    Ok(())
}
pub fn response(
    mut body: Value,
    stale: bool,
    offline: bool,
    at: String,
    message: String,
) -> MobileApiResponse {
    if let Some(o) = body.as_object_mut() {
        o.insert(
            "_cache".into(),
            json!({"stale":stale,"offline":offline,"saved_at":at,"message":message}),
        );
    }
    MobileApiResponse { status: 200, body }
}
pub fn finish(db: &Path,key: &str,saved: Option<(Value,bool,String)>,result: MobileApiResponse)->MobileApiResponse {
    let envelope_failed=result.body["status"]=="failed" || result.body.get("error").is_some();
    if result.status==200 && !envelope_failed {
        let _=write(db,key,&result.body);
    } else if result.status>=500 || (result.status==200 && envelope_failed) {
        if let Some((body,_,at))=saved {
            return response(body,true,false,at,format!("取数失败,显示上次公开数据快照:{}",result.body["message"].as_str().unwrap_or("数据源不可用")));
        }
    }
    result
}
pub fn clear(db: &Path) -> anyhow::Result<usize> {
    if !dir(db).exists() {
        return Ok(0);
    }
    let mut removed = 0;
    for entry in std::fs::read_dir(dir(db))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json")
        {
            std::fs::remove_file(path)?;
            removed += 1;
        }
    }
    Ok(removed)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_envelopes_use_stale_snapshot_without_masking_request_errors() {
        let root=std::env::temp_dir().join(format!("provider-failure-{}",chrono::Utc::now().timestamp_nanos_opt().unwrap()));
        std::fs::create_dir_all(&root).unwrap();let db=root.join("test.sqlite");
        write(&db,"public",&json!({"items":[1]})).unwrap();
        for failed in [json!({"status":"failed"}),json!({"error":"upstream"})] {
            let result=finish(&db,"public",read(&db,"public",0),MobileApiResponse {status:200,body:failed});
            assert_eq!(result.body["items"],json!([1]));
            assert_eq!(result.body["_cache"]["stale"],true);
            assert!(read(&db,"public",0).unwrap().0.get("_cache").is_none());
        }
        let bad_request=finish(&db,"public",read(&db,"public",0),MobileApiResponse {status:400,body:json!({"error":"invalid_symbol"})});
        assert_eq!(bad_request.status,400);
        let partial=finish(&db,"public",read(&db,"public",0),MobileApiResponse {status:200,body:json!({"status":"partial","items":[2]})});
        assert_eq!(partial.body["items"],json!([2]));
        assert_eq!(read(&db,"public",0).unwrap().0["items"],json!([2]));
        let _=std::fs::remove_dir_all(root);
    }
    #[tokio::test]
    async fn offline_api_uses_labeled_cache_and_keeps_local_data_available() {
        let root = std::env::temp_dir().join(format!(
            "offline-api-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let app = crate::AppCore::new(root).unwrap();
        let req = |path: &str| MobileApiRequest {
            method: "GET".into(),
            path: path.into(),
            body: Value::Null,
        };
        fn requires_send<T: Send>(_: T) {}
        requires_send(app.handle(req("/health")));
        let key = key(&req("/mobile/research/profile?symbol=600519"))
            .unwrap()
            .0;
        write(app.db_path(), &key, &json!({"name":"合成公司"})).unwrap();
        settings(app.db_path(), Some(&json!({"offline":true}))).unwrap();
        let response = app
            .handle(req("/mobile/research/profile?symbol=600519"))
            .await;
        assert_eq!(response.status, 200);
        assert_eq!(response.body["_cache"]["offline"], true);
        assert_eq!(response.body["_cache"]["stale"], true);
        assert_eq!(
            app.handle(req("/mobile/research/profile?symbol=000001"))
                .await
                .status,
            503
        );
        assert_eq!(
            app.handle(req("/mobile/shortline/history")).await.status,
            503
        );
        assert_eq!(app.handle(req("/mobile/documents")).await.status, 200);
        assert!(app.reminder_snapshot().unwrap()["rules"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn cache_never_accepts_private_routes_and_round_trips_public_snapshots() {
        let request = |path: &str| MobileApiRequest {
            path: path.into(),
            method: "GET".into(),
            body: Value::Null,
        };
        for path in [
            "/mobile/journal",
            "/mobile/portfolio/perf",
            "/mobile/watchlist",
            "/mobile/ai/chat",
        ] {
            assert!(key(&request(path)).is_none());
        }
        let a = key(&request(
            "/mobile/stock/kline?symbol=600519&period=day&refresh=1",
        ))
        .unwrap();
        let b = key(&request("/mobile/stock/kline?period=day&symbol=600519")).unwrap();
        assert_eq!(a, b);
        let root = std::env::temp_dir().join(format!(
            "provider-cache-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("test.sqlite");
        write(&db, &a.0, &json!({"price":12})).unwrap();
        assert_eq!(read(&db, &a.0, a.1).unwrap().0["price"], 12);
        settings(&db, Some(&json!({"offline":true}))).unwrap();
        assert!(offline(&db));
        assert_eq!(clear(&db).unwrap(), 2);
        assert!(!offline(&db));
        let _ = std::fs::remove_dir_all(root);
    }
}
