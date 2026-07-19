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
fn new_rejects_nonempty_directory() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("occupied");
    std::fs::create_dir_all(&path).expect("mkdir");
    std::fs::write(path.join("existing.txt"), "x").expect("write file");

    let new = truss_cmd(&config)
        .args([
            "new",
            "occupied",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "default",
            "--author",
            "truss-test",
        ])
        .output()
        .expect("run truss new");

    assert!(!new.status.success());
    let stderr = String::from_utf8_lossy(&new.stderr);
    assert!(stderr.contains("not empty"), "stderr={stderr}");
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

#[test]
fn new_uses_prompt_variables_from_define() {
    let config = tempdir().expect("tempdir");
    let template_dir = config.path().join("custom-template");
    std::fs::create_dir(&template_dir).expect("mkdir template");

    std::fs::write(
        template_dir.join("truss.toml"),
        r#"
[prompts]
description = { label = "Project description", kind = "text", default = "A project" }
include_cli = { label = "Include CLI", kind = "bool", default = "true" }
framework = { label = "Web framework", kind = "choice", choices = ["axum", "actix"], default = "axum", condition = { prompt = "include_cli", values = ["true"] } }
"#,
    )
    .expect("write truss.toml");
    std::fs::create_dir(template_dir.join("src")).expect("mkdir src");
    std::fs::write(
        template_dir.join("Cargo.toml"),
        r#"
[package]
name = "{{ project_name }}"
description = "{{ description }}"
edition = "{{ edition }}"

[features]
{{ framework }} = []
"#,
    )
    .expect("write Cargo.toml");
    std::fs::write(template_dir.join("src/main.rs"), "fn main() {}\n").expect("write main");

    let registry_path = config.path().join("registry.json");
    let registry = serde_json::json!({
        "entries": {
            "custom": {
                "name": "custom",
                "source": template_dir,
                "kind": "dir"
            }
        }
    });
    std::fs::write(
        &registry_path,
        serde_json::to_string_pretty(&registry).expect("json"),
    )
    .expect("write registry");

    let path = config.path().join("myproj");
    let output = Command::new(truss_bin())
        .env("XDG_CONFIG_HOME", config.path())
        .env("TRUSS_SYSTEM_REGISTRY", &registry_path)
        .env("NO_COLOR", "1")
        .args([
            "new",
            "myproj",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "custom",
            "--author",
            "truss-test",
            "--license",
            "MIT",
            "--edition",
            "2024",
            "--define",
            "description=Hello world",
            "--define",
            "include_cli=true",
            "--define",
            "framework=actix",
        ])
        .output()
        .expect("run truss new");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let cargo = std::fs::read_to_string(path.join("Cargo.toml")).expect("read cargo");
    assert!(cargo.contains(r#"description = "Hello world""#));
    assert!(cargo.contains(r#"actix = []"#));

    let prompts = std::fs::read_to_string(path.join(".truss/prompts.toml")).expect("read prompts");
    assert!(prompts.contains("description = \"Hello world\""));
    assert!(prompts.contains("framework = \"actix\""));
}

#[test]
fn update_applies_template_changes() {
    let config = tempdir().expect("tempdir");
    let template_dir = config.path().join("custom-template");
    std::fs::create_dir(&template_dir).expect("mkdir template");
    std::fs::write(template_dir.join("README.md"), "# {{ project_name }}\n").expect("write readme");
    std::fs::write(
        template_dir.join("Cargo.toml"),
        "[package]\nname = \"{{ project_name }}\"\n",
    )
    .expect("write cargo");

    let registry_path = config.path().join("registry.json");
    let registry = serde_json::json!({
        "entries": {
            "custom": {
                "name": "custom",
                "source": template_dir,
                "kind": "dir"
            }
        }
    });
    std::fs::write(
        &registry_path,
        serde_json::to_string_pretty(&registry).expect("json"),
    )
    .expect("write registry");

    let path = config.path().join("myproj");
    let new = Command::new(truss_bin())
        .env("XDG_CONFIG_HOME", config.path())
        .env("TRUSS_SYSTEM_REGISTRY", &registry_path)
        .env("NO_COLOR", "1")
        .args([
            "new",
            "myproj",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "custom",
            "--author",
            "truss-test",
            "--license",
            "MIT",
            "--edition",
            "2024",
        ])
        .output()
        .expect("truss new");
    assert!(
        new.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&new.stderr)
    );

    // Update template README and add a new file.
    std::fs::write(
        template_dir.join("README.md"),
        "# {{ project_name }}\nUpdated\n",
    )
    .expect("update readme");
    std::fs::write(template_dir.join("new.txt"), "new\n").expect("new file");

    let dry_run = Command::new(truss_bin())
        .env("XDG_CONFIG_HOME", config.path())
        .env("TRUSS_SYSTEM_REGISTRY", &registry_path)
        .env("NO_COLOR", "1")
        .args([
            "update",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "custom",
            "--dry-run",
        ])
        .output()
        .expect("truss update dry-run");
    assert!(
        dry_run.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert!(!path.join("new.txt").exists());
    let dry_stdout = String::from_utf8_lossy(&dry_run.stdout);
    assert!(
        dry_stdout.contains("applied\tREADME.md"),
        "dry stdout: {dry_stdout}"
    );
    assert!(
        dry_stdout.contains("added\tnew.txt"),
        "dry stdout: {dry_stdout}"
    );

    let update = Command::new(truss_bin())
        .env("XDG_CONFIG_HOME", config.path())
        .env("TRUSS_SYSTEM_REGISTRY", &registry_path)
        .env("NO_COLOR", "1")
        .args([
            "update",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "custom",
        ])
        .output()
        .expect("truss update");
    assert!(
        update.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&update.stderr)
    );

    let readme = std::fs::read_to_string(path.join("README.md")).expect("read readme");
    assert!(readme.contains("Updated"));
    assert!(path.join("new.txt").is_file());
    let base = std::fs::read_to_string(path.join(".truss/base/README.md")).expect("read base");
    assert!(base.contains("Updated"));
}

#[test]
fn new_dry_run_lists_files_and_writes_nothing() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("myapp");
    let new = Command::new(truss_bin())
        .env("XDG_CONFIG_HOME", config.path())
        .env(
            "TRUSS_SYSTEM_REGISTRY",
            config.path().join("no-registry.json"),
        )
        .env("NO_COLOR", "1")
        .args([
            "new",
            "myapp",
            "--path",
            path.to_str().expect("utf8"),
            "--dry-run",
        ])
        .output()
        .expect("truss new");
    assert!(
        new.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&new.stderr)
    );
    let stdout = String::from_utf8_lossy(&new.stdout);
    assert!(stdout.contains("Cargo.toml"), "stdout={stdout}");
    assert!(stdout.contains("dry-run"), "stdout={stdout}");
    assert!(
        !path.exists(),
        "dry-run should not create the project directory"
    );
}

#[test]
fn define_lists_template_variables() {
    let config = tempdir().expect("tempdir");
    let define = Command::new(truss_bin())
        .env("XDG_CONFIG_HOME", config.path())
        .env(
            "TRUSS_SYSTEM_REGISTRY",
            config.path().join("no-registry.json"),
        )
        .env("NO_COLOR", "1")
        .args(["define", "--template", "default"])
        .output()
        .expect("truss define");
    assert!(
        define.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&define.stderr)
    );
    let stdout = String::from_utf8_lossy(&define.stdout);
    assert!(stdout.contains("project_name"), "stdout={stdout}");
    assert!(stdout.contains("author"), "stdout={stdout}");
    assert!(stdout.contains("edition"), "stdout={stdout}");
    assert!(stdout.contains("repository"), "stdout={stdout}");
}

