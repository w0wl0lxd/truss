use crate::error::{Error, Result};
use crate::template::{Template, TemplateFile};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Dir,
    File,
    Json,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub name: String,
    pub source: String,
    #[serde(default)]
    pub kind: Kind,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub pointer: Option<String>,
    #[serde(default)]
    pub file_mode: Option<String>,
    #[serde(default)]
    pub dir_mode: Option<String>,
}

impl RegistryEntry {
    pub fn to_template(&self) -> Result<Template> {
        let source = Path::new(&self.source);
        let file_mode = self
            .file_mode
            .as_ref()
            .map(|m| parse_mode(m.as_str()))
            .transpose()?;

        match self.kind {
            Kind::Dir => {
                let mut template = Template::from_directory(source)?;
                template.name.clone_from(&self.name);
                if let Some(mode) = file_mode {
                    for file in &mut template.files {
                        file.mode = Some(mode);
                    }
                }
                Ok(template)
            }
            Kind::File => {
                let target = self
                    .targets
                    .first()
                    .ok_or_else(|| {
                        Error::Argument(format!("file entry {} is missing a target", self.name))
                    })?
                    .clone();
                let content = std::fs::read_to_string(source)?;
                Ok(Template::new(
                    self.name.clone(),
                    vec![TemplateFile {
                        path: target,
                        content,
                        mode: file_mode,
                    }],
                ))
            }
            Kind::Json => Err(Error::UnsupportedKind(self.name.clone())),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    entries: IndexMap<String, RegistryEntry>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load() -> Result<Self> {
        let mut registry = Self::new();

        let system_path = PathBuf::from("/etc/nixos/truss/registry.json");
        if system_path.try_exists()? {
            registry = Self::load_from(&system_path)?;
        }

        if let Some(path) = directories::BaseDirs::new()
            .map(|b| b.config_dir().join("truss").join("registry.json"))
        {
            if path.try_exists()? {
                let user = Self::load_from(&path)?;
                registry.entries.extend(user.entries);
            }
        }

        Ok(registry)
    }

    pub fn save(&self) -> Result<()> {
        let user_path = directories::BaseDirs::new()
            .map(|b| b.config_dir().join("truss").join("registry.json"))
            .ok_or(Error::ProjectDir)?;

        if let Some(parent) = user_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&user_path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn add(&mut self, entry: RegistryEntry) -> Result<&RegistryEntry> {
        if entry.name.is_empty() {
            return Err(Error::Argument("registry entry name cannot be empty".to_string()));
        }

        let key = entry.name.clone();
        self.entries.insert(key.clone(), entry);
        self.entries.get(&key).ok_or_else(|| {
            Error::Argument(format!("entry {key:?} missing after insert"))
        })
    }

    pub fn get(&self, name: &str) -> Option<&RegistryEntry> {
        self.entries.get(name)
    }

    pub fn entries(&self) -> &IndexMap<String, RegistryEntry> {
        &self.entries
    }

    fn load_from(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        serde_json::from_reader(reader).map_err(Error::Json)
    }
}

fn parse_mode(value: &str) -> Result<u32> {
    if let Some(stripped) = value.strip_prefix("0o") {
        u32::from_str_radix(stripped, 8).map_err(|_| {
            Error::Argument(format!("invalid octal mode {value:?}"))
        })
    } else {
        value.parse::<u32>().map_err(|_| {
            Error::Argument(format!("invalid mode {value:?}"))
        })
    }
}
