use std::path::Path;
use tempfile::tempdir;
use truss_core::{SyncContext, check_workspace, new_workspace, sync_workspace};

fn context() -> SyncContext {
    SyncContext::new()
        .with_project_name("demo")
        .with_author("tester")
        .with_license(env!("CARGO_PKG_LICENSE"))
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
    assert!(path.join("crates/app/Cargo.toml").is_file());
    assert!(path.join("crates/app/src/main.rs").is_file());

    let cargo = std::fs::read_to_string(path.join("Cargo.toml")).expect("read cargo");
    assert!(cargo.contains("tester"));
    assert!(cargo.contains(env!("CARGO_PKG_LICENSE")));
    assert!(cargo.contains("https://example.com/demo"));
    assert!(cargo.contains(env!("CARGO_PKG_EDITION")));

    let drift = check_workspace(path, "default", &ctx).expect("check");
    assert!(drift.is_empty(), "unexpected drift: {drift:?}");
}

#[test]
fn context_reads_workspace_package_metadata() {
    let dir = tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        format!(
            "[workspace.package]\nauthors = [\"tester\"]\nlicense = \"{}\"\nedition = \"{}\"\nrepository = \"https://example.com/demo\"\n\n[package]\nauthors = [\"fallback\"]\nlicense = \"fallback-license\"\nedition = \"fallback-edition\"\nrepository = \"https://example.com/fallback\"\n",
            env!("CARGO_PKG_LICENSE"),
            env!("CARGO_PKG_EDITION")
        ),
    )
    .expect("write cargo");

    let ctx = SyncContext::from_workspace(dir.path()).expect("read workspace");

    assert_eq!(ctx.author, "tester");
    assert_eq!(ctx.license, env!("CARGO_PKG_LICENSE"));
    assert_eq!(ctx.edition, env!("CARGO_PKG_EDITION"));
    assert_eq!(ctx.repository, "https://example.com/demo");
}

#[test]
fn context_reads_package_metadata_when_workspace_metadata_is_missing() {
    let dir = tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        format!(
            "[package]\nauthors = [\"tester\"]\nlicense = \"{}\"\nedition = \"{}\"\nrepository = \"https://example.com/demo\"\n",
            env!("CARGO_PKG_LICENSE"),
            env!("CARGO_PKG_EDITION")
        ),
    )
    .expect("write cargo");

    let ctx = SyncContext::from_workspace(dir.path()).expect("read package");

    assert_eq!(ctx.author, "tester");
    assert_eq!(ctx.license, env!("CARGO_PKG_LICENSE"));
    assert_eq!(ctx.edition, env!("CARGO_PKG_EDITION"));
    assert_eq!(ctx.repository, "https://example.com/demo");
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
    for required in ["default", "spec-kit", "agent-rules"] {
        assert!(
            names.iter().any(|n| n == required),
            "missing embedded template {required:?} in {names:?}"
        );
    }
    let template = truss_core::Template::load("default").expect("load default");
    assert!(!template.files.is_empty());
    assert_eq!(template.name, "default");
    let _ = Path::new(".");
}
