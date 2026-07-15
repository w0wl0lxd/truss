use crate::error::{Error, Result};
use rust_embed::RustEmbed;
use std::path::Path;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/templates/default"]
#[prefix = ""]
struct DefaultTemplates;

pub fn default_files() -> Result<Vec<(String, String)>> {
    let mut files = Vec::new();
    for name in DefaultTemplates::iter() {
        let file = DefaultTemplates::get(name.as_ref())
            .ok_or_else(|| Error::TemplateNotFound(name.to_string()))?;
        let bytes = file.data.into_owned();
        let content = String::from_utf8(bytes)?;
        files.push((name.to_string(), content));
    }
    Ok(files)
}

pub fn read_directory_files(dir: &Path) -> Result<Vec<(String, String)>> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(dir)
                    .map_err(|e| Error::Argument(e.to_string()))?;
                let name = rel.to_string_lossy().to_string();
                let content = std::fs::read_to_string(&path)?;
                files.push((name, content));
            }
        }
    }

    Ok(files)
}

pub struct Engine {
    env: minijinja::Environment<'static>,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            env: minijinja::Environment::new(),
        }
    }
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn render<S: serde::Serialize>(&self, source: &str, ctx: &S) -> Result<String> {
        self.env.render_str(source, ctx).map_err(Error::Template)
    }
}
