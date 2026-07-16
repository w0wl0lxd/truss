//! Protected path lists for sync skip behavior.

use crate::error::Result;
use crate::pathsafe::validate_relative_path;
use indexmap::IndexSet;
use std::path::Path;

/// Relative project paths that sync must not overwrite.
#[derive(Debug, Clone, Default)]
pub struct ProtectList {
    paths: IndexSet<String>,
}

impl ProtectList {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a relative path after validation.
    pub fn insert(&mut self, path: impl Into<String>) -> Result<()> {
        let path = path.into();
        validate_relative_path(&path)?;
        self.paths.insert(path);
        Ok(())
    }

    /// True if `path` is protected (exact match).
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.paths.contains(path)
    }

    /// Number of protected paths.
    #[must_use]
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// Whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Iterate protected paths in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.paths.iter().map(String::as_str)
    }

    /// Build from CLI paths plus optional project `.truss/protect` file.
    pub fn load(project_root: &Path, cli_paths: &[String]) -> Result<Self> {
        let mut list = Self::new();
        for p in cli_paths {
            list.insert(p.clone())?;
        }
        let file = project_root.join(".truss").join("protect");
        if file.try_exists()? {
            let text = std::fs::read_to_string(&file)?;
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                list.insert(line.to_string())?;
            }
        }
        Ok(list)
    }

    /// Merge another list into this one.
    pub fn extend(&mut self, other: &Self) -> Result<()> {
        for p in other.iter() {
            self.insert(p.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rejects_parent_paths() {
        let mut list = ProtectList::new();
        assert!(list.insert("../x").is_err());
    }

    #[test]
    fn loads_file_and_cli() {
        let dir = tempdir().expect("temp");
        let truss = dir.path().join(".truss");
        std::fs::create_dir_all(&truss).expect("mkdir");
        std::fs::write(truss.join("protect"), "a.md\n# c\nb.md\n").expect("write");
        let list = ProtectList::load(dir.path(), &["cli.md".to_string()]).expect("load");
        assert!(list.contains("a.md"));
        assert!(list.contains("b.md"));
        assert!(list.contains("cli.md"));
        assert!(!list.contains("c"));
    }

    #[test]
    fn missing_file_ok() {
        let dir = tempdir().expect("temp");
        let list = ProtectList::load(dir.path(), &[]).expect("load");
        assert!(list.is_empty());
    }
}
