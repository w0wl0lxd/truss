use crate::error::{Error, Result};
use crate::exclude::ExcludeList;
use crate::pathsafe::{ensure_under_root, is_symlink, validate_relative_path};
use crate::protect::ProtectList;
use crate::sync::{SyncContext, project_exclude};
use crate::template::{Engine, Template};
use indexmap::IndexMap;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const BASE_DIR: &str = ".truss/base";
const CONFLICT_MARKER_OURS: &str = "<<<<<<< local";
const CONFLICT_MARKER_THEIRS: &str = "=======";
const CONFLICT_MARKER_END: &str = ">>>>>>> template";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateAction {
    Added,
    Applied,
    Unchanged,
    Removed,
    Conflict,
    SkipProtected,
}

#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub path: String,
    pub action: UpdateAction,
    pub content: Option<Vec<u8>>,
    pub mode: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum BaseSnapshot {
    /// Load the base from a directory on disk.
    Path(PathBuf),
    /// Render the named template with the current context and use it as base.
    Template(String),
}

#[derive(Debug, Clone, Default)]
pub struct UpdateOptions {
    pub dry_run: bool,
    pub write_conflicts: bool,
    pub protect: ProtectList,
    pub base: Option<BaseSnapshot>,
}

pub fn update_workspace(
    path: &Path,
    template_name: &str,
    ctx: &SyncContext,
    options: &UpdateOptions,
) -> Result<Vec<UpdateResult>> {
    let template = crate::resolve_template(template_name)?;
    update_workspace_with_template(path, &template, ctx, options)
}

pub fn update_workspace_with_template(
    path: &Path,
    template: &Template,
    ctx: &SyncContext,
    options: &UpdateOptions,
) -> Result<Vec<UpdateResult>> {
    crate::validate_prompts(template, ctx)?;
    let exclude = template.exclude.merge(&project_exclude(path)?);
    let (theirs, theirs_modes) = render_template(template, ctx)?;
    let theirs = filter_map(theirs, &exclude);
    let (base, _base_modes) = load_base(path, options.base.as_ref(), ctx)?;
    let base = filter_map(base, &exclude);
    let (local, _local_modes) = load_local_files(path)?;
    let local = filter_map(local, &exclude);

    let mut plan = Vec::new();
    let all_paths: BTreeSet<String> = base
        .keys()
        .chain(theirs.keys())
        .chain(local.keys())
        .cloned()
        .collect();

    for rel in all_paths {
        validate_relative_path(&rel)?;
        let result = merge_file(
            &rel,
            &base,
            &theirs,
            &local,
            &options.protect,
            &theirs_modes,
        );
        plan.push(result);
    }

    if !options.dry_run {
        let conflicts: Vec<String> = plan
            .iter()
            .filter(|r| r.action == UpdateAction::Conflict)
            .map(|r| r.path.clone())
            .collect();
        if !conflicts.is_empty() && !options.write_conflicts {
            return Err(Error::UpdateConflict(format!(
                "{} file(s) in conflict; use --write-conflicts to write markers: {}",
                conflicts.len(),
                conflicts.join(", ")
            )));
        }

        apply_plan(path, &plan)?;

        let skip_paths: BTreeSet<&str> = plan
            .iter()
            .filter(|r| r.action == UpdateAction::SkipProtected)
            .map(|r| r.path.as_str())
            .collect();
        let snapshot_content: IndexMap<String, Vec<u8>> = theirs
            .iter()
            .filter(|(rel, _)| !skip_paths.contains(rel.as_str()))
            .map(|(rel, bytes)| (rel.clone(), bytes.clone()))
            .collect();
        let snapshot_modes: IndexMap<String, Option<u32>> = theirs_modes
            .iter()
            .filter(|(rel, _)| !skip_paths.contains(rel.as_str()))
            .map(|(rel, mode)| (rel.clone(), *mode))
            .collect();
        write_snapshot(path, &snapshot_content, &snapshot_modes)?;
    }

    Ok(plan)
}

pub fn persist_base_snapshot(path: &Path, template: &Template, ctx: &SyncContext) -> Result<()> {
    let exclude = template.exclude.merge(&project_exclude(path)?);
    let (content, modes) = render_template(template, ctx)?;
    let rendered = filter_map(content, &exclude);
    write_snapshot(path, &rendered, &modes)
}

