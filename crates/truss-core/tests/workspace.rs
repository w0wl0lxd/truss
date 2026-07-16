use std::path::Path;
use tempfile::tempdir;
use truss_core::{
    MemberKind, SyncContext, add_workspace_member, list_workspace_members, new_workspace,
    remove_workspace_member,
};

fn context() -> SyncContext {
    SyncContext::new()
        .with_project_name("demo")
        .with_author("tester")
        .with_license(env!("CARGO_PKG_LICENSE"))
        .with_repository("https://example.com/demo")
        .with_edition(option_env!("CARGO_PKG_EDITION").unwrap_or_else(|| "2024"))
}

fn cargo_toml_members(path: &Path) -> Vec<String> {
    let cargo = std::fs::read_to_string(path.join("Cargo.toml")).expect("read Cargo.toml");
    let document = cargo
        .parse::<toml_edit::DocumentMut>()
        .expect("parse Cargo.toml");
    let Some(array) = document
        .get("workspace")
        .and_then(toml_edit::Item::as_table)
        .and_then(|t| t.get("members"))
        .and_then(toml_edit::Item::as_array)
    else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

#[test]
fn add_library_member() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");

    add_workspace_member(path, "mylib", MemberKind::Lib, None, &context()).expect("add mylib");

    let members = cargo_toml_members(path);
    assert!(members.contains(&"crates/app".to_string()));
    assert!(members.contains(&"crates/mylib".to_string()));
    assert_eq!(members.iter().filter(|m| *m == "crates/mylib").count(), 1);

    assert!(path.join("crates/mylib/Cargo.toml").is_file());
    assert!(path.join("crates/mylib/src/lib.rs").is_file());

    let cargo =
        std::fs::read_to_string(path.join("crates/mylib/Cargo.toml")).expect("read member cargo");
    assert!(cargo.contains("name = \"mylib\""));
}

#[test]
fn add_binary_member() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");

    add_workspace_member(path, "mybin", MemberKind::Bin, None, &context()).expect("add mybin");

    let members = cargo_toml_members(path);
    assert!(members.contains(&"crates/mybin".to_string()));
    assert!(path.join("crates/mybin/src/main.rs").is_file());

    let main_rs =
        std::fs::read_to_string(path.join("crates/mybin/src/main.rs")).expect("read main.rs");
    assert!(main_rs.contains("mybin"));
}

#[test]
fn member_add_is_idempotent() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");

    add_workspace_member(path, "mylib", MemberKind::Lib, None, &context()).expect("first add");
    let first_cargo =
        std::fs::read_to_string(path.join("crates/mylib/Cargo.toml")).expect("read cargo");

    add_workspace_member(path, "mylib", MemberKind::Lib, None, &context()).expect("second add");

    let members = cargo_toml_members(path);
    assert_eq!(members.iter().filter(|m| *m == "crates/mylib").count(), 1);
    let second_cargo =
        std::fs::read_to_string(path.join("crates/mylib/Cargo.toml")).expect("read cargo again");
    assert_eq!(first_cargo, second_cargo);
}

#[test]
fn add_existing_directory_does_not_overwrite() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");

    let existing = path.join("crates/existing");
    std::fs::create_dir_all(existing.join("src")).expect("mkdir");
    std::fs::write(existing.join("Cargo.toml"), "# original").expect("write cargo");
    std::fs::write(existing.join("src/lib.rs"), "// original").expect("write lib");

    add_workspace_member(path, "existing", MemberKind::Lib, None, &context())
        .expect("add existing");

    let members = cargo_toml_members(path);
    assert!(members.contains(&"crates/existing".to_string()));
    assert_eq!(
        std::fs::read_to_string(existing.join("Cargo.toml")).expect("read cargo"),
        "# original"
    );
    assert_eq!(
        std::fs::read_to_string(existing.join("src/lib.rs")).expect("read lib"),
        "// original"
    );
}

#[test]
fn add_scaffolds_orphan_entry() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");

    add_workspace_member(path, "orphan", MemberKind::Lib, None, &context()).expect("add orphan");
    std::fs::remove_dir_all(path.join("crates/orphan")).expect("remove orphan dir");

    add_workspace_member(path, "orphan", MemberKind::Lib, None, &context()).expect("re-add orphan");

    assert!(path.join("crates/orphan/Cargo.toml").is_file());
    assert!(path.join("crates/orphan/src/lib.rs").is_file());
}

#[test]
fn add_fails_without_workspace() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    std::fs::write(path.join("Cargo.toml"), "[package]\nname = \"solo\"\n").expect("write cargo");

    let result = add_workspace_member(path, "mylib", MemberKind::Lib, None, &context());
    assert!(result.is_err(), "expected error for missing [workspace]");
}

#[test]
fn list_workspace_members_returns_paths() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");
    add_workspace_member(path, "mylib", MemberKind::Lib, None, &context()).expect("add");

    let members = list_workspace_members(path).expect("list");
    assert_eq!(members, vec!["crates/app", "crates/mylib"]);
}

