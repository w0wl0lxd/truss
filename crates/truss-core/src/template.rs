use crate::error::{Error, Result};
use crate::exclude::ExcludeList;
use crate::hooks::HookManifest;
use crate::layout::Layout;
use crate::pathsafe::validate_relative_path;
use crate::prompt::PromptManifest;
use crate::sync::SyncContext;
use indexmap::IndexSet;
use rust_embed::RustEmbed;
use serde::Serialize;
use std::path::Path;
use toml_edit::{Array, DocumentMut, Item, value};

/// Instruction fuel budget per template render (DoS guard).
const TEMPLATE_FUEL: u64 = 50_000;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/templates"]
#[prefix = ""]
struct DefaultTemplates;

#[derive(Debug, Clone)]
pub struct Template {
    pub name: String,
    pub files: Vec<TemplateFile>,
    pub layout: Option<Layout>,
    pub prompt_manifest: Option<PromptManifest>,
    pub hooks: Option<HookManifest>,
    pub exclude: ExcludeList,
}

#[derive(Debug, Clone)]
pub struct TemplateFile {
    pub path: String,
    pub content: String,
    pub mode: Option<u32>,
}

impl Template {
    pub fn new(name: impl Into<String>, files: Vec<TemplateFile>) -> Self {
        Self {
            name: name.into(),
            files,
            layout: None,
            prompt_manifest: None,
            hooks: None,
            exclude: ExcludeList::empty(),
        }
    }

    pub fn list_embedded() -> Vec<String> {
        let mut names = IndexSet::new();

        for path in DefaultTemplates::iter() {
            if let Some(name) = path.split('/').next() {
                names.insert(name.to_string());
            }
        }

        Vec::from_iter(names)
    }

    pub fn load(name: &str) -> Result<Self> {
        let prefix = format!("{name}/");

        // First pass: collect manifest files and the full file list so project-
        // local un-excludes can override pack-level excludes later at sync time.
        let mut prompt_manifest = None;
        let mut hooks = None;
        let mut exclude = ExcludeList::empty();
        let mut all_paths = Vec::new();
        for path in DefaultTemplates::iter() {
            if let Some(rel) = path.strip_prefix(prefix.as_str()) {
                if rel == "truss.toml" {
                    let file = DefaultTemplates::get(path.as_ref())
                        .ok_or_else(|| Error::TemplateNotFound(path.to_string()))?;
                    let content = String::from_utf8(file.data.into_owned())?;
                    prompt_manifest = Some(PromptManifest::from_toml(&content)?);
                    hooks = Some(HookManifest::from_toml(&content)?);
                    continue;
                }
                if rel == ".genignore" {
                    let file = DefaultTemplates::get(path.as_ref())
                        .ok_or_else(|| Error::TemplateNotFound(path.to_string()))?;
                    let content = String::from_utf8(file.data.into_owned())?;
                    exclude = ExcludeList::parse(&content)?;
                    continue;
                }
                all_paths.push(path.to_string());
            }
        }

        let mut files = Vec::new();
        for path in all_paths {
            let rel = path
                .strip_prefix(prefix.as_str())
                .ok_or_else(|| Error::TemplateNotFound(path.clone()))?;
            validate_relative_path(rel)?;
            let file = DefaultTemplates::get(path.as_ref())
                .ok_or_else(|| Error::TemplateNotFound(path.clone()))?;
            let bytes = file.data.into_owned();
            let content = String::from_utf8(bytes)?;
            files.push(TemplateFile {
                path: rel.to_string(),
                content,
                mode: None,
            });
        }

        if files.is_empty() {
            return Err(Error::TemplateNotFound(name.to_string()));
        }

        let (files, layout) = extract_layout(files)?;
        Ok(Self {
            name: name.to_string(),
            files,
            layout,
            prompt_manifest,
            hooks,
            exclude,
        })
    }

