use std::{fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let app_manifest = manifest_dir.join("../src-tauri/Cargo.toml");
    println!("cargo:rerun-if-changed={}", app_manifest.display());

    let manifest = fs::read_to_string(&app_manifest)
        .unwrap_or_else(|error| panic!("read {}: {error}", app_manifest.display()));
    let version = package_version(&manifest)
        .unwrap_or_else(|| panic!("missing [package].version in {}", app_manifest.display()));

    println!("cargo:rustc-env=VIBE_APP_VERSION={version}");
}

fn package_version(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for raw in manifest.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package || line.starts_with('#') || !line.starts_with("version") {
            continue;
        }
        let (_, value) = line.split_once('=')?;
        let version = value.trim().trim_matches('"').trim();
        if !version.is_empty() {
            return Some(version.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::package_version;

    #[test]
    fn reads_only_package_version() {
        let manifest = r#"
[package]
name = "app"
version = "1.2.3"

[dependencies]
foo = { version = "9.9.9" }
"#;
        assert_eq!(package_version(manifest).as_deref(), Some("1.2.3"));
    }
}
