use crate::auth::{CredentialResolver, GitCredentials, apply_credentials};
use crate::error::{Error, Result};
use crate::pathsafe::normalize_relative_path;
use crate::registry::RegistryEntry;
use std::path::{Path, PathBuf};
use std::process::Command;

const GIT: &str = "git";

/// A Git repository URL after shorthand expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitUrl {
    pub original: String,
    pub resolved: String,
}

impl GitUrl {
    /// Parse a user-provided source string and expand common Git hosting shorthands.
    ///
    /// Supported shorthands:
    /// - `gh:owner/repo` -> `https://github.com/owner/repo.git`
    /// - `gl:owner/repo` -> `https://gitlab.com/owner/repo.git`
    /// - `bb:owner/repo` -> `https://bitbucket.org/owner/repo.git`
    /// - `sr:owner/repo` -> `https://git.sr.ht/~owner/repo`
    /// - `owner/repo`      -> `https://github.com/owner/repo.git` (bare shorthand)
    ///
    /// Full URLs (`https://`, `http://`, `ssh://`, `git@...`) are passed through.
    /// `file://` URLs are allowed for local testing but must be explicit.
    pub fn parse(source: &str) -> Result<Self> {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidGitUrl("empty URL".into()));
        }

        let resolved = if let Some(rest) = trimmed.strip_prefix("gh:") {
            expand_host(rest, "https://github.com/")?
        } else if let Some(rest) = trimmed.strip_prefix("gl:") {
            expand_host(rest, "https://gitlab.com/")?
        } else if let Some(rest) = trimmed.strip_prefix("bb:") {
            expand_host(rest, "https://bitbucket.org/")?
        } else if let Some(rest) = trimmed.strip_prefix("sr:") {
            expand_sourcehut(rest)?
        } else if is_bare_shorthand(trimmed) {
            expand_host(trimmed, "https://github.com/")?
        } else if is_full_url(trimmed) {
            trimmed.to_string()
        } else {
            return Err(Error::InvalidGitUrl(format!(
                "not a valid git URL or shorthand: {trimmed}"
            )));
        };

        Ok(Self {
            original: trimmed.to_string(),
            resolved,
        })
    }
}

fn expand_host(rest: &str, prefix: &str) -> Result<String> {
    let rest = rest.trim_start_matches('/');
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() != 2 || parts.iter().any(|p| p.is_empty()) {
        return Err(Error::InvalidGitUrl(format!(
            "shorthand must be owner/repo, got {rest:?}"
        )));
    }
    let with_git = if std::path::Path::new(rest)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("git"))
    {
        rest.to_string()
    } else {
        format!("{rest}.git")
    };
    Ok(format!("{prefix}{with_git}"))
}

fn expand_sourcehut(rest: &str) -> Result<String> {
    let rest = rest.trim_start_matches('/');
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() != 2 || parts.iter().any(|p| p.is_empty()) {
        return Err(Error::InvalidGitUrl(format!(
            "sr: shorthand must be owner/repo, got {rest:?}"
        )));
    }
    Ok(format!("https://git.sr.ht/~{rest}"))
}

fn is_bare_shorthand(s: &str) -> bool {
    // owner/repo with no scheme and a single slash, no colons.
    let parts: Vec<&str> = s.split('/').collect();
    parts.len() == 2 && !s.contains(':') && !s.contains(' ') && parts.iter().all(|p| !p.is_empty())
}

fn is_full_url(s: &str) -> bool {
    s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("ssh://")
        || s.starts_with("git@")
        || s.starts_with("file://")
}

/// Local cache for one remote Git template.
#[derive(Debug, Clone)]
pub struct GitCache {
    pub key: String,
    pub repo_path: PathBuf,
}

impl GitCache {
    /// Create a cache entry keyed by the registry entry name.
    pub fn for_entry(name: &str) -> Result<Self> {
        Self::with_root(name, cache_root()?)
    }

