use crate::error::{Error, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Record of the preset used to create a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetRecord {
    /// Name of the preset used.
    pub preset: String,
    /// Final variable values after merging preset defaults with CLI overrides.
    pub variables: IndexMap<String, String>,
}

impl PresetRecord {
    /// Save the preset record to `.truss/preset.toml` in the project directory.
    pub fn save(&self, project_path: &Path) -> Result<()> {
        let truss_dir = project_path.join(".truss");
        std::fs::create_dir_all(&truss_dir)?;
        let record_path = truss_dir.join("preset.toml");
        let content = toml_edit::ser::to_string_pretty(self)
            .map_err(|e| Error::Argument(format!("failed to serialize preset record: {e}")))?;
        std::fs::write(&record_path, content)?;
        Ok(())
    }

    /// Load the preset record from `.truss/preset.toml` in the project directory.
    /// Returns `None` if the file does not exist.
    pub fn load(project_path: &Path) -> Result<Option<Self>> {
        let record_path = project_path.join(".truss/preset.toml");
        if !record_path.try_exists()? {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&record_path)?;
        let record: Self = toml_edit::de::from_str(&content)
            .map_err(|e| Error::Argument(format!("failed to parse preset record: {e}")))?;
        Ok(Some(record))
    }
}

/// A project-type preset that maps to a pack/template with default variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    /// Unique name of the preset.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Pack or template name to use.
    pub pack: String,
    /// Default variable bindings (can be overridden by CLI flags).
    #[serde(default)]
    pub variables: IndexMap<String, String>,
}

impl Preset {
    /// Resolve the template name for this preset.
    /// Returns the pack name (validation should be done by the caller).
    pub fn resolve_template_name(&self) -> String {
        self.pack.clone()
    }

    /// Merge preset defaults with CLI-provided variables.
    /// CLI values take precedence over preset defaults.
    pub fn merge_variables(&self, cli_vars: &IndexMap<String, String>) -> IndexMap<String, String> {
        let mut merged = self.variables.clone();
        for (k, v) in cli_vars {
            merged.insert(k.clone(), v.clone());
        }
        merged
    }
}

/// Registry of built-in and user-defined presets.
#[derive(Debug, Clone)]
pub struct PresetRegistry {
    built_in: IndexMap<String, Preset>,
    custom: IndexMap<String, Preset>,
}

impl PresetRegistry {
    /// Load built-in presets and any custom presets from user config.
    pub fn load() -> Result<Self> {
        let mut registry = Self {
            built_in: Self::built_in_presets(),
            custom: IndexMap::new(),
        };

        // Load custom presets from user config
        if let Ok(path) = Self::user_path() {
            if path.try_exists()? {
                let custom_presets = Self::load_from_file(&path)?;
                for (name, preset) in custom_presets {
                    registry.custom.insert(name, preset);
                }
            }
        }

        Ok(registry)
    }

    /// Get a preset by name (custom presets take precedence over built-in).
    pub fn get(&self, name: &str) -> Option<&Preset> {
        self.custom.get(name).or_else(|| self.built_in.get(name))
    }

    /// List all preset names and descriptions (custom first, then built-in).
    pub fn list(&self) -> Vec<(&str, &str)> {
        let mut out = Vec::new();
        for (name, preset) in &self.custom {
            out.push((name.as_str(), preset.description.as_str()));
        }
        for (name, preset) in &self.built_in {
            if !self.custom.contains_key(name) {
                out.push((name.as_str(), preset.description.as_str()));
            }
        }
        out
    }

    /// Get detailed information about a preset.
    pub fn get_details(&self, name: &str) -> Option<&Preset> {
        self.get(name)
    }

    /// Get a preset by name, returning an error if not found.
    pub fn require(&self, name: &str) -> Result<&Preset> {
        self.get(name)
            .ok_or_else(|| Error::PresetNotFound(name.to_string()))
    }

    /// Platform user preset path (`$XDG_CONFIG_HOME/truss/presets.toml`).
    pub fn user_path() -> Result<PathBuf> {
        directories::BaseDirs::new()
            .map(|b| b.config_dir().join("truss").join("presets.toml"))
            .ok_or(Error::ProjectDir)
    }