#[test]
fn genignore_excludes_files_and_directories() {
    let config = tempdir().expect("tempdir");
    let template_dir = config.path().join("custom-template");
    std::fs::create_dir(&template_dir).expect("mkdir template");

    std::fs::write(template_dir.join("truss.toml"), "[prompts]\n").expect("write truss.toml");
    std::fs::write(template_dir.join(".genignore"), "*.log\ndata/\n").expect("write genignore");
    std::fs::write(
        template_dir.join("Cargo.toml"),
        "[package]\nname = \"{{ project_name }}\"\n",
    )
    .expect("write cargo");
    std::fs::create_dir(template_dir.join("src")).expect("mkdir src");
    std::fs::write(template_dir.join("src/main.rs"), "fn main() {}\n").expect("write main");
    std::fs::write(template_dir.join("debug.log"), "ignored\n").expect("write log");
    std::fs::create_dir(template_dir.join("data")).expect("mkdir data");
    std::fs::write(template_dir.join("data/secret.txt"), "ignored\n").expect("write data");

    let registry_path = config.path().join("registry.json");
    let registry = serde_json::json!({
        "entries": {
            "custom": {
                "name": "custom",
                "source": template_dir,
                "kind": "dir"
            }
        }
    });
    std::fs::write(
        &registry_path,
        serde_json::to_string_pretty(&registry).expect("json"),
    )
    .expect("write registry");

    let path = config.path().join("myproj");
    let output = Command::new(truss_bin())
        .env("XDG_CONFIG_HOME", config.path())
        .env("TRUSS_SYSTEM_REGISTRY", &registry_path)
        .env("NO_COLOR", "1")
        .args([
            "new",
            "myproj",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "custom",
            "--author",
            "truss-test",
        ])
        .output()
        .expect("run truss new");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(path.join("Cargo.toml").is_file());
    assert!(path.join("src/main.rs").is_file());
    assert!(!path.join("debug.log").exists());
    assert!(!path.join("data").exists());
    assert!(!path.join(".genignore").exists());
}

