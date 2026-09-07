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

/// A library caller renders a pack directly, without the CLI's prompt pass that
/// fills manifest defaults in. The condition selects the file by the default,
/// so the file body has to see that same default rather than an undefined
/// value.
#[test]
fn a_manifest_default_reaches_the_file_body() {
    let dir = tempdir().expect("tempdir");
    let pack = dir.path().join("pack");
    std::fs::create_dir_all(&pack).expect("mkdir pack");
    std::fs::write(pack.join("lang.txt"), "lang={{ lang }}\n").expect("write source");
    std::fs::write(
        pack.join("truss-pack.json"),
        r#"{
  "name": "defaultspack",
  "variables": [
    { "name": "lang", "type": "string", "default": "rust" }
  ],
  "files": [
    { "source": "lang.txt", "destination": "lang.txt", "condition": "lang == \"rust\"" }
  ]
}
"#,
    )
    .expect("write manifest");

    let template = truss_core::Template::from_directory(&pack).expect("from_directory");
    // The caller supplies no answer for `lang`.
    let rendered = template
        .render(&context(), &truss_core::Engine::new())
        .expect("render");

    let file = rendered
        .iter()
        .find(|f| f.path == "lang.txt")
        .expect("the default condition must select the file");
    assert_eq!(
        file.content.as_str().map_or("", |text| text).trim_end(),
        "lang=rust",
        "the body must see the default the condition selected it with"
    );
}

/// A pack may ship a binary asset — an icon, a font, a fixture. Reading every
/// source as UTF-8 text destroyed those bytes, so the rendered file no longer
/// matched what the pack author committed.
#[test]
fn a_binary_asset_survives_rendering_unchanged() {
    let dir = tempdir().expect("tempdir");
    let pack = dir.path().join("pack");
    std::fs::create_dir_all(&pack).expect("mkdir pack");

    // A one-pixel PNG: byte 0x89 alone is not valid UTF-8.
    let png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0xFF, 0xFE, 0xFD,
    ];
    std::fs::write(pack.join("icon.png"), png).expect("write asset");
    std::fs::write(
        pack.join("truss-pack.json"),
        r#"{
  "name": "assetpack",
  "files": [
    { "source": "icon.png", "destination": "icon.png", "is_template": false }
  ]
}
"#,
    )
    .expect("write manifest");

    let template = truss_core::Template::from_directory(&pack).expect("from_directory");
    let rendered = template
        .render(&context(), &truss_core::Engine::new())
        .expect("render");

    let file = rendered
        .iter()
        .find(|f| f.path == "icon.png")
        .expect("the asset must be rendered");
    assert_eq!(
        file.content.as_bytes(),
        png,
        "the asset bytes must reach the destination unchanged"
    );
}

/// A mapping that asks to be rendered has to be text. Silently replacing the
/// invalid bytes would write a corrupt file and report success.
#[test]
fn a_non_utf8_source_marked_as_a_template_is_rejected() {
    let dir = tempdir().expect("tempdir");
    let pack = dir.path().join("pack");
    std::fs::create_dir_all(&pack).expect("mkdir pack");
    std::fs::write(pack.join("blob.bin"), [0xFFu8, 0xFE, 0x00]).expect("write asset");
    std::fs::write(
        pack.join("truss-pack.json"),
        r#"{
  "name": "blobpack",
  "files": [
    { "source": "blob.bin", "destination": "blob.bin" }
  ]
}
"#,
    )
    .expect("write manifest");

    let err = truss_core::Template::from_directory(&pack)
        .expect_err("a non-UTF-8 template source must be rejected")
        .to_string();
    assert!(
        err.contains("is_template") && err.contains("blob.bin"),
        "the error must name the file and the fix: {err}"
    );
}