    /// Load presets from a TOML file.
    fn load_from_file(path: &PathBuf) -> Result<IndexMap<String, Preset>> {
        let content = std::fs::read_to_string(path)?;
        #[derive(Debug, Deserialize)]
        struct PresetsFile {
            #[serde(default)]
            presets: IndexMap<String, PresetConfig>,
        }
        #[derive(Debug, Deserialize)]
        struct PresetConfig {
            description: String,
            pack: String,
            #[serde(default)]
            variables: IndexMap<String, String>,
        }

        let file: PresetsFile = toml_edit::de::from_str(&content)
            .map_err(|e| Error::Argument(format!("failed to parse presets file: {e}")))?;

        let mut presets = IndexMap::new();
        for (name, config) in file.presets {
            if name.is_empty() {
                return Err(Error::Argument("preset name cannot be empty".into()));
            }
            if !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(Error::Argument(format!(
                    "preset name {name:?} must be ASCII alphanumeric, '-' or '_'"
                )));
            }
            if config.pack.is_empty() {
                return Err(Error::Argument(format!(
                    "preset {name:?} must specify a pack"
                )));
            }
            presets.insert(
                name.clone(),
                Preset {
                    name,
                    description: config.description,
                    pack: config.pack,
                    variables: config.variables,
                },
            );
        }
        Ok(presets)
    }

    /// Built-in presets shipped with truss.
    fn built_in_presets() -> IndexMap<String, Preset> {
        let mut presets = IndexMap::new();

        // binary: single binary crate using default pack
        presets.insert(
            "binary".to_string(),
            Preset {
                name: "binary".to_string(),
                description: "Single binary crate application".to_string(),
                pack: "default".to_string(),
                variables: IndexMap::new(),
            },
        );

        // library: single library crate using the library pack
        presets.insert(
            "library".to_string(),
            Preset {
                name: "library".to_string(),
                description: "Single library crate for reuse".to_string(),
                pack: "library".to_string(),
                variables: IndexMap::new(),
            },
        );

        // workspace: multi-crate workspace using monorepo pack
        presets.insert(
            "workspace".to_string(),
            Preset {
                name: "workspace".to_string(),
                description: "Multi-crate workspace with apps, libs, and tools".to_string(),
                pack: "monorepo".to_string(),
                variables: IndexMap::new(),
            },
        );

        presets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_presets_exist() {
        let registry = PresetRegistry::load().expect("load registry");
        assert!(registry.get("binary").is_some());
        assert!(registry.get("library").is_some());
        assert!(registry.get("workspace").is_some());
    }

    #[test]
    fn preset_merges_variables() {
        let preset = Preset {
            name: "test".to_string(),
            description: "test".to_string(),
            pack: "default".to_string(),
            variables: {
                let mut vars = IndexMap::new();
                vars.insert("license".to_string(), "MIT".to_string());
                vars.insert("author".to_string(), "default".to_string());
                vars
            },
        };

        let mut cli_vars = IndexMap::new();
        cli_vars.insert("license".to_string(), "Apache-2.0".to_string());

        let merged = preset.merge_variables(&cli_vars);
        assert_eq!(merged.get("license"), Some(&"Apache-2.0".to_string()));
        assert_eq!(merged.get("author"), Some(&"default".to_string()));
    }

    #[test]
    fn custom_preset_overrides_builtin() {
        let mut registry = PresetRegistry {
            built_in: PresetRegistry::built_in_presets(),
            custom: IndexMap::new(),
        };

        // Add a custom preset with the same name as a built-in
        registry.custom.insert(
            "binary".to_string(),
            Preset {
                name: "binary".to_string(),
                description: "Custom binary preset".to_string(),
                pack: "custom-pack".to_string(),
                variables: IndexMap::new(),
            },
        );

        let preset = registry.get("binary").expect("get binary");
        assert_eq!(preset.description, "Custom binary preset");
        assert_eq!(preset.pack, "custom-pack");
    }
}