    /// Create a cache entry with an explicit cache root (useful for tests).
    pub fn with_root(name: &str, root: impl AsRef<Path>) -> Result<Self> {
        let key = sanitize_key(name);
        Ok(Self {
            repo_path: root.as_ref().join(&key),
            key,
        })
    }

    /// Ensure the repository is cloned and checked out at the requested ref.
    /// Returns the directory that should be loaded as a `dir` template
    /// (respecting `subfolder` if present).
    pub fn resolve(
        &self,
        url: &GitUrl,
        pointer: Option<&str>,
        subfolder: Option<&str>,
    ) -> Result<PathBuf> {
        verify_git()?;

        if self.repo_path.try_exists()? {
            fetch_and_checkout(&self.repo_path, &url.resolved, pointer)?;
        } else {
            clone(&self.repo_path, &url.resolved, pointer)?;
        }

        let base = self.repo_path.clone();
        let target = if let Some(sub) = subfolder {
            let sub = normalize_relative_path(sub)?;
            base.join(&sub)
        } else {
            base
        };

        if !target.exists() {
            return Err(Error::Argument(format!(
                "template subfolder does not exist: {}",
                target.display()
            )));
        }
        if !target.is_dir() {
            return Err(Error::Argument(format!(
                "template subfolder is not a directory: {}",
                target.display()
            )));
        }

        Ok(target)
    }

    /// Resolve with authentication support for private repositories.
    pub fn resolve_with_auth(
        &self,
        url: &GitUrl,
        pointer: Option<&str>,
        subfolder: Option<&str>,
        entry: &RegistryEntry,
    ) -> Result<PathBuf> {
        verify_git()?;

        let (creds, _source) = CredentialResolver::resolve(url, entry)?;

        if self.repo_path.try_exists()? {
            fetch_and_checkout_with_auth(&self.repo_path, &url.resolved, pointer, &creds)?;
        } else {
            clone_with_auth(&self.repo_path, &url.resolved, pointer, &creds)?;
        }

        let base = self.repo_path.clone();
        let target = if let Some(sub) = subfolder {
            let sub = normalize_relative_path(sub)?;
            base.join(&sub)
        } else {
            base
        };

        if !target.exists() {
            return Err(Error::Argument(format!(
                "template subfolder does not exist: {}",
                target.display()
            )));
        }
        if !target.is_dir() {
            return Err(Error::Argument(format!(
                "template subfolder is not a directory: {}",
                target.display()
            )));
        }

        Ok(target)
    }

    /// Remove the cached repository, if it exists.
    pub fn remove(&self) -> Result<()> {
        if self.repo_path.try_exists()? {
            std::fs::remove_dir_all(&self.repo_path)?;
        }
        Ok(())
    }
}

fn cache_root() -> Result<PathBuf> {
    directories::BaseDirs::new()
        .map(|b| b.cache_dir().join("truss").join("git"))
        .ok_or(Error::ProjectDir)
}

fn sanitize_key(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_start_matches('_')
        .to_string()
}

fn verify_git() -> Result<()> {
    let output = Command::new(GIT)
        .arg("--version")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;
    if !output.status.success() {
        return Err(Error::GitNotInstalled);
    }
    Ok(())
}

fn git_base() -> Command {
    let mut cmd = Command::new(GIT);
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat");
    cmd
}

fn clone(repo_path: &Path, url: &str, pointer: Option<&str>) -> Result<()> {
    // Clone a single branch if a pointer is supplied; otherwise use the remote
    // default branch. This keeps the first clone fast and deterministic.
    let mut cmd = git_base();
    cmd.arg("clone");
    if let Some(p) = pointer {
        cmd.arg("--branch").arg(p).arg("--single-branch");
    } else {
        cmd.arg("--single-branch");
    }
    cmd.arg("--").arg(url).arg(repo_path);

    run_git(&mut cmd, "clone")
}

