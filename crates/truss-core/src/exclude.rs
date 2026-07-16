use crate::error::{Error, Result};
use globset::{Glob, GlobMatcher};
use std::path::Path;

/// A pack-level or project-level ordered list of include/exclude glob patterns.
#[derive(Debug, Clone, Default)]
pub struct ExcludeList {
    patterns: Vec<ExcludePattern>,
}

#[derive(Debug, Clone)]
struct ExcludePattern {
    include: bool,
    dir_name: Option<String>,
    matcher: GlobMatcher,
}

impl ExcludeList {
    pub fn new() -> Self {
        Self::empty()
    }

    pub fn empty() -> Self {
        Self::default()
    }

    /// Load an exclude list from a file, returning an empty list if it does not exist.
    pub fn from_file(path: &Path) -> Result<Self> {
        if !path.try_exists()? {
            return Ok(Self::empty());
        }
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text)
    }

    /// Parse an exclude list from text (.genignore format).
    pub fn parse(text: &str) -> Result<Self> {
        let mut patterns = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            patterns.push(ExcludePattern::parse(line)?);
        }
        Ok(Self { patterns })
    }

    /// Append another list's patterns to this one so later rules override earlier ones.
    #[must_use]
    pub fn merge(&self, other: &Self) -> Self {
        let mut patterns = self.patterns.clone();
        patterns.extend(other.patterns.clone());
        Self { patterns }
    }

    /// Return true when the relative path should be skipped.
    pub fn is_excluded(&self, rel_path: &str, is_dir: bool) -> bool {
        let mut excluded = false;
        for pattern in &self.patterns {
            if pattern.is_match(rel_path, is_dir) {
                excluded = !pattern.include;
            }
        }
        excluded
    }
}

impl ExcludePattern {
    fn parse(line: &str) -> Result<Self> {
        let mut raw = line.to_string();
        let include = if let Some(stripped) = raw.strip_prefix('!') {
            raw = stripped.to_string();
            true
        } else {
            false
        };

        let dir_only = raw.ends_with('/');
        if let Some(stripped) = raw.strip_suffix('/') {
            raw = stripped.to_string();
        }

        if raw.is_empty() {
            return Err(Error::Argument("empty exclude pattern".into()));
        }
        if raw.contains("..") {
            return Err(Error::Argument(format!(
                "exclude pattern cannot contain '..': {line}"
            )));
        }
        if raw.starts_with('/') || raw.starts_with('\\') {
            return Err(Error::Argument(format!(
                "exclude pattern must be relative: {line}"
            )));
        }

        // Directory patterns match the directory and all descendants.
        let glob_pattern = if dir_only {
            format!("{raw}/**")
        } else {
            raw.clone()
        };
        let glob = Glob::new(&glob_pattern).map_err(|e| {
            Error::Argument(format!("invalid exclude pattern {line:?}: {e}"))
        })?;
        let matcher = glob.compile_matcher();

        let dir_name = if dir_only { Some(raw) } else { None };

        Ok(Self {
            include,
            dir_name,
            matcher,
        })
    }

    fn is_match(&self, rel_path: &str, is_dir: bool) -> bool {
        if let Some(dir_name) = &self.dir_name {
            // A directory pattern matches the directory itself and everything
            // under it, but not a file that happens to share the same name.
            if rel_path == dir_name {
                return is_dir;
            }
        }
        self.matcher.is_match(rel_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_files_and_directories() {
        let list = ExcludeList::parse("target/\n*.log\n").unwrap();
        assert!(list.is_excluded("target", true));
        assert!(list.is_excluded("target/debug", false));
        assert!(list.is_excluded("target/debug/foo", false));
        assert!(!list.is_excluded("target", false)); // file named target
        assert!(list.is_excluded("debug.log", false));
        assert!(list.is_excluded("debug.log", true));
        assert!(list.is_excluded("foo/bar.log", false)); // * matches across / when no literal separator
    }

    #[test]
    fn supports_double_star_and_unexclude() {
        let list = ExcludeList::parse("**/*.bak\n!important.bak\n").unwrap();
        assert!(list.is_excluded("a/b/backup.bak", false));
        assert!(!list.is_excluded("important.bak", false));
    }

    #[test]
    fn rejects_unsafe_patterns() {
        assert!(ExcludeList::parse("../secret").is_err());
        assert!(ExcludeList::parse("/absolute").is_err());
    }
}