    pub fn from_directory(dir: &Path) -> Result<Self> {
        let name = dir
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().to_string());
        let manifest_path = dir.join("truss.toml");
        let (prompt_manifest, hooks) = if manifest_path.try_exists()? {
            let content = std::fs::read_to_string(&manifest_path)?;
            (
                Some(PromptManifest::from_toml(&content)?),
                Some(HookManifest::from_toml(&content)?),
            )
        } else {
            (None, None)
        };
        let exclude = ExcludeList::from_file(&dir.join(".genignore"))?;
        let genignore_path = dir.join(".genignore");
        let mut files = Vec::new();
        let mut stack = vec![dir.to_path_buf()];

        while let Some(current) = stack.pop() {
            for entry in std::fs::read_dir(&current)? {
                let entry = entry?;
                let path = entry.path();
                let file_type = entry.file_type()?;

                // Never follow or read symlinks from untrusted template packs.
                if file_type.is_symlink() {
                    continue;
                }

                let rel = normalize_path_sep(
                    path.strip_prefix(dir)
                        .map_err(|e| Error::Argument(e.to_string()))?,
                );
                validate_relative_path(&rel)?;

                if file_type.is_dir() {
                    if path.file_name().is_some_and(|n| n == ".git") {
                        continue;
                    }
                    stack.push(path);
                } else if file_type.is_file() {
                    if path == manifest_path || path == genignore_path {
                        continue;
                    }
                    let content = std::fs::read_to_string(&path)?;
                    let mode = file_mode(&path)?;
                    files.push(TemplateFile {
                        path: rel,
                        content,
                        mode,
                    });
                }
            }
        }

        let (files, layout) = extract_layout(files)?;
        Ok(Self {
            name,
            files,
            layout,
            prompt_manifest,
            hooks,
            exclude,
        })
    }

    pub fn render(&self, ctx: &SyncContext, engine: &Engine) -> Result<Vec<TemplateFile>> {
        let mut rendered = Vec::with_capacity(self.files.len());
        let ctx_value = ctx.render_context()?;

        for file in &self.files {
            validate_relative_path(&file.path)?;
            let path = if is_templated(&file.path) {
                engine.render_str(&file.path, &ctx_value)?
            } else {
                file.path.clone()
            };
            let content = if is_templated(&file.content) {
                engine.render_str(&file.content, &ctx_value)?
            } else {
                file.content.clone()
            };

            rendered.push(TemplateFile {
                path,
                content,
                mode: file.mode,
            });
        }

        if let Some(layout) = &self.layout {
            inject_layout_members(&mut rendered, layout)?;
        }

        Ok(rendered)
    }
}

pub struct Engine {
    env: minijinja::Environment<'static>,
}

impl Default for Engine {
    fn default() -> Self {
        let mut env = minijinja::Environment::new();
        // Cap instruction budget so malicious templates cannot hang the process.
        env.set_fuel(Some(TEMPLATE_FUEL));
        Self { env }
    }
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_template(&mut self, name: &str, source: &str) -> Result<()> {
        self.env
            .add_template_owned(name.to_string(), source.to_string())
            .map_err(Error::Template)
    }

    pub fn render_str<S: Serialize>(&self, source: &str, ctx: S) -> Result<String> {
        self.env.render_str(source, ctx).map_err(Error::Template)
    }
}

fn is_templated(content: &str) -> bool {
    content.contains("{{") || content.contains("{%") || content.contains("{#")
}

fn extract_layout(mut files: Vec<TemplateFile>) -> Result<(Vec<TemplateFile>, Option<Layout>)> {
    if let Some(index) = files.iter().position(|f| f.path == "layout.toml") {
        let layout_file = files.swap_remove(index);
        let layout = Layout::parse(&layout_file.content)?;
        let paths = layout.member_paths()?;
        let prefixes: Vec<String> = paths.values().cloned().collect();
        files.retain(|f| !is_under_member_path(&f.path, &prefixes));
        return Ok((files, Some(layout)));
    }
    Ok((files, None))
}

/// Return true when `file_path` is exactly a member directory or lives inside one.
/// Member directory paths are normalized and use `/` as the separator.
fn is_under_member_path(file_path: &str, prefixes: &[String]) -> bool {
    for prefix in prefixes {
        let prefix = prefix.trim_end_matches('/');
        if file_path == prefix {
            return true;
        }
        if let Some(rest) = file_path.strip_prefix(prefix) {
            if rest.starts_with('/') {
                return true;
            }
        }
    }
    false
}

