//! Path safety helpers for template IO.

use crate::error::{Error, Result};
use std::path::{Component, Path, PathBuf};

/// Normalize a user-supplied relative path and reject attempts to escape.
///
/// Accepts `foo`, `./foo`, `foo/`, and `foo/./bar`, and normalizes them to
/// `foo`, `foo/bar`, etc. Rejects absolute paths, `..`, and the root `.`.
pub fn normalize_relative_path(path: &str) -> Result<String> {
    if path.is_empty() {
        return Err(Error::Argument("template path cannot be empty".to_string()));
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err(Error::Argument(format!(
            "absolute template path rejected: {path}"
        )));
    }
    let mut parts = Vec::new();
    for component in p.components() {
        match component {
            Component::Normal(s) => parts.push(s.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(Error::Argument(format!("path traversal rejected: {path}")));
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(Error::Argument(format!(
                    "absolute template path rejected: {path}"
                )));
            }
        }
    }
    if parts.is_empty() {
        return Err(Error::Argument(format!("invalid relative path: {path}")));
    }
    Ok(parts.join("/"))
}

/// Reject absolute paths, `..`, and non-normalized relative paths.
///
/// Use `normalize_relative_path` for user input that should be cleaned up;
/// this function is for internal/template paths that must already be normalized.
pub fn validate_relative_path(path: &str) -> Result<()> {
    let normalized = normalize_relative_path(path)?;
    if normalized != path {
        return Err(Error::Argument(format!(
            "path is not normalized: expected {normalized:?}, got {path:?}"
        )));
    }
    Ok(())
}

/// Resolve `.` and `..` against the current directory without touching symlinks.
fn logical_path(path: &Path) -> Result<PathBuf> {
    let mut out = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().map_err(Error::Io)?
    };
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c.as_os_str()),
        }
    }
    Ok(out)
}

/// Ensure `child` is still under `root` after join (no breakout via `..`).
pub fn ensure_under_root(root: &Path, child: &Path) -> Result<()> {
    let root_canon = match root.canonicalize() {
        Ok(p) => p,
        Err(_) if !root.exists() => logical_path(root)?,
        Err(err) => return Err(Error::Io(err)),
    };

    let candidate = if child.exists() {
        child.canonicalize().map_err(Error::Io)?
    } else {
        logical_path(child)?
    };

    if !candidate.starts_with(&root_canon) {
        return Err(Error::Argument(format!(
            "write path escapes target directory: {}",
            child.display()
        )));
    }
    Ok(())
}

/// Return true if the path is a symlink (including a dangling link).
pub fn is_symlink(path: &Path) -> Result<bool> {
    match path.symlink_metadata() {
        Ok(meta) => Ok(meta.file_type().is_symlink()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(Error::Io(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_dir() {
        assert!(validate_relative_path("../etc/passwd").is_err());
        assert!(validate_relative_path("foo/../../bar").is_err());
    }

    #[test]
    fn rejects_absolute() {
        assert!(validate_relative_path("/etc/passwd").is_err());
    }

    #[test]
    fn accepts_normal() {
        assert!(validate_relative_path("src/main.rs").is_ok());
        assert!(validate_relative_path("crates/app/Cargo.toml").is_ok());
    }

    #[test]
    fn rejects_non_normalized_and_root() {
        assert!(validate_relative_path("./src/main.rs").is_err());
        assert!(validate_relative_path("src/main.rs/").is_err());
        assert!(validate_relative_path(".").is_err());
    }

    #[test]
    fn normalizes_user_paths() {
        assert_eq!(
            normalize_relative_path("./src/main.rs").unwrap(),
            "src/main.rs"
        );
        assert_eq!(
            normalize_relative_path("src/main.rs/").unwrap(),
            "src/main.rs"
        );
        assert_eq!(
            normalize_relative_path("src/./main.rs").unwrap(),
            "src/main.rs"
        );
        assert!(normalize_relative_path("../etc/passwd").is_err());
        assert!(normalize_relative_path("/etc/passwd").is_err());
        assert!(normalize_relative_path(".").is_err());
    }

    #[test]
    fn ensure_under_root_blocks_breakout() {
        let tmp = std::env::temp_dir();
        let child = tmp.join("foo").join("..").join("bar");
        let root = tmp.join("foo");
        assert!(ensure_under_root(&root, &child).is_err());
    }

    #[test]
    fn ensure_under_root_accepts_logical_child_for_missing_root() {
        let tmp = std::env::temp_dir();
        let root = tmp.join("not-yet-exists");
        let child = root.join("src").join("main.rs");
        assert!(ensure_under_root(&root, &child).is_ok());
    }
}
