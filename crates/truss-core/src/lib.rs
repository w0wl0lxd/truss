pub mod auth;
pub mod error;
pub mod extract;
pub mod git;
pub mod layout;
pub mod pathsafe;
pub mod prompt;
pub mod protect;
pub mod registry;
pub mod sync;
pub mod template;
pub mod update;
pub mod workspace;

pub use error::{Error, Result};
pub use extract::{ExtractOptions, extract_pack};
pub use git::GitCache;
pub use prompt::{Prompt, PromptCondition, PromptKind, PromptManifest};
pub use prompt::{load_answers, save_answers};
pub use protect::ProtectList;
pub use registry::{Kind, Registry, RegistryEntry};
pub use sync::{Drift, PlanAction, PlannedWrite, SyncContext, SyncOptions};
pub use template::{Engine, Template, TemplateFile, TemplateVariable, list_variables};
pub use update::{
    BaseSnapshot, UpdateAction, UpdateOptions, UpdateResult, update_workspace,
    update_workspace_with_template,
};
pub use workspace::{
    MemberKind, add_workspace_member, list_workspace_members, remove_workspace_member,
};

use std::path::Path;

pub fn new_workspace(path: &Path, template_name: &str, ctx: &SyncContext) -> Result<()> {
    ensure_new_workspace_directory(path)?;
    let template = resolve_template(template_name)?;
    validate_prompts(&template, ctx)?;
    sync::sync_workspace(path, &template, ctx)?;
    persist_prompt_answers(path, &template, ctx)?;
    update::persist_base_snapshot(path, &template, ctx)?;
    if let Some(layout) = template.layout {
        layout.apply(path, ctx)?;
    }
    Ok(())
}

pub fn new_workspace_with(
    path: &Path,
    template_name: &str,
    ctx: &SyncContext,
    options: &SyncOptions,
) -> Result<Vec<PlannedWrite>> {
    ensure_new_workspace_directory(path)?;
    let template = resolve_template(template_name)?;
    validate_prompts(&template, ctx)?;
    let plan = sync::sync_workspace_with(path, &template, ctx, options)?;
    if !options.dry_run {
        persist_prompt_answers(path, &template, ctx)?;
        update::persist_base_snapshot(path, &template, ctx)?;
    }
    if let Some(layout) = template.layout {
        if options.dry_run {
            // Layout application creates additional member crates on disk; the
            // plan returned above covers the root files only. Member-level
            // dry-run details are deferred to a later iteration.
            return Ok(plan);
        }
        layout.apply(path, ctx)?;
    }
    Ok(plan)
}

/// Refuse to scaffold into an existing non-empty directory so `truss new` cannot
/// be accidentally re-run on an already-generated workspace.
fn ensure_new_workspace_directory(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if !path.is_dir() {
        return Err(Error::Argument(format!(
            "workspace path is not a directory: {}",
            path.display()
        )));
    }
    if std::fs::read_dir(path)?.next().is_some() {
        return Err(Error::Argument(format!(
            "workspace directory is not empty: {}",
            path.display()
        )));
    }
    Ok(())
}

pub fn sync_workspace(path: &Path, template_name: &str, ctx: &SyncContext) -> Result<()> {
    let template = resolve_template(template_name)?;
    validate_prompts(&template, ctx)?;
    sync::sync_workspace(path, &template, ctx)?;
    persist_prompt_answers(path, &template, ctx)?;
    update::persist_base_snapshot(path, &template, ctx)?;
    Ok(())
}

pub fn sync_workspace_with(
    path: &Path,
    template_name: &str,
    ctx: &SyncContext,
    options: &SyncOptions,
) -> Result<Vec<PlannedWrite>> {
    let template = resolve_template(template_name)?;
    validate_prompts(&template, ctx)?;
    let plan = sync::sync_workspace_with(path, &template, ctx, options)?;
    if !options.dry_run {
        persist_prompt_answers(path, &template, ctx)?;
        update::persist_base_snapshot(path, &template, ctx)?;
    }
    Ok(plan)
}

pub fn plan_workspace(
    path: &Path,
    template_name: &str,
    ctx: &SyncContext,
    protect: &ProtectList,
) -> Result<Vec<PlannedWrite>> {
    let template = resolve_template(template_name)?;
    validate_prompts(&template, ctx)?;
    sync::plan_workspace(path, &template, ctx, protect)
}

pub fn check_workspace(path: &Path, template_name: &str, ctx: &SyncContext) -> Result<Vec<Drift>> {
    let template = resolve_template(template_name)?;
    validate_prompts(&template, ctx)?;
    sync::check_workspace(path, &template, ctx)
}

pub fn resolve_template(name: &str) -> Result<Template> {
    let registry = Registry::load()?;
    if let Some(entry) = registry.get(name) {
        return entry.to_template();
    }
    Template::load(name)
}

/// Names of embedded templates union registry keys (for listing UIs).
pub fn list_templates() -> Result<Vec<(String, String, String)>> {
    let mut out = Vec::new();
    for name in Template::list_embedded() {
        out.push((name, "embedded".to_string(), "(built-in)".to_string()));
    }
    let registry = Registry::load()?;
    for (name, entry) in registry.entries() {
        out.push((name.clone(), entry.kind.to_string(), entry.source.clone()));
    }
    Ok(out)
}

fn validate_prompts(template: &Template, ctx: &SyncContext) -> Result<()> {
    if let Some(manifest) = &template.prompt_manifest {
        manifest.validate(&ctx.extra)?;
    }
    Ok(())
}

fn persist_prompt_answers(path: &Path, template: &Template, ctx: &SyncContext) -> Result<()> {
    if template.prompt_manifest.is_some() && !ctx.extra.is_empty() {
        let answers_path = path.join(".truss/prompts.toml");
        save_answers(&answers_path, &ctx.extra)?;
    }
    Ok(())
}
