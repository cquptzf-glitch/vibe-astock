mod anomalies;
mod observations;
mod stock_context;
mod ai_backtest;
mod ai_chat;
mod ai_config;
mod ai_review;
mod ai_vision;
mod backup;
mod backtest;
mod historical_metadata;
mod boards;
mod calendar;
mod calendar_sources;
mod chart;
mod chat_tools;
mod chat_documents;
mod daily_review;
mod data_clear;
mod data_summary;
mod documents;
mod deterministic_calc;
mod document_import;
mod emotion;
mod emotion_history;
mod discovery;
mod industry_signals;
mod industry_library;
mod industry_revenue;
mod intraday;
mod news;
mod journal;
mod journal_attribution;
mod journal_env;
mod journal_excursion;
mod journal_inbox;
mod journal_modes;
mod journal_problems;
mod journal_mutation;
mod journal_review;
mod journal_risk;
mod journal_weekly;
#[cfg(test)] mod accounting_tests;
#[cfg(test)] mod test_http;
mod journal_settings;
mod market;
mod market_scan;
mod market_panel;
mod messages;
mod monitor;
mod portfolio;
mod portfolio_perf;
mod portfolio_replay;
mod portfolio_xray;
mod provider_cache;
mod probability;
mod prev_broken;
mod quote;
mod reminders;
mod research;
mod research_extended;
mod research_workflow;
mod research_calculations;
mod review_tiers;
mod rotation;
mod samples;
mod schema;
mod search;
mod shortline;
mod storage;
mod structure;
mod symbol;
mod theme_tree;
mod trade_import;
mod watch;

pub use symbol::{natural_market, normalize_stock_symbol, Market, SymbolKey};

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use futures_util::future::{AbortHandle, Abortable};
use std::{collections::HashMap, fs, path::PathBuf, sync::{Arc, Mutex}, time::Duration};

