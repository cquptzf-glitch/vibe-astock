mod commands;

use tauri::Manager;
use vibe_astock_core::AppCore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            app.manage(AppCore::new(data_dir)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::mobile_api])
        .run(tauri::generate_context!())
        .expect("error while running Vibe AStock mobile");
}
