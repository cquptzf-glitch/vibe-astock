mod anomalies;
mod observations;
mod stock_context;
mod ai_chat;
mod ai_config;
mod ai_review;
mod backup;
mod backtest;
mod boards;
mod calendar;
mod chart;
mod daily_review;
mod data_summary;
mod documents;
mod emotion;
mod emotion_history;
mod discovery;
mod industry_signals;
mod news;
mod journal;
mod journal_mutation;
mod journal_review;
mod market;
mod monitor;
mod quote;
mod reminders;
mod research;
mod rotation;
mod schema;
mod search;
mod shortline;
mod storage;
mod symbol;
mod watch;

pub use symbol::{natural_market, normalize_stock_symbol, Market, SymbolKey};

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, time::Duration};

const APP_VERSION: &str = env!("VIBE_APP_VERSION");

#[derive(Debug, Deserialize)]
pub struct MobileApiRequest {
    pub path: String,
    pub method: String,
    #[serde(default)]
    pub body: Value,
}

#[derive(Debug, Serialize)]
pub struct MobileApiResponse {
    pub status: u16,
    pub body: Value,
}

impl MobileApiResponse {
    fn ok(body: Value) -> Self {
        Self { status: 200, body }
    }

    fn error(status: u16, code: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            body: json!({ "error": code, "message": message.into() }),
        }
    }
}

pub struct AppCore {
    client: Client,
    db_path: PathBuf,
    watch: watch::SharedWatch,
}

