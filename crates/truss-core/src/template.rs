use crate::error::{Error, Result};
use crate::exclude::ExcludeList;
use crate::hooks::HookManifest;
use crate::layout::Layout;
use crate::pack_manifest::PackManifest;
use crate::pathsafe::validate_relative_path;
use crate::prompt::PromptManifest;
use crate::sync::SyncContext;
use indexmap::IndexMap;
use indexmap::IndexSet;
use rust_embed::RustEmbed;
use serde::Serialize;
use std::path::Path;
use toml_edit::{Array, DocumentMut, Item, value};

/// Instruction fuel budget per template render (DoS guard).
/// File name that marks a directory as a JSON-described pack.
pub const PACK_MANIFEST_FILE: &str = "truss-pack.json";

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
    pub pack_manifest: Option<PackManifest>,
}

#[derive(Debug, Clone)]
pub struct TemplateFile {
    pub path: String,
    pub content: Content,
    pub mode: Option<u32>,
}

/// A template file's body.
///
/// A mapping marked `is_template: false` promises a byte-for-byte copy, and a
/// pack may legitimately carry an asset that is not UTF-8. Text is kept as text
/// so it can be rendered and compared; anything else is kept as raw bytes and
/// copied through untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Text(String),
    Bytes(Vec<u8>),
}

impl Content {
    /// Text if the body is UTF-8, raw bytes otherwise.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        match String::from_utf8(bytes) {
            Ok(text) => Self::Text(text),
            Err(e) => Self::Bytes(e.into_bytes()),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Bytes(_) => None,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Bytes(bytes) => bytes,
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::Text(text) => text.into_bytes(),
            Self::Bytes(bytes) => bytes,
        }
    }

    /// A form safe to print in a drift report.
    pub fn to_display_string(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Bytes(bytes) => format!("<binary, {} bytes>", bytes.len()),
        }
    }
}

