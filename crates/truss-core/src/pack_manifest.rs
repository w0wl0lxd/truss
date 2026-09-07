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

        for token in identifier_tokens(condition) {
            if declared.contains(token) || BUILTIN_CONTEXT_KEYS.contains(&token) {
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
            match values.get(&var.name) {
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
        let mut ctx = base.clone();

        for var in &self.variables {
            let raw = match ctx.get(&var.name) {
                // The context carries every answer as a string, so re-type it
                // below rather than trusting whatever shape it arrived in.
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(other) => {
                    ctx.insert(var.name.clone(), other.clone());
                    continue;
                }
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

        let template = format!("{{% if {condition} %}}true{{% else %}}false{{% endif %}}");
        let rendered = engine
            .render_str(&template, &ctx)
            .map_err(|e| Error::Validation(format!("condition evaluation failed: {}", e)))?;

        Ok(rendered.trim() == "true")
    }

    /// Build a Template from the manifest and pack directory.
    /// Directory mappings are expanded here; conditions are re-evaluated during rendering.
    pub fn to_template(&self, pack_dir: &Path) -> Result<Template> {
        let mut files = Vec::new();

        for mapping in &self.files {
            let source_path = Self::resolve_source(pack_dir, mapping)?;

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
                        files.push(TemplateFile {
                            path: dest.to_string_lossy().replace('\\', "/"),
                            content: std::fs::read_to_string(&path)?,
                            mode: None,
                        });
                    }
                }
            } else {
                let content = std::fs::read_to_string(&source_path)?;
                files.push(TemplateFile {
                    path: mapping.destination.clone(),
                    content,
                    mode: None,
                });
            }
        }

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

/// Expression keywords and literals that are not variable references.
const EXPRESSION_KEYWORDS: &[&str] = &[
    "and", "or", "not", "true", "false", "none", "in", "is", "if", "else", "None", "True", "False",
];

/// Yield the identifier tokens of a condition expression.
///
/// String literals, numbers, operators, keywords, attribute names (`a.b`) and
/// filter names (`a | lower`) are skipped, so only the names the expression
/// actually resolves from the context are returned.
fn identifier_tokens(condition: &str) -> Vec<&str> {
    let mut out = Vec::new();
    // The last non-space character before the current token. `.` and `|` mean
    // the token that follows is an attribute or a filter, not a context lookup.
    let mut previous = '\0';
    let mut chars = condition.char_indices().peekable();

    while let Some((start, c)) = chars.next() {
        if c == '"' || c == '\'' {
            // Consume through the closing quote, honouring backslash escapes.
            while let Some((_, q)) = chars.next() {
                if q == '\\' {
                    chars.next();
                } else if q == c {
                    break;
                }
            }
            previous = '"';
            continue;
        }

        if c.is_ascii_alphabetic() || c == '_' {
            let mut end = start + c.len_utf8();
            while let Some(&(next_start, next)) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    end = next_start + next.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            if let Some(token) = condition.get(start..end) {
                if previous != '.' && previous != '|' && !EXPRESSION_KEYWORDS.contains(&token) {
                    out.push(token);
                }
            }
            previous = 'x';
            continue;
        }

        if !c.is_whitespace() {
            previous = c;
        }
    }
    out
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

    #[test]
    fn is_template_defaults_to_true() {
        let json = r#"{"name":"p","files":[{"source":"a","destination":"a"}]}"#;
        let manifest = PackManifest::from_json(json).unwrap();
        assert!(manifest.files[0].is_template);

        let json = r#"{"name":"p","files":[{"source":"a","destination":"a","is_template":false}]}"#;
        let manifest = PackManifest::from_json(json).unwrap();
        assert!(!manifest.files[0].is_template);
    }
}
