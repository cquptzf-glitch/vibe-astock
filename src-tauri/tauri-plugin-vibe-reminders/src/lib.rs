mod error;

pub use error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{plugin::TauriPlugin, Manager, Runtime};

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

#[cfg(desktop)]
pub use desktop::VibeReminders;
#[cfg(mobile)]
pub use mobile::VibeReminders;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderSyncStatus {
    pub scheduled: bool,
    pub rule_count: usize,
    pub interval_minutes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderNavigation {
    pub route: Option<String>,
}

pub trait VibeRemindersExt<R: Runtime> {
    fn vibe_reminders(&self) -> &VibeReminders<R>;
}

impl<R: Runtime, T: Manager<R>> VibeRemindersExt<R> for T {
    fn vibe_reminders(&self) -> &VibeReminders<R> {
        self.state::<VibeReminders<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::<R>::new("vibe-reminders")
        .setup(|app, api| {
            #[cfg(mobile)]
            let reminders = mobile::init(app, api)?;
            #[cfg(desktop)]
            let reminders = desktop::init(app, api)?;
            app.manage(reminders);
            Ok(())
        })
        .build()
}

pub(crate) fn rule_count(snapshot: &Value) -> usize {
    snapshot
        .get("rules")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}
