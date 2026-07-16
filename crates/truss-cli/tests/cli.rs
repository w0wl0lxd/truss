use std::process::Command;
use tempfile::tempdir;

fn truss_bin() -> std::path::PathBuf {
    env!("CARGO_BIN_EXE_truss").into()
}

#[test]
fn new_creates_workspace_noninteractive() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("myproj");

    let output = Command::new(truss_bin())
        .args([
            "new",
            "myproj",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "default",
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
    assert!(cargo.contains("owner") || cargo.contains("myproj"));
}

#[test]
fn check_passes_after_new() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("chk");

    let new = Command::new(truss_bin())
        .args([
            "new",
            "chk",
            "--path",
            path.to_str().expect("utf8 path"),
            "--template",
            "default",
        ])
        .output()
        .expect("new");
    assert!(
        new.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&new.stderr)
    );

    let check = Command::new(truss_bin())
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