impl AppCore {
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&data_dir)
            .with_context(|| format!("create app data dir: {}", data_dir.display()))?;
        let db_path = data_dir.join("vibe-astock-mobile.sqlite3");
        storage::init(&db_path)?;
        journal::init(&db_path)?;
        daily_review::init(&db_path)?;
        ai_review::init(&db_path)?;
        ai_chat::init(&db_path)?;
        schema::migrate(&db_path)?;

        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(15))
            .user_agent(format!("Mozilla/5.0 VibeAStock-Mobile/{APP_VERSION}"))
            .build()?;

        Ok(Self { client, db_path, watch: watch::SharedWatch::default() })
    }

    #[cfg(test)]
    pub(crate) fn db_path(&self) -> &std::path::Path {
        &self.db_path
    }

    pub fn legacy_ai_credential(&self) -> Result<Option<(String, String)>> {
        Ok(ai_config::legacy_credential(&self.db_path)?
            .map(|credential| (credential.base_url, credential.api_key)))
    }

    pub fn clear_legacy_ai_key(&self) -> Result<()> {
        ai_config::clear_legacy_key(&self.db_path)
    }

    pub fn ai_base_url(&self) -> Result<Option<String>> {
        ai_config::base_url(&self.db_path)
    }

    pub fn reminder_snapshot(&self) -> Result<Value> {
        reminders::native_snapshot(&self.db_path)
    }

    pub async fn handle(&self, request: MobileApiRequest) -> MobileApiResponse {
        match self.handle_inner(request).await {
            Ok(response) => response,
            Err(error) => MobileApiResponse::error(500, "mobile_core", error.to_string()),
        }
    }

    async fn handle_inner(&self, request: MobileApiRequest) -> Result<MobileApiResponse> {
        let route = request.path.split('?').next().unwrap_or("/");
        let method = request.method.to_uppercase();
        let secure_key_present = request
            .body
            .get("__secure_key_present")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mobile_version = format!("{APP_VERSION}-mobile");

        match (method.as_str(), route) {
            ("GET", "/health") => Ok(MobileApiResponse::ok(json!({
                "ok": true,
                "version": mobile_version
            }))),
            ("GET", "/product") => Ok(MobileApiResponse::ok(json!({
                "version": mobile_version,
                "provider": {
                    "profile": null,
                    "kind": null,
                    "label": null,
                    "base_url": null,
                    "env_key": null,
                    "key_present": null,
                    "verification": null
                },
                "defaults": {},
                "paths": { "data_root": "android-app-data", "python": "" },
                "sources": ["tencent", "eastmoney", "sina"],
                "ignored_legacy": [],
                "config_error": null,
                "provider_templates": {}
            }))),
            ("GET", "/local-agents") => Ok(MobileApiResponse::ok(json!([]))),
            ("GET", "/mobile/data/summary") => Ok(MobileApiResponse::ok(
                data_summary::summary(&self.db_path, secure_key_present)?,
            )),
            ("GET", "/mobile/data/schema") => {
                Ok(MobileApiResponse::ok(schema::info(&self.db_path)?))
            }
            ("GET", "/mobile/reminders") => match reminders::list(&self.db_path) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "reminders", error.to_string())),
            },
            ("POST", "/mobile/reminders") => match reminders::save(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "reminders", error.to_string())),
            },
            ("DELETE", "/mobile/reminders") => match reminders::delete(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "reminders", error.to_string())),
            },
            ("GET", "/mobile/calendar") => match calendar::list(&self.db_path, &request.path) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "calendar", error.to_string())),
            },
            ("POST", "/mobile/calendar") => match calendar::save(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "calendar", error.to_string())),
            },
            ("DELETE", "/mobile/calendar") => match calendar::delete(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "calendar", error.to_string())),
            },
            ("GET", "/mobile/documents") => match documents::list(&self.db_path, &request.path) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "documents", error.to_string())),
            },
            ("POST", "/mobile/documents") => match documents::save(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "documents", error.to_string())),
            },
            ("DELETE", "/mobile/documents") => match documents::delete(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "documents", error.to_string())),
            },
            ("POST", "/mobile/documents/clear") => match documents::clear(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "documents", error.to_string())),
            },
            ("GET", "/mobile/backup/export") => {
                Ok(MobileApiResponse::ok(backup::export(&self.db_path)?))
            }
            ("POST", "/mobile/backup/validate") => match backup::validate(&request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "backup_validate", error.to_string())),
            },
            ("POST", "/mobile/backup/import") => match backup::restore(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "backup_restore", error.to_string())),
            },
            ("GET", "/mobile/daily-review") => match daily_review::get(&self.db_path, &request.path) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "daily_review", error.to_string())),
            },
            ("POST", "/mobile/daily-review") => match daily_review::save(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "daily_review", error.to_string())),
            },
            ("GET", "/mobile/watchlist") => {
                Ok(MobileApiResponse::ok(storage::list(&self.db_path)?))
            }
            ("POST", "/mobile/watchlist") => {
                Ok(MobileApiResponse::ok(storage::upsert(&self.db_path, &request.body)?))
            }
            ("DELETE", "/mobile/watchlist") => {
                Ok(MobileApiResponse::ok(storage::delete(&self.db_path, &request.body)?))
            }
            ("GET", "/mobile/journal") => {
                Ok(MobileApiResponse::ok(journal::view(&self.db_path)?))
            }
            ("GET", "/mobile/journal/review") => {
                Ok(MobileApiResponse::ok(journal_review::view(&self.db_path)?))
            }
            ("POST", "/mobile/journal") => match journal::act(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "journal", error.to_string())),
            },
            ("POST", "/mobile/journal/append") => match journal_mutation::append_fills(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "journal_append", error.to_string())),
            },
            ("POST", "/mobile/journal/update") => match journal_mutation::update_entry(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "journal_update", error.to_string())),
            },
            ("GET", "/mobile/ai/config") => Ok(MobileApiResponse::ok(
                ai_review::config_view(&self.db_path, secure_key_present)?,
            )),
            ("POST", "/mobile/ai/config") => match ai_review::save_config(
                &self.db_path,
                &request.body,
                secure_key_present,
            ) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "ai_config", error.to_string())),
            },
            ("DELETE", "/mobile/ai/config") => {
                Ok(MobileApiResponse::ok(ai_review::clear_config(&self.db_path)?))
            }
            ("GET", "/mobile/ai/chat") => {
                Ok(MobileApiResponse::ok(ai_chat::history(&self.db_path)?))
            }
            ("DELETE", "/mobile/ai/chat") => {
                Ok(MobileApiResponse::ok(ai_chat::clear(&self.db_path)?))
            }
            ("POST", "/mobile/ai/chat") => match ai_chat::send(&self.client, &self.db_path, &request.body).await {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(ai_chat::ChatError::BadRequest(message)) => {
                    Ok(MobileApiResponse::error(400, "ai_chat", message))
                }
                Err(ai_chat::ChatError::Upstream(message)) => {
                    Ok(MobileApiResponse::error(502, "ai_chat_upstream", message))
                }
                Err(ai_chat::ChatError::Internal(error)) => Err(error),
            },
            ("POST", "/mobile/ai/review/preview") => match ai_review::preview(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "ai_preview", error.to_string())),
            },
            ("POST", "/mobile/ai/review") => match ai_review::run(&self.client, &self.db_path, &request.body).await {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(ai_review::RunError::BadRequest(message)) => {
                    Ok(MobileApiResponse::error(400, "ai_review", message))
                }
                Err(ai_review::RunError::Upstream(message)) => {
                    Ok(MobileApiResponse::error(502, "ai_upstream", message))
                }
                Err(ai_review::RunError::Internal(error)) => Err(error),
            },
            ("POST", "/mobile/symbol-search") => match search::symbols(&self.client, &request.body).await {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(search::SearchError::BadRequest(message)) => {
                    Ok(MobileApiResponse::error(400, "symbol_search", message))
                }
                Err(search::SearchError::Upstream(error)) => {
                    Ok(MobileApiResponse::error(502, "symbol_search_upstream", error.to_string()))
                }
            },
            ("GET", "/mobile/market-overview") => {
                Ok(MobileApiResponse::ok(market::overview(&self.client).await?))
            }
            ("GET", "/mobile/monitor") => match monitor::snapshot(&self.client, &self.db_path).await {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(502, "monitor", error.to_string())),
            },
            ("POST", "/mobile/watch/poll") => Ok(discovery_response(watch::poll(&self.client, &self.db_path, &self.watch).await)),
            ("GET", "/mobile/watch/alerts") => Ok(discovery_response(watch::alerts(&self.db_path, &request.path))),
            ("POST", "/mobile/backtest") => match backtest::run(&self.client, &request.body).await {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "backtest", error.to_string())),
            },
            ("GET", "/mobile/shortline/limit-up") => Ok(MobileApiResponse::ok(
                shortline::limit_up_pool(&self.client, &request.path).await?,
            )),
            ("GET", "/mobile/stock/fund-flow") => Ok(discovery_response(stock_context::fund_flow(&self.client, &request.path).await)),
            ("GET", "/mobile/stock/kline") => Ok(discovery_response(chart::kline(&self.client, &request.path).await)),
            ("GET", "/mobile/stock/intraday") => Ok(discovery_response(chart::intraday(&self.client, &request.path).await)),
            ("GET", "/mobile/stock/snapshot") => Ok(discovery_response(chart::snapshot(&self.client, &request.path).await)),
            ("GET", "/mobile/stock/boards") => Ok(discovery_response(stock_context::stock_boards(&self.client, &request.path).await)),
            ("GET", "/mobile/stock/observations") => Ok(discovery_response(observations::fetch(&self.client, &request.path).await)),
            ("GET", "/mobile/shortline/risk") => Ok(discovery_response(anomalies::fetch(&self.client, &request.path).await)),
            ("GET", "/mobile/shortline/emotion") => Ok(discovery_response(emotion::emotion(&self.client, &request.path).await)),
            ("GET", "/mobile/shortline/cycle") => Ok(discovery_response(emotion::cycle(&self.client, &request.path).await)),
            ("GET", "/mobile/shortline/history") => Ok(discovery_response(emotion_history::history(&self.client, &self.db_path, &request.path).await)),
            ("GET", "/mobile/shortline/weekly") => Ok(discovery_response(emotion_history::weekly(&self.client, &self.db_path, &request.path).await)),
            ("GET", "/mobile/shortline/verify") => Ok(discovery_response(emotion_history::verify(&self.client, &self.db_path, &request.path).await)),
            ("POST", "/mobile/shortline/verify") => Ok(discovery_response(emotion_history::save_verify(&self.client, &self.db_path, &request.body).await)),
            ("GET", "/mobile/news/catalog") => Ok(MobileApiResponse::ok(news::catalog()?)),
            ("GET", "/mobile/news") => Ok(discovery_response(news::fetch(&self.client, &request.path).await)),
            ("GET", "/mobile/industry/gpu-rent") => Ok(discovery_response(industry_signals::gpu_rent(&self.client, &request.path).await)),
            ("GET", "/mobile/boards/detail") => Ok(discovery_response(boards::detail(&self.client, &request.path).await)),
            ("GET", "/mobile/rotation") => Ok(MobileApiResponse::ok(
                rotation::snapshot(&self.client, &request.path).await?,
            )),
            ("GET", "/mobile/research/profile") => {
                Ok(research_response(research::profile(&self.client, &request.path).await))
            }
            ("GET", "/mobile/research/announcements") => Ok(research_response(
                research::announcements(&self.client, &request.path).await,
            )),
            ("GET", "/mobile/research/financials") => Ok(research_response(
                research::financials(&self.client, &request.path).await,
            )),
            ("GET", "/mobile/research/estimates") => Ok(research_response(
                research::estimates(&self.client, &request.path).await,
            )),
            ("POST", "/fetch") => match quote::fetch_endpoint(&self.client, &request.body).await {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(quote::FetchError::BadRequest(message)) => {
                    Ok(MobileApiResponse::error(400, "bad_request", message))
                }
                Err(quote::FetchError::NotMigrated(endpoint)) => Ok(MobileApiResponse::error(
                    501,
                    "mobile_endpoint_not_migrated",
                    format!("Android 尚未迁移数据端点: {endpoint}"),
                )),
                Err(quote::FetchError::Other(error)) => Err(error),
            },
            _ => Ok(MobileApiResponse::error(
                501,
                "mobile_not_migrated",
                format!("Android Rust Core 尚未迁移接口: {} {}", method, request.path),
            )),
        }
    }
}