fn fetch_and_checkout(repo_path: &Path, _url: &str, pointer: Option<&str>) -> Result<()> {
    let mut fetch = git_base();
    fetch
        .arg("-C")
        .arg(repo_path)
        .arg("fetch")
        .arg("origin")
        .arg("--tags");
    run_git(&mut fetch, "fetch")?;

    let ref_name = match pointer {
        Some(p) => p,
        None => "origin/HEAD",
    };

    // Make the worktree match the requested ref. checkout -f ensures local
    // changes (which should never exist in a cache) do not block us.
    let mut checkout = git_base();
    checkout
        .arg("-C")
        .arg(repo_path)
        .arg("checkout")
        .arg("-f")
        .arg("--detach")
        .arg(ref_name);
    if run_git(&mut checkout, "checkout").is_ok() {
        return Ok(());
    }

    // The ref may be a tag or branch not yet fetched with --single-branch.
    // Try fetching it explicitly.
    let mut fetch_ref = git_base();
    fetch_ref
        .arg("-C")
        .arg(repo_path)
        .arg("fetch")
        .arg("origin")
        .arg(ref_name);
    run_git(&mut fetch_ref, "fetch ref")?;

    let mut checkout2 = git_base();
    checkout2
        .arg("-C")
        .arg(repo_path)
        .arg("checkout")
        .arg("-f")
        .arg("--detach")
        .arg(ref_name);
    run_git(&mut checkout2, "checkout")
}

fn run_git(cmd: &mut Command, context: &str) -> Result<()> {
    let output = cmd.output()?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(Error::Git(format!(
            "{context} failed ({}): {stderr}{stdout}",
            output.status
        )))
    }
}

fn clone_with_auth(
    repo_path: &Path,
    url: &str,
    pointer: Option<&str>,
    creds: &GitCredentials,
) -> Result<()> {
    let mut cmd = git_base();
    cmd.arg("clone");
    if let Some(p) = pointer {
        cmd.arg("--branch").arg(p).arg("--single-branch");
    } else {
        cmd.arg("--single-branch");
    }
    cmd.arg("--").arg(url).arg(repo_path);

    apply_credentials(&mut cmd, creds)?;
    run_git(&mut cmd, "clone")
}

fn fetch_and_checkout_with_auth(
    repo_path: &Path,
    _url: &str,
    pointer: Option<&str>,
    creds: &GitCredentials,
) -> Result<()> {
    let mut fetch = git_base();
    fetch
        .arg("-C")
        .arg(repo_path)
        .arg("fetch")
        .arg("origin")
        .arg("--tags");
    apply_credentials(&mut fetch, creds)?;
    run_git(&mut fetch, "fetch")?;

    let ref_name = match pointer {
        Some(p) => p,
        None => "origin/HEAD",
    };

    // Make the worktree match the requested ref. checkout -f ensures local
    // changes (which should never exist in a cache) do not block us.
    let mut checkout = git_base();
    checkout
        .arg("-C")
        .arg(repo_path)
        .arg("checkout")
        .arg("-f")
        .arg("--detach")
        .arg(ref_name);
    if run_git(&mut checkout, "checkout").is_ok() {
        return Ok(());
    }

    // The ref may be a tag or branch not yet fetched with --single-branch.
    // Try fetching it explicitly.
    let mut fetch_ref = git_base();
    fetch_ref
        .arg("-C")
        .arg(repo_path)
        .arg("fetch")
        .arg("origin")
        .arg(ref_name);
    apply_credentials(&mut fetch_ref, creds)?;
    run_git(&mut fetch_ref, "fetch ref")?;

    let mut checkout2 = git_base();
    checkout2
        .arg("-C")
        .arg(repo_path)
        .arg("checkout")
        .arg("-f")
        .arg("--detach")
        .arg(ref_name);
    run_git(&mut checkout2, "checkout")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_github_shorthand() {
        let url = GitUrl::parse("gh:truss/packs").expect("parse");
        assert_eq!(url.resolved, "https://github.com/truss/packs.git");
    }

    #[test]
    fn passes_through_https_url() {
        let url = GitUrl::parse("https://example.com/repo.git").expect("parse");
        assert_eq!(url.resolved, "https://example.com/repo.git");
    }

    #[test]
    fn rejects_invalid_url() {
        assert!(GitUrl::parse("not a url").is_err());
    }
}
