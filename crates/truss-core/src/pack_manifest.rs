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
    #[serde(default)]
    pub is_template: bool,
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

    /// Validate that a condition expression only references declared variables.
    fn validate_condition(&self, condition: &str) -> Result<()> {
        let declared: indexmap::IndexSet<&str> =
            self.variables.iter().map(|v| v.name.as_str()).collect();

        // Simple heuristic: strip punctuation and verify remaining tokens are declared.
        // minijinja will do full validation during evaluation.
        for token in condition.split_whitespace() {
            let token = token.trim_matches(|c: char| {
                !(c.is_ascii_alphanumeric() || c == '_' || c == '-')
            });
            if token.is_empty() {
                continue;
            }
            // Skip operators and literals
            if token == "and"
                || token == "or"
                || token == "not"
                || token == "true"
                || token == "false"
            {
                continue;
            }
            // Skip numeric literals (e.g. port >= 1024)
            if token.parse::<f64>().is_ok() {
                continue;
            }
            // Variable names may contain hyphens and underscores
            if token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                && !declared.contains(token)
            {
                return Err(Error::Validation(format!(
                    "condition references undeclared variable: {}",
                    token
                )));
            }
        }
        Ok(())
    }

    /// Validate that all source files exist in the given pack directory and do not escape it.
    pub fn validate_source_files(&self, pack_dir: &Path) -> Result<()> {
        for mapping in &self.files {
            validate_relative_path(&mapping.source)?;
            let source_path = pack_dir.join(&mapping.source);
            ensure_under_root(pack_dir, &source_path)?;
            if crate::pathsafe::is_symlink(&source_path)? {
                return Err(Error::Validation(format!(
                    "manifest source is a symlink: {}",
                    mapping.source
                )));
            }
            if !source_path.exists() {
                return Err(Error::Validation(format!(
                    "source file does not exist: {}",
                    mapping.source
                )));
            }
        }
        Ok(())
    }

    /// Validate that destination paths are under the project root.
    pub fn validate_destination_paths(&self, project_root: &Path) -> Result<()> {
        for mapping in &self.files {
            let dest_path = project_root.join(&mapping.destination);
            ensure_under_root(project_root, &dest_path)?;
        }
        Ok(())
    }

    /// Validate variable values against the manifest.
    pub fn validate_values(&self, values: &IndexMap<String, String>) -> Result<()> {
        for var in &self.variables {
            if let Some(val) = values.get(&var.name) {
                var.validate_value(val)?;
            }
        }
        Ok(())
    }

    /// Evaluate a condition expression using the given variable values.
    pub fn eval_condition(
        &self,
        condition: &str,
        values: &IndexMap<String, String>,
    ) -> Result<bool> {
        let mut ctx = serde_json::Map::new();
        for var in &self.variables {
            let value = match values.get(&var.name) {
                Some(v) => v.clone(),
                None => match var.default.as_ref() {
                    Some(d) => match d {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Number(n) => n.to_string(),
                        serde_json::Value::Bool(b) => b.to_string(),
                        _ => String::new(),
                    },
                    None => String::new(),
                },
            };

            // Convert to appropriate type based on variable type
            let json_value = match var.var_type {
                VariableType::String => serde_json::Value::String(value),
                VariableType::Integer => match value.parse::<i64>() {
                    Ok(n) => serde_json::Value::Number(n.into()),
                    Err(_) => serde_json::Value::String(value),
                },
                VariableType::Bool => serde_json::Value::Bool(value == "true"),
            };
            ctx.insert(var.name.clone(), json_value);
        }

        let engine = crate::template::Engine::new();
        let template = format!("{{% if {condition} %}}true{{% else %}}false{{% endif %}}");
        let rendered = engine
            .render_str(&template, &ctx)
            .map_err(|e| Error::Validation(format!("condition evaluation failed: {}", e)))?;

        // Parse the rendered result as a boolean
        let trimmed = rendered.trim().to_lowercase();
        Ok(trimmed == "true" || trimmed == "1")
    }

    /// Build a Template from the manifest and pack directory.
    /// Directory mappings are expanded here; conditions are re-evaluated during rendering.
    pub fn to_template(&self, pack_dir: &Path) -> Result<Template> {
        let mut files = Vec::new();

        for mapping in &self.files {
            validate_relative_path(&mapping.source)?;
            let source_path = pack_dir.join(&mapping.source);
            ensure_under_root(pack_dir, &source_path)?;
            if crate::pathsafe::is_symlink(&source_path)? {
                return Err(Error::Validation(format!(
                    "manifest source is a symlink: {}",
                    mapping.source
                )));
            }
            if !source_path.exists() {
                return Err(Error::Validation(format!(
                    "source file does not exist: {}",
                    mapping.source
                )));
            }

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
                        let rel = path.strip_prefix(&source_path)
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
        if !self
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(Error::Validation(format!(
                "variable name '{}' must be ASCII alphanumeric, '-' or '_'",
                self.name
            )));
        }

        // Validate regex if present
        if let Some(regex) = &self.regex {
            regex::Regex::new(regex).map_err(|e| {
                Error::Validation(format!("invalid regex for variable '{}': {}", self.name, e))
            })?;
        }

        // Validate that default value matches type if present
        if let Some(default) = &self.default {
            self.validate_value(&json_value_to_string(default))?;
        }

        // Validate choices if present
        if !self.choices.is_empty() {
            if let Some(default) = &self.default {
                let default_str = json_value_to_string(default);
                if !self.choices.iter().any(|c| c == &default_str) {
                    return Err(Error::Validation(format!(
                        "default value '{}' for variable '{}' is not in choices {:?}",
                        default_str, self.name, self.choices
                    )));
                }
            }
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

        let mut values = IndexMap::new();
        values.insert("has_cli".into(), "true".into());
        let result = manifest.eval_condition("has_cli == true", &values).unwrap();
        eprintln!("Result for has_cli=true: {:?}", result);
        assert!(result, "has_cli=true should be truthy");

        values.insert("has_cli".into(), "false".into());
        let result = manifest.eval_condition("has_cli == true", &values).unwrap();
        assert!(!result, "has_cli=false should be falsy");
    }
}