/// For templates that declare a layout, inject the computed `workspace.members`
/// list into the rendered root `Cargo.toml`. This lets `sync` and `check` treat
/// the generated workspace as matching the descriptor.
fn inject_layout_members(files: &mut [TemplateFile], layout: &Layout) -> Result<()> {
    let paths = layout.member_paths()?;
    if paths.is_empty() {
        return Ok(());
    }

    let Some(root) = files.iter_mut().find(|f| f.path == "Cargo.toml") else {
        return Ok(());
    };

    let mut document = root.content.parse::<DocumentMut>().map_err(Error::Toml)?;
    let workspace = document
        .get_mut("workspace")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| {
            Error::Argument(
                "template Cargo.toml has no [workspace] table for layout members".into(),
            )
        })?;

    let mut members = Array::new();
    for path in paths.values() {
        members.push(path.as_str());
    }
    workspace["members"] = value(members);
    root.content = document.to_string();

    Ok(())
}

/// User-facing description of a variable expected by a template pack.
#[derive(Debug, Clone)]
pub struct TemplateVariable {
    pub name: String,
    pub required: bool,
    pub default: Option<String>,
    pub description: String,
    pub kind: String,
}

/// List the built-in and custom variables that a pack requires.
pub fn list_variables(
    template: &Template,
    default_author: &str,
    default_edition: &str,
) -> Vec<TemplateVariable> {
    let mut out = vec![
        TemplateVariable {
            name: "project_name".into(),
            required: true,
            default: None,
            description: "Project name".into(),
            kind: "text".into(),
        },
        TemplateVariable {
            name: "author".into(),
            required: false,
            default: Some(default_author.into()),
            description: "Project author".into(),
            kind: "text".into(),
        },
        TemplateVariable {
            name: "license".into(),
            required: false,
            default: None,
            description: "Project license".into(),
            kind: "text".into(),
        },
        TemplateVariable {
            name: "edition".into(),
            required: false,
            default: Some(default_edition.into()),
            description: "Rust edition".into(),
            kind: "text".into(),
        },
        TemplateVariable {
            name: "repository".into(),
            required: false,
            default: None,
            description: "Project repository URL".into(),
            kind: "text".into(),
        },
    ];

    if let Some(manifest) = &template.prompt_manifest {
        for prompt in &manifest.prompts {
            let default = prompt.default.clone();
            let required = prompt.required && default.is_none();
            let kind = match prompt.kind {
                crate::prompt::PromptKind::Text => "text",
                crate::prompt::PromptKind::Choice => "choice",
                crate::prompt::PromptKind::Bool => "bool",
            };
            out.push(TemplateVariable {
                name: prompt.name.clone(),
                required,
                default,
                description: prompt.label.clone(),
                kind: kind.into(),
            });
        }
    }

    out
}

fn normalize_path_sep(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

fn file_mode(path: &Path) -> Result<Option<u32>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let meta = std::fs::metadata(path)?;
        let mode = meta.permissions().mode() & 0o777;
        Ok(Some(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn from_directory_filters_files_under_layout_member_paths() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("Cargo.toml"), "[workspace]").expect("write root cargo");
        std::fs::write(
            dir.path().join("layout.toml"),
            r#"
[[members]]
name = "app"
kind = "bin"
path = "apps/app"
"#,
        )
        .expect("write layout");
        std::fs::create_dir_all(dir.path().join("apps/app")).expect("mkdir");
        std::fs::write(dir.path().join("apps/app/Cargo.toml"), "should be filtered")
            .expect("write member cargo");
        std::fs::write(dir.path().join("README.md"), "kept").expect("write readme");

        let template = Template::from_directory(dir.path()).expect("load template");
        let paths: Vec<&str> = template.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"Cargo.toml"));
        assert!(paths.contains(&"README.md"));
        assert!(!paths.contains(&"apps/app/Cargo.toml"));
        assert!(!paths.contains(&"layout.toml"));
    }
}
