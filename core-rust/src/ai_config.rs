use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use reqwest::Url;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug, Clone)]
pub(crate) struct AiConfig {
    pub base_url: String,
    pub model: String,
    pub updated_at: String,
    /// 用户标记「这个模型能看图」。不标就不会把截图发给模型;标了也不代表一定能用,页面有「测试图片识别」
    pub vision: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct LegacyCredential {
    pub base_url: String,
    pub api_key: String,
}

pub(crate) fn init(db_path: &Path) -> Result<()> {
    let conn = open(db_path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_ai_config(
            id INTEGER PRIMARY KEY CHECK(id = 1),
            base_url TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            api_key TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL,
            vision INTEGER NOT NULL DEFAULT 0
        );",
    )?;
    Ok(())
}

pub(crate) fn view(db_path: &Path, key_present: bool) -> Result<Value> {
    let config = load(db_path)?;
    Ok(match config {
        Some(config) => public_config(&config, key_present),
        None => empty_config(false),
    })
}

pub(crate) fn save(db_path: &Path, body: &Value, key_present: bool) -> Result<Value> {
    let current = load(db_path)?;
    let base_url = match body.get("base_url") {
        Some(Value::String(value)) => normalize_base_url(value)?,
        Some(_) => return Err(anyhow!("base_url 必须是字符串")),
        None => current
            .as_ref()
            .map(|value| value.base_url.clone())
            .unwrap_or_default(),
    };
    let model = match body.get("model") {
        Some(Value::String(value)) => normalize_model(value)?,
        Some(_) => return Err(anyhow!("model 必须是字符串")),
        None => current
            .as_ref()
            .map(|value| value.model.clone())
            .unwrap_or_default(),
    };
    let vision = match body.get("vision") {
        Some(Value::Bool(value)) => *value,
        Some(_) => return Err(anyhow!("vision 必须是布尔值")),
        None => current.as_ref().map(|value| value.vision).unwrap_or(false),
    };
    if base_url.is_empty() {
        return Err(anyhow!("请填写模型 API Base URL"));
    }
    if model.is_empty() {
        return Err(anyhow!("请填写模型名称"));
    }

    let updated_at = Utc::now().to_rfc3339();
    let conn = open(db_path)?;
    conn.execute(
        "INSERT INTO mobile_ai_config(id, base_url, model, api_key, updated_at, vision)
         VALUES (1, ?1, ?2, '', ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET
           base_url = excluded.base_url,
           model = excluded.model,
           api_key = '',
           updated_at = excluded.updated_at,
           vision = excluded.vision",
        params![base_url, model, updated_at, vision as i64],
    )?;
    view(db_path, key_present)
}

pub(crate) fn clear(db_path: &Path) -> Result<Value> {
    let conn = open(db_path)?;
    conn.execute("DELETE FROM mobile_ai_config WHERE id = 1", [])?;
    Ok(empty_config(false))
}

pub(crate) fn load(db_path: &Path) -> Result<Option<AiConfig>> {
    let conn = open(db_path)?;
    conn.query_row(
        "SELECT base_url, model, updated_at, vision FROM mobile_ai_config WHERE id = 1",
        [],
        |row| {
            Ok(AiConfig {
                base_url: row.get(0)?,
                model: row.get(1)?,
                updated_at: row.get(2)?,
                vision: row.get::<_, i64>(3)? != 0,
            })
        },
    )
    .optional()
    .context("load mobile AI config")
}

pub(crate) fn base_url(db_path: &Path) -> Result<Option<String>> {
    Ok(load(db_path)?.map(|config| config.base_url))
}

pub(crate) fn legacy_credential(db_path: &Path) -> Result<Option<LegacyCredential>> {
    let conn = open(db_path)?;
    conn.query_row(
        "SELECT base_url, api_key FROM mobile_ai_config
         WHERE id = 1 AND LENGTH(TRIM(api_key)) > 0",
        [],
        |row| {
            Ok(LegacyCredential {
                base_url: row.get(0)?,
                api_key: row.get(1)?,
            })
        },
    )
    .optional()
    .context("load legacy mobile AI credential")
}

pub(crate) fn clear_legacy_key(db_path: &Path) -> Result<()> {
    let conn = open(db_path)?;
    conn.execute(
        "UPDATE mobile_ai_config SET api_key = '' WHERE id = 1 AND api_key <> ''",
        [],
    )?;
    Ok(())
}

pub(crate) fn injected_api_key(body: &Value) -> &str {
    body.get("__secure_api_key")
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn public_config(config: &AiConfig, key_present: bool) -> Value {
    json!({
        "protocol": "openai_compatible_chat_completions",
        "base_url": config.base_url,
        "model": config.model,
        "vision": config.vision,
        "key_present": key_present,
        "configured": !config.base_url.is_empty() && !config.model.is_empty(),
        "updated_at": config.updated_at,
        "storage": "android_keystore_aes_256_gcm",
        "key_echoed": false
    })
}

fn empty_config(key_present: bool) -> Value {
    json!({
        "protocol": "openai_compatible_chat_completions",
        "base_url": "",
        "model": "",
        "vision": false,
        "key_present": key_present,
        "configured": false,
        "updated_at": null,
        "storage": "android_keystore_aes_256_gcm",
        "key_echoed": false
    })
}

fn normalize_model(value: &str) -> Result<String> {
    let value = value.trim();
    if value.chars().count() > 200 {
        return Err(anyhow!("模型名称过长"));
    }
    Ok(value.to_string())
}

fn normalize_base_url(value: &str) -> Result<String> {
    let value = value.trim().trim_end_matches('/');
    if value.is_empty() {
        return Ok(String::new());
    }
    let url = Url::parse(value).map_err(|_| anyhow!("Base URL 不是有效 URL"))?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(anyhow!("Base URL 只支持 http 或 https"));
    }
    if url.host_str().is_none() {
        return Err(anyhow!("Base URL 缺少主机名"));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(anyhow!("Base URL 不应包含账号、密码、查询参数或片段"));
    }
    Ok(value.to_string())
}

fn open(db_path: &Path) -> Result<Connection> {
    Connection::open(db_path).context("open mobile AI config sqlite")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db() -> std::path::PathBuf {
        // 并行测试在 Windows 上可能拿到同一个纳秒时间戳,再加进程内计数保证路径不撞。
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!("vibe-ai-config-{stamp}-{n}.sqlite3"))
    }

    #[test]
    fn save_never_persists_api_key() {
        let path = temp_db();
        init(&path).expect("init config");
        let result = save(
            &path,
            &json!({
                "base_url": "https://example.com/v1/",
                "model": "example-model",
                "api_key": "plaintext-should-never-be-stored"
            }),
            true,
        )
        .expect("save metadata");

        assert_eq!(result["key_present"], true);
        assert_eq!(result["base_url"], "https://example.com/v1");
        assert!(legacy_credential(&path).expect("read legacy").is_none());

        let conn = Connection::open(&path).expect("open db");
        let persisted: String = conn
            .query_row(
                "SELECT api_key FROM mobile_ai_config WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .expect("read persisted key");
        assert!(persisted.is_empty());
        drop(conn);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn legacy_key_can_be_scrubbed_after_secure_migration() {
        let path = temp_db();
        init(&path).expect("init config");
        let conn = Connection::open(&path).expect("open db");
        conn.execute(
            "INSERT INTO mobile_ai_config(id, base_url, model, api_key, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4)",
            params![
                "https://legacy.example/v1",
                "legacy-model",
                "legacy-secret",
                "now"
            ],
        )
        .expect("insert legacy config");
        drop(conn);

        let legacy = legacy_credential(&path)
            .expect("load legacy")
            .expect("legacy exists");
        assert_eq!(legacy.base_url, "https://legacy.example/v1");
        assert_eq!(legacy.api_key, "legacy-secret");

        clear_legacy_key(&path).expect("scrub legacy key");
        assert!(legacy_credential(&path).expect("reload legacy").is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn vision_switch_defaults_off_persists_and_is_kept_when_other_fields_change() {
        let path = std::env::temp_dir().join(format!(
            "vibe-ai-vision-{}.sqlite3",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        init(&path).unwrap();
        assert_eq!(
            view(&path, false).unwrap()["vision"],
            false,
            "没配置时是关的"
        );
        let saved = save(
            &path,
            &json!({"base_url": "https://example.com/v1", "model": "m1"}),
            false,
        )
        .unwrap();
        assert_eq!(saved["vision"], false, "不填就是关,不替用户打开");
        assert_eq!(
            save(&path, &json!({"vision": true}), false).unwrap()["vision"],
            true
        );
        assert_eq!(
            save(&path, &json!({"model": "m2"}), false).unwrap()["vision"],
            true,
            "只改模型名不动开关"
        );
        assert!(save(&path, &json!({"vision": "yes"}), false)
            .unwrap_err()
            .to_string()
            .contains("布尔值"));
        assert_eq!(
            save(&path, &json!({"vision": false}), false).unwrap()["vision"],
            false
        );
        assert!(!load(&path).unwrap().unwrap().vision);
        let _ = std::fs::remove_file(path);
    }
}
