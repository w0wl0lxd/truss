use crate::error::{Error, Result};
use globset::{GlobBuilder, GlobMatcher};
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
        // Build the list of paths to check: every ancestor directory prefix
        // (which is always a directory) followed by the full path.
        let parts: Vec<&str> = rel_path.split('/').collect();
        let mut to_check: Vec<(String, bool)> = Vec::with_capacity(parts.len());
        for i in 1..parts.len() {
            let prefix = parts.get(..i).map_or_else(String::new, |p| p.join("/"));
            to_check.push((prefix, true));
        }
        to_check.push((rel_path.to_string(), is_dir));

        for (path, path_is_dir) in to_check {
            let mut excluded = false;
            for pattern in &self.patterns {
                if pattern.is_match(&path, path_is_dir) {
                    excluded = !pattern.include;
                }
            }
            if excluded {
                return true;
            }
        }
        false
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
        // Patterns without a slash match at any level of the tree, like .gitignore.
        let has_slash = raw.contains('/') || raw.contains('\\');
        let glob_pattern = match (has_slash, dir_only) {
            (true, true) => format!("{raw}/**"),
            (true, false) => raw.clone(),
            (false, true) => format!("**/{raw}/**"),
            (false, false) => format!("**/{raw}"),
        };
        let glob = GlobBuilder::new(&glob_pattern)
            .literal_separator(true)
            .build()
            .map_err(|e| Error::Argument(format!("invalid exclude pattern {line:?}: {e}")))?;
        let matcher = glob.compile_matcher();

        let dir_name = if dir_only {
            // Strip any leading **/ we added for directory-only matching so the
            // directory name check can compare against plain relative paths.
            let plain = match raw.strip_prefix("**/") {
                Some(stripped) => stripped.to_string(),
                None => raw.clone(),
            };
            Some(plain)
        } else {
            None
        };

        Ok(Self {
            include,
            dir_name,
            matcher,
        })
    }

    fn is_match(&self, rel_path: &str, is_dir: bool) -> bool {
        if let Some(dir_name) = &self.dir_name {
            // A directory pattern matches the directory itself, a directory
            // nested elsewhere in the tree, and everything under it, but not a
            // file that happens to share the same name.
            let is_exact_or_suffix =
                rel_path == dir_name || rel_path.ends_with(&format!("/{dir_name}"));
            if is_exact_or_suffix {
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
        assert!(list.is_excluded("foo/bar.log", false));
        // A directory pattern without a slash should match at any depth.
        assert!(list.is_excluded("crates/app/target", true));
        assert!(list.is_excluded("crates/app/target/debug", false));
    }

    #[test]
    fn supports_double_star_and_unexclude() {
        let list = ExcludeList::parse("**/*.bak\n!important.bak\n").unwrap();
        assert!(list.is_excluded("a/b/backup.bak", false));
        assert!(!list.is_excluded("important.bak", false));
    }

    #[test]
    fn anchored_pattern_honors_literal_separator() {
        let list = ExcludeList::parse("data/*.tmp\n").unwrap();
        assert!(list.is_excluded("data/foo.tmp", false));
        assert!(!list.is_excluded("data/nested/foo.tmp", false));
    }

    #[test]
    fn ancestor_exclusion_skips_descendants() {
        let list = ExcludeList::parse("target/\n").unwrap();
        assert!(list.is_excluded("target/debug/foo", false));
        // Negation cannot re-include files under an excluded directory.
        let list = ExcludeList::parse("target/\n!target/debug/foo\n").unwrap();
        assert!(list.is_excluded("target/debug/foo", false));
    }

    #[test]
    fn rejects_unsafe_patterns() {
        assert!(ExcludeList::parse("../secret").is_err());
        assert!(ExcludeList::parse("/absolute").is_err());
    }
}