impl From<String> for Content {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
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
            pack_manifest: None,
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
                    if let Ok(hook_manifest) = HookManifest::from_toml(&content) {
                        hooks = Some(hook_manifest);
                    }
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
                content: Content::Text(content),
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
            pack_manifest: None,
        })
    }

    pub fn from_directory(dir: &Path) -> Result<Self> {
        let name = dir
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().to_string());

        // A pack that ships a JSON manifest is described by it: the manifest
        // decides which sources map to which destinations, so loading the
        // directory verbatim would copy the pack's own layout instead.
        let pack_manifest_path = dir.join(PACK_MANIFEST_FILE);
        if pack_manifest_path.try_exists()? {
            let mut template = Self::from_manifest(&pack_manifest_path, dir, &IndexMap::new())?;
            template.name = name;
            return Ok(template);
        }

        let manifest_path = dir.join("truss.toml");
        let (prompt_manifest, hooks) = if manifest_path.try_exists()? {
            let content = std::fs::read_to_string(&manifest_path)?;
            let prompt = PromptManifest::from_toml(&content).ok();
            let hook = HookManifest::from_toml(&content).ok();
            (prompt, hook)
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
                    // A directory template has no per-file mapping, so the
                    // bytes decide: text is rendered, anything else is copied.
                    let content = Content::from_bytes(std::fs::read(&path)?);
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
            pack_manifest: None,
        })
    }

    /// Load a template from a JSON manifest with given variable values.
    ///
    /// Callers normally reach this through [`Template::from_directory`], which
    /// detects `truss-pack.json` automatically.
    pub fn from_manifest(
        manifest_path: &Path,
        pack_dir: &Path,
        values: &IndexMap<String, String>,
    ) -> Result<Self> {
        let manifest = PackManifest::from_path(manifest_path)?;
        // Validate values against manifest type constraints (only if values are provided)
        if !values.is_empty() {
            manifest.validate_values(values)?;
        }
        let mut template = manifest.to_template(pack_dir)?;

        // Convert manifest variables to a PromptManifest for compatibility
        let mut prompts = Vec::with_capacity(manifest.variables.len());
        for var in &manifest.variables {
            let kind = match var.var_type {
                crate::pack_manifest::VariableType::String
                | crate::pack_manifest::VariableType::Integer => crate::prompt::PromptKind::Text,
                crate::pack_manifest::VariableType::Bool => crate::prompt::PromptKind::Bool,
            };
            prompts.push(crate::prompt::Prompt {
                name: var.name.clone(),
                label: var.description.clone().unwrap_or_else(|| var.name.clone()),
                kind,
                default: var.default.as_ref().and_then(|d| match d {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    serde_json::Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                }),
                choices: var.choices.clone(),
                regex: var.regex.clone(),
                required: var.required,
                condition: None,
            });
        }

        // Store the manifest for later validation
        template.pack_manifest = Some(manifest);

        if !prompts.is_empty() {
            template.prompt_manifest = Some(crate::prompt::PromptManifest { prompts });
        }

        // Load existing truss.toml for hooks if present
        // Match the directory loader: a truss.toml that carries no [hooks] table
        // is not an error, it just means the pack declares no hooks.
        let toml_path = pack_dir.join("truss.toml");
        if toml_path.try_exists()? {
            let content = std::fs::read_to_string(&toml_path)?;
            template.hooks = HookManifest::from_toml(&content).ok();
        }

        // Load .genignore if present
        template.exclude = ExcludeList::from_file(&pack_dir.join(".genignore"))?;

        Ok(template)
    }

    pub fn render(&self, ctx: &SyncContext, engine: &Engine) -> Result<Vec<TemplateFile>> {
        let mut rendered = Vec::with_capacity(self.files.len());
        // Rendered destination -> the source template that claimed it.
        let mut destinations: IndexMap<String, String> = IndexMap::new();
        let mut ctx_value = ctx.render_context()?;

        // For manifest-based packs, re-evaluate conditions against the real
        // context. Each file is owned by exactly one mapping -- the one with the
        // longest matching destination -- so overlapping mappings can neither
        // duplicate a file nor resurrect one its own mapping excluded.
        let files_to_render: Vec<(&TemplateFile, bool)> =
            if let Some(pack_manifest) = &self.pack_manifest {
                // Validate the answers that will actually be rendered. `from_manifest`
                // is normally given an empty map, so this is the only point at which
                // types, regexes, choices and `required` are checked against real
                // values.
                pack_manifest.validate_values(&ctx.extra)?;

                let base = ctx_value.as_object().ok_or_else(|| {
                    Error::Argument("render context did not serialize to a JSON object".into())
                })?;
                // Layer in the manifest defaults, so a file body sees the same
                // value a condition selected it with. A library caller that
                // omits an optional answer otherwise gets a file chosen by its
                // default and rendered with the variable undefined.
                ctx_value = serde_json::Value::Object(pack_manifest.resolve_context(base)?);
                let base = ctx_value.as_object().ok_or_else(|| {
                    Error::Argument("render context did not serialize to a JSON object".into())
                })?;

                let mut selected = Vec::with_capacity(self.files.len());
                for file in &self.files {
                    let Some(mapping) = pack_manifest.mapping_for(&file.path) else {
                        continue;
                    };
                    if let Some(condition) = &mapping.condition {
                        if !pack_manifest.eval_condition(condition, base, engine)? {
                            continue;
                        }
                    }
                    selected.push((file, mapping.is_template));
                }
                selected
            } else {
                self.files.iter().map(|f| (f, true)).collect()
            };

        for (file, is_template) in files_to_render {
            validate_relative_path(&file.path)?;
            // The destination is always rendered: it is how a pack parameterizes
            // file names. Only the body honours `is_template`.
            let path = if is_templated(&file.path) {
                let rendered = engine.render_str(&file.path, &ctx_value)?;
                validate_relative_path(&rendered)?;
                rendered
            } else {
                file.path.clone()
            };
            // Only text can be rendered; a literal asset is copied through.
            let content = match file.content.as_str() {
                Some(text) if is_template && is_templated(text) => {
                    Content::Text(engine.render_str(text, &ctx_value)?)
                }
                _ => file.content.clone(),
            };

            // Two destinations that render to one path would both be written,
            // and whichever landed last would silently win.
            if let Some(first) = destinations.get(&path) {
                return Err(Error::Argument(format!(
                    "template files '{first}' and '{}' both render to the destination '{path}'",
                    file.path
                )));
            }
            destinations.insert(path.clone(), file.path.clone());

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

    /// Compile `source` without rendering it, to report syntax errors early.
    pub fn check_syntax(&self, source: &str) -> Result<()> {
        self.env
            .template_from_str(source)
            .map(|_| ())
            .map_err(Error::Template)
    }
}

fn is_templated(content: &str) -> bool {
    content.contains("{{") || content.contains("{%") || content.contains("{#")
}

fn extract_layout(mut files: Vec<TemplateFile>) -> Result<(Vec<TemplateFile>, Option<Layout>)> {
    if let Some(index) = files.iter().position(|f| f.path == "layout.toml") {
        let layout_file = files.swap_remove(index);
        let layout_source = layout_file
            .content
            .as_str()
            .ok_or_else(|| Error::Argument("layout file is not valid UTF-8".into()))?;
        let layout = Layout::parse(layout_source)?;
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

    let root_source = root
        .content
        .as_str()
        .ok_or_else(|| Error::Argument("workspace Cargo.toml is not valid UTF-8".into()))?;
    let mut document = root_source.parse::<DocumentMut>().map_err(Error::Toml)?;
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
    root.content = Content::Text(document.to_string());

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

pub(crate) fn file_mode(path: &Path) -> Result<Option<u32>> {
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