fn filter_map(
    map: IndexMap<String, Vec<u8>>,
    exclude: &ExcludeList,
) -> IndexMap<String, Vec<u8>> {
    map.into_iter()
        .filter(|(rel, _)| !exclude.is_excluded(rel, false))
        .collect()
}

fn render_template(
    template: &Template,
    ctx: &SyncContext,
) -> Result<(IndexMap<String, Vec<u8>>, IndexMap<String, Option<u32>>)> {
    let engine = Engine::new();
    let mut content = IndexMap::new();
    let mut modes = IndexMap::new();
    for file in template.render(ctx, &engine)? {
        content.insert(file.path.clone(), file.content.into_bytes());
        modes.insert(file.path, file.mode);
    }
    Ok((content, modes))
}

fn load_base(
    path: &Path,
    base: Option<&BaseSnapshot>,
    ctx: &SyncContext,
) -> Result<(IndexMap<String, Vec<u8>>, IndexMap<String, Option<u32>>)> {
    match base {
        Some(BaseSnapshot::Path(dir)) => load_snapshot(dir),
        Some(BaseSnapshot::Template(name)) => {
            let template = crate::resolve_template(name)?;
            render_template(&template, ctx)
        }
        None => load_snapshot(&path.join(BASE_DIR)),
    }
}

fn load_snapshot(dir: &Path) -> Result<(IndexMap<String, Vec<u8>>, IndexMap<String, Option<u32>>)> {
    let mut content = IndexMap::new();
    let modes = IndexMap::new();
    if !dir.try_exists()? {
        return Ok((content, modes));
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)? {
            let entry = entry?;
            let path = entry.path();
            if is_symlink(&path)? {
                continue;
            }
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                let rel = path
                    .strip_prefix(dir)
                    .map_err(|e| Error::Argument(e.to_string()))?;
                let rel = normalize_snapshot_path(rel);
                validate_relative_path(&rel)?;
                let bytes = std::fs::read(&path)?;
                content.insert(rel, bytes);
            }
        }
    }
    Ok((content, modes))
}

fn normalize_snapshot_path(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

fn load_local_files(
    path: &Path,
) -> Result<(IndexMap<String, Vec<u8>>, IndexMap<String, Option<u32>>)> {
    let mut content = IndexMap::new();
    let modes = IndexMap::new();
    if !path.try_exists()? {
        return Ok((content, modes));
    }
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)? {
            let entry = entry?;
            let file_path = entry.path();
            if is_symlink(&file_path)? {
                continue;
            }
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                if should_skip_dir(&file_path, path) {
                    continue;
                }
                stack.push(file_path);
            } else if file_type.is_file() {
                if should_skip_dir(&file_path, path) {
                    continue;
                }
                let rel = file_path
                    .strip_prefix(path)
                    .map_err(|e| Error::Argument(e.to_string()))?;
                let rel = normalize_snapshot_path(rel);
                validate_relative_path(&rel)?;
                let bytes = std::fs::read(&file_path)?;
                content.insert(rel, bytes);
            }
        }
    }
    Ok((content, modes))
}

fn should_skip_dir(dir: &Path, root: &Path) -> bool {
    // Skip .truss and .git at the project root only.
    if let Some(name) = dir.file_name() {
        let n = name.to_string_lossy();
        if n == ".git" || n == ".truss" {
            return dir.parent() == Some(root);
        }
    }
    false
}

fn merge_file(
    rel: &str,
    base: &IndexMap<String, Vec<u8>>,
    theirs: &IndexMap<String, Vec<u8>>,
    local: &IndexMap<String, Vec<u8>>,
    protect: &ProtectList,
    theirs_modes: &IndexMap<String, Option<u32>>,
) -> UpdateResult {
    if protect.contains(rel) {
        return UpdateResult {
            path: rel.into(),
            action: UpdateAction::SkipProtected,
            content: None,
            mode: None,
        };
    }

    let b = base.get(rel);
    let t = theirs.get(rel);
    let l = local.get(rel);
    let mode = theirs_modes.get(rel).copied().flatten();

    match (b, t, l) {
        (Some(b), Some(t), Some(l)) => {
            if b == t && t == l {
                unchanged(rel)
            } else if b == t {
                // Template unchanged, local changed.
                unchanged(rel)
            } else if b == l {
                // Local unchanged, template changed.
                applied(rel, t.clone(), mode)
            } else if t == l {
                // Both changed to the same value.
                unchanged(rel)
            } else {
                conflict(rel, b, l, t, mode)
            }
        }
        (Some(b), Some(t), None) => {
            if b == t {
                // File was in base and template, but user deleted it. Keep deleted.
                unchanged(rel)
            } else {
                // Template changed the file the user deleted.
                conflict(rel, b, &[], t, mode)
            }
        }
        (Some(b), None, Some(l)) => {
            if b == l {
                // Template removed the file and local is unchanged.
                removed(rel)
            } else {
                // Template removed the file but the user edited it.
                conflict(rel, b, l, &[], None)
            }
        }
        (Some(_b), None, None) => {
            // Template removed a file that was already removed locally.
            unchanged(rel)
        }
        (None, Some(t), Some(l)) => {
            if t == l {
                // Both added the same file.
                unchanged(rel)
            } else {
                // Local file collides with a new template file.
                conflict(rel, &[], l, t, mode)
            }
        }
        (None, Some(t), None) => added(rel, t.clone(), mode),
        (None, None, Some(_) | None) => unchanged(rel),
    }
}

