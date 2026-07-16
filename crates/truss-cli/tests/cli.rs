use std::process::Command;
use tempfile::{TempDir, tempdir};

fn truss_bin() -> std::path::PathBuf {
    env!("CARGO_BIN_EXE_truss").into()
}

fn truss_cmd(config: &TempDir) -> Command {
    let mut cmd = Command::new(truss_bin());
    let system = config.path().join("no-registry.json");
    cmd.env("XDG_CONFIG_HOME", config.path())
        .env("TRUSS_SYSTEM_REGISTRY", system.as_os_str());
    cmd
}

#[test]
fn new_creates_workspace_noninteractive() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("myproj");

    let output = truss_cmd(&config)
        .args([
            "new",
            "myproj",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "default",
            "--author",
            "truss-test",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run truss new");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(path.join("Cargo.toml").is_file());
    assert!(path.join("flake.nix").is_file());

    let cargo = std::fs::read_to_string(path.join("Cargo.toml")).expect("read cargo");
    assert!(cargo.contains(r#"edition = "2024""#));
    assert!(cargo.contains(r#"resolver = "3""#));
    assert!(cargo.contains("truss-test"));
    // Guard against workspace metadata leaking into the generated project.
    let workspace_authors = env!("CARGO_PKG_AUTHORS");
    let workspace_repository = env!("CARGO_PKG_REPOSITORY");
    if !workspace_authors.is_empty() {
        assert!(
            !cargo.contains(workspace_authors),
            "workspace authors leaked into generated Cargo.toml"
        );
    }
    if !workspace_repository.is_empty() {
        assert!(
            !cargo.contains(workspace_repository),
            "workspace repository leaked into generated Cargo.toml"
        );
    }

    let flake = std::fs::read_to_string(path.join("flake.nix")).expect("read flake");
    assert!(flake.contains("myproj"));
}

#[test]
fn check_passes_after_new() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("chk");

    let new = truss_cmd(&config)
        .args([
            "new",
            "chk",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "default",
            "--author",
            "truss-test",
            "--license",
            "Apache-2.0",
            "--edition",
            "2021",
        ])
        .output()
        .expect("new");
    assert!(
        new.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&new.stderr)
    );

    let check = truss_cmd(&config)
        .args([
            "check",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "default",
        ])
        .output()
        .expect("check");
    assert!(
        check.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&check.stderr),
        String::from_utf8_lossy(&check.stdout)
    );
}

#[test]
fn sync_dry_run_and_protect() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("syncproj");

    let new = truss_cmd(&config)
        .args([
            "new",
            "syncproj",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "default",
            "--author",
            "truss-test",
        ])
        .output()
        .expect("new");
    assert!(
        new.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&new.stderr)
    );

    std::fs::write(path.join("AGENTS.md"), "keep-me").expect("edit AGENTS.md");
    std::fs::write(path.join("flake.nix"), "changed-flake").expect("edit flake.nix");

    let dry_run = truss_cmd(&config)
        .args([
            "sync",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "default",
            "--dry-run",
        ])
        .output()
        .expect("dry-run");
    assert!(
        dry_run.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    let dry_stdout = String::from_utf8_lossy(&dry_run.stdout);
    assert!(
        dry_stdout.contains("AGENTS.md"),
        "dry-run stdout: {dry_stdout}"
    );
    assert!(
        dry_stdout.contains("flake.nix"),
        "dry-run stdout: {dry_stdout}"
    );
    assert!(
        dry_stdout.contains("dry-run:"),
        "dry-run stdout: {dry_stdout}"
    );

    assert_eq!(
        std::fs::read_to_string(path.join("AGENTS.md")).expect("read AGENTS.md"),
        "keep-me"
    );
    assert_eq!(
        std::fs::read_to_string(path.join("flake.nix")).expect("read flake.nix"),
        "changed-flake"
    );

    let sync = truss_cmd(&config)
        .args([
            "sync",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "default",
            "--protect",
            "AGENTS.md",
        ])
        .output()
        .expect("sync");
    assert!(
        sync.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&sync.stderr)
    );
    let sync_stdout = String::from_utf8_lossy(&sync.stdout);
    assert!(
        sync_stdout.contains("protected skips: 1"),
        "sync stdout: {sync_stdout}"
    );

    assert_eq!(
        std::fs::read_to_string(path.join("AGENTS.md")).expect("read AGENTS.md"),
        "keep-me"
    );
    let flake = std::fs::read_to_string(path.join("flake.nix")).expect("read flake.nix");
    assert!(
        !flake.contains("changed-flake"),
        "flake.nix should have been restored: {flake}"
    );
}

#[test]
fn registry_add_list_remove() {
    let config = tempdir().expect("tempdir");
    let pack = config.path().join("pack");
    std::fs::create_dir_all(&pack).expect("mkdir pack");
    std::fs::write(pack.join("hello.md"), "# hello").expect("write hello.md");

    let add = truss_cmd(&config)
        .args([
            "registry",
            "add",
            "mypack",
            "--source",
            pack.to_str().expect("utf8 path"),
            "--kind",
            "dir",
        ])
        .output()
        .expect("registry add");
    assert!(
        add.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&add.stderr)
    );

    let list = truss_cmd(&config)
        .args(["registry", "list"])
        .output()
        .expect("registry list");
    assert!(
        list.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&list.stderr)
    );
    let list_stdout = String::from_utf8_lossy(&list.stdout);
    assert!(list_stdout.contains("mypack"), "list stdout: {list_stdout}");
    assert!(list_stdout.contains("dir"), "list stdout: {list_stdout}");

    let remove = truss_cmd(&config)
        .args(["registry", "remove", "mypack"])
        .output()
        .expect("registry remove");
    assert!(
        remove.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&remove.stderr)
    );

    let list2 = truss_cmd(&config)
        .args(["registry", "list"])
        .output()
        .expect("registry list again");
    let list2_stdout = String::from_utf8_lossy(&list2.stdout);
    assert!(
        !list2_stdout.contains("mypack"),
        "list2 stdout: {list2_stdout}"
    );
}

