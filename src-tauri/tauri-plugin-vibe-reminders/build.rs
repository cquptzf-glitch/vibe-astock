const COMMANDS: &[&str] = &["sync_rules", "status"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
