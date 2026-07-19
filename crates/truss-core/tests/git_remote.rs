use std::path::Path;
use std::process::Command;
use tempfile::tempdir;
use truss_core::git::GitUrl;
use truss_core::{GitCache, Kind, RegistryEntry};

fn git(args: &[&str], cwd: Option<&Path>) -> Result<(), String> {
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
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

fn init_bare_repo(bare: &Path, work: &Path) {
    git(
        &[
            "init",
            "--bare",
            "--initial-branch=main",
            bare.to_str().unwrap(),
        ],
        None,
    )
    .expect("init bare");
    std::fs::create_dir_all(work.join("src")).expect("mkdir");
    std::fs::write(
        work.join("Cargo.toml"),
        "[package]\nname = \"{{ project_name }}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[[bin]]\nname = \"{{ project_name }}\"\npath = \"src/main.rs\"\n",
    )
    .expect("write cargo");
    std::fs::write(work.join("src/main.rs"), "fn main() {}").expect("write main");

    git(&["init", "--initial-branch=main"], Some(work)).expect("init");
    git(&["add", "."], Some(work)).expect("add");
    git(&["commit", "-m", "initial"], Some(work)).expect("commit");
    git(&["push", bare.to_str().unwrap(), "main"], Some(work)).expect("push");
}

fn file_url(path: &Path) -> String {
    format!(
        "file://{}",
        path.canonicalize().expect("canonicalize").display()
    )
}

#[test]
fn git_cache_clones_and_resolves_default_branch() {
    let tmp = tempdir().expect("tempdir");
    let bare = tmp.path().join("remote.git");
    let work = tmp.path().join("work");
    init_bare_repo(&bare, &work);

    let cache = GitCache::with_root("remote", tmp.path().join("cache")).expect("cache");
    let url = GitUrl::parse(&file_url(&bare)).expect("parse");
    let dir = cache.resolve(&url, None, None).expect("resolve");

    assert!(dir.join("Cargo.toml").is_file());
    assert!(dir.join("src/main.rs").is_file());
    // The resolved worktree is a normal git clone (Template::from_directory
    // must skip the .git directory when loading files).
    assert!(dir.join(".git").is_dir());
}

#[test]
fn git_cache_resolves_subfolder() {
    let tmp = tempdir().expect("tempdir");
    let bare = tmp.path().join("remote.git");
    let work = tmp.path().join("work");
    std::fs::create_dir_all(work.join("templates/rust/src")).expect("mkdir");
    std::fs::write(
        work.join("templates/rust/Cargo.toml"),
        "[package]\nname = \"{{ project_name }}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    std::fs::write(work.join("templates/rust/src/lib.rs"), "pub fn lib() {}").expect("write lib");

    git(
        &[
            "init",
            "--bare",
            "--initial-branch=main",
            bare.to_str().unwrap(),
        ],
        None,
    )
    .expect("init bare");
    git(&["init", "--initial-branch=main"], Some(&work)).expect("init");
    git(&["add", "."], Some(&work)).expect("add");
    git(&["commit", "-m", "initial"], Some(&work)).expect("commit");
    git(&["push", bare.to_str().unwrap(), "main"], Some(&work)).expect("push");

    let cache = GitCache::with_root("sub", tmp.path().join("cache")).expect("cache");
    let url = GitUrl::parse(&file_url(&bare)).expect("parse");
    let dir = cache
        .resolve(&url, None, Some("templates/rust"))
        .expect("resolve");

    assert!(dir.join("Cargo.toml").is_file());
    assert!(dir.join("src/lib.rs").is_file());
}

#[test]
fn git_registry_entry_rejects_path_traversal_subfolder() {
    let tmp = tempdir().expect("tempdir");
    let bare = tmp.path().join("remote.git");
    git(&["init", "--bare", bare.to_str().unwrap()], None).expect("init bare");

    let entry = RegistryEntry {
        name: "bad".into(),
        source: file_url(&bare),
        kind: Kind::Git,
        targets: vec![],
        pointer: None,
        subfolder: Some("../escape".into()),
        file_mode: None,
    };

    assert!(entry.to_template().is_err());
}

#[test]
fn git_url_expands_shorthands() {
    let cases = [
        ("gh:truss/packs", "https://github.com/truss/packs.git"),
        ("gl:truss/packs", "https://gitlab.com/truss/packs.git"),
        ("bb:truss/packs", "https://bitbucket.org/truss/packs.git"),
        ("sr:truss/packs", "https://git.sr.ht/~truss/packs"),
        ("truss/packs", "https://github.com/truss/packs.git"),
        (
            "https://example.com/repo.git",
            "https://example.com/repo.git",
        ),
    ];

    for (input, expected) in cases {
        let url = GitUrl::parse(input).unwrap_or_else(|e| panic!("{input}: {e}"));
        assert_eq!(url.resolved, expected, "input: {input}");
    }
}

#[test]
fn git_url_rejects_invalid_shorthands() {
    assert!(GitUrl::parse("gh:").is_err());
    assert!(GitUrl::parse("gh:owner").is_err());
    assert!(GitUrl::parse("not a url").is_err());
}