#[test]
fn new_monorepo_creates_multi_crate_workspace() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("monoproj");

    let new = truss_cmd(&config)
        .args([
            "new",
            "monoproj",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "monorepo",
            "--author",
            "truss-test",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run truss new monorepo");
    assert!(
        new.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&new.stderr)
    );

    let cargo = std::fs::read_to_string(path.join("Cargo.toml")).expect("read cargo");
    assert!(cargo.contains(r#""apps/app""#));
    assert!(cargo.contains(r#""libs/shared""#));
    assert!(cargo.contains(r#""tools/dev""#));

    assert!(path.join("apps/app/src/main.rs").is_file());
    assert!(path.join("libs/shared/src/lib.rs").is_file());
    assert!(path.join("tools/dev/src/main.rs").is_file());

    let app_cargo =
        std::fs::read_to_string(path.join("apps/app/Cargo.toml")).expect("read app cargo");
    assert!(app_cargo.contains(r#"shared = { path = "../../libs/shared" }"#));

    let dev_cargo =
        std::fs::read_to_string(path.join("tools/dev/Cargo.toml")).expect("read dev cargo");
    assert!(dev_cargo.contains(r#"shared = { path = "../../libs/shared" }"#));
}

#[test]
fn member_add_and_list() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("ws");

    let new = truss_cmd(&config)
        .args([
            "new",
            "ws",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "default",
            "--author",
            "truss-test",
        ])
        .output()
        .expect("new");
    assert!(
        new.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&new.stderr)
    );

    let add = truss_cmd(&config)
        .args([
            "member",
            "add",
            "mylib",
            "--kind",
            "lib",
            "--path",
            path.to_str().expect("utf8 path"),
        ])
        .output()
        .expect("member add");
    assert!(
        add.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&add.stderr)
    );

    assert!(path.join("crates/mylib/Cargo.toml").is_file());
    assert!(path.join("crates/mylib/src/lib.rs").is_file());

    let list = truss_cmd(&config)
        .args([
            "member",
            "list",
            "--path",
            path.to_str().expect("utf8 path"),
        ])
        .output()
        .expect("member list");
    assert!(
        list.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&list.stderr)
    );
    let list_stdout = String::from_utf8_lossy(&list.stdout);
    assert!(list_stdout.contains("crates/app"), "list: {list_stdout}");
    assert!(list_stdout.contains("crates/mylib"), "list: {list_stdout}");
}

#[test]
fn member_add_bin_and_remove() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("ws");

    let new = truss_cmd(&config)
        .args([
            "new",
            "ws",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "default",
            "--author",
            "truss-test",
        ])
        .output()
        .expect("new");
    assert!(
        new.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&new.stderr)
    );

    let add = truss_cmd(&config)
        .args([
            "member",
            "add",
            "mybin",
            "--kind",
            "bin",
            "--path",
            path.to_str().expect("utf8 path"),
        ])
        .output()
        .expect("member add bin");
    assert!(
        add.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&add.stderr)
    );
    assert!(path.join("crates/mybin/src/main.rs").is_file());

    let remove = truss_cmd(&config)
        .args([
            "member",
            "remove",
            "mybin",
            "--path",
            path.to_str().expect("utf8 path"),
            "--delete",
        ])
        .output()
        .expect("member remove");
    assert!(
        remove.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&remove.stderr)
    );

    let cargo = std::fs::read_to_string(path.join("Cargo.toml")).expect("read cargo");
    assert!(!cargo.contains("crates/mybin"));
    assert!(!path.join("crates/mybin").exists());
}

#[test]
fn member_add_fails_without_workspace() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("solo");
    std::fs::create_dir_all(&path).expect("mkdir");
    std::fs::write(path.join("Cargo.toml"), "[package]\nname = \"solo\"\n").expect("write cargo");

    let add = truss_cmd(&config)
        .args([
            "member",
            "add",
            "mylib",
            "--kind",
            "lib",
            "--path",
            path.to_str().expect("utf8 path"),
        ])
        .output()
        .expect("member add");
    assert!(
        !add.status.success(),
        "expected failure, stderr={}",
        String::from_utf8_lossy(&add.stderr)
    );
}

#[test]
fn member_custom_path() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("ws");

    let new = truss_cmd(&config)
        .args([
            "new",
            "ws",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "default",
            "--author",
            "truss-test",
        ])
        .output()
        .expect("new");
    assert!(
        new.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&new.stderr)
    );

    let add = truss_cmd(&config)
        .args([
            "member",
            "add",
            "shared",
            "--kind",
            "lib",
            "--member-path",
            "libs/shared",
            "--path",
            path.to_str().expect("utf8 path"),
        ])
        .output()
        .expect("member add custom");
    assert!(
        add.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&add.stderr)
    );

    assert!(path.join("libs/shared/Cargo.toml").is_file());

    let remove = truss_cmd(&config)
        .args([
            "member",
            "remove",
            "libs/shared",
            "--path",
            path.to_str().expect("utf8 path"),
            "--delete",
        ])
        .output()
        .expect("member remove custom");
    assert!(
        remove.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&remove.stderr)
    );
}
