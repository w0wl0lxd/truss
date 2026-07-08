use std::path::Path;
use tempfile::tempdir;
use truss_core::{check_workspace, new_workspace, sync_workspace, SyncContext};

fn context() -> SyncContext {
    SyncContext::new()
        .with_project_name("demo")
        .with_author("tester")
        .with_license("MIT")
        .with_repository("https://example.com/demo")
}

#[test]
fn new_then_check_has_no_drift() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    let ctx = context();

    new_workspace(path, "default", &ctx).expect("new_workspace");
    assert!(path.join("Cargo.toml").is_file());
    assert!(path.join("flake.nix").is_file());
    assert!(path.join("AGENTS.md").is_file());

    let cargo = std::fs::read_to_string(path.join("Cargo.toml")).expect("read cargo");
    assert!(cargo.contains("tester"));
    assert!(cargo.contains("MIT"));
    assert!(cargo.contains("https://example.com/demo"));
    assert!(cargo.contains("2024"));

    let drift = check_workspace(path, "default", &ctx).expect("check");
    assert!(drift.is_empty(), "unexpected drift: {drift:?}");
}

#[test]
fn sync_then_check_is_idempotent() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    let ctx = context();

    new_workspace(path, "default", &ctx).expect("new");
    sync_workspace(path, "default", &ctx).expect("sync");
    let drift = check_workspace(path, "default", &ctx).expect("check");
    assert!(drift.is_empty());
}

#[test]
fn check_detects_modified_file() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    let ctx = context();

    new_workspace(path, "default", &ctx).expect("new");
    std::fs::write(path.join("AGENTS.md"), "changed").expect("write");
    let drift = check_workspace(path, "default", &ctx).expect("check");
    assert!(!drift.is_empty());
    assert!(drift.iter().any(|d| d.file == "AGENTS.md"));
}

#[test]
fn missing_template_errors() {
    let dir = tempdir().expect("tempdir");
    let err = new_workspace(dir.path(), "does-not-exist", &context());
    assert!(err.is_err());
}

#[test]
fn template_load_lists_default() {
    let names = truss_core::Template::list_embedded();
    assert!(names.iter().any(|n| n == "default"));
    let template = truss_core::Template::load("default").expect("load default");
    assert!(!template.files.is_empty());
    assert_eq!(template.name, "default");
    let _ = Path::new(".");
}
