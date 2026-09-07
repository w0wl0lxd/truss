use crate::error::{Error, Result};
use crate::pathsafe::{ensure_under_root, validate_relative_path};
use crate::template::{Template, TemplateFile};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// JSON manifest describing a template pack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackManifest {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Who wrote the pack. Metadata only: it never enters the render context,
    /// where `author` names the author of the generated project.
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub variables: Vec<ManifestVariable>,
    #[serde(default)]
    pub files: Vec<FileMapping>,
}

/// A variable declaration in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestVariable {
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: VariableType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub regex: Option<String>,
    #[serde(default)]
    pub choices: Vec<String>,
}

/// Supported variable types.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VariableType {
    String,
    Integer,
    Bool,
}

/// A file mapping from source in the pack to destination in the generated project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMapping {
    pub source: String,
    pub destination: String,
    #[serde(default)]
    pub condition: Option<String>,
    /// Render the file through the template engine. Set `false` for assets that
    /// must be copied byte-for-byte.
    #[serde(default = "default_true")]
    pub is_template: bool,
}

fn default_true() -> bool {
    true
}

impl PackManifest {
    /// Load a manifest from a JSON file.
    pub fn from_path(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content)
    }

    /// Parse a manifest from a JSON string.
    pub fn from_json(json: &str) -> Result<Self> {
        let manifest: Self = serde_json::from_str(json)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate the manifest structure and references.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(Error::Validation("manifest name cannot be empty".into()));
        }

        let mut var_names = indexmap::IndexSet::new();
        for var in &self.variables {
            if !var_names.insert(var.name.clone()) {
                return Err(Error::Validation(format!(
                    "duplicate variable name: {}",
                    var.name
                )));
            }
            var.validate()?;
        }

        let mut dest_paths = indexmap::IndexSet::new();
        for mapping in &self.files {
            validate_relative_path(&mapping.destination)?;
            if !dest_paths.insert(mapping.destination.clone()) {
                return Err(Error::Validation(format!(
                    "duplicate destination path: {}",
                    mapping.destination
                )));
            }
            if let Some(condition) = &mapping.condition {
                self.validate_condition(condition)?;
            }
        }

        Ok(())
    }

    /// Validate that a condition expression only references declared variables
    /// or built-in context fields.
    fn validate_condition(&self, condition: &str) -> Result<()> {
        let declared: indexmap::IndexSet<&str> =
            self.variables.iter().map(|v| v.name.as_str()).collect();

        for token in condition_variables(condition)? {
            if declared.contains(token.as_str()) || BUILTIN_CONTEXT_KEYS.contains(&token.as_str()) {
                continue;
            }
            return Err(Error::Validation(format!(
                "condition references undeclared variable: {}",
                token
            )));
        }
        Ok(())
    }

    /// Resolve one mapping's source inside the pack, rejecting paths that escape
    /// the pack root, symlinks, and sources that do not exist.
    fn resolve_source(pack_dir: &Path, mapping: &FileMapping) -> Result<std::path::PathBuf> {
        validate_relative_path(&mapping.source)?;
        let source_path = pack_dir.join(&mapping.source);
        ensure_under_root(pack_dir, &source_path)?;
        if crate::pathsafe::is_symlink(&source_path)? {
            return Err(Error::Validation(format!(
                "manifest source is a symlink: {}",
                mapping.source
            )));
        }
        if !source_path.try_exists()? {
            return Err(Error::Validation(format!(
                "source file does not exist: {}",
                mapping.source
            )));
        }
        Ok(source_path)
    }

    /// Validate that all source files exist in the given pack directory and do not escape it.
    pub fn validate_source_files(&self, pack_dir: &Path) -> Result<()> {
        for mapping in &self.files {
            Self::resolve_source(pack_dir, mapping)?;
        }
        Ok(())
    }

    /// Validate that every destination stays inside the generated project.
    ///
    /// This is a purely lexical check. Anchoring it to a real directory (the
    /// system temp directory, say) would let the result depend on whatever
    /// happens to exist on the validating machine.
    pub fn validate_destination_paths(&self) -> Result<()> {
        for mapping in &self.files {
            validate_relative_path(&mapping.destination)?;
        }
        Ok(())
    }

    /// Validate variable values against the manifest.
    pub fn validate_values(&self, values: &IndexMap<String, String>) -> Result<()> {
        for var in &self.variables {
            // A caller that omits an optional answer supplies an empty string
            // for it. That is an absent answer, not an integer or a boolean
            // the author got wrong, so it must not be type-checked.
            let supplied = values
                .get(&var.name)
                .filter(|val| !val.is_empty() || var.required);
            match supplied {
                Some(val) => var.validate_value(val)?,
                None => {
                    if var.required && var.default.is_none() {
                        return Err(Error::Validation(format!(
                            "missing required variable: {}",
                            var.name
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Layer the manifest's declared variables over the render context.
    ///
    /// Every answer arrives as a string, so each declared variable is re-typed
    /// to its manifest type, and one the context does not supply falls back to
    /// its manifest default. A variable with neither stays undefined, which
    /// minijinja treats as false.
    ///
    /// Conditions and file bodies both render against this, so a default can
    /// never select a file that then renders with the value undefined.
    pub fn resolve_context(
        &self,
        base: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut ctx = base.clone();

        for var in &self.variables {
            // `SyncContext` always carries the built-in keys, empty or not, so
            // a manifest that redeclares one -- `license`, say -- found a
            // present-but-empty value and never reached its own default. An
            // empty string is an omitted answer, exactly as it is in
            // `validate_values`.
            let supplied = match ctx.get(&var.name) {
                Some(serde_json::Value::String(s)) if !s.is_empty() => Some(s.clone()),
                Some(serde_json::Value::String(_)) | None => None,
                Some(other) => {
                    ctx.insert(var.name.clone(), other.clone());
                    continue;
                }
            };
            let raw = match supplied {
                Some(value) => value,
                None => match var.default.as_ref() {
                    Some(d) => json_value_to_string(d),
                    None => continue,
                },
            };

            let typed = match var.var_type {
                VariableType::String => serde_json::Value::String(raw),
                VariableType::Integer => match raw.parse::<i64>() {
                    Ok(n) => serde_json::Value::Number(n.into()),
                    Err(_) => serde_json::Value::String(raw),
                },
                VariableType::Bool => match raw.as_str() {
                    "true" => serde_json::Value::Bool(true),
                    "false" => serde_json::Value::Bool(false),
                    other => {
                        return Err(Error::Validation(format!(
                            "variable '{}' expects a boolean (true/false), got '{}'",
                            var.name, other
                        )));
                    }
                },
            };
            ctx.insert(var.name.clone(), typed);
        }

        Ok(ctx)
    }

    /// Evaluate a condition expression against the render context.
    ///
    /// `base` is the full render context, so conditions see the built-in fields
    /// (`project_name`, `edition`, ...) as well as the pack variables. Declared
    /// variables that the context does not supply fall back to their manifest
    /// default; a variable with neither stays undefined, which minijinja treats
    /// as false.
    pub fn eval_condition(
        &self,
        condition: &str,
        base: &serde_json::Map<String, serde_json::Value>,
        engine: &crate::template::Engine,
    ) -> Result<bool> {
        let ctx = self.resolve_context(base)?;

        let template = format!("{{% if {condition} %}}true{{% else %}}false{{% endif %}}");
        let rendered = engine
            .render_str(&template, &ctx)
            .map_err(|e| Error::Validation(format!("condition evaluation failed: {}", e)))?;

        Ok(rendered.trim() == "true")
    }

    /// Return the mapping that owns `path`: the one whose destination is the
    /// longest match, so a mapping for `src/api` wins over one for `src`.
    pub fn mapping_for(&self, path: &str) -> Option<&FileMapping> {
        self.files
            .iter()
            .filter(|m| {
                path == m.destination
                    || path
                        .strip_prefix(m.destination.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .max_by_key(|m| m.destination.len())
    }

    /// Build a Template from the manifest and pack directory.
    ///
    /// Directory mappings are expanded here; conditions are re-evaluated during
    /// rendering. A destination produced by more than one mapping -- an explicit
    /// file mapping inside a directory mapping, say -- is emitted once, from the
    /// most specific mapping, so generation never writes the same path twice.
    pub fn to_template(&self, pack_dir: &Path) -> Result<Template> {
        // Destination -> (owning mapping's destination length, file).
        let mut files: IndexMap<String, (usize, TemplateFile)> = IndexMap::new();

        let mut insert =
            |dest: String, file: TemplateFile, specificity: usize| match files.get(&dest) {
                Some((existing, _)) if *existing >= specificity => {}
                _ => {
                    files.insert(dest, (specificity, file));
                }
            };

        for mapping in &self.files {
            let source_path = Self::resolve_source(pack_dir, mapping)?;
            let specificity = mapping.destination.len();

            if source_path.is_dir() {
                // Recursively expand the directory into destination-relative file mappings.
                let mut stack = vec![source_path.clone()];
                while let Some(current) = stack.pop() {
                    for entry in std::fs::read_dir(&current)? {
                        let entry = entry?;
                        let path = entry.path();
                        let file_type = entry.file_type()?;
                        if file_type.is_symlink() {
                            continue;
                        }
                        if file_type.is_dir() {
                            if path.file_name().is_some_and(|n| n == ".git") {
                                continue;
                            }
                            stack.push(path);
                            continue;
                        }
                        if !file_type.is_file() {
                            continue;
                        }
                        let rel = path
                            .strip_prefix(&source_path)
                            .map_err(|e| Error::Argument(e.to_string()))?;
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        let dest = std::path::Path::new(&mapping.destination).join(&rel_str);
                        let dest = dest.to_string_lossy().replace('\\', "/");
                        insert(
                            dest.clone(),
                            TemplateFile {
                                path: dest,
                                content: read_body(&path, mapping.is_template)?,
                                mode: crate::template::file_mode(&path)?,
                            },
                            specificity,
                        );
                    }
                }
            } else {
                let content = read_body(&source_path, mapping.is_template)?;
                insert(
                    mapping.destination.clone(),
                    TemplateFile {
                        path: mapping.destination.clone(),
                        content,
                        mode: crate::template::file_mode(&source_path)?,
                    },
                    specificity,
                );
            }
        }

        let files = files.into_values().map(|(_, file)| file).collect();
        Ok(Template::new(&self.name, files))
    }
}

impl ManifestVariable {
    /// Validate a single variable declaration.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(Error::Validation("variable name cannot be empty".into()));
        }

        // Validate variable name format
        // A hyphen is a subtraction operator in a minijinja expression, so a
        // hyphenated name parses but can never be referenced from a condition.
        if !self
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
            || self.name.starts_with(|c: char| c.is_ascii_digit())
        {
            return Err(Error::Validation(format!(
                "variable name '{}' must be ASCII alphanumeric or '_', and must not start with a digit",
                self.name
            )));
        }

        // Validate regex if present
        if let Some(regex) = &self.regex {
            regex::Regex::new(regex).map_err(|e| {
                Error::Validation(format!("invalid regex for variable '{}': {}", self.name, e))
            })?;
        }

        // Check the default against every constraint, including `choices`.
        if let Some(default) = &self.default {
            self.validate_value(&json_value_to_string(default))?;
        }

        Ok(())
    }

    /// Validate a value against this variable's constraints.
    pub fn validate_value(&self, value: &str) -> Result<()> {
        match self.var_type {
            VariableType::String => {
                // String accepts any value
            }
            VariableType::Integer => {
                if value.parse::<i64>().is_err() {
                    return Err(Error::Validation(format!(
                        "variable '{}' expects an integer, got '{}'",
                        self.name, value
                    )));
                }
            }
            VariableType::Bool => {
                if value != "true" && value != "false" {
                    return Err(Error::Validation(format!(
                        "variable '{}' expects a boolean (true/false), got '{}'",
                        self.name, value
                    )));
                }
            }
        }

        // Validate regex if present
        if let Some(regex) = &self.regex {
            let re = regex::Regex::new(regex).map_err(|e| {
                Error::Validation(format!("invalid regex for variable '{}': {}", self.name, e))
            })?;
            if !re.is_match(value) {
                return Err(Error::Validation(format!(
                    "variable '{}' value '{}' does not match pattern '{}'",
                    self.name, value, regex
                )));
            }
        }

        // Validate choices if present
        if !self.choices.is_empty() && !self.choices.iter().any(|c| c == value) {
            return Err(Error::Validation(format!(
                "variable '{}' value '{}' is not one of the allowed choices {:?}",
                self.name, value, self.choices
            )));
        }

        Ok(())
    }
}

/// Context fields that `SyncContext` always supplies, so a condition may name
/// them without declaring them as pack variables.
const BUILTIN_CONTEXT_KEYS: &[&str] =
    &["project_name", "author", "license", "repository", "edition"];

impl ManifestVariable {
    /// A value that satisfies this variable's declared constraints, for a
    /// trial render where no real answer exists. `None` when the constraints
    /// describe a value that cannot be invented -- a regex, most often.
    pub fn placeholder_value(&self) -> Option<String> {
        if let Some(first) = self.choices.first() {
            return Some(first.clone());
        }
        if self.regex.is_some() {
            return None;
        }
        Some(
            match self.var_type {
                VariableType::String => "example",
                VariableType::Integer => "0",
                VariableType::Bool => "true",
            }
            .to_string(),
        )
    }
}

/// Read a pack file. A literal mapping keeps whatever bytes it holds; one that
/// asks to be rendered has to be text.
fn read_body(path: &Path, is_template: bool) -> Result<crate::template::Content> {
    let content = crate::template::Content::from_bytes(std::fs::read(path)?);
    if is_template && content.as_str().is_none() {
        return Err(Error::Validation(format!(
            "{} is not valid UTF-8; set \"is_template\": false on its mapping to copy it verbatim",
            path.display()
        )));
    }
    Ok(content)
}

/// Yield the context names a condition expression resolves.
///
/// MiniJinja's own parser decides what is a variable, so filters, tests,
/// attribute names, literals and keywords are excluded by the same rules the
/// renderer applies. A syntax error is reported here rather than at render
/// time. The names come back sorted, so a condition with several undeclared
/// variables always reports the same one first.
fn condition_variables(condition: &str) -> Result<Vec<String>> {
    let env = minijinja::Environment::new();
    let source = format!("{{{{ {condition} }}}}");
    let template = env
        .template_from_str(&source)
        .map_err(|e| Error::Validation(format!("invalid condition '{condition}': {}", e.kind())))?;
    let mut names: Vec<String> = template.undeclared_variables(false).into_iter().collect();
    names.sort_unstable();
    Ok(names)
}

/// Convert a `serde_json::Value` to its display string without JSON quoting.
fn json_value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_manifest() {
        let json = r#"
        {
            "name": "test-pack",
            "version": "1.0.0",
            "description": "A test pack",
            "variables": [
                {
                    "name": "project_name",
                    "type": "string",
                    "required": true,
                    "description": "Project name"
                }
            ],
            "files": [
                {
                    "source": "Cargo.toml",
                    "destination": "Cargo.toml"
                }
            ]
        }
        "#;
        let manifest = PackManifest::from_json(json).unwrap();
        assert_eq!(manifest.name, "test-pack");
        assert_eq!(manifest.version, Some("1.0.0".to_string()));
        assert_eq!(manifest.variables.len(), 1);
        assert_eq!(manifest.files.len(), 1);
    }

    #[test]
    fn validate_rejects_empty_name() {
        let json = r#"
        {
            "name": "",
            "variables": [],
            "files": []
        }
        "#;
        assert!(PackManifest::from_json(json).is_err());
    }

    #[test]
    fn validate_rejects_duplicate_variables() {
        let json = r#"
        {
            "name": "test",
            "variables": [
                {"name": "foo", "type": "string"},
                {"name": "foo", "type": "string"}
            ],
            "files": []
        }
        "#;
        assert!(PackManifest::from_json(json).is_err());
    }

    #[test]
    fn validate_rejects_duplicate_destinations() {
        let json = r#"
        {
            "name": "test",
            "variables": [],
            "files": [
                {"source": "a", "destination": "Cargo.toml"},
                {"source": "b", "destination": "Cargo.toml"}
            ]
        }
        "#;
        assert!(PackManifest::from_json(json).is_err());
    }

    #[test]
    fn validate_integer_type() {
        let var = ManifestVariable {
            name: "count".into(),
            var_type: VariableType::Integer,
            required: true,
            default: None,
            description: None,
            regex: None,
            choices: vec![],
        };
        assert!(var.validate_value("42").is_ok());
        assert!(var.validate_value("not-a-number").is_err());
    }

    #[test]
    fn validate_bool_type() {
        let var = ManifestVariable {
            name: "enabled".into(),
            var_type: VariableType::Bool,
            required: true,
            default: None,
            description: None,
            regex: None,
            choices: vec![],
        };
        assert!(var.validate_value("true").is_ok());
        assert!(var.validate_value("false").is_ok());
        assert!(var.validate_value("yes").is_err());
    }

    #[test]
    fn validate_choices() {
        let var = ManifestVariable {
            name: "license".into(),
            var_type: VariableType::String,
            required: true,
            default: None,
            description: None,
            regex: None,
            choices: vec!["MIT".into(), "Apache-2.0".into()],
        };
        assert!(var.validate_value("MIT").is_ok());
        assert!(var.validate_value("GPL").is_err());
    }

    #[test]
    fn eval_condition_simple() {
        let manifest = PackManifest {
            name: "test".into(),
            version: None,
            description: None,
            author: None,
            variables: vec![ManifestVariable {
                name: "has_cli".into(),
                var_type: VariableType::Bool,
                required: false,
                default: Some(serde_json::json!(true)),
                description: None,
                regex: None,
                choices: vec![],
            }],
            files: vec![],
        };

        let engine = crate::template::Engine::new();
        let mut values = serde_json::Map::new();
        values.insert("has_cli".into(), serde_json::json!("true"));
        let result = manifest
            .eval_condition("has_cli == true", &values, &engine)
            .unwrap();
        assert!(result, "has_cli=true should be truthy");

        values.insert("has_cli".into(), serde_json::json!("false"));
        let result = manifest
            .eval_condition("has_cli == true", &values, &engine)
            .unwrap();
        assert!(!result, "has_cli=false should be falsy");
    }

    fn string_var(name: &str, default: Option<serde_json::Value>) -> ManifestVariable {
        ManifestVariable {
            name: name.into(),
            var_type: VariableType::String,
            required: false,
            default,
            description: None,
            regex: None,
            choices: vec![],
        }
    }

    fn manifest_with(variables: Vec<ManifestVariable>, files: Vec<FileMapping>) -> PackManifest {
        PackManifest {
            name: "test".into(),
            version: None,
            description: None,
            author: None,
            variables,
            files,
        }
    }

    fn mapping(source: &str, destination: &str, condition: Option<&str>) -> FileMapping {
        FileMapping {
            source: source.into(),
            destination: destination.into(),
            condition: condition.map(str::to_string),
            is_template: true,
        }
    }

    #[test]
    fn condition_may_compare_against_a_string_literal() {
        let manifest = manifest_with(
            vec![string_var("lang", None)],
            vec![mapping("a.txt", "a.txt", Some(r#"lang == "rust""#))],
        );
        // "rust" is a literal, not an undeclared variable reference.
        manifest
            .validate()
            .expect("string comparison must validate");
    }

    #[test]
    fn condition_may_reference_builtin_context_fields() {
        let manifest = manifest_with(
            vec![],
            vec![mapping("a.txt", "a.txt", Some(r#"edition == "2024""#))],
        );
        manifest.validate().expect("built-ins must validate");
    }

    #[test]
    fn condition_still_rejects_an_undeclared_variable() {
        let manifest = manifest_with(
            vec![],
            vec![mapping("a.txt", "a.txt", Some("has_cli and nope"))],
        );
        let err = manifest.validate().unwrap_err().to_string();
        assert!(err.contains("has_cli"), "unexpected error: {err}");
    }

    #[test]
    fn condition_skips_attribute_and_filter_names() {
        let manifest = manifest_with(
            vec![string_var("name", None)],
            vec![mapping("a.txt", "a.txt", Some("name | lower == \"x\""))],
        );
        manifest.validate().expect("filter names are not variables");
    }

    #[test]
    fn condition_sees_builtin_context_values() {
        let manifest = manifest_with(vec![], vec![]);
        let engine = crate::template::Engine::new();
        let mut ctx = serde_json::Map::new();
        ctx.insert("edition".into(), serde_json::json!("2024"));
        assert!(
            manifest
                .eval_condition(r#"edition == "2024""#, &ctx, &engine)
                .unwrap()
        );
    }

    #[test]
    fn hyphenated_variable_names_are_rejected() {
        let manifest = manifest_with(vec![string_var("has-cli", None)], vec![]);
        let err = manifest.validate().unwrap_err().to_string();
        assert!(err.contains("has-cli"), "unexpected error: {err}");
    }

    #[test]
    fn required_variable_without_a_value_is_rejected() {
        let manifest = manifest_with(
            vec![ManifestVariable {
                required: true,
                ..string_var("api_url", None)
            }],
            vec![],
        );
        let mut values = IndexMap::new();
        values.insert("other".to_string(), "x".to_string());
        let err = manifest.validate_values(&values).unwrap_err().to_string();
        assert!(err.contains("api_url"), "unexpected error: {err}");

        // A default satisfies the requirement.
        let manifest = manifest_with(
            vec![ManifestVariable {
                required: true,
                ..string_var("api_url", Some(serde_json::json!("http://localhost")))
            }],
            vec![],
        );
        manifest.validate_values(&values).unwrap();
    }

    #[test]
    fn default_outside_choices_is_rejected() {
        let manifest = manifest_with(
            vec![ManifestVariable {
                choices: vec!["a".into(), "b".into()],
                ..string_var("pick", Some(serde_json::json!("c")))
            }],
            vec![],
        );
        let err = manifest.validate().unwrap_err().to_string();
        assert!(err.contains("choices"), "unexpected error: {err}");
    }

    // `value is defined` names a minijinja test, not a second context lookup.
    // Treating `defined` as a variable rejected every valid test expression.
    #[test]
    fn a_condition_may_use_a_minijinja_test() {
        for condition in [
            "lang is defined",
            "lang is not defined",
            "lang is string",
            "lang is defined and lang == 'rust'",
        ] {
            let manifest = manifest_with(
                vec![string_var("lang", None)],
                vec![mapping("a", "a", Some(condition))],
            );
            assert!(
                manifest.validate().is_ok(),
                "condition rejected: {condition}: {:?}",
                manifest.validate().unwrap_err().to_string()
            );
        }
    }

    // The test name is skipped, but a real undeclared variable after one is not.
    #[test]
    fn a_test_expression_still_reports_an_undeclared_variable() {
        let manifest = manifest_with(
            vec![string_var("lang", None)],
            vec![mapping("a", "a", Some("lang is defined and missing"))],
        );
        let err = manifest.validate().unwrap_err().to_string();
        assert!(err.contains("missing"), "unexpected error: {err}");
    }

    #[test]
    fn is_template_defaults_to_true() {
        let json = r#"{"name":"p","files":[{"source":"a","destination":"a"}]}"#;
        let manifest = PackManifest::from_json(json).unwrap();
        assert!(manifest.files[0].is_template);

        let json = r#"{"name":"p","files":[{"source":"a","destination":"a","is_template":false}]}"#;
        let manifest = PackManifest::from_json(json).unwrap();
        assert!(!manifest.files[0].is_template);
    }
    #[test]
    fn resolve_context_uses_default_when_builtin_key_is_empty() {
        // `SyncContext` supplies every built-in key, empty string included. A
        // manifest that redeclares one must still reach its own default.
        let json = r#"
        {
            "name": "p",
            "variables": [
                {"name": "license", "type": "string", "default": "MIT"}
            ],
            "files": []
        }
        "#;
        let manifest = PackManifest::from_json(json).unwrap();
        let mut base = serde_json::Map::new();
        base.insert(
            "license".to_string(),
            serde_json::Value::String(String::new()),
        );
        let ctx = manifest.resolve_context(&base).unwrap();
        assert_eq!(ctx.get("license").and_then(|v| v.as_str()), Some("MIT"));
    }

    #[test]
    fn placeholder_value_prefers_choice_and_skips_regex() {
        let json = r#"
        {
            "name": "p",
            "variables": [
                {"name": "kind", "type": "string", "choices": ["cli", "lib"]},
                {"name": "slug", "type": "string", "regex": "^[a-z]+$"},
                {"name": "port", "type": "integer"},
                {"name": "ci", "type": "bool"},
                {"name": "title", "type": "string"}
            ],
            "files": []
        }
        "#;
        let manifest = PackManifest::from_json(json).unwrap();
        let by = |n: &str| {
            manifest
                .variables
                .iter()
                .find(|v| v.name == n)
                .unwrap()
                .placeholder_value()
        };
        assert_eq!(by("kind").as_deref(), Some("cli"));
        assert_eq!(by("slug"), None);
        assert_eq!(by("port").as_deref(), Some("0"));
        assert_eq!(by("ci").as_deref(), Some("true"));
        assert_eq!(by("title").as_deref(), Some("example"));
    }

    // A filter keyword argument is not a context lookup. The hand-written
    // lexer this replaced pushed `sep` and failed the manifest.
    #[test]
    fn condition_allows_a_filter_keyword_argument() {
        let manifest = manifest_with(
            vec![string_var("lang", None)],
            vec![mapping(
                "a",
                "a",
                Some(r#"(lang | trim(chars=" ")) == "rust""#),
            )],
        );
        manifest
            .validate()
            .expect("a filter keyword argument must validate");
    }

    #[test]
    fn condition_reports_a_syntax_error() {
        let manifest = manifest_with(vec![], vec![mapping("a", "a", Some("lang =="))]);
        let err = manifest.validate().unwrap_err().to_string();
        assert!(err.contains("invalid condition"), "unexpected error: {err}");
    }

    #[test]
    fn manifest_carries_author_metadata() {
        let json = r#"{"name":"p","author":"Ada","files":[]}"#;
        let manifest = PackManifest::from_json(json).unwrap();
        assert_eq!(manifest.author.as_deref(), Some("Ada"));
        // Round-trips, so `truss extract` and the marketplace keep it.
        let back = PackManifest::from_json(&serde_json::to_string(&manifest).unwrap()).unwrap();
        assert_eq!(back.author.as_deref(), Some("Ada"));
    }
}