#[test]
fn project_exclude_unexcludes_pack_patterns() {
    let config = tempdir().expect("tempdir");
    let template_dir = config.path().join("custom-template");
    std::fs::create_dir(&template_dir).expect("mkdir template");

    std::fs::write(template_dir.join("truss.toml"), "[prompts]\n").expect("write truss.toml");
    std::fs::write(template_dir.join(".genignore"), "*.tmp\n").expect("write genignore");
    std::fs::write(
        template_dir.join("Cargo.toml"),
        "[package]\nname = \"{{ project_name }}\"\n",
    )
    .expect("write cargo");
    std::fs::write(template_dir.join("keep.tmp"), "kept\n").expect("write tmp");

    let registry_path = config.path().join("registry.json");
    let registry = serde_json::json!({
        "entries": {
            "custom": {
                "name": "custom",
                "source": template_dir,
                "kind": "dir"
            }
        }
    });
    std::fs::write(
        &registry_path,
        serde_json::to_string_pretty(&registry).expect("json"),
    )
    .expect("write registry");

    let path = config.path().join("myproj");
    let new = Command::new(truss_bin())
        .env("XDG_CONFIG_HOME", config.path())
        .env("TRUSS_SYSTEM_REGISTRY", &registry_path)
        .env("NO_COLOR", "1")
        .args([
            "new",
            "myproj",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "custom",
            "--author",
            "truss-test",
        ])
        .output()
        .expect("run truss new");
    assert!(
        new.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&new.stderr)
    );

    // Un-exclude a specific .tmp file from the project side.
    std::fs::create_dir_all(path.join(".truss")).expect("mkdir .truss");
    std::fs::write(path.join(".truss/exclude"), "!keep.tmp\n").expect("write exclude");

    let sync = Command::new(truss_bin())
        .env("XDG_CONFIG_HOME", config.path())
        .env("TRUSS_SYSTEM_REGISTRY", &registry_path)
        .env("NO_COLOR", "1")
        .args([
            "sync",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "custom",
        ])
        .output()
        .expect("run truss sync");
    assert!(
        sync.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&sync.stderr)
    );

    assert!(path.join("keep.tmp").is_file());
}

#[test]
fn extract_creates_pack_and_replaces_project_values() {
    let config = tempdir().expect("tempdir");
    let source = config.path().join("source");
    std::fs::create_dir(&source).expect("mkdir source");
    std::fs::write(
        source.join("Cargo.toml"),
        "[package]\nname = \"myapp\"\nversion = \"0.1.0\"\nauthors = [\"Alice\"]\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    std::fs::create_dir(source.join("src")).expect("mkdir src");
    std::fs::write(
        source.join("src/main.rs"),
        "fn main() { println!(\"myapp\"); }",
    )
    .expect("write main");

    let pack = config.path().join("pack");
    let extract = Command::new(truss_bin())
        .env("XDG_CONFIG_HOME", config.path())
        .env(
            "TRUSS_SYSTEM_REGISTRY",
            config.path().join("no-registry.json"),
        )
        .env("NO_COLOR", "1")
        .args([
            "extract",
            "--source",
            source.to_str().expect("utf8"),
            "--pack",
            pack.to_str().expect("utf8"),
            "--force",
        ])
        .output()
        .expect("truss extract");
    assert!(
        extract.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&extract.stderr)
    );

    let cargo = std::fs::read_to_string(pack.join("Cargo.toml")).expect("read pack cargo");
    assert!(cargo.contains("{{ project_name }}"));
    assert!(cargo.contains("{{ author }}"));
    assert!(cargo.contains("{{ edition }}"));

    let main = std::fs::read_to_string(pack.join("src/main.rs")).expect("read main");
    assert!(main.contains("{{ project_name }}"));
    assert!(!main.contains("myapp"));
}
