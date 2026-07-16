use std::path::Path;
use tempfile::TempDir;
use truss_core::{
    ProtectList, SyncContext, Template, UpdateAction, UpdateOptions, update_workspace_with_template,
};

fn setup_template(template_dir: &Path) {
    std::fs::write(template_dir.join("README.md"), "# {{ project_name }}\n").expect("write readme");
    std::fs::write(
        template_dir.join("Cargo.toml"),
        "[package]\nname = \"{{ project_name }}\"\n",
    )
    .expect("write cargo");
    std::fs::write(template_dir.join("old.txt"), "old\n").expect("write old");
}

fn create_project(project: &Path, template_dir: &Path) {
    let template = Template::from_directory(template_dir).unwrap();
    let ctx = ctx();
    truss_core::sync::sync_workspace(project, &template, &ctx).unwrap();
    truss_core::update::persist_base_snapshot(project, &template, &ctx).unwrap();
}

fn ctx() -> SyncContext {
    SyncContext::new()
        .with_project_name("myproj")
        .with_author("me")
        .with_license("MIT")
        .with_repository("")
        .with_edition("2024")
}

fn options() -> UpdateOptions {
    UpdateOptions {
        dry_run: false,
        write_conflicts: false,
        protect: ProtectList::new(),
        base: None,
    }
}

#[test]
fn update_applies_template_changes_and_preserves_local_edits() {
    let template_dir = TempDir::new().unwrap();
    setup_template(template_dir.path());

    let project = TempDir::new().unwrap();
    create_project(project.path(), template_dir.path());

    // Edit locally; template unchanged.
    std::fs::write(project.path().join("README.md"), "# myproj\nlocal edit\n").unwrap();

    // Change template: README unchanged, Cargo.toml changed, old.txt removed, new.txt added.
    std::fs::write(
        template_dir.path().join("Cargo.toml"),
        "[package]\nname = \"{{ project_name }}\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::remove_file(template_dir.path().join("old.txt")).unwrap();
    std::fs::write(template_dir.path().join("new.txt"), "new\n").unwrap();

    let template = Template::from_directory(template_dir.path()).unwrap();
    let plan =
        update_workspace_with_template(project.path(), &template, &ctx(), &options()).unwrap();

    let readme_result = plan.iter().find(|r| r.path == "README.md").unwrap();
    assert_eq!(readme_result.action, UpdateAction::Unchanged);

    let cargo_result = plan.iter().find(|r| r.path == "Cargo.toml").unwrap();
    assert_eq!(cargo_result.action, UpdateAction::Applied);

    let new_result = plan.iter().find(|r| r.path == "new.txt").unwrap();
    assert_eq!(new_result.action, UpdateAction::Added);

    let old_result = plan.iter().find(|r| r.path == "old.txt").unwrap();
    assert_eq!(old_result.action, UpdateAction::Removed);

    // Verify local edit preserved and template change applied.
    let readme = std::fs::read_to_string(project.path().join("README.md")).unwrap();
    assert!(readme.contains("local edit"));
    let cargo = std::fs::read_to_string(project.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("version = \"0.1.0\""));
    assert!(project.path().join("new.txt").is_file());
    assert!(!project.path().join("old.txt").exists());
}

#[test]
fn update_reports_and_writes_conflicts() {
    let template_dir = TempDir::new().unwrap();
    setup_template(template_dir.path());

    let project = TempDir::new().unwrap();
    create_project(project.path(), template_dir.path());

    std::fs::write(project.path().join("README.md"), "# local\n").unwrap();
    std::fs::write(template_dir.path().join("README.md"), "# template\n").unwrap();

    let template = Template::from_directory(template_dir.path()).unwrap();
    let result = update_workspace_with_template(project.path(), &template, &ctx(), &options());
    assert!(result.is_err(), "expected conflict error");

    let mut conflict_options = options();
    conflict_options.write_conflicts = true;
    let plan = update_workspace_with_template(project.path(), &template, &ctx(), &conflict_options)
        .unwrap();
    let conflict = plan.iter().find(|r| r.path == "README.md").unwrap();
    assert_eq!(conflict.action, UpdateAction::Conflict);

    let readme = std::fs::read_to_string(project.path().join("README.md")).unwrap();
    assert!(readme.contains("<<<<<<< local"));
    assert!(readme.contains("======="));
    assert!(readme.contains(">>>>>>> template"));
    assert!(readme.contains("# local"));
    assert!(readme.contains("# template"));
}

#[test]
fn update_dry_run_matches_actual() {
    let template_dir = TempDir::new().unwrap();
    setup_template(template_dir.path());

    let project = TempDir::new().unwrap();
    create_project(project.path(), template_dir.path());

    std::fs::write(template_dir.path().join("new.txt"), "new\n").unwrap();

    let template = Template::from_directory(template_dir.path()).unwrap();
    let mut dry = options();
    dry.dry_run = true;
    let dry_plan = update_workspace_with_template(project.path(), &template, &ctx(), &dry).unwrap();
    assert!(!project.path().join("new.txt").exists());

    let actual_plan =
        update_workspace_with_template(project.path(), &template, &ctx(), &options()).unwrap();
    assert_eq!(dry_plan.len(), actual_plan.len());
    for (dry, actual) in dry_plan.iter().zip(actual_plan.iter()) {
        assert_eq!(dry.path, actual.path);
        assert_eq!(dry.action, actual.action);
    }
    assert!(project.path().join("new.txt").is_file());
}

#[test]
fn update_respects_protected_paths() {
    let template_dir = TempDir::new().unwrap();
    setup_template(template_dir.path());

    let project = TempDir::new().unwrap();
    create_project(project.path(), template_dir.path());

    std::fs::write(
        template_dir.path().join("Cargo.toml"),
        "[package]\nname = \"{{ project_name }}\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let template = Template::from_directory(template_dir.path()).unwrap();
    let mut protected = options();
    protected.protect.insert("Cargo.toml").unwrap();
    let plan =
        update_workspace_with_template(project.path(), &template, &ctx(), &protected).unwrap();
    let cargo = plan.iter().find(|r| r.path == "Cargo.toml").unwrap();
    assert_eq!(cargo.action, UpdateAction::SkipProtected);

    let cargo = std::fs::read_to_string(project.path().join("Cargo.toml")).unwrap();
    assert!(!cargo.contains("version"));
}
