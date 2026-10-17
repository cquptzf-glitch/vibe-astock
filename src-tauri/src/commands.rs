use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, State};
use tauri_plugin_vibe_reminders::VibeRemindersExt;
use tauri_plugin_vibe_share::VibeShareExt;
use vibe_astock_core::{AppCore, MobileApiRequest, MobileApiResponse};

use crate::secure_ai;

#[derive(Debug, Deserialize)]
pub struct ShareTextCommand {
    pub title: Option<String>,
    pub text: String,
    pub chooser_title: Option<String>,
}

fn response_error(status: u16, code: &str, message: impl Into<String>) -> MobileApiResponse {
    MobileApiResponse {
        status,
        body: json!({ "error": code, "message": message.into() }),
    }
}

fn put_internal(body: &mut Value, key: &str, value: Value) {
    if !body.is_object() {
        *body = json!({});
    }
    if let Some(object) = body.as_object_mut() {
        object.insert(key.to_string(), value);
    }
}

fn normalized_scope(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn sync_reminders(app: &AppHandle, state: &AppCore) -> Result<Value, String> {
    let snapshot = state
        .reminder_snapshot()
        .map_err(|error| format!("读取提醒规则失败: {error}"))?;
    let status = app
        .vibe_reminders()
        .sync_rules(&snapshot)
        .map_err(|error| format!("同步 Android 后台提醒失败: {error}"))?;
    serde_json::to_value(status).map_err(|error| format!("序列化提醒调度状态失败: {error}"))
}

#[tauri::command]
pub fn take_reminder_navigation(app: AppHandle) -> Result<Option<String>, String> {
    app.vibe_reminders()
        .take_navigation()
        .map(|navigation| navigation.route)
        .map_err(|_| "读取提醒入口失败".to_string())
}

#[tauri::command]
pub fn share_text(app: AppHandle, request: ShareTextCommand) -> Result<Value, String> {
    let title = request.title.as_deref().map(str::trim).filter(|value| !value.is_empty());
    let text = request.text.trim();
    let chooser_title = request
        .chooser_title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if text.is_empty() {
        return Err("没有可分享的内容".to_string());
    }
    if text.chars().count() > 100_000 {
        return Err("分享内容过长".to_string());
    }
    if title.is_some_and(|value| value.chars().count() > 300) {
        return Err("分享标题过长".to_string());
    }

    app.vibe_share()
        .share_text(title, text, chooser_title)
        .map_err(|error| format!("打开系统分享失败: {error}"))?;

    Ok(json!({ "ok": true }))
}

#[tauri::command]
pub async fn mobile_api(
    app: AppHandle,
    state: State<'_, AppCore>,
    mut request: MobileApiRequest,
) -> Result<MobileApiResponse, String> {
    let route = request.path.split('?').next().unwrap_or("").to_string();
    let method = request.method.to_uppercase();

    if method == "GET" && route == "/mobile/reminders/status" {
        return match app.vibe_reminders().status() {
            Ok(status) => Ok(MobileApiResponse {
                status: 200,
                body: serde_json::to_value(status)
                    .map_err(|error| format!("序列化提醒调度状态失败: {error}"))?,
            }),
            Err(error) => Ok(response_error(500, "reminder_scheduler", error.to_string())),
        };
    }

    // 统一清除全部本地数据:SQLite 由 Core 清;这里再删 Android Keystore 里的 API Key,并按清空后的规则重新同步提醒调度(取消所有已排的提醒)
    if route == "/mobile/data/clear" && method == "POST" {
        let mut response = state.handle(request).await;
        if response.status == 200 {
            let key_result = secure_ai::delete(&app);
            let sync_result = sync_reminders(&app, &state);
            if let Some(object) = response.body.as_object_mut() {
                object.insert("keystore_cleared".to_string(), Value::Bool(key_result.is_ok()));
                object.insert("reminders_cancelled".to_string(), Value::Bool(sync_result.is_ok()));
                if let Err(error) = &key_result {
                    object.insert("keystore_error".to_string(), Value::String(error.to_string()));
                }
                if let Err(error) = &sync_result {
                    object.insert("reminders_error".to_string(), Value::String(error.to_string()));
                }
            }
        }
        return Ok(response);
    }

    if (route == "/mobile/reminders" && (method == "POST" || method == "DELETE"))
        || (route == "/mobile/backup/import" && method == "POST")
    {
        let mut response = state.handle(request).await;
        if response.status == 200 {
            match sync_reminders(&app, &state) {
                Ok(status) => {
                    if let Some(object) = response.body.as_object_mut() {
                        object.insert("scheduler".to_string(), status);
                    }
                }
                Err(error) => return Ok(response_error(500, "reminder_scheduler", error)),
            }
        }
        return Ok(response);
    }

    if method == "GET" && (route == "/mobile/ai/config" || route == "/mobile/data/summary") {
        let present = match secure_ai::key_present(&app, &state) {
            Ok(value) => value,
            Err(error) => return Ok(response_error(500, "secure_storage", error)),
        };
        put_internal(&mut request.body, "__secure_key_present", Value::Bool(present));
        return Ok(state.handle(request).await);
    }

    if method == "POST" && route == "/mobile/ai/config" {
        let old_base = state
            .ai_base_url()
            .map_err(|error| format!("读取当前模型配置失败: {error}"))?
            .unwrap_or_default();
        let clear_key = request
            .body
            .get("clear_key")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let pending_key = match request.body.get("api_key") {
            None => None,
            Some(Value::String(value)) => {
                let trimmed = value.trim();
                if trimmed.len() > 4096 {
                    return Ok(response_error(400, "ai_config", "API Key 过长"));
                }
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            Some(_) => return Ok(response_error(400, "ai_config", "api_key 必须是字符串")),
        };
        if let Some(object) = request.body.as_object_mut() {
            object.remove("api_key");
        }
        let current_present = match secure_ai::key_present(&app, &state) {
            Ok(value) => value,
            Err(error) => return Ok(response_error(500, "secure_storage", error)),
        };
        put_internal(
            &mut request.body,
            "__secure_key_present",
            Value::Bool(current_present),
        );

        let mut response = state.handle(request).await;
        if response.status != 200 {
            return Ok(response);
        }

        let new_base = response
            .body
            .get("base_url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let storage_result = if clear_key {
            secure_ai::delete(&app)
        } else if let Some(api_key) = pending_key.as_deref() {
            secure_ai::save(&app, &new_base, api_key)
        } else if normalized_scope(&old_base) != normalized_scope(&new_base) {
            secure_ai::delete(&app)
        } else {
            Ok(())
        };
        if let Err(error) = storage_result {
            return Ok(response_error(500, "secure_storage", error));
        }

        let present = match secure_ai::key_present(&app, &state) {
            Ok(value) => value,
            Err(error) => return Ok(response_error(500, "secure_storage", error)),
        };
        if let Some(object) = response.body.as_object_mut() {
            object.insert("key_present".to_string(), Value::Bool(present));
            object.insert(
                "storage".to_string(),
                Value::String("android_keystore_aes_256_gcm".to_string()),
            );
        }
        return Ok(response);
    }

    if method == "DELETE" && route == "/mobile/ai/config" {
        if let Err(error) = secure_ai::delete(&app) {
            return Ok(response_error(500, "secure_storage", error));
        }
        return Ok(state.handle(request).await);
    }

    if method == "POST" && (route == "/mobile/ai/review" || route == "/mobile/ai/chat" || route == "/mobile/ai/recognize") {
        let key = match secure_ai::key_for_current_provider(&app, &state) {
            Ok(value) => value,
            Err(error) => return Ok(response_error(500, "secure_storage", error)),
        };
        if let Some(key) = key {
            put_internal(
                &mut request.body,
                "__secure_api_key",
                Value::String(key),
            );
        }
        return Ok(state.handle(request).await);
    }

    Ok(state.handle(request).await)
}
