use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use encoding_rs::GBK;
use reqwest::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, time::{Duration, Instant}};

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
        Self { status, body: json!({ "error": code, "message": message.into() }) }
    }
}

pub struct AppCore {
    client: Client,
    db_path: PathBuf,
}

impl AppCore {
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&data_dir).with_context(|| format!("create app data dir: {}", data_dir.display()))?;
        let db_path = data_dir.join("vibe-astock-mobile.sqlite3");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS watchlist (
               symbol TEXT PRIMARY KEY NOT NULL,
               name TEXT,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );",
        )?;
        drop(conn);

        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(15))
            .user_agent("VibeAStock-Mobile/0.1")
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
                "version": "0.1.0-mobile"
            }))),
            ("GET", "/product") => Ok(MobileApiResponse::ok(json!({
                "version": "0.1.0-mobile",
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
            ("GET", "/mobile/watchlist") => Ok(MobileApiResponse::ok(self.watchlist_list()?)),
            ("POST", "/mobile/watchlist") => Ok(MobileApiResponse::ok(self.watchlist_upsert(&request.body)?)),
            ("DELETE", "/mobile/watchlist") => Ok(MobileApiResponse::ok(self.watchlist_delete(&request.body)?)),
            ("POST", "/fetch") => self.fetch_endpoint(&request.body).await,
            _ => Ok(MobileApiResponse::error(
                501,
                "mobile_not_migrated",
                format!("Android Rust Core 尚未迁移接口: {} {}", method, request.path),
            )),
        }
    }

    fn open_db(&self) -> Result<Connection> {
        Connection::open(&self.db_path).context("open mobile sqlite")
    }

    fn watchlist_list(&self) -> Result<Value> {
        let conn = self.open_db()?;
        let mut stmt = conn.prepare(
            "SELECT symbol, COALESCE(name, ''), created_at, updated_at FROM watchlist ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(json!({
                "symbol": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "created_at": row.get::<_, String>(2)?,
                "updated_at": row.get::<_, String>(3)?,
            }))
        })?;
        let items = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(json!({ "items": items }))
    }

    fn watchlist_upsert(&self, body: &Value) -> Result<Value> {
        let symbol = body.get("symbol").and_then(Value::as_str).unwrap_or("").trim();
        if symbol.is_empty() {
            return Err(anyhow!("watchlist symbol is required"));
        }
        let name = body.get("name").and_then(Value::as_str).unwrap_or("").trim();
        let now = Utc::now().to_rfc3339();
        let conn = self.open_db()?;
        conn.execute(
            "INSERT INTO watchlist(symbol, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(symbol) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at",
            params![symbol, name, now],
        )?;
        self.watchlist_list()
    }

    fn watchlist_delete(&self, body: &Value) -> Result<Value> {
        let symbol = body.get("symbol").and_then(Value::as_str).unwrap_or("").trim();
        if symbol.is_empty() {
            return Err(anyhow!("watchlist symbol is required"));
        }
        let conn = self.open_db()?;
        let removed = conn.execute("DELETE FROM watchlist WHERE symbol = ?1", params![symbol])? > 0;
        Ok(json!({ "removed": removed, "symbol": symbol }))
    }

    async fn fetch_endpoint(&self, body: &Value) -> Result<MobileApiResponse> {
        let endpoint = body.get("endpoint").and_then(Value::as_str).unwrap_or("");
        let symbol = body.get("symbol").and_then(Value::as_str).unwrap_or("");
        if symbol.is_empty() {
            return Ok(MobileApiResponse::error(400, "bad_symbol", "fetch.symbol 不能为空"));
        }
        let started = Instant::now();

        let envelope = match endpoint {
            "tx_quote" | "quote" | "fetch_quote" => self.fetch_quote(symbol).await?,
            "em_kline" | "kline" | "fetch_kline" => {
                let bars = body
                    .get("args")
                    .and_then(|v| v.get("bars"))
                    .and_then(Value::as_u64)
                    .unwrap_or(250)
                    .clamp(20, 1000) as usize;
                self.fetch_kline(symbol, bars).await?
            }
            _ => {
                return Ok(MobileApiResponse::error(
                    501,
                    "mobile_endpoint_not_migrated",
                    format!("Android 尚未迁移数据端点: {endpoint}"),
                ));
            }
        };

        let fetched_at = Utc::now().to_rfc3339();
        Ok(MobileApiResponse::ok(json!({
            "envelope": envelope,
            "duration_ms": started.elapsed().as_millis() as u64,
            "cached": false,
            "fetched_at": fetched_at,
        })))
    }

    async fn fetch_quote(&self, symbol: &str) -> Result<Value> {
        match self.fetch_tencent_quote(symbol).await {
            Ok(value) => Ok(value),
            Err(primary_error) => {
                let mut fallback = self.fetch_eastmoney_quote(symbol).await?;
                if let Some(obj) = fallback.as_object_mut() {
                    obj.insert("status".into(), Value::String("partial".into()));
                    obj.insert(
                        "errors".into(),
                        json!([{ "source": "tencent", "endpoint": "qt.gtimg.cn/q", "message": primary_error.to_string() }]),
                    );
                    if let Some(extra) = obj.get_mut("extra").and_then(Value::as_object_mut) {
                        extra.insert("degraded".into(), Value::String("主源腾讯失败,已降级东财 push2".into()));
                    }
                }
                Ok(fallback)
            }
        }
    }

    async fn fetch_tencent_quote(&self, raw_symbol: &str) -> Result<Value> {
        let symbol = SymbolInfo::parse(raw_symbol)?;
        let url = format!("https://qt.gtimg.cn/q={}", symbol.tencent_code);
        let bytes = self.client.get(url).send().await?.error_for_status()?.bytes().await?;
        let (decoded, _, _) = GBK.decode(&bytes);
        let text = decoded.into_owned();
        let payload = text.split('"').nth(1).ok_or_else(|| anyhow!("腾讯返回无载荷"))?;
        let fields: Vec<&str> = payload.split('~').collect();
        if fields.len() < 53 || fields.get(3).copied().unwrap_or("").is_empty() {
            return Err(anyhow!("腾讯字段不足或无报价"));
        }

        let now = Utc::now().to_rfc3339();
        let quote_time = fields.get(30).copied().unwrap_or("");
        let period = quote_date(quote_time).unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());
        let price = number(fields.get(3).copied());
        let last_close = number(fields.get(4).copied());
        let amount = number(fields.get(37).copied());
        let stale = matches!((price, last_close, amount), (Some(p), Some(c), Some(a)) if a == 0.0 && p == c && p > 0.0);
        let specs = [
            ("price", price, "元", "none"),
            ("last_close", last_close, "元", "none"),
            ("change_pct", number(fields.get(32).copied()), "%", "none"),
            ("turnover_pct", number(fields.get(38).copied()), "%", "none"),
            ("pe_ttm", number(fields.get(39).copied()), "倍", "not_applicable"),
            ("pe_static", number(fields.get(52).copied()), "倍", "not_applicable"),
            ("pb", number(fields.get(46).copied()), "倍", "not_applicable"),
            ("float_market_cap", number(fields.get(44).copied()), "亿元", "not_applicable"),
            ("total_market_cap", number(fields.get(45).copied()), "亿元", "not_applicable"),
            ("amount", amount, "万元", "not_applicable"),
        ];
        let (evidence, missing) = evidence_set(&symbol, &now, &period, "tencent", "qt.gtimg.cn/q", &specs);

        Ok(json!({
            "script": "fetch_quote",
            "symbol": symbol.code,
            "market": symbol.market,
            "status": if stale { "partial" } else { "ok" },
            "fetched_at": now,
            "primary_source": "tencent",
            "used_sources": ["tencent"],
            "evidence": evidence,
            "extra": {
                "name": fields.get(1).copied().unwrap_or(""),
                "quote_date": period,
                "quote_time": if quote_time.is_empty() { Value::Null } else { Value::String(quote_time.to_string()) },
                "is_stale": stale,
                "degraded": if stale { Value::String("成交额为 0 且现价等于昨收,可能是盘前/停牌/无效报价".into()) } else { Value::Null }
            },
            "errors": [],
            "missing": missing,
        }))
    }

    async fn fetch_eastmoney_quote(&self, raw_symbol: &str) -> Result<Value> {
        let symbol = SymbolInfo::parse(raw_symbol)?;
        let response = self.client
            .get("https://push2.eastmoney.com/api/qt/stock/get")
            .query(&[
                ("fltt", "2"),
                ("invt", "2"),
                ("secid", symbol.eastmoney_secid.as_str()),
                ("fields", "f43,f48,f58,f60,f86,f116,f117,f163,f164,f167,f168,f170"),
            ])
            .send().await?.error_for_status()?;
        let root: Value = response.json().await?;
        let data = root.get("data").and_then(Value::as_object).ok_or_else(|| anyhow!("东财 push2 无数据"))?;
        let price = value_number(data.get("f43"));
        if price.is_none() { return Err(anyhow!("东财 push2 无报价")); }
        let now = Utc::now().to_rfc3339();
        let period = Utc::now().format("%Y-%m-%d").to_string();
        let specs = [
            ("price", price, "元", "none"),
            ("last_close", value_number(data.get("f60")), "元", "none"),
            ("change_pct", value_number(data.get("f170")), "%", "none"),
            ("turnover_pct", value_number(data.get("f168")), "%", "none"),
            ("pe_ttm", value_number(data.get("f164")), "倍", "not_applicable"),
            ("pe_static", value_number(data.get("f163")), "倍", "not_applicable"),
            ("pb", value_number(data.get("f167")), "倍", "not_applicable"),
            ("float_market_cap", value_number(data.get("f117")), "元", "not_applicable"),
            ("total_market_cap", value_number(data.get("f116")), "元", "not_applicable"),
            ("amount", value_number(data.get("f48")), "元", "not_applicable"),
        ];
        let (evidence, missing) = evidence_set(&symbol, &now, &period, "eastmoney", "push2/api/qt/stock/get", &specs);
        Ok(json!({
            "script": "fetch_quote",
            "symbol": symbol.code,
            "market": symbol.market,
            "status": "ok",
            "fetched_at": now,
            "primary_source": "eastmoney",
            "used_sources": ["eastmoney"],
            "evidence": evidence,
            "extra": {
                "name": data.get("f58").cloned().unwrap_or(Value::Null),
                "quote_date": period,
                "quote_time": data.get("f86").cloned().unwrap_or(Value::Null),
                "is_stale": "unknown"
            },
            "errors": [],
            "missing": missing,
        }))
    }

    async fn fetch_kline(&self, raw_symbol: &str, bars: usize) -> Result<Value> {
        let symbol = SymbolInfo::parse(raw_symbol)?;
        let response = self.client
            .get("https://push2his.eastmoney.com/api/qt/stock/kline/get")
            .query(&[
                ("secid", symbol.eastmoney_secid.as_str()),
                ("fields1", "f1,f2,f3,f4,f5,f6"),
                ("fields2", "f51,f52,f53,f54,f55,f56,f57,f58"),
                ("klt", "101"),
                ("fqt", "1"),
                ("end", "20500101"),
                ("lmt", &bars.to_string()),
            ])
            .send().await?.error_for_status()?;
        let root: Value = response.json().await?;
        let klines = root.pointer("/data/klines").and_then(Value::as_array).ok_or_else(|| anyhow!("东财 K 线为空"))?;

        let mut rows = Vec::with_capacity(klines.len());
        for raw in klines {
            let Some(line) = raw.as_str() else { continue; };
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() < 6 { continue; }
            let Some(open) = number(parts.get(1).copied()) else { continue; };
            let Some(close) = number(parts.get(2).copied()) else { continue; };
            let Some(high) = number(parts.get(3).copied()) else { continue; };
            let Some(low) = number(parts.get(4).copied()) else { continue; };
            let Some(volume) = number(parts.get(5).copied()) else { continue; };
            rows.push(json!([parts[0], open, close, high, low, volume]));
        }
        let last = rows.last().ok_or_else(|| anyhow!("东财 K 线无合格行"))?;
        let period = last.get(0).and_then(Value::as_str).unwrap_or("").to_string();
        let close = last.get(2).and_then(Value::as_f64).unwrap_or(0.0);
        let volume = last.get(5).and_then(Value::as_f64).unwrap_or(0.0);
        let now = Utc::now().to_rfc3339();
        let start = rows.first().and_then(|r| r.get(0)).and_then(Value::as_str).unwrap_or("");
        let evidence = vec![
            evidence(&symbol, &now, &period, "eastmoney", "push2his/api/qt/stock/kline/get", "volume_latest", volume, "源单位", "not_applicable"),
            evidence(&symbol, &now, &period, "eastmoney", "push2his/api/qt/stock/kline/get", "close_qfq_latest", close, "元", "qfq"),
            evidence(&symbol, &now, &format!("{}..{}", start, period), "eastmoney", "push2his/api/qt/stock/kline/get", "kline_points", rows.len() as f64, "根", "qfq"),
        ];

        Ok(json!({
            "script": "fetch_kline",
            "symbol": symbol.code,
            "market": symbol.market,
            "status": "ok",
            "fetched_at": now,
            "primary_source": "eastmoney",
            "used_sources": ["eastmoney"],
            "evidence": evidence,
            "extra": {
                "start": start,
                "end": period,
                "bars": rows.len(),
                "rows": rows,
                "adjustment": "qfq"
            },
            "errors": [],
            "missing": [],
        }))
    }
}