#[test]
fn list_workspace_members_fails_without_workspace() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    std::fs::write(path.join("Cargo.toml"), "[package]\nname = \"solo\"\n").expect("write cargo");

    let result = list_workspace_members(path);
    assert!(result.is_err());
}

#[test]
fn remove_member_preserves_directory() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");
    add_workspace_member(path, "toremove", MemberKind::Lib, None, &context()).expect("add");

    remove_workspace_member(path, "toremove", false).expect("remove");

    let members = cargo_toml_members(path);
    assert!(!members.contains(&"crates/toremove".to_string()));
    assert!(path.join("crates/toremove").is_dir());
}

#[test]
fn remove_member_with_delete() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");
    add_workspace_member(path, "toremove", MemberKind::Lib, None, &context()).expect("add");

    remove_workspace_member(path, "toremove", true).expect("remove --delete");

    let members = cargo_toml_members(path);
    assert!(!members.contains(&"crates/toremove".to_string()));
    assert!(!path.join("crates/toremove").exists());
}

#[test]
fn custom_member_path() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");

    add_workspace_member(
        path,
        "shared",
        MemberKind::Lib,
        Some("libs/shared"),
        &context(),
    )
    .expect("add custom path");

    let members = cargo_toml_members(path);
    assert!(members.contains(&"libs/shared".to_string()));
    assert!(path.join("libs/shared/Cargo.toml").is_file());
}

#[test]
fn remove_custom_member_path() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");
    add_workspace_member(
        path,
        "shared",
        MemberKind::Lib,
        Some("libs/shared"),
        &context(),
    )
    .expect("add");

    remove_workspace_member(path, "libs/shared", true).expect("remove custom");

    let members = cargo_toml_members(path);
    assert!(!members.contains(&"libs/shared".to_string()));
    assert!(!path.join("libs/shared").exists());
}

#[test]
fn add_rejects_member_path_escape() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");

    let result = add_workspace_member(
        path,
        "escape",
        MemberKind::Lib,
        Some("../escape"),
        &context(),
    );
    assert!(result.is_err(), "expected path traversal to be rejected");
}

#[test]
fn add_rejects_existing_file_at_member_path() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");

    let file_path = path.join("crates/notdir");
    std::fs::create_dir_all(file_path.parent().unwrap()).expect("mkdir crates");
    std::fs::write(&file_path, "not a directory").expect("write file");

    let result = add_workspace_member(path, "notdir", MemberKind::Lib, None, &context());
    assert!(
        result.is_err(),
        "expected error when member path is an existing file"
    );
}

#[test]
fn formatting_and_comments_preserved() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    let cargo_toml = r#"[workspace]
resolver = "3"
# keep this comment
members = [
    "crates/app", # app crate
]
"#;
    std::fs::create_dir_all(path).expect("mkdir");
    std::fs::write(path.join("Cargo.toml"), cargo_toml).expect("write cargo");

    add_workspace_member(path, "mylib", MemberKind::Lib, None, &context()).expect("add");

    let updated = std::fs::read_to_string(path.join("Cargo.toml")).expect("read cargo");
    assert!(updated.contains("# keep this comment"));
    assert!(updated.contains("# app crate"));
    assert!(updated.contains("crates/app"));
    assert!(updated.contains("crates/mylib"));

    let members = cargo_toml_members(path);
    assert_eq!(members, vec!["crates/app", "crates/mylib"]);
}

#[test]
fn add_rejects_invalid_member_name() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");

    assert!(add_workspace_member(path, "1bad", MemberKind::Lib, None, &context()).is_err());
    assert!(add_workspace_member(path, "my/lib", MemberKind::Lib, None, &context()).is_err());
    assert!(add_workspace_member(path, "my lib", MemberKind::Lib, None, &context()).is_err());
    assert!(add_workspace_member(path, "", MemberKind::Lib, None, &context()).is_err());
    assert!(add_workspace_member(path, "con", MemberKind::Lib, None, &context()).is_err());
}

#[test]
fn add_normalizes_backslash_member_path() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");

    add_workspace_member(
        path,
        "shared",
        MemberKind::Lib,
        Some("libs\\shared"),
        &context(),
    )
    .expect("add with backslash");

    let members = cargo_toml_members(path);
    assert!(members.contains(&"libs/shared".to_string()));
    assert!(path.join("libs/shared/Cargo.toml").is_file());
}

#[test]
fn add_existing_file_does_not_modify_members() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path();
    new_workspace(path, "default", &context()).expect("new_workspace");

    let file_path = path.join("crates/notdir");
    std::fs::create_dir_all(file_path.parent().unwrap()).expect("mkdir crates");
    std::fs::write(&file_path, "not a directory").expect("write file");

    let result = add_workspace_member(path, "notdir", MemberKind::Lib, None, &context());
    assert!(result.is_err());

    let members = cargo_toml_members(path);
    assert!(!members.contains(&"crates/notdir".to_string()));
}
