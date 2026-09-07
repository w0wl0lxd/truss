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
        auth_env: None,
        ssh_key: None,
        marketplace: false,
        marketplace_version: None,
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

/// The first resolve clones; every later one fetches into the existing cache.
/// That second path is the one a scaffold takes after any template has been
/// used once, so it has to work for the default ref and for a named ref.
#[test]
fn git_cache_resolves_the_same_repository_twice() {
    let tmp = tempdir().expect("tempdir");
    let bare = tmp.path().join("remote.git");
    let work = tmp.path().join("work");
    init_bare_repo(&bare, &work);
    git(&["tag", "v1"], Some(&work)).expect("tag");
    git(&["push", bare.to_str().unwrap(), "v1"], Some(&work)).expect("push tag");

    let url = GitUrl::parse(&file_url(&bare)).expect("parse");

    for pointer in [None, Some("main"), Some("v1")] {
        let root = tmp
            .path()
            .join(format!("cache-{}", pointer.unwrap_or("head")));
        let cache = GitCache::with_root("remote", &root).expect("cache");

        let first = cache.resolve(&url, pointer, None).expect("first resolve");
        assert!(first.join("Cargo.toml").is_file());

        let second = cache
            .resolve(&url, pointer, None)
            .unwrap_or_else(|e| panic!("second resolve for {pointer:?} failed: {e}"));
        assert!(second.join("Cargo.toml").is_file());
    }
}

/// `org/pack` and `org_pack` are different templates. Replacing the unsafe
/// character with `_` mapped both to one cache directory, so the second
/// template read the first one's clone.
#[test]
fn git_cache_keys_do_not_collide() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path().join("cache");

    // Two remotes whose contents differ, so a shared cache is visible.
    let bare_a = tmp.path().join("a.git");
    let work_a = tmp.path().join("work-a");
    init_bare_repo(&bare_a, &work_a);

    let bare_b = tmp.path().join("b.git");
    let work_b = tmp.path().join("work-b");
    init_bare_repo(&bare_b, &work_b);
    std::fs::write(work_b.join("MARKER-B"), "b").expect("write marker");
    git(&["add", "."], Some(&work_b)).expect("add");
    git(&["commit", "-m", "marker"], Some(&work_b)).expect("commit");
    git(&["push", bare_b.to_str().unwrap(), "main"], Some(&work_b)).expect("push");

    let slashed = GitCache::with_root("org/pack", &root).expect("cache a");
    let dir_a = slashed
        .resolve(
            &GitUrl::parse(&file_url(&bare_a)).expect("parse a"),
            None,
            None,
        )
        .expect("resolve a");

    let underscored = GitCache::with_root("org_pack", &root).expect("cache b");
    let dir_b = underscored
        .resolve(
            &GitUrl::parse(&file_url(&bare_b)).expect("parse b"),
            None,
            None,
        )
        .expect("resolve b");

    assert_ne!(
        dir_a, dir_b,
        "distinct template names must not share a cache directory"
    );
    assert!(
        !dir_a.join("MARKER-B").exists(),
        "the first template must not see the second one's clone"
    );
    assert!(
        dir_b.join("MARKER-B").is_file(),
        "the second template must get its own clone"
    );
}
