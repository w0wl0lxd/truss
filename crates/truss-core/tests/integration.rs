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
    assert!(cargo.contains(option_env!("CARGO_PKG_EDITION").unwrap_or_else(|| "2024")));

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
            option_env!("CARGO_PKG_EDITION").unwrap_or_else(|| "2024")
        ),
    )
    .expect("write cargo");

    let ctx = SyncContext::from_workspace(dir.path()).expect("read workspace");

    assert_eq!(ctx.author, "tester");
    assert_eq!(ctx.license, env!("CARGO_PKG_LICENSE"));
    assert_eq!(
        ctx.edition,
        option_env!("CARGO_PKG_EDITION").unwrap_or_else(|| "2024")
    );
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
            option_env!("CARGO_PKG_EDITION").unwrap_or_else(|| "2024")
        ),
    )
    .expect("write cargo");

    let ctx = SyncContext::from_workspace(dir.path()).expect("read package");

    assert_eq!(ctx.author, "tester");
    assert_eq!(ctx.license, env!("CARGO_PKG_LICENSE"));
    assert_eq!(
        ctx.edition,
        option_env!("CARGO_PKG_EDITION").unwrap_or_else(|| "2024")
    );
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
    for required in ["default", "spec-kit", "agent-rules", "monorepo"] {
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

#[test]
fn new_monorepo_workspace_creates_members_and_deps() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    let ctx = context();

    new_workspace(path, "monorepo", &ctx).expect("new monorepo");

    let root_cargo = std::fs::read_to_string(path.join("Cargo.toml")).expect("read root cargo");
    assert!(root_cargo.contains("\"apps/app\""));
    assert!(root_cargo.contains("\"libs/shared\""));
    assert!(root_cargo.contains("\"tools/dev\""));

    assert!(path.join("apps/app/Cargo.toml").is_file());
    assert!(path.join("apps/app/src/main.rs").is_file());
    assert!(path.join("libs/shared/Cargo.toml").is_file());
    assert!(path.join("libs/shared/src/lib.rs").is_file());
    assert!(path.join("tools/dev/Cargo.toml").is_file());
    assert!(path.join("tools/dev/src/main.rs").is_file());

    let app_cargo =
        std::fs::read_to_string(path.join("apps/app/Cargo.toml")).expect("read app cargo");
    assert!(app_cargo.contains(r#"shared = { path = "../../libs/shared" }"#));

    let dev_cargo =
        std::fs::read_to_string(path.join("tools/dev/Cargo.toml")).expect("read dev cargo");
    assert!(dev_cargo.contains(r#"shared = { path = "../../libs/shared" }"#));

    // layout.toml should never be copied into the generated workspace.
    assert!(!path.join("layout.toml").exists());

    let drift = check_workspace(path, "monorepo", &ctx).expect("check");
    assert!(drift.is_empty(), "unexpected drift: {drift:?}");
}

#[test]
fn new_workspace_rejects_nonempty_directory() {
    let dir = tempdir().expect("tempdir");
    std::fs::write(dir.path().join("existing.txt"), "x").expect("write file");

    let err = new_workspace(dir.path(), "default", &context());
    assert!(err.is_err());
    let msg = err.unwrap_err().to_string();
    assert!(msg.contains("not empty"), "unexpected error: {msg}");
}

#[test]
fn new_workspace_rejects_nondirectory_path() {
    let dir = tempdir().expect("tempdir");
    let file_path = dir.path().join("not-a-dir");
    std::fs::write(&file_path, "x").expect("write file");

    let err = new_workspace(&file_path, "default", &context());
    assert!(err.is_err());
    let msg = err.unwrap_err().to_string();
    assert!(msg.contains("not a directory"), "unexpected error: {msg}");
}

#[test]
fn from_workspace_defaults_when_cargo_toml_missing() {
    let dir = tempdir().expect("tempdir");
    let ctx = SyncContext::from_workspace(dir.path()).expect("read workspace");
    assert!(ctx.project_name.is_empty());
    assert!(ctx.author.is_empty());
    assert!(ctx.license.is_empty());
    assert!(ctx.repository.is_empty());
    assert_eq!(
        ctx.edition,
        option_env!("CARGO_PKG_EDITION").unwrap_or_else(|| "2024")
    );
}