fn unchanged(rel: &str) -> UpdateResult {
    UpdateResult {
        path: rel.into(),
        action: UpdateAction::Unchanged,
        content: None,
        mode: None,
    }
}

fn applied(rel: &str, content: Vec<u8>, mode: Option<u32>) -> UpdateResult {
    UpdateResult {
        path: rel.into(),
        action: UpdateAction::Applied,
        content: Some(content),
        mode,
    }
}

fn added(rel: &str, content: Vec<u8>, mode: Option<u32>) -> UpdateResult {
    UpdateResult {
        path: rel.into(),
        action: UpdateAction::Added,
        content: Some(content),
        mode,
    }
}

fn removed(rel: &str) -> UpdateResult {
    UpdateResult {
        path: rel.into(),
        action: UpdateAction::Removed,
        content: None,
        mode: None,
    }
}

fn conflict(
    rel: &str,
    _base: &[u8],
    local: &[u8],
    theirs: &[u8],
    mode: Option<u32>,
) -> UpdateResult {
    let content = if is_binary(local) || is_binary(theirs) {
        // Do not attempt textual conflict markers for binary files.
        Vec::new()
    } else {
        let mut buf = Vec::new();
        buf.extend_from_slice(CONFLICT_MARKER_OURS.as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(local);
        if !local.is_empty() && !local.ends_with(b"\n") {
            buf.push(b'\n');
        }
        buf.extend_from_slice(CONFLICT_MARKER_THEIRS.as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(theirs);
        if !theirs.is_empty() && !theirs.ends_with(b"\n") {
            buf.push(b'\n');
        }
        buf.extend_from_slice(CONFLICT_MARKER_END.as_bytes());
        buf.push(b'\n');
        buf
    };
    UpdateResult {
        path: rel.into(),
        action: UpdateAction::Conflict,
        content: Some(content),
        mode,
    }
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0)
}

fn apply_plan(path: &Path, plan: &[UpdateResult]) -> Result<()> {
    for result in plan {
        let target = path.join(&result.path);
        ensure_under_root(path, &target)?;
        match result.action {
            UpdateAction::Added | UpdateAction::Applied | UpdateAction::Conflict => {
                if let Some(content) = &result.content {
                    if !content.is_empty() {
                        if let Some(parent) = target.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(&target, content)?;
                        #[cfg(unix)]
                        if let Some(mode) = result.mode {
                            use std::os::unix::fs::PermissionsExt;
                            let perms = std::fs::Permissions::from_mode(mode);
                            std::fs::set_permissions(&target, perms)?;
                        }
                    }
                }
            }
            UpdateAction::Removed if target.try_exists()? && !target.is_dir() => {
                std::fs::remove_file(&target)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn write_snapshot(
    path: &Path,
    theirs: &IndexMap<String, Vec<u8>>,
    modes: &IndexMap<String, Option<u32>>,
) -> Result<()> {
    let snapshot_dir = path.join(BASE_DIR);
    if snapshot_dir.try_exists()? {
        std::fs::remove_dir_all(&snapshot_dir)?;
    }
    std::fs::create_dir_all(&snapshot_dir)?;
    for (rel, content) in theirs {
        validate_relative_path(rel)?;
        let target = snapshot_dir.join(rel);
        ensure_under_root(&snapshot_dir, &target)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, content)?;
        #[cfg(unix)]
        if let Some(mode) = modes.get(rel).copied().flatten() {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(mode);
            std::fs::set_permissions(&target, perms)?;
        }
    }
    Ok(())
}
