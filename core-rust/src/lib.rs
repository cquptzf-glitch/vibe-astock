mod journal;
mod market;
mod quote;
mod rotation;
mod shortline;
mod storage;

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, time::Duration};

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
}

impl AppCore {
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&data_dir)
            .with_context(|| format!("create app data dir: {}", data_dir.display()))?;
        let db_path = data_dir.join("vibe-astock-mobile.sqlite3");
        storage::init(&db_path)?;
        journal::init(&db_path)?;

        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(15))
            .user_agent("Mozilla/5.0 VibeAStock-Mobile/0.1.3")
            .build()?;

        Ok(Self { client, db_path })
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

        match (method.as_str(), route) {
            ("GET", "/health") => Ok(MobileApiResponse::ok(json!({
                "ok": true,
                "version": "0.1.3-mobile"
            }))),
            ("GET", "/product") => Ok(MobileApiResponse::ok(json!({
                "version": "0.1.3-mobile",
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
                "sources": ["tencent", "eastmoney"],
                "ignored_legacy": [],
                "config_error": null,
                "provider_templates": {}
            }))),
            ("GET", "/local-agents") => Ok(MobileApiResponse::ok(json!([]))),
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
            ("POST", "/mobile/journal") => match journal::act(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "journal", error.to_string())),
            },
            ("GET", "/mobile/market-overview") => {
                Ok(MobileApiResponse::ok(market::overview(&self.client).await?))
            }
            ("GET", "/mobile/shortline/limit-up") => Ok(MobileApiResponse::ok(
                shortline::limit_up_pool(&self.client, &request.path).await?,
            )),
            ("GET", "/mobile/rotation") => Ok(MobileApiResponse::ok(
                rotation::snapshot(&self.client, &request.path).await?,
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
