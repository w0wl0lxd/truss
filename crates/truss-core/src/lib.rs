pub mod error;
pub mod registry;
pub mod sync;
pub mod template;

pub use error::{Error, Result};
pub use registry::{Kind, Registry, RegistryEntry};
pub use sync::{Drift, SyncContext};
pub use template::{Engine, Template, TemplateFile};

use std::path::Path;

pub fn new_workspace(path: &Path, template_name: &str, ctx: &SyncContext) -> Result<()> {
    let template = resolve_template(template_name)?;
    sync::sync_workspace(path, &template, ctx)
}

pub fn sync_workspace(path: &Path, template_name: &str, ctx: &SyncContext) -> Result<()> {
    let template = resolve_template(template_name)?;
    sync::sync_workspace(path, &template, ctx)
}

pub fn check_workspace(path: &Path, template_name: &str, ctx: &SyncContext) -> Result<Vec<Drift>> {
    let template = resolve_template(template_name)?;
    sync::check_workspace(path, &template, ctx)
}

fn resolve_template(name: &str) -> Result<Template> {
    let registry = Registry::load()?;
    if let Some(entry) = registry.get(name) {
        return entry.to_template();
    }
    Template::load(name)
}