fn discovery_response(result: std::result::Result<Value, discovery::DiscoveryError>) -> MobileApiResponse {
    match result {
        Ok(body) => MobileApiResponse { status: if body["status"] == "failed" { 502 } else { 200 }, body },
        Err(discovery::DiscoveryError::BadRequest(message)) => MobileApiResponse::error(400, "bad_request", message),
        Err(discovery::DiscoveryError::Upstream(error)) => {
            let failure = discovery::failure("discovery", &error);
            MobileApiResponse::error(502, "discovery_upstream", failure["message"].as_str().unwrap_or("数据源暂不可用"))
        }
    }
}

fn research_response(result: std::result::Result<Value, research::ResearchError>) -> MobileApiResponse {
    match result {
        Ok(value) => MobileApiResponse::ok(value),
        Err(research::ResearchError::BadRequest(message)) => {
            MobileApiResponse::error(400, "research", message)
        }
        Err(research::ResearchError::Upstream(error)) => {
            MobileApiResponse::error(502, "research_upstream", error.to_string())
        }
    }
}

#[cfg(test)]
mod discovery_contract_tests {
    use super::*;
    #[tokio::test]
    async fn discovery_routes_reject_bad_inputs_before_network_access() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let directory = std::env::temp_dir().join(format!("vibe-discovery-contract-{nonce}"));
        let app = AppCore::new(directory.clone()).unwrap();
        for path in [
            "/mobile/news?industry=unknown",
            "/mobile/news?industry=ai&source=not-a-configured-source",
            "/mobile/news?industry=ai&days=0",
            "/mobile/industry/gpu-rent?days=100000",
            "/mobile/boards/detail?code=BK0475%26fs=arbitrary",
            "/mobile/boards/detail?code=BK0475&page=0",
            "/mobile/boards/detail?code=BK0475&sort=not-a-field",
            "/mobile/stock/fund-flow?symbol=invalid",
            "/mobile/stock/fund-flow?symbol=600519&mode=arbitrary",
            "/mobile/stock/fund-flow?symbol=600519&limit=10000",
            "/mobile/stock/boards?symbol=600519.SZ",
            "/mobile/stock/observations?symbol=../../private",
            "/mobile/shortline/risk?kind=arbitrary",
            "/mobile/shortline/risk?page=0",
            "/mobile/shortline/emotion?date=2026-13-40",
            "/mobile/shortline/emotion?date=../../x",
            "/mobile/shortline/cycle?days=99",
            "/mobile/shortline/history?days=4",
            "/mobile/stock/kline?symbol=600519&period=m1",
            "/mobile/stock/kline?symbol=600519&adjust=hfq",
            "/mobile/stock/kline?symbol=bad",
            "/mobile/stock/intraday?symbol=600519&days=3",
            "/mobile/stock/snapshot?symbol=../x",
            "/mobile/shortline/history?refresh=yes",
            "/mobile/shortline/weekly?date=20261340",
            "/mobile/shortline/verify?refresh=2",
            "/mobile/watch/alerts?date=2026-02-30",
            "/mobile/watch/alerts?date=2999-01-01",
        ] {
            let response=app.handle(MobileApiRequest {path:path.into(),method:"GET".into(),body:Value::Null}).await;
            assert_eq!(response.status,400,"{path}: {}",response.body);
        }
        let response=app.handle(MobileApiRequest {path:"/mobile/news/catalog".into(),method:"GET".into(),body:Value::Null}).await;
        assert_eq!(response.status,200);
        assert_eq!(response.body["industries"].as_array().unwrap().len(),12);
        drop(app);
        let _=std::fs::remove_dir_all(directory);
    }
    #[test]
    fn source_total_failure_maps_to_502_without_hiding_partial_data() {
        assert_eq!(discovery_response(Ok(json!({"status":"failed","message":"upstream"}))).status,502);
        let partial=discovery_response(Ok(json!({"status":"partial","items":[1],"errors":[{"source":"x"}]})));
        assert_eq!(partial.status,200);
        assert_eq!(partial.body["items"],json!([1]));
    }
}