#[derive(Debug)]
struct SymbolInfo {
    code: String,
    market: String,
    tencent_code: String,
    eastmoney_secid: String,
}

impl SymbolInfo {
    fn parse(raw: &str) -> Result<Self> {
        let upper = raw.trim().to_uppercase().replace(' ', "");
        let (code, market) = if let Some(code) = upper.strip_prefix("SH") {
            (code.trim_start_matches(['.', ':']).to_string(), "SH")
        } else if let Some(code) = upper.strip_prefix("SZ") {
            (code.trim_start_matches(['.', ':']).to_string(), "SZ")
        } else if upper.ends_with(".SH") {
            (upper.trim_end_matches(".SH").to_string(), "SH")
        } else if upper.ends_with(".SZ") {
            (upper.trim_end_matches(".SZ").to_string(), "SZ")
        } else if upper.len() == 6 && upper.chars().all(|c| c.is_ascii_digit()) {
            (upper.clone(), if upper.starts_with('6') || upper.starts_with('9') { "SH" } else { "SZ" })
        } else {
            return Err(anyhow!("Android MVP 当前先支持沪深 A 股代码: {raw}"));
        };
        if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
            return Err(anyhow!("股票代码格式错误: {raw}"));
        }
        let is_sh = market == "SH";
        Ok(Self {
            tencent_code: format!("{}{}", if is_sh { "sh" } else { "sz" }, code),
            eastmoney_secid: format!("{}.{}", if is_sh { "1" } else { "0" }, code),
            code,
            market: market.to_string(),
        })
    }
}

