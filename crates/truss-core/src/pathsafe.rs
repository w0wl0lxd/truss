//! Path safety helpers for template IO.

use crate::error::{Error, Result};
use std::path::{Component, Path};

/// Reject absolute paths and `..` components in a template-relative path.
pub fn validate_relative_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(Error::Argument("template path cannot be empty".to_string()));
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err(Error::Argument(format!(
            "absolute template path rejected: {path}"
        )));
    }
    for component in p.components() {
        match component {
            Component::ParentDir => {
                return Err(Error::Argument(format!(
                    "path traversal rejected: {path}"
                )));
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(Error::Argument(format!(
                    "absolute template path rejected: {path}"
                )));
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    Ok(())
}

/// Ensure `child` is still under `root` after join (no breakout via `..`).
pub fn ensure_under_root(root: &Path, child: &Path) -> Result<()> {
    let root_canon = match root.canonicalize() {
        Ok(p) => p,
        Err(err) => {
            // Root may not exist yet when creating a new workspace.
            if !root.exists() {
                return Ok(());
            }
            return Err(Error::Io(err));
        }
    };
    // Child may not exist yet; canonicalize parent when possible.
    let candidate = if child.exists() {
        child.canonicalize().map_err(Error::Io)?
    } else if let Some(parent) = child.parent() {
        if parent.as_os_str().is_empty() || parent == Path::new("") {
            root_canon.join(child.file_name().ok_or_else(|| {
                Error::Argument("invalid destination path".to_string())
            })?)
        } else if parent.exists() {
            let parent_c = parent.canonicalize().map_err(Error::Io)?;
            match child.file_name() {
                Some(name) => parent_c.join(name),
                None => parent_c,
            }
        } else {
            // Parent not created yet; check logical components against root.
            let rel = match child.strip_prefix(root) {
                Ok(p) => p,
                Err(_) => child,
            };
            validate_relative_path(&rel.to_string_lossy())?;
            return Ok(());
        }
    } else {
        return Err(Error::Argument("invalid destination path".to_string()));
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
}