/// `?thread=编号`:没给 = None;不是正整数直接拒绝
fn thread_param(path: &str) -> std::result::Result<Option<i64>, String> {
    match discovery::parameter(path, "thread") {
        None => Ok(None),
        Some(raw) => raw.trim().parse::<i64>().ok().filter(|v| *v > 0).map(Some).ok_or_else(|| "thread 必须是正整数".to_string()),
    }
}

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
    intraday: intraday::SharedIntraday,
    /// 正在流式回复的请求:请求号 → 中止句柄,「停止」按钮靠它取消
    ai_cancel: Arc<Mutex<HashMap<String, AbortHandle>>>,
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

        Ok(Self { client, db_path, watch: watch::SharedWatch::default(), intraday: intraday::SharedIntraday::default(), ai_cancel: Arc::default() })
    }

    /// 流式 AI 回复(命令层用):每来一段增量调 on_delta;请求号登记后可以被 `ai_chat_cancel` 取消,取消时什么都不存
    pub async fn ai_chat_stream(&self, body: Value, on_delta: &mut (dyn FnMut(&str) + Send)) -> MobileApiResponse {
        let id = body.get("request_id").and_then(Value::as_str).unwrap_or("").to_string();
        if id.is_empty() || id.len() > 64 {
            return MobileApiResponse::error(400, "ai_chat", "request_id 必须是 1–64 个字符");
        }
        let (handle, registration) = AbortHandle::new_pair();
        if let Ok(mut map) = self.ai_cancel.lock() {
            map.insert(id.clone(), handle);
        }
        let outcome = Abortable::new(ai_chat::send_stream(&self.client, &self.db_path, &body, on_delta), registration).await;
        if let Ok(mut map) = self.ai_cancel.lock() {
            map.remove(&id);
        }
        match outcome {
            Err(_) => MobileApiResponse::error(499, "ai_chat_cancelled", "已取消,这一轮没有保存"),
            Ok(Ok(value)) => MobileApiResponse::ok(value),
            Ok(Err(ai_chat::ChatError::BadRequest(message))) => MobileApiResponse::error(400, "ai_chat", message),
            Ok(Err(ai_chat::ChatError::Upstream(message))) => MobileApiResponse::error(502, "ai_chat_upstream", message),
            Ok(Err(ai_chat::ChatError::Internal(error))) => MobileApiResponse::error(500, "ai_chat_internal", error.to_string()),
        }
    }

    /// 取消一个正在流式回复的请求;返回是否真的有这个请求在跑
    pub fn ai_chat_cancel(&self, request_id: &str) -> bool {
        self.ai_cancel.lock().ok().and_then(|mut map| map.remove(request_id)).map(|handle| handle.abort()).is_some()
    }

    pub async fn ai_chat_tools(&self,mut body:Value)->MobileApiResponse{
        let id=body["request_id"].as_str().unwrap_or("").to_string();
        if id.is_empty()||id.len()>64{return MobileApiResponse::error(400,"ai_tools","request_id 必须为 1–64 字符");}
        body["tools_enabled"]=json!(true);
        let (handle,registration)=AbortHandle::new_pair();
        if let Ok(mut map)=self.ai_cancel.lock(){map.insert(id.clone(),handle);}
        let result=Abortable::new(ai_chat::send(&self.client,&self.db_path,&body),registration).await;
        if let Ok(mut map)=self.ai_cancel.lock(){map.remove(&id);}
        match result{
            Err(_)=>MobileApiResponse::error(499,"ai_tools_cancelled","已取消,这一轮没有保存"),
            Ok(Ok(value))=>MobileApiResponse::ok(value),
            Ok(Err(ai_chat::ChatError::BadRequest(message)))=>MobileApiResponse::error(400,"ai_tools",message),
            Ok(Err(ai_chat::ChatError::Upstream(message)))=>MobileApiResponse::error(502,"ai_tools",message),
            Ok(Err(ai_chat::ChatError::Internal(error)))=>MobileApiResponse::error(500,"ai_tools",error.to_string()),
        }
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
        if self.offline(){Ok(json!({"schema":1,"rules":[],"offline":true}))}else{reminders::native_snapshot(&self.db_path)}
    }
    pub async fn background_calendar(&self) -> Result<Value> {
        if self.offline(){return Err(anyhow::anyhow!("离线模式不能刷新后台交易日历"));}
        let month=calendar_sources::month(&self.client,"/").await?;
        let days=month["days"].as_array().ok_or_else(||anyhow::anyhow!("完整月历缺失"))?;
        let mut states=serde_json::Map::new();
        for day in days {states.insert(day["date"].as_str().ok_or_else(||anyhow::anyhow!("月历日期缺失"))?.replace('-',""),day["is_open"].clone());}
        Ok(json!({"trading_days":states,"calendar_month":month["month"],"calendar_source":month["source"],"calendar_fetched_at":month["fetched_at"]}))
    }
    pub async fn background_symbols(&self,scope:&str)->Result<Vec<String>> {market_scan::native_symbols(&self.client,&self.db_path,scope).await}
    pub fn offline(&self)->bool{provider_cache::offline(&self.db_path)}

    pub async fn handle(&self, request: MobileApiRequest) -> MobileApiResponse {
        let cache=provider_cache::key(&request);
        let offline=provider_cache::offline(&self.db_path)||discovery::parameter(&request.path,"offline").as_deref()==Some("1");
        let refresh=discovery::parameter(&request.path,"refresh").as_deref()==Some("1");
        let saved=cache.as_ref().and_then(|(key,ttl)|provider_cache::read(&self.db_path,key,*ttl));
        if cache.is_some()&&offline{
            return match saved {Some((body,_,at))=>provider_cache::response(body,true,true,at,"离线模式:显示以前保存的公开数据,不是实时行情".into()),None=>MobileApiResponse::error(503,"offline_cache_miss","离线缓存中没有这份数据,请联网加载一次")};
        }
        let route=request.path.split('?').next().unwrap_or("");
        let post=request.method.eq_ignore_ascii_case("POST");
        if offline&&((post&&matches!(route,"/fetch"|"/mobile/ai/chat"|"/mobile/ai/chat/stream"|"/mobile/ai/chat/tools"|"/mobile/ai/review"|"/mobile/ai/recognize"|"/mobile/ai/backtest"|"/mobile/ai/workflow"|"/mobile/backtest"|"/mobile/watch/poll"|"/mobile/scan"|"/mobile/calendar/sync"|"/mobile/symbol-search"|"/mobile/messages/generate"|"/mobile/ai/config/test"|"/mobile/ai/config"|"/mobile/shortline/verify"))||matches!(route,"/mobile/intraday"|"/mobile/monitor"|"/mobile/portfolio/perf"|"/mobile/portfolio/xray"|"/mobile/portfolio/replay"|"/mobile/journal/excursion"|"/mobile/journal/environment"|"/mobile/journal/attribution"|"/mobile/shortline/verify"|"/mobile/shortline/history"|"/mobile/shortline/cycle"|"/mobile/shortline/risk"|"/mobile/shortline/samples"|"/mobile/review/structure"|"/mobile/review/tiers"|"/mobile/shortline/prev-broken"|"/mobile/shortline/theme-tree"|"/mobile/news/probability"|"/mobile/research/estimates"|"/mobile/shortline/limit-up")){
            // Local workflow load/create/cancel do not call a provider.
            if route!="/mobile/ai/workflow"||request.body["action"]=="advance"{return MobileApiResponse::error(503,"offline_mode","当前为离线模式,请在设置中恢复联网后执行此操作");}
        }
        if !refresh{if let Some((body,false,at))=saved.as_ref(){return provider_cache::response(body.clone(),false,false,at.clone(),String::new());}}
        let result=match self.handle_inner(request).await {
            Ok(response) => response,
            Err(error) => MobileApiResponse::error(500, "mobile_core", error.to_string()),
        };
        if let Some((key,_))=cache{
            return provider_cache::finish(&self.db_path,&key,saved,result);
        }
        result
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
            ("GET","/mobile/industry/library/catalog")=>Ok(MobileApiResponse::ok(industry_library::catalog())),
            ("GET","/mobile/industry/library")=>Ok(match discovery::parameter(&request.path,"sector").ok_or_else(||anyhow::anyhow!("请选择产业赛道")).and_then(|key|industry_library::list(&self.db_path,&key,None)){Ok(v)=>MobileApiResponse::ok(v),Err(e)=>MobileApiResponse::error(400,"industry_library",e.to_string())}),
            ("POST","/mobile/industry/library")=>Ok(match industry_library::mutate(&self.db_path,&request.body){Ok(v)=>MobileApiResponse::ok(v),Err(e)=>MobileApiResponse::error(400,"industry_library",e.to_string())}),
            ("GET","/mobile/calc/catalog")=>Ok(MobileApiResponse::ok(deterministic_calc::catalog())),
            ("POST","/mobile/calc")=>{
                let valid=request.body.as_object().is_some_and(|m|m.keys().all(|k|["function","args"].contains(&k.as_str())));
                if !valid{return Ok(MobileApiResponse::error(400,"calc_args","仅允许 function 与 args"));}
                Ok(MobileApiResponse::ok(deterministic_calc::execute(request.body["function"].as_str().unwrap_or(""),&request.body["args"])))
            },
            ("GET","/mobile/research/calculations")=>Ok(match research_calculations::view(&self.db_path,&request.path){Ok(value)=>MobileApiResponse::ok(value),Err(e)=>MobileApiResponse::error(400,"calculation_artifact",e.to_string())}),
            ("GET","/mobile/cache")=>Ok(MobileApiResponse::ok(provider_cache::settings(&self.db_path,None)?)),
            ("POST","/mobile/cache")=>Ok(match provider_cache::settings(&self.db_path,Some(&request.body)){Ok(value)=>MobileApiResponse::ok(value),Err(e)=>MobileApiResponse::error(400,"cache_settings",e.to_string())}),
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
            ("POST", "/mobile/documents/parse") => Ok(match document_import::parse(&request.body) {Ok(value)=>MobileApiResponse::ok(value),Err(error)=>MobileApiResponse::error(400,"document_parse",error.to_string())}),
            ("POST", "/mobile/documents/policy") => Ok(match documents::policy(&self.db_path,&request.body) {Ok(value)=>MobileApiResponse::ok(value),Err(error)=>MobileApiResponse::error(400,"document_policy",error.to_string())}),
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
            ("POST", "/mobile/data/clear") => match data_clear::run(&self.db_path, &request.body) {
                Ok(mut value) => {value["provider_cache_removed"]=json!(provider_cache::clear(&self.db_path)?);Ok(MobileApiResponse::ok(value))},
                Err(error) => Ok(MobileApiResponse::error(400, "data_clear", error.to_string())),
            },
            ("GET", "/mobile/messages") => Ok(MobileApiResponse::ok(messages::list(&self.db_path)?)),
            ("POST", "/mobile/messages") => match messages::act(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "messages", error.to_string())),
            },
            ("POST", "/mobile/messages/generate") => Ok(discovery_response(messages::generate(&self.client, &self.db_path).await)),
            ("GET", "/mobile/portfolio/perf") => Ok(discovery_response(portfolio_perf::view(&self.client, &self.db_path).await)),
            ("GET", "/mobile/portfolio/replay") => Ok(discovery_response(portfolio_replay::view(&self.client, &self.db_path).await)),
            ("GET", "/mobile/portfolio/xray") => Ok(discovery_response(portfolio_xray::view(&self.client, &self.db_path).await)),
            ("GET", "/mobile/portfolio/summary") => Ok(MobileApiResponse::ok(portfolio::view(&self.db_path)?)),
            ("POST", "/mobile/portfolio/dividend") => match portfolio::act(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "portfolio_dividend", error.to_string())),
            },
            ("POST", "/mobile/journal/import/preview") => match trade_import::preview(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "trade_import", error.to_string())),
            },
            ("POST", "/mobile/journal/import") => match trade_import::import(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "trade_import", error.to_string())),
            },
            ("POST", "/mobile/watchlist/meta") => match storage::set_meta(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "watchlist_meta", error.to_string())),
            },
            ("POST", "/mobile/watchlist/order") => match storage::set_order(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "watchlist_order", error.to_string())),
            },
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
            ("GET", "/mobile/journal/risk") => match journal_risk::view(&self.db_path) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(500, "journal_risk", error.to_string())),
            },
            ("GET", "/mobile/journal/modes") => match journal_modes::view(&self.db_path) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(500, "journal_modes", error.to_string())),
            },
            ("POST", "/mobile/journal/modes") => match journal_modes::act(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "journal_modes", error.to_string())),
            },
            ("GET", "/mobile/journal/weekly") => Ok(MobileApiResponse::ok(journal_weekly::view(&self.db_path,&request.path)?)),
            ("GET", "/mobile/journal/excursion") => Ok(discovery_response(journal_excursion::view(&self.client, &self.db_path, &request.path).await)),
            ("GET", "/mobile/journal/environment") => Ok(discovery_response(journal_env::view(&self.client, &self.db_path).await)),
            ("GET", "/mobile/journal/attribution") => Ok(discovery_response(journal_attribution::view(&self.client, &self.db_path, &request.path).await)),
            ("GET", "/mobile/journal/problems") => match journal_problems::view(&self.db_path) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(500, "journal_problems", error.to_string())),
            },
            ("POST", "/mobile/journal/problems") => match journal_problems::act(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "journal_problems", error.to_string())),
            },
            ("GET", "/mobile/journal/inbox") => match journal_inbox::view(&self.db_path) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(500, "journal_inbox", error.to_string())),
            },
            ("GET", "/mobile/journal/settings") => match journal_settings::view(&self.db_path) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(500, "journal_settings", error.to_string())),
            },
            ("POST", "/mobile/journal/settings") => match journal_settings::save(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "journal_settings", error.to_string())),
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
            ("GET", "/mobile/ai/chat") => match thread_param(&request.path) {
                Ok(thread) => match ai_chat::history(&self.db_path, thread.unwrap_or(ai_chat::DEFAULT_THREAD)) {
                    Ok(value) => Ok(MobileApiResponse::ok(value)),
                    Err(error) => Ok(MobileApiResponse::error(400, "ai_chat", error.to_string())),
                },
                Err(message) => Ok(MobileApiResponse::error(400, "ai_chat", message)),
            },
            ("DELETE", "/mobile/ai/chat") => match thread_param(&request.path) {
                Ok(thread) => Ok(MobileApiResponse::ok(ai_chat::clear(&self.db_path, thread)?)),
                Err(message) => Ok(MobileApiResponse::error(400, "ai_chat", message)),
            },
            ("GET", "/mobile/ai/threads") => Ok(MobileApiResponse::ok(ai_chat::threads(&self.db_path)?)),
            ("POST", "/mobile/ai/threads") => match ai_chat::thread_act(&self.db_path, &request.body) {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "ai_threads", error.to_string())),
            },
            ("POST", "/mobile/ai/chat/cancel") => {
                let id = request.body.get("request_id").and_then(Value::as_str).unwrap_or("");
                Ok(MobileApiResponse::ok(json!({"cancelled": self.ai_chat_cancel(id)})))
            }
            ("POST", "/mobile/ai/chat") | ("POST", "/mobile/ai/chat/stream") => match ai_chat::send(&self.client, &self.db_path, &request.body).await {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(ai_chat::ChatError::BadRequest(message)) => {
                    Ok(MobileApiResponse::error(400, "ai_chat", message))
                }
                Err(ai_chat::ChatError::Upstream(message)) => {
                    Ok(MobileApiResponse::error(502, "ai_chat_upstream", message))
                }
                Err(ai_chat::ChatError::Internal(error)) => Err(error),
            },
            ("POST", "/mobile/ai/chat/tools") => Ok(self.ai_chat_tools(request.body).await),
            ("POST", "/mobile/ai/recognize") => match ai_vision::recognize(&self.client, &self.db_path, &request.body).await {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(ai_vision::VisionError::BadRequest(message)) => Ok(MobileApiResponse::error(400, "ai_vision", message)),
                Err(ai_vision::VisionError::Upstream(message)) => Ok(MobileApiResponse::error(502, "ai_vision_upstream", message)),
                Err(ai_vision::VisionError::Internal(error)) => Err(error),
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
            ("POST", "/mobile/scan") => Ok(match market_scan::scan(&self.client,&self.db_path,&request.body).await{Ok(value)=>MobileApiResponse::ok(value),Err(e)=>MobileApiResponse::error(400,"market_scan",e.to_string())}),
            ("POST", "/mobile/watch/poll") => {
                let response = discovery_response(watch::poll(&self.client, &self.db_path, &self.watch).await);
                // 盯盘页开着时顺带检查竞价与盘中时点(每分钟最多一次):手机上没有常驻的定时任务
                let _ = intraday::beat(&self.client, &self.db_path, &self.intraday).await;
                Ok(response)
            }
            ("GET", "/mobile/intraday") => Ok(discovery_response(intraday::view(&self.client, &self.db_path, &self.intraday, &request.path).await)),
            ("POST", "/mobile/intraday") => {
                if request.body.get("action").and_then(Value::as_str) == Some("beat") {
                    let message = intraday::beat(&self.client, &self.db_path, &self.intraday).await;
                    return Ok(MobileApiResponse::ok(json!({ "message": message })));
                }
                Ok(discovery_response(intraday::act(&self.client, &self.db_path, &self.intraday, &request.body).await))
            }
            ("GET", "/mobile/watch/alerts") => Ok(discovery_response(watch::alerts(&self.db_path, &request.path))),
            ("POST", "/mobile/backtest") => match backtest::run(&self.client, &request.body).await {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(error) => Ok(MobileApiResponse::error(400, "backtest", error.to_string())),
            },
            ("GET", "/mobile/research/workflows/catalog") => Ok(MobileApiResponse::ok(research_workflow::catalog())),
            ("GET", "/mobile/calendar/month") => Ok(match calendar_sources::month(&self.client,&request.path).await { Ok(value)=>MobileApiResponse::ok(value),Err(error)=>MobileApiResponse::error(502,"exchange_calendar",error.to_string()) }),
            ("POST", "/mobile/calendar/sync") => Ok(match calendar_sources::sync(&self.client,&self.db_path,&request.body).await { Ok(value)=>MobileApiResponse::ok(value),Err(error)=>MobileApiResponse::error(400,"calendar_sync",error.to_string()) }),
            ("POST", "/mobile/calendar/reminder") => Ok(match calendar_sources::set_reminder(&self.db_path,&request.body) { Ok(value)=>MobileApiResponse::ok(value),Err(error)=>MobileApiResponse::error(400,"calendar_reminder",error.to_string()) }),
            ("GET", "/mobile/research/workflows") => Ok(MobileApiResponse::ok(research_workflow::list(&self.db_path)?)),
            ("POST", "/mobile/ai/workflow") => {
                let action = request.body["action"].as_str().unwrap_or("create");
                let id = request.body["id"].as_i64().unwrap_or(0);
                let outcome = match action {
                    "create" => research_workflow::create(&self.db_path, &request.body),
                    "load" => research_workflow::load(&self.db_path, id),
                    "cancel" => research_workflow::cancel(&self.db_path, id),
                    "advance" => research_workflow::advance(&self.client, &self.db_path, id, &request.body).await,
                    _ => Err(anyhow::anyhow!("无效的研究操作")),
                };
                Ok(match outcome {
                    Ok(value) => MobileApiResponse::ok(value),
                    Err(error) => MobileApiResponse::error(400, "research_workflow", error.to_string()),
                })
            }
            ("GET", "/mobile/backtest/tools") => Ok(MobileApiResponse::ok(ai_backtest::tool_catalog())),
            ("POST", "/mobile/ai/backtest") => match ai_backtest::run(&self.client, &self.db_path, &request.body).await {
                Ok(value) => Ok(MobileApiResponse::ok(value)),
                Err(ai_chat::ChatError::BadRequest(message)) => Ok(MobileApiResponse::error(400, "ai_backtest", message)),
                Err(ai_chat::ChatError::Upstream(message)) => Ok(MobileApiResponse::error(502, "ai_backtest_upstream", message)),
                Err(ai_chat::ChatError::Internal(error)) => Ok(MobileApiResponse::error(500, "ai_backtest_internal", error.to_string())),
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
            ("GET", "/mobile/review/structure") => Ok(discovery_response(structure::fetch(&self.client, &self.db_path, &request.path).await)),
            ("POST", "/mobile/review/structure") => Ok(discovery_response(structure::act(&self.db_path, &request.body))),
            ("GET", "/mobile/review/tiers") => Ok(discovery_response(review_tiers::fetch(&self.client, &self.db_path, &request.path).await)),
            ("GET", "/mobile/review/market") => Ok(discovery_response(market_panel::fetch(&self.client, &request.path).await)),
            ("GET", "/mobile/shortline/prev-broken") => Ok(discovery_response(prev_broken::fetch(&self.client, &request.path).await)),
            ("GET", "/mobile/shortline/theme-tree") => Ok(discovery_response(theme_tree::fetch(&self.client, &self.db_path, &request.path).await)),
            ("GET", "/mobile/shortline/samples") => Ok(discovery_response(samples::fetch(&self.client, &self.db_path, &request.path).await)),
            ("GET", "/mobile/shortline/verify") => Ok(discovery_response(emotion_history::verify(&self.client, &self.db_path, &request.path).await)),
            ("POST", "/mobile/shortline/verify") => Ok(discovery_response(emotion_history::save_verify(&self.client, &self.db_path, &request.body).await)),
            ("GET", "/mobile/news/catalog") => Ok(MobileApiResponse::ok(news::catalog()?)),
            ("GET", "/mobile/news/probability") => Ok(match probability::fetch(&self.client).await{Ok(value)=>MobileApiResponse::ok(value),Err(e)=>MobileApiResponse::error(502,"probability",e.to_string())}),
            ("GET", "/mobile/news") => Ok(discovery_response(news::fetch(&self.client, &request.path).await)),
            ("GET", "/mobile/industry/revenue") => Ok(match industry_revenue::fetch(&self.client).await{Ok(v)=>MobileApiResponse::ok(v),Err(e)=>MobileApiResponse::error(502,"industry_revenue",e.to_string())}),
            ("GET", "/mobile/industry/gpu-rent") => Ok(discovery_response(industry_signals::gpu_rent(&self.client, &request.path).await)),
            ("GET", "/mobile/boards/detail") => Ok(discovery_response(boards::detail(&self.client, &request.path).await)),
            ("GET", "/mobile/rotation") => Ok(MobileApiResponse::ok(
                rotation::snapshot(&self.client, &request.path).await?,
            )),
            ("GET", "/mobile/research/profile") => {
                Ok(research_response(research::profile(&self.client, &request.path).await))
            }
            ("GET", "/mobile/research/statements") => Ok(research_response(research_extended::statements(&self.client, &request.path).await)),
            ("GET", "/mobile/research/consensus") => Ok(research_response(research_extended::consensus(&self.client, &request.path).await)),
            ("GET", "/mobile/research/valuation") => Ok(research_response(research_extended::valuation(&self.client, &request.path).await)),
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
            "/mobile/intraday?date=2999-01-01",
            "/mobile/intraday?date=bad",
            "/mobile/shortline/samples?days=9",
            "/mobile/shortline/samples?days=91",
            "/mobile/shortline/samples?date=2026-02-30",
            "/mobile/shortline/samples?refresh=2",
            "/mobile/shortline/theme-tree?top=2",
            "/mobile/shortline/prev-broken?date=2026-02-30",
            "/mobile/shortline/theme-tree?date=2026-02-30",
            "/mobile/review/market?date=2026-02-30",
            "/mobile/review/tiers?date=2026-02-30",
            "/mobile/review/structure?date=2026-02-30",
            "/mobile/review/structure?refresh=2",
            "/mobile/review/tiers?date=2999-01-01",
            "/mobile/review/market?date=2999-01-01",
            "/mobile/journal/excursion?mode=live",
            "/mobile/journal/attribution?date=2999-01-01",
            "/mobile/journal/attribution?date=2026-02-30",
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
