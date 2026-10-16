mod commands;
mod secure_ai;

use tauri::Manager;
use tauri_plugin_vibe_reminders::VibeRemindersExt;
use vibe_astock_core::AppCore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_vibe_share::init())
        .plugin(tauri_plugin_vibe_reminders::init())
        .plugin(tauri_plugin_vibe_secure_storage::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let core = AppCore::new(data_dir)?;
            secure_ai::migrate_legacy(app.handle(), &core)
                .map_err(std::io::Error::other)?;
            let reminder_snapshot = core.reminder_snapshot()?;
            app.vibe_reminders()
                .sync_rules(&reminder_snapshot)
                .map_err(std::io::Error::other)?;
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::mobile_api, commands::share_text, commands::take_reminder_navigation])
        .run(tauri::generate_context!())
        .expect("error while running Vibe AStock mobile");
}
