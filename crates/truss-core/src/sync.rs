use crate::error::{Error, Result};
use crate::exclude::ExcludeList;
use crate::pathsafe::{ensure_under_root, is_symlink, validate_relative_path};
use crate::protect::ProtectList;
use crate::template::{Engine, Template};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::Path;
use toml_edit::{DocumentMut, Item, TableLike};

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
            author: String::new(),
            license: String::new(),
            repository: String::new(),
            edition: option_env!("CARGO_PKG_EDITION")
                .unwrap_or_else(|| "2024")
                .to_string(),
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

    pub fn from_workspace(path: &Path) -> Result<Self> {
        let cargo_path = path.join("Cargo.toml");
        if !cargo_path.try_exists()? {
            return Ok(Self::new());
        }
        let manifest = std::fs::read_to_string(cargo_path)?;
        let document = manifest.parse::<DocumentMut>()?;
        let workspace_package = document
            .get("workspace")
            .and_then(Item::as_table_like)
            .and_then(|workspace| workspace.get("package"))
            .and_then(Item::as_table_like);
        let package = document.get("package").and_then(Item::as_table_like);
        let mut context = Self::new();

        if let Some(name) = metadata_string(workspace_package, package, "name") {
            context.project_name = name;
        }
        if let Some(author) = metadata_author(workspace_package, package) {
            context.author = author;
        }
        if let Some(license) = metadata_string(workspace_package, package, "license") {
            context.license = license;
        }
        if let Some(repository) = metadata_string(workspace_package, package, "repository") {
            context.repository = repository;
        }
        if let Some(edition) = metadata_string(workspace_package, package, "edition") {
            context.edition = edition;
        }

        Ok(context)
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

    /// Return a JSON object suitable for `minijinja` where built-in context
    /// variables take precedence over custom prompt answers.
    pub fn render_context(&self) -> Result<serde_json::Value> {
        let mut value = serde_json::to_value(self).map_err(Error::Json)?;
        let map = value.as_object_mut().ok_or_else(|| {
            Error::Argument("SyncContext did not serialize to a JSON object".into())
        })?;
        let extra_entries: Vec<(String, serde_json::Value)> = map
            .get("extra")
            .and_then(|extra| extra.as_object())
            .map_or(Vec::new(), |extra_map| {
                extra_map
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            });
        for (k, v) in extra_entries {
            map.entry(k).or_insert(v);
        }
        Ok(value)
    }
}

fn metadata_string(
    workspace: Option<&dyn TableLike>,
    package: Option<&dyn TableLike>,
    key: &str,
) -> Option<String> {
    table_string(workspace, key).or_else(|| table_string(package, key))
}

fn metadata_author(
    workspace: Option<&dyn TableLike>,
    package: Option<&dyn TableLike>,
) -> Option<String> {
    table_author(workspace).or_else(|| table_author(package))
}

fn table_string(table: Option<&dyn TableLike>, key: &str) -> Option<String> {
    table?.get(key).and_then(Item::as_str).map(str::to_string)
}

fn table_author(table: Option<&dyn TableLike>) -> Option<String> {
    table?
        .get("authors")
        .and_then(Item::as_array)
        .and_then(|authors| authors.get(0))
        .and_then(|author| author.as_str())
        .map(str::to_string)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    pub file: String,
    pub expected: String,
    pub actual: String,
}

/// Action planned for a template destination file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanAction {
    WouldWrite,
    Unchanged,
    SkipProtected,
}

/// One planned sync operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedWrite {
    pub path: String,
    pub action: PlanAction,
}

/// Options controlling sync/write behavior.
#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    pub protect: ProtectList,
    pub dry_run: bool,
}

