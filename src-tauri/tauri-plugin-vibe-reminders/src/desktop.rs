use serde::de::DeserializeOwned;
use serde_json::Value;
use std::marker::PhantomData;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::{rule_count, ReminderNavigation, ReminderSyncStatus, Result};

pub struct VibeReminders<R: Runtime>(PhantomData<R>);

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> Result<VibeReminders<R>> {
    Ok(VibeReminders(PhantomData))
}

impl<R: Runtime> VibeReminders<R> {
    pub fn monitor(&self, _request: &Value) -> Result<Value> {
        Ok(
            serde_json::json!({"supported":false,"running":false,"message":"持续后台盯盘需要 Android 原生服务"}),
        )
    }
    pub fn sync_rules(&self, snapshot: &Value) -> Result<ReminderSyncStatus> {
        Ok(ReminderSyncStatus {
            scheduled: false,
            rule_count: rule_count(snapshot),
            interval_minutes: 0,
        })
    }

    pub fn take_navigation(&self) -> Result<ReminderNavigation> {
        Ok(ReminderNavigation { route: None })
    }

    pub fn status(&self) -> Result<ReminderSyncStatus> {
        Ok(ReminderSyncStatus {
            scheduled: false,
            rule_count: 0,
            interval_minutes: 0,
        })
    }
}
