use crate::error::{Error, Result};
use crate::pathsafe::{ensure_under_root, is_symlink, validate_relative_path};
use crate::template::{Engine, Template};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncContext {
    pub project_name: String,
    pub author: String,
    pub license: String,
    pub repository: String,
    pub edition: String,
    pub extra: IndexMap<String, String>,
}

impl Default for SyncContext {
    fn default() -> Self {
        Self {
            project_name: String::new(),
            author: "owner".to_string(),
            license: "MIT".to_string(),
            repository: String::new(),
            edition: "2024".to_string(),
            extra: IndexMap::new(),
        }
    }
}

impl SyncContext {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn builder() -> Self {
        Self::new()
    }

    #[must_use]
    pub fn with_project_name(mut self, project_name: impl Into<String>) -> Self {
        self.project_name = project_name.into();
        self
    }

    #[must_use]
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }

    #[must_use]
    pub fn with_license(mut self, license: impl Into<String>) -> Self {
        self.license = license.into();
        self
    }

    #[must_use]
    pub fn with_repository(mut self, repository: impl Into<String>) -> Self {
        self.repository = repository.into();
        self
    }

    #[must_use]
    pub fn with_edition(mut self, edition: impl Into<String>) -> Self {
        self.edition = edition.into();
        self
    }

    #[must_use]
    pub fn with_extra(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    pub file: String,
    pub expected: String,
    pub actual: String,
}

pub fn sync_workspace(path: &Path, template: &Template, ctx: &SyncContext) -> Result<()> {
    let engine = Engine::new();
    let files = template.render(ctx, &engine)?;

    for file in files {
        validate_relative_path(&file.path)?;
        let file_path = path.join(&file.path);
        ensure_under_root(path, &file_path)?;
        if is_symlink(&file_path)? {
            return Err(Error::Argument(format!(
                "refusing to overwrite symlink: {}",
                file_path.display()
            )));
        }
        if let Some(parent) = file_path.parent() {
            if is_symlink(parent)? {
                return Err(Error::Argument(format!(
                    "refusing to write through symlink parent: {}",
                    parent.display()
                )));
            }
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, file.content.as_bytes())?;
        set_mode(&file_path, file.mode)?;
    }

    Ok(())
}

pub fn check_workspace(path: &Path, template: &Template, ctx: &SyncContext) -> Result<Vec<Drift>> {
    let engine = Engine::new();
    let files = template.render(ctx, &engine)?;
    let mut drifts = Vec::new();

    for file in files {
        let file_path = path.join(&file.path);
        if !file_path.try_exists()? {
            drifts.push(Drift {
                file: file.path,
                expected: file.content,
                actual: String::new(),
            });
            continue;
        }

        let actual = std::fs::read_to_string(&file_path)?;
        if actual != file.content {
            drifts.push(Drift {
                file: file.path,
                expected: file.content,
                actual,
            });
        }
    }

    Ok(drifts)
}

fn set_mode(path: &Path, mode: Option<u32>) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = match mode {
            Some(m) => m,
            None => 0o644,
        };
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(path, perms)?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }

    Ok(())
}