pub fn plan_workspace(
    path: &Path,
    template: &Template,
    ctx: &SyncContext,
    options: &SyncOptions,
    exclude: &ExcludeList,
) -> Result<Vec<PlannedWrite>> {
    let engine = Engine::new();
    let files = template.render(ctx, &engine)?;
    let mut plan = Vec::with_capacity(files.len());

    for file in files {
        validate_relative_path(&file.path)?;
        if exclude.is_excluded(&file.path, false) {
            continue;
        }
        if options.protect.contains(&file.path) {
            plan.push(PlannedWrite {
                path: file.path,
                action: PlanAction::SkipProtected,
            });
            continue;
        }
        let file_path = path.join(&file.path);
        if has_symlink_in_path(&file_path, path)? {
            return Err(Error::Argument(format!(
                "refusing to follow symlink: {}",
                file_path.display()
            )));
        }
        let action = if file_path.try_exists()? {
            if file_path.is_dir() {
                return Err(Error::Argument(format!(
                    "cannot overwrite directory with file: {}",
                    file_path.display()
                )));
            }
            let actual = std::fs::read_to_string(&file_path)?;
            if actual == file.content {
                PlanAction::Unchanged
            } else {
                PlanAction::WouldWrite
            }
        } else {
            PlanAction::WouldWrite
        };
        plan.push(PlannedWrite {
            path: file.path,
            action,
        });
    }
    Ok(plan)
}

pub fn sync_workspace(path: &Path, template: &Template, ctx: &SyncContext) -> Result<()> {
    let _ = sync_workspace_with(path, template, ctx, &SyncOptions::default())?;
    Ok(())
}

pub(crate) fn project_exclude(path: &Path) -> Result<ExcludeList> {
    let exclude_file = path.join(".truss/exclude");
    ExcludeList::from_file(&exclude_file)
}

pub fn sync_workspace_with(
    path: &Path,
    template: &Template,
    ctx: &SyncContext,
    options: &SyncOptions,
) -> Result<Vec<PlannedWrite>> {
    let exclude = template.exclude.merge(&project_exclude(path)?);
    let plan = plan_workspace(path, template, ctx, options, &exclude)?;
    if options.dry_run {
        return Ok(plan);
    }

    let engine = Engine::new();
    let files: Vec<_> = template
        .render(ctx, &engine)?
        .into_iter()
        .filter(|f| !exclude.is_excluded(&f.path, false))
        .collect();

    for (file, item) in files.iter().zip(plan.iter()) {
        validate_relative_path(&file.path)?;
        if item.action != PlanAction::WouldWrite {
            continue;
        }
        let file_path = path.join(&file.path);
        ensure_under_root(path, &file_path)?;
        if has_symlink_in_path(&file_path, path)? {
            return Err(Error::Argument(format!(
                "refusing to write through symlink: {}",
                file_path.display()
            )));
        }
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, file.content.as_bytes())?;
        set_mode(&file_path, file.mode)?;
    }

    Ok(plan)
}

pub fn check_workspace(path: &Path, template: &Template, ctx: &SyncContext) -> Result<Vec<Drift>> {
    let exclude = template.exclude.merge(&project_exclude(path)?);
    let engine = Engine::new();
    let files: Vec<_> = template
        .render(ctx, &engine)?
        .into_iter()
        .filter(|f| !exclude.is_excluded(&f.path, false))
        .collect();
    let mut drifts = Vec::new();

    for file in files {
        let file_path = path.join(&file.path);
        if has_symlink_in_path(&file_path, path)? {
            return Err(Error::Argument(format!(
                "refusing to follow symlink: {}",
                file_path.display()
            )));
        }
        if !file_path.try_exists()? {
            drifts.push(Drift {
                file: file.path,
                expected: file.content,
                actual: String::new(),
            });
            continue;
        }
        if file_path.is_dir() {
            return Err(Error::Argument(format!(
                "cannot overwrite directory with file: {}",
                file_path.display()
            )));
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

/// True if `path` or any of its existing ancestors below `root` is a symlink.
fn has_symlink_in_path(path: &Path, root: &Path) -> Result<bool> {
    let mut current = Some(path);
    while let Some(p) = current {
        if p.as_os_str().is_empty() || p == root {
            break;
        }
        if is_symlink(p)? {
            return Ok(true);
        }
        current = p.parent();
    }
    Ok(false)
}

fn set_mode(path: &Path, mode: Option<u32>) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = match mode {
            Some(m) => m & 0o777,
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
