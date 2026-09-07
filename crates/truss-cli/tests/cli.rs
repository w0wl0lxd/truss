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
fn new_dry_run_lists_hooks_and_leaves_directory_empty() {
    let config = tempdir().expect("tempdir");
    let template_dir = config.path().join("custom-template");
    std::fs::create_dir(&template_dir).expect("mkdir template");

    std::fs::write(
        template_dir.join("truss.toml"),
        r#"
[[hooks]]
phase = "pre"
command = "echo"
args = ["hello from pre"]
commands = ["new"]
"#,
    )
    .expect("write truss.toml");
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
            "--dry-run",
        ])
        .output()
        .expect("run truss new");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hook pre:"), "stdout={stdout}");
    assert!(stdout.contains("hello from pre"), "stdout={stdout}");
    assert!(
        !path.exists(),
        "dry-run should not create the project directory"
    );
}

#[test]
fn new_runs_post_hook_and_command_restriction_filters_hooks() {
    let config = tempdir().expect("tempdir");
    let template_dir = config.path().join("custom-template");
    std::fs::create_dir(&template_dir).expect("mkdir template");

    std::fs::write(
        template_dir.join("truss.toml"),
        r#"
[[hooks]]
phase = "post"
command = "touch"
args = ["marker"]
commands = ["new"]

[[hooks]]
phase = "post"
command = "touch"
args = ["sync-marker"]
commands = ["sync"]
"#,
    )
    .expect("write truss.toml");
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
    assert!(path.join("marker").is_file());
    assert!(!path.join("sync-marker").exists());
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

#[test]
fn new_with_type_binary_scaffolds_correctly() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("mybin");

    let output = truss_cmd(&config)
        .args([
            "new",
            "mybin",
            "--path",
            path.to_str().expect("utf8 path"),
            "--type",
            "binary",
            "--author",
            "truss-test",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run truss new --type binary");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(path.join("Cargo.toml").is_file());
    assert!(path.join("crates/app/src/main.rs").is_file());

    let cargo = std::fs::read_to_string(path.join("Cargo.toml")).expect("read cargo");
    assert!(cargo.contains(r#"edition = "2024""#));
    assert!(cargo.contains("truss-test"));

    // Check that preset record was saved
    let preset_record =
        std::fs::read_to_string(path.join(".truss/preset.toml")).expect("read preset record");
    assert!(preset_record.contains("preset = \"binary\""));
}

#[test]
fn new_with_type_workspace_uses_monorepo() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("myws");

    let output = truss_cmd(&config)
        .args([
            "new",
            "myws",
            "--path",
            path.to_str().expect("utf8 path"),
            "--type",
            "workspace",
            "--author",
            "truss-test",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run truss new --type workspace");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let cargo = std::fs::read_to_string(path.join("Cargo.toml")).expect("read cargo");
    assert!(cargo.contains(r#""apps/app""#));
    assert!(cargo.contains(r#""libs/shared""#));
    assert!(cargo.contains(r#""tools/dev""#));

    assert!(path.join("apps/app/src/main.rs").is_file());
    assert!(path.join("libs/shared/src/lib.rs").is_file());
    assert!(path.join("tools/dev/src/main.rs").is_file());

    // Check that preset record was saved
    let preset_record =
        std::fs::read_to_string(path.join(".truss/preset.toml")).expect("read preset record");
    assert!(preset_record.contains("preset = \"workspace\""));
}

#[test]
fn new_with_type_and_license_override() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("myapp");

    let output = truss_cmd(&config)
        .args([
            "new",
            "myapp",
            "--path",
            path.to_str().expect("utf8 path"),
            "--type",
            "binary",
            "--license",
            "Apache-2.0",
            "--author",
            "truss-test",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run truss new --type binary --license Apache-2.0");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let cargo = std::fs::read_to_string(path.join("Cargo.toml")).expect("read cargo");
    assert!(cargo.contains("Apache-2.0"));
}

#[test]
fn types_lists_presets() {
    let config = tempdir().expect("tempdir");

    let output = truss_cmd(&config)
        .args(["types"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run truss types");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("binary"));
    assert!(stdout.contains("library"));
    assert!(stdout.contains("workspace"));
}

#[test]
fn types_with_details_shows_preset_info() {
    let config = tempdir().expect("tempdir");

    let output = truss_cmd(&config)
        .args(["types", "--details", "binary"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run truss types --details binary");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Preset: binary"));
    assert!(stdout.contains("Pack: default"));
}

#[test]
fn custom_preset_from_user_config() {
    let config = tempdir().expect("tempdir");
    let truss_config = config.path().join("truss");
    std::fs::create_dir_all(&truss_config).expect("mkdir truss config");

    // Create custom preset file
    let presets_toml = r#"
[presets.custom-service]
description = "Custom service preset"
pack = "default"
variables = { license = "MIT" }
"#;
    std::fs::write(truss_config.join("presets.toml"), presets_toml).expect("write presets.toml");

    let path = config.path().join("custom-app");

    let output = Command::new(truss_bin())
        .env("XDG_CONFIG_HOME", config.path())
        .env("NO_COLOR", "1")
        .args([
            "new",
            "custom-app",
            "--path",
            path.to_str().expect("utf8 path"),
            "--type",
            "custom-service",
            "--author",
            "truss-test",
        ])
        .output()
        .expect("run truss new --type custom-service");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let cargo = std::fs::read_to_string(path.join("Cargo.toml")).expect("read cargo");
    assert!(cargo.contains("MIT"));
}

#[test]
fn sync_uses_recorded_preset() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("mypreset");

    // Create project with preset
    let new = truss_cmd(&config)
        .args([
            "new",
            "mypreset",
            "--path",
            path.to_str().expect("utf8 path"),
            "--type",
            "binary",
            "--author",
            "truss-test",
        ])
        .output()
        .expect("new with preset");
    assert!(
        new.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&new.stderr)
    );

    // Modify a file
    let mut cargo_content = std::fs::read_to_string(path.join("Cargo.toml")).expect("read cargo");
    cargo_content.push_str("\n# modified\n");
    std::fs::write(path.join("Cargo.toml"), cargo_content).expect("modify cargo");

    // Sync without --type or --template should use recorded preset
    let sync = truss_cmd(&config)
        .args(["sync", "--path", path.to_str().expect("utf8 path")])
        .output()
        .expect("sync with recorded preset");
    assert!(
        sync.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&sync.stderr)
    );

    let cargo = std::fs::read_to_string(path.join("Cargo.toml")).expect("read cargo");
    assert!(!cargo.contains("modified"));
}

#[test]
fn type_and_template_mutually_exclusive() {
    let config = tempdir().expect("tempdir");
    let path = config.path().join("myapp");

    let output = truss_cmd(&config)
        .args([
            "new",
            "myapp",
            "--path",
            path.to_str().expect("utf8 path"),
            "--type",
            "binary",
            "--template",
            "monorepo",
            "--author",
            "truss-test",
        ])
        .output()
        .expect("run truss new with --type and --template");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("mutually exclusive"));
}

/// Build a pack directory described by a JSON manifest.
fn write_json_pack(root: &std::path::Path) -> std::path::PathBuf {
    let pack = root.join("jsonpack");
    std::fs::create_dir_all(pack.join("src")).expect("mkdir pack");
    std::fs::write(pack.join("README.md"), "# {{ project_name }}\n").expect("write README");
    std::fs::write(pack.join("literal.txt"), "{{ project_name }}\n").expect("write literal");
    std::fs::write(pack.join("src").join("main.rs"), "fn main() {}\n").expect("write main");
    std::fs::write(pack.join("src").join("cli.rs"), "// cli\n").expect("write cli");
    std::fs::write(
        pack.join("truss-pack.json"),
        r#"{
  "name": "jsonpack",
  "variables": [
    { "name": "has_cli", "type": "bool", "default": false },
    { "name": "lang", "type": "string", "default": "rust" }
  ],
  "files": [
    { "source": "README.md", "destination": "README.md", "condition": "lang == \"rust\"" },
    { "source": "literal.txt", "destination": "literal.txt", "is_template": false },
    { "source": "src", "destination": "src" },
    { "source": "src/cli.rs", "destination": "src/cli.rs", "condition": "has_cli" }
  ]
}
"#,
    )
    .expect("write manifest");
    pack
}

fn add_json_pack(config: &TempDir, pack: &std::path::Path) {
    let add = truss_cmd(config)
        .args([
            "registry",
            "add",
            "jsonpack",
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
}

#[test]
fn json_pack_is_used_by_new() {
    let config = tempdir().expect("tempdir");
    let pack = write_json_pack(config.path());
    add_json_pack(&config, &pack);

    let path = config.path().join("proj");
    let output = truss_cmd(&config)
        .args([
            "new",
            "proj",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "jsonpack",
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

    // The manifest decides the layout, so truss-pack.json itself is never copied.
    assert!(
        !path.join("truss-pack.json").exists(),
        "the manifest must not be emitted into the project"
    );

    // A true condition includes the file, and it is rendered.
    // minijinja drops the trailing newline when it renders, which is itself the
    // contrast with literal.txt below.
    let readme = std::fs::read_to_string(path.join("README.md")).expect("README.md");
    assert_eq!(readme.trim_end(), "# proj", "README should be rendered");

    // is_template=false copies the bytes through untouched.
    let literal = std::fs::read_to_string(path.join("literal.txt")).expect("literal.txt");
    assert_eq!(
        literal, "{{ project_name }}\n",
        "a non-template file must be copied verbatim"
    );

    // The directory mapping brought src/main.rs in exactly once.
    assert!(path.join("src").join("main.rs").exists(), "src/main.rs");

    // has_cli defaults to false, and the more specific src/cli.rs mapping owns
    // that file, so the enclosing src/ mapping must not resurrect it.
    assert!(
        !path.join("src").join("cli.rs").exists(),
        "a false condition on the most specific mapping must exclude the file"
    );
}

#[test]
fn json_pack_condition_can_read_builtin_context() {
    let config = tempdir().expect("tempdir");
    let pack = config.path().join("editionpack");
    std::fs::create_dir_all(&pack).expect("mkdir pack");
    std::fs::write(pack.join("modern.md"), "modern\n").expect("write modern");
    std::fs::write(pack.join("legacy.md"), "legacy\n").expect("write legacy");
    std::fs::write(
        pack.join("truss-pack.json"),
        r#"{
  "name": "editionpack",
  "files": [
    { "source": "modern.md", "destination": "modern.md", "condition": "edition == \"2024\"" },
    { "source": "legacy.md", "destination": "legacy.md", "condition": "edition == \"2015\"" }
  ]
}
"#,
    )
    .expect("write manifest");

    let add = truss_cmd(&config)
        .args([
            "registry",
            "add",
            "editionpack",
            "--source",
            pack.to_str().expect("utf8 path"),
            "--kind",
            "dir",
        ])
        .output()
        .expect("registry add");
    assert!(add.status.success());

    let path = config.path().join("modernproj");
    let output = truss_cmd(&config)
        .args([
            "new",
            "modernproj",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "editionpack",
            "--author",
            "truss-test",
            "--edition",
            "2024",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run truss new");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        path.join("modern.md").exists(),
        "a true condition on a built-in context field must include the file"
    );
    assert!(
        !path.join("legacy.md").exists(),
        "a false condition on a built-in context field must exclude the file"
    );
}

#[test]
fn pack_validate_accepts_a_well_formed_pack() {
    let config = tempdir().expect("tempdir");
    let pack = write_json_pack(config.path());

    let output = truss_cmd(&config)
        .args(["pack", "validate", pack.to_str().expect("utf8")])
        .env("NO_COLOR", "1")
        .output()
        .expect("run pack validate");
    assert!(
        output.status.success(),
        "a valid pack must validate regardless of the host temp directory; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn pack_validate_rejects_a_template_that_does_not_compile() {
    let config = tempdir().expect("tempdir");
    let pack = config.path().join("badpack");
    std::fs::create_dir_all(&pack).expect("mkdir pack");
    std::fs::write(pack.join("broken.md"), "{% if %}\n").expect("write broken");
    std::fs::write(
        pack.join("truss-pack.json"),
        r#"{
  "name": "badpack",
  "files": [{ "source": "broken.md", "destination": "broken.md" }]
}
"#,
    )
    .expect("write manifest");

    let output = truss_cmd(&config)
        .args(["pack", "validate", pack.to_str().expect("utf8")])
        .env("NO_COLOR", "1")
        .output()
        .expect("run pack validate");
    assert!(
        !output.status.success(),
        "a pack with a broken template must fail validation"
    );
}

/// Generate a project from a pack whose directory mapping and file mapping both
/// produce `src/cli.rs`, with the two mappings declared in the given order.
fn overlapping_pack_content(name: &str, files_json: &str) -> String {
    let config = tempdir().expect("tempdir");
    let pack = config.path().join(name);
    std::fs::create_dir_all(pack.join("dir")).expect("mkdir pack");
    std::fs::write(pack.join("dir").join("cli.rs"), "from directory\n").expect("write dir copy");
    std::fs::write(pack.join("special.rs"), "from specific mapping\n").expect("write specific");
    std::fs::write(
        pack.join("truss-pack.json"),
        format!("{{\n  \"name\": \"{name}\",\n  \"files\": [{files_json}]\n}}\n"),
    )
    .expect("write manifest");

    let add = truss_cmd(&config)
        .args([
            "registry",
            "add",
            name,
            "--source",
            pack.to_str().expect("utf8 path"),
            "--kind",
            "dir",
        ])
        .output()
        .expect("registry add");
    assert!(add.status.success());

    let path = config.path().join("proj");
    let output = truss_cmd(&config)
        .args([
            "new",
            "proj",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            name,
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

    std::fs::read_to_string(path.join("src").join("cli.rs")).expect("src/cli.rs")
}

#[test]
fn json_pack_emits_an_overlapping_destination_once() {
    let dir_mapping = r#"{ "source": "dir", "destination": "src" }"#;
    let file_mapping = r#"{ "source": "special.rs", "destination": "src/cli.rs" }"#;

    // Both mappings produce src/cli.rs. The more specific one owns it, so the
    // directory copy must never be written -- whichever order they appear in.
    for (order, files) in [
        ("directory first", format!("{dir_mapping}, {file_mapping}")),
        ("file first", format!("{file_mapping}, {dir_mapping}")),
    ] {
        let content = overlapping_pack_content("overlap", &files);
        assert_eq!(
            content.trim_end(),
            "from specific mapping",
            "the most specific mapping must own the destination ({order})"
        );
    }
}

#[test]
fn json_pack_validates_answers_against_the_manifest() {
    let config = tempdir().expect("tempdir");
    let pack = config.path().join("typedpack");
    std::fs::create_dir_all(&pack).expect("mkdir pack");
    std::fs::write(pack.join("a.txt"), "port {{ port }}\n").expect("write a");
    std::fs::write(
        pack.join("truss-pack.json"),
        r#"{
  "name": "typedpack",
  "variables": [{ "name": "port", "type": "integer", "default": 8080 }],
  "files": [{ "source": "a.txt", "destination": "a.txt" }]
}
"#,
    )
    .expect("write manifest");

    let add = truss_cmd(&config)
        .args([
            "registry",
            "add",
            "typedpack",
            "--source",
            pack.to_str().expect("utf8 path"),
            "--kind",
            "dir",
        ])
        .output()
        .expect("registry add");
    assert!(add.status.success());

    let path = config.path().join("typedproj");
    let output = truss_cmd(&config)
        .args([
            "new",
            "typedproj",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "typedpack",
            "--author",
            "truss-test",
            "--define",
            "port=not-a-number",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run truss new");
    assert!(
        !output.status.success(),
        "an integer variable must reject a non-numeric answer"
    );
}

#[test]
fn pack_validate_accepts_a_literal_file_with_template_delimiters() {
    let config = tempdir().expect("tempdir");
    let pack = config.path().join("literalpack");
    std::fs::create_dir_all(&pack).expect("mkdir pack");
    // Not valid minijinja, but generation copies it without parsing.
    std::fs::write(pack.join("snippet.txt"), "{% if %}\n").expect("write snippet");
    std::fs::write(
        pack.join("truss-pack.json"),
        r#"{
  "name": "literalpack",
  "files": [
    { "source": "snippet.txt", "destination": "snippet.txt", "is_template": false }
  ]
}
"#,
    )
    .expect("write manifest");

    let output = truss_cmd(&config)
        .args(["pack", "validate", pack.to_str().expect("utf8")])
        .env("NO_COLOR", "1")
        .output()
        .expect("run pack validate");
    assert!(
        output.status.success(),
        "a literal file must not be syntax-checked; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn json_pack_preserves_executable_mode() {
    use std::os::unix::fs::PermissionsExt;

    let config = tempdir().expect("tempdir");
    let pack = config.path().join("scriptpack");
    std::fs::create_dir_all(pack.join("bin")).expect("mkdir pack");
    let script = pack.join("bin").join("run.sh");
    std::fs::write(&script, "#!/bin/sh\necho hi\n").expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    std::fs::write(
        pack.join("truss-pack.json"),
        r#"{
  "name": "scriptpack",
  "files": [{ "source": "bin", "destination": "bin" }]
}
"#,
    )
    .expect("write manifest");

    let add = truss_cmd(&config)
        .args([
            "registry",
            "add",
            "scriptpack",
            "--source",
            pack.to_str().expect("utf8 path"),
            "--kind",
            "dir",
        ])
        .output()
        .expect("registry add");
    assert!(add.status.success());

    let path = config.path().join("scriptproj");
    let output = truss_cmd(&config)
        .args([
            "new",
            "scriptproj",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "scriptpack",
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

    let mode = std::fs::metadata(path.join("bin").join("run.sh"))
        .expect("generated script")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode & 0o111,
        0o111,
        "an executable pack source must stay executable, mode was {mode:o}"
    );
}

#[test]
fn marketplace_search_local_index() {
    let config = tempdir().expect("tempdir");
    let index_path = config.path().join("marketplace.json");
    let index_content = r#"{
        "version": 1,
        "entries": [
            {
                "name": "test-template",
                "description": "A test template",
                "author": "test-author",
                "tags": ["test", "rust"],
                "source": "https://example.com/test",
                "kind": "git",
                "ref": "main",
                "subfolder": null,
                "version": "1.0.0"
            }
        ]
    }"#;
    std::fs::write(&index_path, index_content).expect("write index");

    let output = truss_cmd(&config)
        .env(
            "TRUSS_MARKETPLACE_INDEX",
            index_path.to_str().expect("utf8"),
        )
        .args(["marketplace", "search", "test"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace search");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test-template"));
    assert!(stdout.contains("test-author"));
}

#[test]
fn marketplace_install_from_local_index() {
    let config = tempdir().expect("tempdir");
    let template_dir = config.path().join("template-source");
    std::fs::create_dir(&template_dir).expect("mkdir template");
    std::fs::write(
        template_dir.join("Cargo.toml"),
        "[package]\nname = \"test\"\n",
    )
    .expect("write cargo");

    let index_path = config.path().join("marketplace.json");
    let index_content = r#"{
        "version": 1,
        "entries": [
            {
                "name": "test-template",
                "description": "A test template",
                "author": "test-author",
                "tags": ["test"],
                "source": "TEMPLATE_PATH",
                "kind": "dir",
                "ref": null,
                "subfolder": null,
                "version": "1.0.0"
            }
        ]
    }"#
    .replace("TEMPLATE_PATH", template_dir.to_str().expect("utf8"));
    std::fs::write(&index_path, index_content).expect("write index");

    let output = truss_cmd(&config)
        .env(
            "TRUSS_MARKETPLACE_INDEX",
            index_path.to_str().expect("utf8"),
        )
        .args(["marketplace", "install", "test-template"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace install");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("installed test-template"));

    let registry_path = config.path().join("truss/registry.json");
    assert!(registry_path.exists());
    let registry_content = std::fs::read_to_string(&registry_path).expect("read registry");
    assert!(registry_content.contains("test-template"));
}

#[test]
fn marketplace_list_installed_and_available() {
    let config = tempdir().expect("tempdir");
    let template_dir = config.path().join("template-source");
    std::fs::create_dir(&template_dir).expect("mkdir template");
    std::fs::write(
        template_dir.join("Cargo.toml"),
        "[package]\nname = \"test\"\n",
    )
    .expect("write cargo");

    let index_path = config.path().join("marketplace.json");
    let index_content = r#"{
        "version": 1,
        "entries": [
            {
                "name": "installed-template",
                "description": "An installed template",
                "author": "test",
                "tags": ["test"],
                "source": "TEMPLATE_PATH",
                "kind": "dir",
                "ref": null,
                "subfolder": null,
                "version": "1.0.0"
            },
            {
                "name": "available-template",
                "description": "An available template",
                "author": "test",
                "tags": ["test"],
                "source": "TEMPLATE_PATH",
                "kind": "dir",
                "ref": null,
                "subfolder": null,
                "version": "1.0.0"
            }
        ]
    }"#
    .replace("TEMPLATE_PATH", template_dir.to_str().expect("utf8"));
    std::fs::write(&index_path, index_content).expect("write index");

    let install = truss_cmd(&config)
        .env(
            "TRUSS_MARKETPLACE_INDEX",
            index_path.to_str().expect("utf8"),
        )
        .args(["marketplace", "install", "installed-template"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace install");
    assert!(install.status.success());

    let output = truss_cmd(&config)
        .env(
            "TRUSS_MARKETPLACE_INDEX",
            index_path.to_str().expect("utf8"),
        )
        .args(["marketplace", "list"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace list");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("installed-template"));
    assert!(stdout.contains("available-template"));
    assert!(stdout.contains("installed"));
    assert!(stdout.contains("available"));
}

#[test]
fn marketplace_publish_appends_to_local_index() {
    let config = tempdir().expect("tempdir");
    let pack_dir = config.path().join("pack");
    std::fs::create_dir(&pack_dir).expect("mkdir pack");
    std::fs::write(pack_dir.join("Cargo.toml"), "[package]\nname = \"test\"\n")
        .expect("write cargo");

    let output = truss_cmd(&config)
        .args([
            "marketplace",
            "publish",
            pack_dir.to_str().expect("utf8"),
            "--name",
            "published-template",
            "--description",
            "A published template",
            "--author",
            "test-author",
            "--tag",
            "test",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace publish");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("published published-template"));

    let index_path = config.path().join("truss/marketplace.json");
    assert!(index_path.exists());
    let index_content = std::fs::read_to_string(&index_path).expect("read index");
    assert!(index_content.contains("published-template"));
    assert!(index_content.contains("A published template"));
}

#[test]
fn marketplace_network_error_handling() {
    let config = tempdir().expect("tempdir");

    // Port 1 on the loopback address refuses the connection immediately. The
    // previous host name relied on DNS not resolving, which a resolving proxy or
    // a wildcard resolver would defeat.
    let output = truss_cmd(&config)
        .env("TRUSS_MARKETPLACE_INDEX", "http://127.0.0.1:1/index.json")
        .args(["marketplace", "search", "test"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace search");

    assert!(
        !output.status.success(),
        "an unreachable index must not report success"
    );
}

/// Write a local marketplace index and point the CLI at it.
fn marketplace_cmd(config: &TempDir, index: &std::path::Path, json: &str) -> Command {
    std::fs::write(index, json).expect("write index");
    let mut cmd = truss_cmd(config);
    cmd.env("TRUSS_MARKETPLACE_INDEX", index.as_os_str());
    cmd
}

fn index_json(source: &str, kind: &str) -> String {
    format!(
        r#"{{
  "version": 1,
  "entries": [
    {{
      "name": "demo",
      "description": "a demo template pack",
      "author": "someone",
      "tags": ["demo"],
      "source": "{source}",
      "kind": "{kind}"
    }}
  ]
}}
"#
    )
}

#[test]
fn marketplace_update_replaces_without_force() {
    let config = tempdir().expect("tempdir");
    let index = config.path().join("index.json");
    let pack_a = config.path().join("a");
    let pack_b = config.path().join("b");
    for p in [&pack_a, &pack_b] {
        std::fs::create_dir_all(p).expect("mkdir");
        std::fs::write(p.join("f.md"), "x").expect("write");
    }

    let install = marketplace_cmd(
        &config,
        &index,
        &index_json(pack_a.to_str().expect("utf8"), "dir"),
    )
    .args(["marketplace", "install", "demo"])
    .output()
    .expect("install");
    assert!(
        install.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&install.stderr)
    );

    // The listing now points somewhere else. An update is a replacement by
    // definition, so it must not demand the install-time --force flag.
    let update = marketplace_cmd(
        &config,
        &index,
        &index_json(pack_b.to_str().expect("utf8"), "dir"),
    )
    .args(["marketplace", "update"])
    .output()
    .expect("update");
    assert!(
        update.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&update.stderr)
    );
    assert!(
        String::from_utf8_lossy(&update.stdout).contains("updated 1"),
        "stdout={}",
        String::from_utf8_lossy(&update.stdout)
    );

    let list = truss_cmd(&config)
        .args(["registry", "list"])
        .output()
        .expect("registry list");
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(
        stdout.contains(pack_b.to_str().expect("utf8")),
        "the registry must now point at the new source: {stdout}"
    );
}

#[test]
fn marketplace_update_leaves_a_local_template_alone() {
    let config = tempdir().expect("tempdir");
    let index = config.path().join("index.json");
    let local = config.path().join("local");
    let listed = config.path().join("listed");
    for p in [&local, &listed] {
        std::fs::create_dir_all(p).expect("mkdir");
        std::fs::write(p.join("f.md"), "x").expect("write");
    }

    // A hand-registered template that happens to share the listing's name.
    let add = truss_cmd(&config)
        .args([
            "registry",
            "add",
            "demo",
            "--source",
            local.to_str().expect("utf8"),
            "--kind",
            "dir",
        ])
        .output()
        .expect("registry add");
    assert!(add.status.success());

    let update = marketplace_cmd(
        &config,
        &index,
        &index_json(listed.to_str().expect("utf8"), "dir"),
    )
    .args(["marketplace", "update"])
    .output()
    .expect("update");
    assert!(update.status.success());

    let list = truss_cmd(&config)
        .args(["registry", "list"])
        .output()
        .expect("registry list");
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(
        stdout.contains(local.to_str().expect("utf8")),
        "a local template must survive a bulk marketplace update: {stdout}"
    );
    assert!(
        !stdout.contains(listed.to_str().expect("utf8")),
        "the marketplace listing must not overwrite it: {stdout}"
    );
}

#[test]
fn marketplace_update_notices_a_kind_change() {
    let config = tempdir().expect("tempdir");
    let index = config.path().join("index.json");
    let pack = config.path().join("pack");
    std::fs::create_dir_all(&pack).expect("mkdir");
    std::fs::write(pack.join("f.md"), "x").expect("write");

    let install = marketplace_cmd(
        &config,
        &index,
        &index_json(pack.to_str().expect("utf8"), "dir"),
    )
    .args(["marketplace", "install", "demo"])
    .output()
    .expect("install");
    assert!(install.status.success());

    // Same source, different kind. `kind` picks the loader, so this is a real
    // change and the update must act on it. Here the new kind does not match the
    // source, so acting on it means refusing loudly. Before the kind was
    // compared, this listing was skipped and the entry silently kept the
    // obsolete loader: the command exited 0 with "updated 0".
    let update = marketplace_cmd(
        &config,
        &index,
        &index_json(pack.to_str().expect("utf8"), "file"),
    )
    .args(["marketplace", "update"])
    .output()
    .expect("update");
    let stdout = String::from_utf8_lossy(&update.stdout);
    assert!(
        !stdout.contains("updated 0"),
        "a kind change must not be skipped; stdout={stdout} stderr={}",
        String::from_utf8_lossy(&update.stderr)
    );
    assert!(
        !update.status.success(),
        "a kind that does not match the source must be refused, not applied"
    );
}

#[test]
fn marketplace_publish_records_an_ssh_source_as_git() {
    let config = tempdir().expect("tempdir");
    let pack = config.path().join("pack");
    std::fs::create_dir_all(&pack).expect("mkdir");
    std::fs::write(pack.join("f.md"), "x").expect("write");

    let publish = truss_cmd(&config)
        .args([
            "marketplace",
            "publish",
            pack.to_str().expect("utf8"),
            "--name",
            "sshpack",
            "--source",
            "git@github.com:owner/repo.git",
        ])
        .output()
        .expect("publish");
    assert!(
        publish.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&publish.stderr)
    );

    let index_path = config.path().join("truss").join("marketplace.json");
    let written = std::fs::read_to_string(&index_path).expect("index written");
    assert!(
        written.contains("\"kind\": \"git\""),
        "an ssh source must publish as a git template: {written}"
    );
}

#[test]
fn marketplace_search_shows_the_description() {
    let config = tempdir().expect("tempdir");
    let index = config.path().join("index.json");
    let pack = config.path().join("pack");
    std::fs::create_dir_all(&pack).expect("mkdir");

    let search = marketplace_cmd(
        &config,
        &index,
        &index_json(pack.to_str().expect("utf8"), "dir"),
    )
    .args(["marketplace", "search", "demo"])
    .output()
    .expect("search");
    assert!(search.status.success());
    let stdout = String::from_utf8_lossy(&search.stdout);
    assert!(
        stdout.contains("a demo template pack"),
        "search must show the description: {stdout}"
    );
}

/// Register a pack directory built from `files` (relative path, contents) plus
/// a `truss-pack.json` manifest body.
fn add_pack(
    config: &TempDir,
    name: &str,
    manifest: &str,
    files: &[(&str, &str)],
) -> std::path::PathBuf {
    let pack = config.path().join(name);
    std::fs::create_dir_all(&pack).expect("mkdir pack");
    for (relative, contents) in files {
        let target = pack.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("mkdir pack subdir");
        }
        std::fs::write(target, contents).expect("write pack file");
    }
    std::fs::write(pack.join("truss-pack.json"), manifest).expect("write manifest");

    let add = truss_cmd(config)
        .args(["registry", "add", name, "--source"])
        .arg(&pack)
        .args(["--kind", "dir"])
        .output()
        .expect("registry add");
    assert!(
        add.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&add.stderr)
    );
    pack
}

/// Two mappings whose destinations render to one path would both be written,
/// and the last one would silently win.
#[test]
fn a_pack_that_writes_one_destination_twice_is_rejected() {
    let config = tempdir().expect("tempdir");
    add_pack(
        &config,
        "collidepack",
        r#"{
  "name": "collidepack",
  "variables": [
    { "name": "leaf", "type": "string", "default": "app" }
  ],
  "files": [
    { "source": "first.txt", "destination": "{{ leaf }}.txt" },
    { "source": "second.txt", "destination": "app.txt" }
  ]
}
"#,
        &[("first.txt", "first\n"), ("second.txt", "second\n")],
    );

    let path = config.path().join("proj");
    let output = truss_cmd(&config)
        .args(["new", "proj", "--path"])
        .arg(&path)
        .args(["--template", "collidepack", "--author", "truss-test"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run truss new");

    assert!(
        !output.status.success(),
        "a colliding pack must not scaffold"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("render to the destination"),
        "stderr={stderr}"
    );
}

/// Compiling is not generating. Two mappings that render to one destination
/// compile fine and then fail at generation time, when the user has already
/// committed to the pack. Validation must reach that.
#[test]
fn pack_validate_reports_a_destination_collision() {
    let config = tempdir().expect("tempdir");
    let pack = config.path().join("pack");
    std::fs::create_dir_all(&pack).expect("mkdir pack");
    std::fs::write(pack.join("a.txt"), "a\n").expect("write a");
    std::fs::write(pack.join("b.txt"), "b\n").expect("write b");
    std::fs::write(
        pack.join("truss-pack.json"),
        r#"{
  "name": "collidepack",
  "variables": [ { "name": "out", "type": "string", "default": "same.txt" } ],
  "files": [
    { "source": "a.txt", "destination": "{{ out }}" },
    { "source": "b.txt", "destination": "same.txt" }
  ]
}
"#,
    )
    .expect("write manifest");

    let output = truss_cmd(&config)
        .args(["pack", "validate", pack.to_str().expect("utf8")])
        .env("NO_COLOR", "1")
        .output()
        .expect("run pack validate");

    assert!(
        !output.status.success(),
        "a pack that cannot render must not validate: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("same.txt"),
        "the error must name the destination: {stderr}"
    );
}

fn index_json_versioned(source: &str, kind: &str, version: &str) -> String {
    format!(
        r#"{{
  "version": 1,
  "entries": [
    {{
      "name": "demo",
      "description": "a demo template pack",
      "author": "someone",
      "tags": ["demo"],
      "source": "{source}",
      "kind": "{kind}",
      "version": "{version}"
    }}
  ]
}}
"#
    )
}

/// A release that moves only the version still has to reach the registry.
#[test]
fn marketplace_update_applies_a_version_only_release() {
    let config = tempdir().expect("tempdir");
    let index = config.path().join("index.json");
    let pack = config.path().join("pack");
    std::fs::create_dir_all(&pack).expect("mkdir pack");
    std::fs::write(pack.join("Cargo.toml"), "name = \"{{ project_name }}\"\n").expect("write pack");
    let source = pack.to_str().expect("utf8 path").replace('\\', "\\\\");

    let install = marketplace_cmd(
        &config,
        &index,
        &index_json_versioned(&source, "dir", "1.0.0"),
    )
    .args(["marketplace", "install", "demo"])
    .output()
    .expect("marketplace install");
    assert!(
        install.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&install.stderr)
    );

    let update = marketplace_cmd(
        &config,
        &index,
        &index_json_versioned(&source, "dir", "2.0.0"),
    )
    .args(["marketplace", "update"])
    .output()
    .expect("marketplace update");
    assert!(
        update.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&update.stderr)
    );
    let stdout = String::from_utf8_lossy(&update.stdout);
    assert!(
        stdout.contains("updated 1 marketplace template(s)"),
        "stdout={stdout}"
    );

    // The new version is recorded, so a second pass has nothing to do.
    let again = marketplace_cmd(
        &config,
        &index,
        &index_json_versioned(&source, "dir", "2.0.0"),
    )
    .args(["marketplace", "update"])
    .output()
    .expect("marketplace update");
    let stdout = String::from_utf8_lossy(&again.stdout);
    assert!(
        stdout.contains("updated 0 marketplace template(s)"),
        "stdout={stdout}"
    );
}

#[test]
fn marketplace_search_shows_the_source() {
    let config = tempdir().expect("tempdir");
    let index = config.path().join("index.json");
    let output = marketplace_cmd(
        &config,
        &index,
        &index_json("https://example.com/demo.git", "git"),
    )
    .args(["marketplace", "search", "demo"])
    .output()
    .expect("marketplace search");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("https://example.com/demo.git"),
        "stdout={stdout}"
    );
}

/// Index contents are untrusted, so an unusable listing has to be rejected
/// before any of it reaches the registry.
#[test]
fn marketplace_index_rejects_an_unusable_listing() {
    let cases: [(&str, &str); 3] = [
        (
            r#"{"version":1,"entries":[
              {"name":"demo","description":"d","author":"a","source":"https://example.com/a.git","kind":"git"},
              {"name":"demo","description":"d","author":"a","source":"https://example.com/b.git","kind":"git"}]}"#,
            "more than once",
        ),
        (
            r#"{"version":1,"entries":[
              {"name":"","description":"d","author":"a","source":"https://example.com/a.git","kind":"git"}]}"#,
            "empty name",
        ),
        (
            r#"{"version":1,"entries":[
              {"name":"demo","description":"d","author":"a","source":"not a url","kind":"git"}]}"#,
            "unusable git source",
        ),
    ];

    for (json, expected) in cases {
        let config = tempdir().expect("tempdir");
        let index = config.path().join("index.json");
        let output = marketplace_cmd(&config, &index, json)
            .args(["marketplace", "list"])
            .env("NO_COLOR", "1")
            .output()
            .expect("marketplace list");
        assert!(!output.status.success(), "{expected} must be rejected");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "stderr={stderr}");
    }
}

fn git_in(args: &[&str], cwd: Option<&std::path::Path>) {
    let mut cmd = Command::new("git");
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_TEMPLATE_DIR", "")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test");
    if let Some(dir) = cwd {
        cmd.arg("-C").arg(dir);
    }
    cmd.arg("-c").arg("core.hooksPath=/dev/null");
    let out = cmd.args(args).output().expect("run git");
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Publish a one-file template pack to a bare repo and return its `file://` URL.
fn bare_template_repo(root: &std::path::Path, name: &str, marker: &str) -> String {
    let bare = root.join(format!("{name}.git"));
    let work = root.join(name);
    git_in(
        &[
            "init",
            "--bare",
            "--initial-branch=main",
            bare.to_str().expect("utf8"),
        ],
        None,
    );
    std::fs::create_dir_all(&work).expect("mkdir work");
    std::fs::write(work.join("Cargo.toml"), "name = \"{{ project_name }}\"\n")
        .expect("write cargo");
    std::fs::write(work.join("MARK.txt"), marker).expect("write marker");
    git_in(&["init", "--initial-branch=main"], Some(&work));
    git_in(&["add", "."], Some(&work));
    git_in(&["commit", "-m", "initial"], Some(&work));
    git_in(&["push", bare.to_str().expect("utf8"), "main"], Some(&work));
    format!("file://{}", bare.display())
}

/// The Git cache is keyed by template name, so reinstalling over a different
/// source has to drop it or the next scaffold still reads the old repository.
#[test]
fn marketplace_install_over_a_new_git_source_drops_the_stale_cache() {
    let config = tempdir().expect("tempdir");
    let cache = config.path().join("cache");
    std::fs::create_dir_all(&cache).expect("mkdir cache");
    let index = config.path().join("index.json");

    let first = bare_template_repo(config.path(), "first", "one");
    let second = bare_template_repo(config.path(), "second", "two");

    let scaffold = |name: &str| -> std::path::PathBuf {
        let path = config.path().join(name);
        let output = truss_cmd(&config)
            .env("XDG_CACHE_HOME", &cache)
            .args(["new", name, "--path"])
            .arg(&path)
            .args(["--template", "demo", "--author", "truss-test"])
            .env("NO_COLOR", "1")
            .output()
            .expect("run truss new");
        assert!(
            output.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        path
    };

    let install = marketplace_cmd(&config, &index, &index_json(&first, "git"))
        .env("XDG_CACHE_HOME", &cache)
        .args(["marketplace", "install", "demo"])
        .output()
        .expect("marketplace install");
    assert!(
        install.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&install.stderr)
    );
    // Populates the name-keyed cache.
    let one = scaffold("p1");
    assert_eq!(
        std::fs::read_to_string(one.join("MARK.txt")).expect("MARK.txt"),
        "one"
    );

    let reinstall = marketplace_cmd(&config, &index, &index_json(&second, "git"))
        .env("XDG_CACHE_HOME", &cache)
        .args(["marketplace", "install", "demo", "--force"])
        .output()
        .expect("marketplace install --force");
    assert!(
        reinstall.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&reinstall.stderr)
    );

    let two = scaffold("p2");
    assert_eq!(
        std::fs::read_to_string(two.join("MARK.txt")).expect("MARK.txt"),
        "two",
        "the reinstall must scaffold from the new source, not the cached one"
    );
}

/// An empty `--name` was written straight to the index. Every later marketplace
/// command then failed to load that index, so one bad publish blocked them all.
#[test]
fn marketplace_publish_rejects_an_empty_name() {
    let config = tempdir().expect("tempdir");
    let pack_dir = config.path().join("pack");
    std::fs::create_dir(&pack_dir).expect("mkdir pack");
    std::fs::write(pack_dir.join("Cargo.toml"), "[package]\nname = \"test\"\n")
        .expect("write cargo");

    let output = truss_cmd(&config)
        .args([
            "marketplace",
            "publish",
            pack_dir.to_str().expect("utf8"),
            "--name",
            "   ",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace publish");

    assert!(!output.status.success(), "an empty name must be rejected");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("name cannot be empty"), "stderr={stderr}");
    assert!(
        !config.path().join("truss/marketplace.json").exists(),
        "a rejected publish must not create the index"
    );
}

/// Publishing a pack whose manifest does not parse breaks every consumer of the
/// listing, not the author. The failure belongs at publish time.
#[test]
fn marketplace_publish_rejects_an_unusable_pack() {
    let config = tempdir().expect("tempdir");
    let pack_dir = config.path().join("pack");
    std::fs::create_dir(&pack_dir).expect("mkdir pack");
    std::fs::write(pack_dir.join("truss-pack.json"), "{ not json").expect("write manifest");

    let output = truss_cmd(&config)
        .args([
            "marketplace",
            "publish",
            pack_dir.to_str().expect("utf8"),
            "--name",
            "broken",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace publish");

    assert!(!output.status.success(), "a broken pack must be rejected");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not a usable template pack"),
        "stderr={stderr}"
    );
}

/// A template stays installed and usable after its listing is removed from the
/// index. Reporting it as absent from `--installed` misstates what is on the
/// machine.
#[test]
fn marketplace_list_installed_shows_a_delisted_template() {
    let config = tempdir().expect("tempdir");
    let template_dir = config.path().join("template-source");
    std::fs::create_dir(&template_dir).expect("mkdir template");
    std::fs::write(
        template_dir.join("Cargo.toml"),
        "[package]\nname = \"test\"\n",
    )
    .expect("write cargo");
    let source = template_dir.to_str().expect("utf8").to_string();

    let index_path = config.path().join("marketplace.json");
    let listed = format!(
        r#"{{"version":1,"entries":[{{"name":"gone-template","description":"d",
           "author":"test","tags":["test"],"source":"{source}","kind":"dir"}}]}}"#
    );

    let install = marketplace_cmd(&config, &index_path, &listed)
        .args(["marketplace", "install", "gone-template"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace install");
    assert!(
        install.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&install.stderr)
    );

    // The listing is withdrawn; the installed template is untouched.
    let output = marketplace_cmd(&config, &index_path, r#"{"version":1,"entries":[]}"#)
        .args(["marketplace", "list", "--installed"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace list");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("gone-template") && stdout.contains("delisted"),
        "a delisted but installed template must still be listed: {stdout}"
    );
}

// A delisted install is still on the machine and still usable. The default
// listing represents every status, so hiding it there misreports the inventory.
#[test]
fn marketplace_list_shows_a_delisted_install_by_default() {
    let config = tempdir().expect("tempdir");
    let template_dir = config.path().join("template-source");
    std::fs::create_dir(&template_dir).expect("mkdir template");
    std::fs::write(
        template_dir.join("Cargo.toml"),
        "[package]\nname = \"test\"\n",
    )
    .expect("write cargo");

    let index_path = config.path().join("marketplace.json");
    let listed = r#"{
        "version": 1,
        "entries": [
            {
                "name": "gone-template",
                "description": "A template about to be delisted",
                "author": "test",
                "tags": [],
                "source": "TEMPLATE_PATH",
                "kind": "dir",
                "ref": null,
                "subfolder": null,
                "version": "1.0.0"
            }
        ]
    }"#
    .replace("TEMPLATE_PATH", template_dir.to_str().expect("utf8"));
    std::fs::write(&index_path, listed).expect("write index");

    let install = truss_cmd(&config)
        .env(
            "TRUSS_MARKETPLACE_INDEX",
            index_path.to_str().expect("utf8"),
        )
        .args(["marketplace", "install", "gone-template"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace install");
    assert!(install.status.success());

    // The listing disappears; the install does not.
    std::fs::write(&index_path, r#"{"version": 1, "entries": []}"#).expect("rewrite index");

    let output = truss_cmd(&config)
        .env(
            "TRUSS_MARKETPLACE_INDEX",
            index_path.to_str().expect("utf8"),
        )
        .args(["marketplace", "list"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace list");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("gone-template") && stdout.contains("delisted"),
        "the default listing must report a delisted install: {stdout}"
    );
}

// With a distinct --source, the advertised directory is what every consumer
// installs. Validating the directory the command was pointed at instead lets a
// malformed pack into the index.
#[test]
fn marketplace_publish_validates_the_advertised_source() {
    let config = tempdir().expect("tempdir");

    let ok_dir = config.path().join("ok-pack");
    std::fs::create_dir(&ok_dir).expect("mkdir ok");
    std::fs::write(ok_dir.join("Cargo.toml"), "[package]\nname = \"ok\"\n").expect("write ok");

    // A manifest that does not parse: loading this directory fails.
    let bad_dir = config.path().join("bad-pack");
    std::fs::create_dir(&bad_dir).expect("mkdir bad");
    std::fs::write(bad_dir.join("truss-pack.json"), "{ not json").expect("write bad");

    let output = truss_cmd(&config)
        .args([
            "marketplace",
            "publish",
            ok_dir.to_str().expect("utf8"),
            "--name",
            "advertised",
            "--source",
            bad_dir.to_str().expect("utf8"),
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace publish");

    assert!(
        !output.status.success(),
        "publishing an unusable advertised source must fail: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("bad-pack"),
        "the error must name the advertised source: {stderr}"
    );
}

// A marketplace listing cannot know how this machine authenticates to a private
// repository. Taking its empty values would break the template on every update.
#[test]
fn marketplace_update_preserves_private_repository_settings() {
    let config = tempdir().expect("tempdir");
    let template_dir = config.path().join("template-source");
    std::fs::create_dir(&template_dir).expect("mkdir template");
    std::fs::write(
        template_dir.join("Cargo.toml"),
        "[package]\nname = \"test\"\n",
    )
    .expect("write cargo");

    let index_path = config.path().join("marketplace.json");
    let index = r#"{
        "version": 1,
        "entries": [
            {
                "name": "private-template",
                "description": "A private template",
                "author": "test",
                "tags": [],
                "source": "TEMPLATE_PATH",
                "kind": "dir",
                "ref": null,
                "subfolder": null,
                "version": "2.0.0"
            }
        ]
    }"#
    .replace("TEMPLATE_PATH", template_dir.to_str().expect("utf8"));
    std::fs::write(&index_path, index).expect("write index");

    // An installed entry that carries local credential configuration, one
    // version behind the listing so the update applies.
    let registry_path = config.path().join("truss").join("registry.json");
    std::fs::create_dir_all(registry_path.parent().expect("parent")).expect("mkdir config");
    let registry = r#"{
        "entries": {
            "private-template": {
                "name": "private-template",
                "source": "TEMPLATE_PATH",
                "kind": "dir",
                "targets": [],
                "ref": null,
                "subfolder": null,
                "file_mode": null,
                "auth_env": "MY_TOKEN",
                "ssh_key": "/home/me/.ssh/id_ed25519",
                "marketplace": true,
                "marketplace_version": "1.0.0"
            }
        }
    }"#
    .replace("TEMPLATE_PATH", template_dir.to_str().expect("utf8"));
    std::fs::write(&registry_path, registry).expect("write registry");

    let output = truss_cmd(&config)
        .env(
            "TRUSS_MARKETPLACE_INDEX",
            index_path.to_str().expect("utf8"),
        )
        .args(["marketplace", "update"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run marketplace update");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = std::fs::read_to_string(&registry_path).expect("read registry");
    assert!(
        after.contains("MY_TOKEN"),
        "auth_env must survive the update: {after}"
    );
    assert!(
        after.contains("id_ed25519"),
        "ssh_key must survive the update: {after}"
    );
    assert!(
        after.contains("2.0.0"),
        "the update must still have applied: {after}"
    );
}
