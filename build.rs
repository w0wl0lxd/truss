use std::path::Path;

fn main() {
    if let Some(edition) = workspace_edition() {
        println!("cargo:rustc-env=CARGO_PKG_EDITION={edition}");
    }
}

fn workspace_edition() -> Option<String> {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")?;
    let workspace_manifest = Path::new(&manifest_dir).join("../../Cargo.toml");
    println!("cargo:rerun-if-changed={}", workspace_manifest.display());
    let manifest = std::fs::read_to_string(workspace_manifest).ok()?;
    let section = manifest.split("[workspace.package]").nth(1)?;
    let value = section.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "edition").then_some(value.trim())
    })?;
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .map(str::to_string)
}
