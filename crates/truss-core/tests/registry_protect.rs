use tempfile::tempdir;
use truss_core::{
    ExcludeList, Kind, PlanAction, ProtectList, Registry, RegistryEntry, SyncContext, SyncOptions,
    check_workspace, new_workspace, plan_workspace, sync_workspace_with,
};

fn ctx() -> SyncContext {
    SyncContext::new()
        .with_project_name("demo")
        .with_author("tester")
        .with_license(env!("CARGO_PKG_LICENSE"))
}

#[test]
fn registry_add_and_remove_in_memory() {
    let pack = tempdir().expect("pack");
    std::fs::write(pack.path().join("AGENTS.md"), "# team\n").expect("write");

    let mut reg = Registry::new();
    reg.add(
        RegistryEntry {
            name: "team".into(),
            source: pack.path().display().to_string(),
            kind: Kind::Dir,
            targets: vec![],
            pointer: None,
            subfolder: None,
            file_mode: None,
            auth_env: None,
            ssh_key: None,
        },
        false,
    )
    .expect("add");
    assert!(reg.get("team").is_some());
    reg.remove("team").expect("remove");
    assert!(reg.get("team").is_none());
}

#[test]
fn registry_add_missing_source_fails() {
    let mut reg = Registry::new();
    let err = reg.add(
        RegistryEntry {
            name: "gone".into(),
            source: "/no/such/truss/template/path".into(),
            kind: Kind::Dir,
            ..RegistryEntry::default()
        },
        false,
    );
    assert!(err.is_err());
}

#[test]
fn dry_run_does_not_write() {
    let dir = tempdir().expect("proj");
    let path = dir.path();
    new_workspace(path, "default", &ctx()).expect("new");
    std::fs::write(path.join("AGENTS.md"), "local-edit").expect("edit");

    let options = SyncOptions {
        dry_run: true,
        protect: ProtectList::new(),
    };
    let plan = sync_workspace_with(path, "default", &ctx(), &options).expect("plan");
    assert!(
        plan.iter()
            .any(|p| p.path == "AGENTS.md" && p.action == PlanAction::WouldWrite)
    );
    let after = std::fs::read_to_string(path.join("AGENTS.md")).expect("read");
    assert_eq!(after, "local-edit");
}

#[test]
fn protect_skips_file_on_sync() {
    let dir = tempdir().expect("proj");
    let path = dir.path();
    new_workspace(path, "default", &ctx()).expect("new");
    std::fs::write(path.join("AGENTS.md"), "keep-me").expect("edit");

    let mut protect = ProtectList::new();
    protect.insert("AGENTS.md").expect("insert");
    let options = SyncOptions {
        dry_run: false,
        protect,
    };
    let plan = sync_workspace_with(path, "default", &ctx(), &options).expect("sync");
    assert!(
        plan.iter()
            .any(|p| p.path == "AGENTS.md" && p.action == PlanAction::SkipProtected)
    );
    let after = std::fs::read_to_string(path.join("AGENTS.md")).expect("read");
    assert_eq!(after, "keep-me");
}

#[test]
fn protect_file_and_plan() {
    let dir = tempdir().expect("proj");
    let path = dir.path();
    new_workspace(path, "default", &ctx()).expect("new");
    let truss = path.join(".truss");
    std::fs::create_dir_all(&truss).expect("mkdir");
    std::fs::write(truss.join("protect"), "AGENTS.md\n").expect("protect");
    std::fs::write(path.join("AGENTS.md"), "from-file").expect("edit");

    let protect = ProtectList::load(path, &[]).expect("load");
    let options = SyncOptions {
        protect,
        dry_run: false,
    };
    let exclude = ExcludeList::empty();
    let plan = plan_workspace(path, "default", &ctx(), &options, &exclude).expect("plan");
    assert!(
        plan.iter()
            .any(|p| p.path == "AGENTS.md" && p.action == PlanAction::SkipProtected)
    );
}

