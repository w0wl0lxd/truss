pub mod error;
pub mod pathsafe;
pub mod protect;
pub mod registry;
pub mod sync;
pub mod template;

pub use error::{Error, Result};
pub use protect::ProtectList;
pub use registry::{Kind, Registry, RegistryEntry};
pub use sync::{Drift, PlanAction, PlannedWrite, SyncContext, SyncOptions};
pub use template::{Engine, Template, TemplateFile};

use std::path::Path;

pub fn new_workspace(path: &Path, template_name: &str, ctx: &SyncContext) -> Result<()> {
    let template = resolve_template(template_name)?;
    sync::sync_workspace(path, &template, ctx)?;
    Ok(())
}

pub fn new_workspace_with(
    path: &Path,
    template_name: &str,
    ctx: &SyncContext,
    options: &SyncOptions,
) -> Result<Vec<PlannedWrite>> {
    let template = resolve_template(template_name)?;
    sync::sync_workspace_with(path, &template, ctx, options)
}

pub fn sync_workspace(path: &Path, template_name: &str, ctx: &SyncContext) -> Result<()> {
    let template = resolve_template(template_name)?;
    sync::sync_workspace(path, &template, ctx)?;
    Ok(())
}

pub fn sync_workspace_with(
    path: &Path,
    template_name: &str,
    ctx: &SyncContext,
    options: &SyncOptions,
) -> Result<Vec<PlannedWrite>> {
    let template = resolve_template(template_name)?;
    sync::sync_workspace_with(path, &template, ctx, options)
}

pub fn plan_workspace(
    path: &Path,
    template_name: &str,
    ctx: &SyncContext,
    protect: &ProtectList,
) -> Result<Vec<PlannedWrite>> {
    let template = resolve_template(template_name)?;
    sync::plan_workspace(path, &template, ctx, protect)
}

pub fn check_workspace(path: &Path, template_name: &str, ctx: &SyncContext) -> Result<Vec<Drift>> {
    let template = resolve_template(template_name)?;
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
