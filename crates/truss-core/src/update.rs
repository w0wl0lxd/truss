use crate::error::{Error, Result};
use crate::pathsafe::{ensure_under_root, is_symlink, validate_relative_path};
use crate::protect::ProtectList;
use crate::sync::SyncContext;
use crate::template::{Engine, Template};
use indexmap::IndexMap;
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
    let template = resolve_template(template_name)?;
    update_workspace_with_template(path, &template, ctx, options)
}

pub fn update_workspace_with_template(
    path: &Path,
    template: &Template,
    ctx: &SyncContext,
    options: &UpdateOptions,
) -> Result<Vec<UpdateResult>> {
    crate::validate_prompts(template, ctx)?;
    let theirs = render_template(template, ctx)?;
    let base = load_base(path, options.base.as_ref(), ctx)?;
    let local = load_local_files(path)?;

    let mut plan = Vec::new();
    let all_paths: indexmap::IndexSet<String> = base
        .keys()
        .chain(theirs.keys())
        .chain(local.keys())
        .cloned()
        .collect();

    for rel in all_paths {
        validate_relative_path(&rel)?;
        let result = merge_file(&rel, &base, &theirs, &local, &options.protect);
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
        write_snapshot(path, &theirs)?;
    }

    Ok(plan)
}

fn resolve_template(name: &str) -> Result<Template> {
    let registry = crate::registry::Registry::load()?;
    if let Some(entry) = registry.get(name) {
        return entry.to_template();
    }
    Template::load(name)
}

pub fn persist_base_snapshot(path: &Path, template: &Template, ctx: &SyncContext) -> Result<()> {
    let rendered = render_template(template, ctx)?;
    write_snapshot(path, &rendered)
}

fn render_template(template: &Template, ctx: &SyncContext) -> Result<IndexMap<String, Vec<u8>>> {
    let engine = Engine::new();
    let mut out = IndexMap::new();
    for file in template.render(ctx, &engine)? {
        out.insert(file.path, file.content.into_bytes());
    }
    Ok(out)
}

fn load_base(
    path: &Path,
    base: Option<&BaseSnapshot>,
    ctx: &SyncContext,
) -> Result<IndexMap<String, Vec<u8>>> {
    match base {
        Some(BaseSnapshot::Path(dir)) => load_snapshot(dir),
        Some(BaseSnapshot::Template(name)) => {
            let template = resolve_template(name)?;
            render_template(&template, ctx)
        }
        None => load_snapshot(&path.join(BASE_DIR)),
    }
}

fn load_snapshot(dir: &Path) -> Result<IndexMap<String, Vec<u8>>> {
    let mut out = IndexMap::new();
    if !dir.try_exists()? {
        return Ok(out);
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
                let content = std::fs::read(&path)?;
                out.insert(rel, content);
            }
        }
    }
    Ok(out)
}

fn normalize_snapshot_path(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

fn load_local_files(path: &Path) -> Result<IndexMap<String, Vec<u8>>> {
    let mut out = IndexMap::new();
    if !path.try_exists()? {
        return Ok(out);
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
                let content = std::fs::read(&file_path)?;
                out.insert(rel, content);
            }
        }
    }
    Ok(out)
}

fn should_skip_dir(dir: &Path, root: &Path) -> bool {
    // Skip .truss and .git at the project root only.
    if dir == root {
        return false;
    }
    if let Some(name) = dir.file_name() {
        let n = name.to_string_lossy();
        if n == ".git" || n == ".truss" {
            return true;
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
) -> UpdateResult {
    if protect.contains(rel) {
        return UpdateResult {
            path: rel.into(),
            action: UpdateAction::SkipProtected,
            content: None,
        };
    }

    let b = base.get(rel);
    let t = theirs.get(rel);
    let l = local.get(rel);

    match (b, t, l) {
        (Some(b), Some(t), Some(l)) => {
            if b == t && t == l {
                unchanged(rel)
            } else if b == t {
                // Template unchanged, local changed.
                unchanged(rel)
            } else if b == l {
                // Local unchanged, template changed.
                applied(rel, t.clone())
            } else if t == l {
                // Both changed to the same value.
                unchanged(rel)
            } else {
                conflict(rel, b, l, t)
            }
        }
        (Some(b), Some(t), None) => {
            if b == t {
                // File was in base and template, but user deleted it. Keep deleted.
                unchanged(rel)
            } else {
                // Template changed the file the user deleted.
                conflict(rel, b, &[], t)
            }
        }
        (Some(b), None, Some(l)) => {
            if b == l {
                // Template removed the file and local is unchanged.
                removed(rel)
            } else {
                // Template removed the file but the user edited it.
                conflict(rel, b, l, &[])
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
                conflict(rel, &[], l, t)
            }
        }
        (None, Some(t), None) => added(rel, t.clone()),
        (None, None, Some(_) | None) => unchanged(rel),
    }
}

fn unchanged(rel: &str) -> UpdateResult {
    UpdateResult {
        path: rel.into(),
        action: UpdateAction::Unchanged,
        content: None,
    }
}

fn applied(rel: &str, content: Vec<u8>) -> UpdateResult {
    UpdateResult {
        path: rel.into(),
        action: UpdateAction::Applied,
        content: Some(content),
    }
}

fn added(rel: &str, content: Vec<u8>) -> UpdateResult {
    UpdateResult {
        path: rel.into(),
        action: UpdateAction::Added,
        content: Some(content),
    }
}

fn removed(rel: &str) -> UpdateResult {
    UpdateResult {
        path: rel.into(),
        action: UpdateAction::Removed,
        content: None,
    }
}

fn conflict(rel: &str, _base: &[u8], local: &[u8], theirs: &[u8]) -> UpdateResult {
    let content = if is_binary(local) || is_binary(theirs) {
        // Do not attempt textual conflict markers for binary files.
        Vec::new()
    } else {
        let mut buf = Vec::new();
        buf.extend_from_slice(CONFLICT_MARKER_OURS.as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(local);
        buf.extend_from_slice(CONFLICT_MARKER_THEIRS.as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(theirs);
        buf.extend_from_slice(CONFLICT_MARKER_END.as_bytes());
        buf.push(b'\n');
        buf
    };
    UpdateResult {
        path: rel.into(),
        action: UpdateAction::Conflict,
        content: Some(content),
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
            UpdateAction::Added | UpdateAction::Applied => {
                if let Some(content) = &result.content {
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&target, content)?;
                }
            }
            UpdateAction::Removed => {
                if target.try_exists()? && !target.is_dir() {
                    std::fs::remove_file(&target)?;
                }
            }
            UpdateAction::Conflict => {
                if let Some(content) = &result.content {
                    if !content.is_empty() {
                        if let Some(parent) = target.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(&target, content)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn write_snapshot(path: &Path, theirs: &IndexMap<String, Vec<u8>>) -> Result<()> {
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
    }
    Ok(())
}
