use crate::error::{Error, Result};
use crate::pathsafe::validate_relative_path;
use crate::sync::SyncContext;
use indexmap::IndexSet;
use rust_embed::RustEmbed;
use serde::Serialize;
use std::path::Path;

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
        let mut files = Vec::new();
        let prefix = format!("{name}/");

        for path in DefaultTemplates::iter() {
            if let Some(rel) = path.strip_prefix(prefix.as_str()) {
                validate_relative_path(rel)?;
                let file = DefaultTemplates::get(path.as_ref())
                    .ok_or_else(|| Error::TemplateNotFound(path.to_string()))?;
                let bytes = file.data.into_owned();
                let content = String::from_utf8(bytes)?;
                files.push(TemplateFile {
                    path: rel.to_string(),
                    content,
                    mode: None,
                });
            }
        }

        if files.is_empty() {
            return Err(Error::TemplateNotFound(name.to_string()));
        }

        Ok(Self::new(name, files))
    }

    pub fn from_directory(dir: &Path) -> Result<Self> {
        let name = dir
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().to_string());
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

                if file_type.is_dir() {
                    stack.push(path);
                } else if file_type.is_file() {
                    let rel = path
                        .strip_prefix(dir)
                        .map_err(|e| Error::Argument(e.to_string()))?;
                    let rel = rel.to_string_lossy().to_string();
                    validate_relative_path(&rel)?;
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

        Ok(Self::new(name, files))
    }

    pub fn render(&self, ctx: &SyncContext, engine: &Engine) -> Result<Vec<TemplateFile>> {
        let mut rendered = Vec::with_capacity(self.files.len());

        for file in &self.files {
            validate_relative_path(&file.path)?;
            let content = if is_templated(&file.content) {
                engine.render_str(&file.content, ctx)?
            } else {
                file.content.clone()
            };

            rendered.push(TemplateFile {
                path: file.path.clone(),
                content,
                mode: file.mode,
            });
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