fn number(value: Option<&str>) -> Option<f64> {
    let text = value?.trim();
    if text.is_empty() || text == "-" { return None; }
    text.parse::<f64>().ok().filter(|n| n.is_finite())
}

fn value_number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => number(Some(s.as_str())),
        _ => None,
    }
}

fn quote_date(ts: &str) -> Option<String> {
    if ts.len() < 8 || !ts[..8].chars().all(|c| c.is_ascii_digit()) { return None; }
    Some(format!("{}-{}-{}", &ts[0..4], &ts[4..6], &ts[6..8]))
}

fn evidence_set(
    symbol: &SymbolInfo,
    fetched_at: &str,
    period: &str,
    source: &str,
    endpoint: &str,
    specs: &[(&str, Option<f64>, &str, &str)],
) -> (Vec<Value>, Vec<Value>) {
    let mut evidence_rows = Vec::new();
    let mut missing = Vec::new();
    for (field, value, unit, adjustment) in specs {
        match value {
            Some(value) => evidence_rows.push(evidence(symbol, fetched_at, period, source, endpoint, field, *value, unit, adjustment)),
            None => missing.push(json!({ "field": field, "period": period })),
        }
    }
    (evidence_rows, missing)
}

fn evidence(
    symbol: &SymbolInfo,
    fetched_at: &str,
    period: &str,
    source: &str,
    endpoint: &str,
    field: &str,
    value: f64,
    unit: &str,
    adjustment: &str,
) -> Value {
    json!({
        "id": format!("mobile:{}:{}:{}:{}", source, symbol.code, field, period),
        "symbol": symbol.code,
        "market": symbol.market,
        "field": field,
        "value": value,
        "unit": unit,
        "currency": if unit == "元" || unit == "万元" || unit == "亿元" { "CNY" } else { "n/a" },
        "period": period,
        "as_of": period,
        "source": source,
        "endpoint": endpoint,
        "fetched_at": fetched_at,
        "adjustment": adjustment,
        "raw_ref": null,
        "record_key": null
    })
}
