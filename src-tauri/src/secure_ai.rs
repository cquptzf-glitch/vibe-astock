use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use vibe_astock_core::AppCore;

#[cfg(target_os = "android")]
use tauri_plugin_vibe_secure_storage::SecureStorageExt;

const AI_API_KEY_SLOT: &str = "ai.api_key.v1";

#[derive(Debug, Serialize, Deserialize)]
struct StoredCredential {
    base_url: String,
    api_key: String,
}

fn normalize_scope(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

#[cfg(target_os = "android")]
fn load<R: Runtime>(app: &AppHandle<R>) -> Result<Option<StoredCredential>, String> {
    let Some(raw) = app
        .secure_storage()
        .get(AI_API_KEY_SLOT)
        .map_err(|error| format!("读取 Android Keystore 凭据失败: {error}"))?
    else {
        return Ok(None);
    };

    match serde_json::from_str::<StoredCredential>(&raw) {
        Ok(value) => Ok(Some(value)),
        Err(_) => {
            app.secure_storage()
                .delete(AI_API_KEY_SLOT)
                .map_err(|error| format!("清理损坏的 Android Keystore 凭据失败: {error}"))?;
            Ok(None)
        }
    }
}

#[cfg(not(target_os = "android"))]
fn load<R: Runtime>(_app: &AppHandle<R>) -> Result<Option<StoredCredential>, String> {
    Ok(None)
}

pub fn key_for_current_provider<R: Runtime>(
    app: &AppHandle<R>,
    core: &AppCore,
) -> Result<Option<String>, String> {
    let Some(stored) = load(app)? else {
        return Ok(None);
    };
    let current = core
        .ai_base_url()
        .map_err(|error| format!("读取模型配置失败: {error}"))?
        .unwrap_or_default();
    if normalize_scope(&stored.base_url) != normalize_scope(&current) {
        return Ok(None);
    }
    let key = stored.api_key.trim();
    if key.is_empty() {
        Ok(None)
    } else {
        Ok(Some(key.to_string()))
    }
}

pub fn key_present<R: Runtime>(app: &AppHandle<R>, core: &AppCore) -> Result<bool, String> {
    Ok(key_for_current_provider(app, core)?.is_some())
}

#[cfg(target_os = "android")]
pub fn save<R: Runtime>(app: &AppHandle<R>, base_url: &str, api_key: &str) -> Result<(), String> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err("API Key 不能为空".into());
    }
    if api_key.len() > 4096 {
        return Err("API Key 过长".into());
    }
    let encoded = serde_json::to_string(&StoredCredential {
        base_url: normalize_scope(base_url),
        api_key: api_key.to_string(),
    })
    .map_err(|error| format!("序列化安全凭据失败: {error}"))?;
    app.secure_storage()
        .set(AI_API_KEY_SLOT, &encoded)
        .map_err(|error| format!("写入 Android Keystore 凭据失败: {error}"))
}

#[cfg(not(target_os = "android"))]
pub fn save<R: Runtime>(_app: &AppHandle<R>, _base_url: &str, _api_key: &str) -> Result<(), String> {
    Err("安全凭据存储仅在 Android 应用中启用".into())
}

#[cfg(target_os = "android")]
pub fn delete<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    app.secure_storage()
        .delete(AI_API_KEY_SLOT)
        .map_err(|error| format!("清除 Android Keystore 凭据失败: {error}"))
}

#[cfg(not(target_os = "android"))]
pub fn delete<R: Runtime>(_app: &AppHandle<R>) -> Result<(), String> {
    Ok(())
}

pub fn migrate_legacy<R: Runtime>(app: &AppHandle<R>, core: &AppCore) -> Result<(), String> {
    let Some((base_url, api_key)) = core
        .legacy_ai_credential()
        .map_err(|error| format!("读取旧版 SQLite API Key 失败: {error}"))?
    else {
        return Ok(());
    };

    #[cfg(target_os = "android")]
    {
        let already_secure = app
            .secure_storage()
            .contains(AI_API_KEY_SLOT)
            .map_err(|error| format!("检查 Android Keystore 凭据失败: {error}"))?;
        if !already_secure {
            save(app, &base_url, &api_key)?;
        }
        core.clear_legacy_ai_key()
            .map_err(|error| format!("擦除旧版 SQLite API Key 失败: {error}"))?;
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, base_url, api_key);
    }

    Ok(())
}