#[test]
fn registry_file_entry_writes_all_targets() {
    let tmp = tempdir().expect("tmp");
    let source = tmp.path().join("LICENSE");
    std::fs::write(&source, "MIT License").expect("write source");

    let mut reg = Registry::new();
    reg.add(
        RegistryEntry {
            name: "mit".into(),
            source: source.display().to_string(),
            kind: Kind::File,
            targets: vec!["LICENSE".into(), "COPYING".into()],
            pointer: None,
            subfolder: None,
            file_mode: None,
            auth_env: None,
            ssh_key: None,
        },
        false,
    )
    .expect("add");

    let template = reg
        .get("mit")
        .expect("entry")
        .to_template()
        .expect("template");
    assert_eq!(template.files.len(), 2);
    let paths: Vec<&str> = template.files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"LICENSE"));
    assert!(paths.contains(&"COPYING"));
    for file in &template.files {
        assert_eq!(file.content, "MIT License");
    }
}

#[test]
fn registry_file_entry_parses_octal_mode() {
    let tmp = tempdir().expect("tmp");
    let source = tmp.path().join("script");
    std::fs::write(&source, "#!/bin/sh").expect("write source");

    fn make_entry(source: &str, mode: &str) -> RegistryEntry {
        RegistryEntry {
            name: "script".into(),
            source: source.into(),
            kind: Kind::File,
            targets: vec!["run.sh".into()],
            pointer: None,
            subfolder: None,
            file_mode: Some(mode.into()),
            auth_env: None,
            ssh_key: None,
        }
    }

    let source_str = source.display().to_string();

    let template = make_entry(&source_str, "755")
        .to_template()
        .expect("template");
    assert_eq!(template.files.len(), 1);
    assert_eq!(template.files[0].mode, Some(0o755));

    // With explicit 0o prefix also works.
    let template2 = make_entry(&source_str, "0o644")
        .to_template()
        .expect("template");
    assert_eq!(template2.files[0].mode, Some(0o644));

    // Special permission bits are rejected.
    assert!(make_entry(&source_str, "4755").to_template().is_err());
}

#[test]
fn plan_and_check_refuse_to_follow_symlinks() {
    let dir = tempdir().expect("proj");
    let path = dir.path();
    new_workspace(path, "default", &ctx()).expect("new");

    let target = dir.path().join("real_AGENTS.md");
    std::fs::write(&target, "real-content").expect("write target");
    std::fs::remove_file(path.join("AGENTS.md")).expect("remove original");
    std::os::unix::fs::symlink(&target, path.join("AGENTS.md")).expect("symlink");

    let options = SyncOptions {
        protect: ProtectList::new(),
        dry_run: false,
    };
    let exclude = ExcludeList::empty();
    assert!(plan_workspace(path, "default", &ctx(), &options, &exclude).is_err());
    assert!(check_workspace(path, "default", &ctx()).is_err());
}

#[test]
fn plan_and_check_refuse_symlinked_parent() {
    let dir = tempdir().expect("proj");
    let path = dir.path();
    new_workspace(path, "default", &ctx()).expect("new");

    let real_dir = dir.path().join("real_crates");
    std::fs::rename(path.join("crates"), &real_dir).expect("rename");
    std::os::unix::fs::symlink(&real_dir, path.join("crates")).expect("symlink");

    let options = SyncOptions {
        protect: ProtectList::new(),
        dry_run: false,
    };
    let exclude = ExcludeList::empty();
    assert!(plan_workspace(path, "default", &ctx(), &options, &exclude).is_err());
    assert!(check_workspace(path, "default", &ctx()).is_err());
}

#[test]
fn new_workspace_allows_symlinked_root() {
    let dir = tempdir().expect("proj");
    let real = dir.path().join("real");
    std::fs::create_dir(&real).expect("mkdir real");
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).expect("symlink");

    new_workspace(&link, "default", &ctx()).expect("new into symlinked root");
}
