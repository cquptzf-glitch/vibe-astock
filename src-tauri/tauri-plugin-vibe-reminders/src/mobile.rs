use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

use crate::{rule_count, ReminderNavigation, ReminderSyncStatus, Result};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncRulesRequest {
    snapshot_json: String,
}

pub struct VibeReminders<R: Runtime>(PluginHandle<R>);

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> Result<VibeReminders<R>> {
    let handle = api.register_android_plugin("com.vibeastock.reminders", "RemindersPlugin")?;
    Ok(VibeReminders(handle))
}

impl<R: Runtime> VibeReminders<R> {
    pub fn monitor(&self, request:&Value)->Result<Value>{
        self.0.run_mobile_plugin("monitor",SyncRulesRequest{snapshot_json:serde_json::to_string(request)?}).map_err(Into::into)
    }
    pub fn sync_rules(&self, snapshot: &Value) -> Result<ReminderSyncStatus> {
        let rule_count = rule_count(snapshot);
        let request = SyncRulesRequest {
            snapshot_json: serde_json::to_string(snapshot)?,
        };
        let mut status: ReminderSyncStatus = self.0.run_mobile_plugin("syncRules", request)?;
        // The native layer is authoritative for scheduling, but the Rust side
        // keeps the count deterministic even if an older plugin omits it.
        status.rule_count = rule_count;
        Ok(status)
    }

    pub fn take_navigation(&self) -> Result<ReminderNavigation> {
        self.0.run_mobile_plugin("takeNavigation", ())
            .map_err(Into::into)
    }

    pub fn status(&self) -> Result<ReminderSyncStatus> {
        self.0.run_mobile_plugin("status", ())
            .map_err(Into::into)
    }
}
