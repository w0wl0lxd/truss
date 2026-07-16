use tempfile::tempdir;
use truss_core::{
    Kind, PlanAction, ProtectList, Registry, RegistryEntry, SyncContext, SyncOptions,
    new_workspace, plan_workspace, sync_workspace_with,
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
            file_mode: None,
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
    let plan = plan_workspace(path, "default", &ctx(), &protect).expect("plan");
    assert!(
        plan.iter()
            .any(|p| p.path == "AGENTS.md" && p.action == PlanAction::SkipProtected)
    );
}
