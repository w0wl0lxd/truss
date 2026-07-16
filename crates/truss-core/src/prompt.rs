use crate::error::{Error, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Pack-level prompt manifest, usually read from `truss.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptManifest {
    pub prompts: Vec<Prompt>,
}

/// A single user-facing prompt declared by a template pack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub kind: PromptKind,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub choices: Vec<String>,
    #[serde(default)]
    pub regex: Option<String>,
    #[serde(default = "bool_true")]
    pub required: bool,
    #[serde(default)]
    pub condition: Option<PromptCondition>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PromptKind {
    #[default]
    Text,
    Choice,
    Bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptCondition {
    pub prompt: String,
    pub values: Vec<String>,
}

fn bool_true() -> bool {
    true
}

/// Intermediate table used to deserialize `[prompts]` from `truss.toml`.
#[derive(Debug, Clone, Deserialize)]
struct PromptsTable {
    #[serde(default, rename = "prompts")]
    by_name: IndexMap<String, PromptConfig>,
}

/// Prompt configuration as it appears inside `[prompts.<name>]`.
#[derive(Debug, Clone, Deserialize)]
struct PromptConfig {
    label: String,
    #[serde(default)]
    kind: PromptKind,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    choices: Vec<String>,
    #[serde(default)]
    regex: Option<String>,
    #[serde(default = "bool_true")]
    required: bool,
    #[serde(default)]
    condition: Option<PromptCondition>,
}

impl PromptManifest {
    /// Load and validate a manifest from a TOML string.
    pub fn from_toml(s: &str) -> Result<Self> {
        let table: PromptsTable = toml_edit::de::from_str(s)
            .map_err(|e| Error::Argument(format!("failed to parse prompt manifest: {e}")))?;
        let mut prompts = Vec::with_capacity(table.by_name.len());
        let mut seen = indexmap::IndexSet::new();
        for (name, config) in table.by_name {
            validate_prompt_name(&name)?;
            if !seen.insert(name.clone()) {
                return Err(Error::Validation(format!("duplicate prompt {name:?}")));
            }
            if config.kind == PromptKind::Choice {
                if config.choices.is_empty() {
                    return Err(Error::Validation(format!(
                        "choice prompt {name:?} must have at least one choice"
                    )));
                }
                if let Some(default) = &config.default {
                    if !config.choices.contains(default) {
                        return Err(Error::Validation(format!(
                            "choice prompt {name:?}: default value {default:?} is not one of the allowed choices {:?}",
                            config.choices
                        )));
                    }
                }
            }
            if let Some(regex) = &config.regex {
                regex::Regex::new(regex).map_err(|e| {
                    Error::Validation(format!("invalid regex for prompt {name:?}: {e}"))
                })?;
            }
            if let Some(condition) = &config.condition {
                if !seen.contains(&condition.prompt) {
                    return Err(Error::Validation(format!(
                        "prompt {name:?}: condition references unknown or subsequent prompt {:?}",
                        condition.prompt
                    )));
                }
            }
            prompts.push(Prompt {
                name,
                label: config.label,
                kind: config.kind,
                default: config.default,
                choices: config.choices,
                regex: config.regex,
                required: config.required,
                condition: config.condition,
            });
        }
        Ok(Self { prompts })
    }

    /// Load from a file, returning an empty manifest if the file does not exist.
    pub fn from_path(path: &Path) -> Result<Self> {
        if !path.try_exists()? {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }

    /// Validate that `answers` satisfies every visible prompt in the manifest.
    pub fn validate(&self, answers: &IndexMap<String, String>) -> Result<()> {
        for prompt in &self.prompts {
            if prompt.is_visible(answers) {
                prompt.validate(answers)?;
            }
        }
        Ok(())
    }

    /// Return the final value for a prompt, or `None` if the prompt is hidden.
    pub fn value_for<'a>(
        &'a self,
        name: &str,
        answers: &'a IndexMap<String, String>,
    ) -> Option<&'a str> {
        let prompt = self.prompts.iter().find(|p| p.name == name)?;
        if !prompt.is_visible(answers) {
            return None;
        }
        answers
            .get(name)
            .map(String::as_str)
            .or(prompt.default.as_deref())
    }
}

impl Prompt {
    /// Return true when this prompt should be shown given the answers collected
    /// so far.
    pub fn is_visible(&self, answers: &IndexMap<String, String>) -> bool {
        let Some(condition) = &self.condition else {
            return true;
        };
        match answers.get(&condition.prompt) {
            Some(value) => condition.values.contains(value),
            None => false,
        }
    }

    /// Validate the answer for this prompt against its constraints.
    pub fn validate(&self, answers: &IndexMap<String, String>) -> Result<()> {
        if !self.is_visible(answers) {
            return Ok(());
        }
        let answer = answers
            .get(&self.name)
            .cloned()
            .or_else(|| self.default.clone())
            .unwrap_or_else(String::new);
        if self.required && answer.is_empty() {
            return Err(Error::Validation(format!(
                "prompt {:?} requires a value",
                self.name
            )));
        }
        if !self.choices.is_empty() && !answer.is_empty() && !self.choices.contains(&answer) {
            return Err(Error::Validation(format!(
                "prompt {:?}: value {:?} is not one of the allowed choices {:?}",
                self.name, answer, self.choices
            )));
        }
        if let Some(regex) = &self.regex {
            if !answer.is_empty() {
                let re = regex::Regex::new(regex).map_err(|e| {
                    Error::Validation(format!("invalid regex for prompt {:?}: {e}", self.name))
                })?;
                if !re.is_match(&answer) {
                    return Err(Error::Validation(format!(
                        "prompt {:?}: value {:?} does not match the required pattern",
                        self.name, answer
                    )));
                }
            }
        }
        if self.kind == PromptKind::Bool
            && !answer.is_empty()
            && answer != "true"
            && answer != "false"
        {
            return Err(Error::Validation(format!(
                "prompt {:?}: boolean value must be \"true\" or \"false\", got {:?}",
                self.name, answer
            )));
        }
        Ok(())
    }
}

fn validate_prompt_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::Validation("prompt name cannot be empty".into()));
    }
    const RESERVED: &[&str] = &[
        "project_name",
        "author",
        "license",
        "edition",
        "repository",
        "extra",
    ];
    if RESERVED.contains(&name) {
        return Err(Error::Validation(format!(
            "prompt name {:?} is reserved",
            name
        )));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(Error::Validation(format!(
            "prompt name {:?} must be ASCII alphanumeric, '-' or '_'",
            name
        )));
    }
    Ok(())
}

/// Read prompt answers previously persisted in a project.
pub fn load_answers(path: &Path) -> Result<IndexMap<String, String>> {
    if !path.try_exists()? {
        return Ok(IndexMap::new());
    }
    let content = std::fs::read_to_string(path)?;
    #[derive(Debug, Default, Deserialize)]
    struct AnswersTable {
        #[serde(default)]
        answers: IndexMap<String, String>,
    }
    let table: AnswersTable = toml_edit::de::from_str(&content)
        .map_err(|e| Error::Argument(format!("failed to parse prompt answers: {e}")))?;
    Ok(table.answers)
}

/// Persist prompt answers for later sync/check reuse.
pub fn save_answers(path: &Path, answers: &IndexMap<String, String>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut doc = toml_edit::DocumentMut::new();
    let mut table = toml_edit::Table::new();
    table.set_dotted(true);
    for (k, v) in answers {
        table.insert(k, toml_edit::value(v));
    }
    doc.insert("answers", toml_edit::Item::Table(table));
    std::fs::write(path, doc.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_text_prompt() {
        let toml = r#"
[prompts]
description = { label = "Project description", kind = "text" }
"#;
        let manifest = PromptManifest::from_toml(toml).unwrap();
        assert_eq!(manifest.prompts.len(), 1);
        assert_eq!(manifest.prompts[0].name, "description");
        assert_eq!(manifest.prompts[0].label, "Project description");
        assert_eq!(manifest.prompts[0].kind, PromptKind::Text);
    }

    #[test]
    fn choice_validation_rejects_invalid() {
        let prompt = Prompt {
            name: "license".into(),
            label: "License".into(),
            kind: PromptKind::Choice,
            default: Some("MIT".into()),
            choices: vec!["MIT".into(), "Apache-2.0".into()],
            regex: None,
            required: true,
            condition: None,
        };
        let mut answers = IndexMap::new();
        answers.insert("license".into(), "GPL".into());
        assert!(prompt.validate(&answers).is_err());
    }

    #[test]
    fn conditional_prompt_hidden() {
        let prompt = Prompt {
            name: "framework".into(),
            label: "Framework".into(),
            kind: PromptKind::Choice,
            default: None,
            choices: vec!["axum".into(), "actix".into()],
            regex: None,
            required: true,
            condition: Some(PromptCondition {
                prompt: "include_cli".into(),
                values: vec!["true".into()],
            }),
        };
        let mut answers = IndexMap::new();
        answers.insert("include_cli".into(), "false".into());
        // hidden, so required does not trigger
        prompt.validate(&answers).unwrap();
    }
}
