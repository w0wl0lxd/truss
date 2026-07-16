use crate::error::{Error, Result};
use crate::git::{GitCache, GitUrl};
use crate::pathsafe::normalize_relative_path;
use crate::template::{Template, TemplateFile};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Dir,
    File,
    Git,
    Json,
}

impl std::str::FromStr for Kind {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "dir" => Ok(Self::Dir),
            "file" => Ok(Self::File),
            "git" => Ok(Self::Git),
            "json" => Ok(Self::Json),
            other => Err(Error::Argument(format!(
                "unknown registry kind {other:?} (expected dir, file, git, or json)"
            ))),
        }
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dir => write!(f, "dir"),
            Self::File => write!(f, "file"),
            Self::Git => write!(f, "git"),
            Self::Json => write!(f, "json"),
        }
    }
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
    pub subfolder: Option<String>,
    #[serde(default)]
    pub file_mode: Option<String>,
    #[serde(default)]
    pub auth_env: Option<String>,
    #[serde(default)]
    pub ssh_key: Option<String>,
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
                if self.targets.is_empty() {
                    return Err(Error::Argument(format!(
                        "file entry {} is missing a target",
                        self.name
                    )));
                }
                let content = std::fs::read_to_string(source)?;
                let mut files = Vec::with_capacity(self.targets.len());
                for target in &self.targets {
                    let target = normalize_relative_path(target)?;
                    files.push(TemplateFile {
                        path: target,
                        content: content.clone(),
                        mode: file_mode,
                    });
                }
                Ok(Template::new(self.name.clone(), files))
            }
            Kind::Git => {
                let url = GitUrl::parse(&self.source)?;
                let cache = GitCache::for_entry(&self.name)?;
                let dir = cache.resolve_with_auth(
                    &url,
                    self.pointer.as_deref(),
                    self.subfolder.as_deref(),
                    self,
                )?;
                let mut template = Template::from_directory(&dir)?;
                template.name.clone_from(&self.name);
                if let Some(mode) = file_mode {
                    for file in &mut template.files {
                        file.mode = Some(mode);
                    }
                }
                Ok(template)
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

    /// Platform user registry path (`$XDG_CONFIG_HOME/truss/registry.json`).
    pub fn user_path() -> Result<PathBuf> {
        directories::BaseDirs::new()
            .map(|b| b.config_dir().join("truss").join("registry.json"))
            .ok_or(Error::ProjectDir)
    }

    pub fn load() -> Result<Self> {
        let mut registry = Self::new();

        // Optional site-wide registry. Prefer TRUSS_SYSTEM_REGISTRY if set;
        // otherwise try common multi-user locations without hard-coding a single OS layout.
        for system_path in system_registry_candidates() {
            if system_path.try_exists()? {
                registry = Self::load_from(&system_path)?;
                break;
            }
        }

        let user_path = Self::user_path()?;
        if user_path.try_exists()? {
            let user = Self::load_from(&user_path)?;
            registry.entries.extend(user.entries);
        }

        Ok(registry)
    }

    /// Load only the user registry file (for mutation / remove).
    pub fn load_user() -> Result<Self> {
        let path = Self::user_path()?;
        if path.try_exists()? {
            Self::load_from(&path)
        } else {
            Ok(Self::new())
        }
    }

    pub fn save(&self) -> Result<()> {
        let user_path = Self::user_path()?;
        if let Some(parent) = user_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&user_path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Validate source and insert (or replace when `force`).
    pub fn add(&mut self, entry: RegistryEntry, force: bool) -> Result<&RegistryEntry> {
        if entry.name.is_empty() {
            return Err(Error::Argument(
                "registry entry name cannot be empty".to_string(),
            ));
        }
        if !force && self.entries.contains_key(&entry.name) {
            return Err(Error::Argument(format!(
                "registry entry {:?} already exists (pass --force to replace)",
                entry.name
            )));
        }
        validate_entry_source(&entry)?;
        let key = entry.name.clone();
        self.entries.insert(key.clone(), entry);
        self.entries
            .get(&key)
            .ok_or_else(|| Error::Argument(format!("entry {key:?} missing after insert")))
    }

    pub fn remove(&mut self, name: &str) -> Result<RegistryEntry> {
        self.entries
            .shift_remove(name)
            .ok_or_else(|| Error::Argument(format!("registry entry {name:?} not found")))
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

fn validate_entry_source(entry: &RegistryEntry) -> Result<()> {
    match entry.kind {
        Kind::Dir => {
            let source = Path::new(&entry.source);
            if !source.try_exists()? {
                return Err(Error::Argument(format!(
                    "registry source does not exist: {}",
                    entry.source
                )));
            }
            if !source.is_dir() {
                return Err(Error::Argument(format!(
                    "registry source is not a directory: {}",
                    entry.source
                )));
            }
        }
        Kind::File => {
            let source = Path::new(&entry.source);
            if !source.try_exists()? {
                return Err(Error::Argument(format!(
                    "registry source does not exist: {}",
                    entry.source
                )));
            }
            if !source.is_file() {
                return Err(Error::Argument(format!(
                    "registry source is not a file: {}",
                    entry.source
                )));
            }
            if entry.targets.is_empty() {
                return Err(Error::Argument(
                    "file registry entries require at least one --target".to_string(),
                ));
            }
            for target in &entry.targets {
                normalize_relative_path(target)?;
            }
        }
        Kind::Git => {
            // Validate URL syntax and reject local filesystem paths.
            GitUrl::parse(&entry.source)?;
            if !entry.targets.is_empty() {
                return Err(Error::Argument(
                    "git registry entries do not use --target".to_string(),
                ));
            }
            if let Some(sub) = &entry.subfolder {
                normalize_relative_path(sub)?;
            }
            // Validate auth fields don't contain secret values
            if let Some(auth_env) = &entry.auth_env {
                if looks_like_secret(auth_env) {
                    return Err(Error::InvalidCredentialSource(
                        "auth_env value appears to be a secret (use environment variable name instead)".into(),
                    ));
                }
            }
            if let Some(ssh_key) = &entry.ssh_key {
                // Validate it's a path, not a secret
                if looks_like_secret(ssh_key) {
                    return Err(Error::InvalidCredentialSource(
                        "ssh_key value appears to be a secret (use path to key file instead)"
                            .into(),
                    ));
                }
            }
        }
        Kind::Json => {
            return Err(Error::UnsupportedKind(entry.name.clone()));
        }
    }
    Ok(())
}

fn looks_like_secret(value: &str) -> bool {
    // Heuristic: if it looks like a token or password, it's probably a secret
    value.len() > 20
        && (value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            || value.contains(':'))
}

fn parse_mode(value: &str) -> Result<u32> {
    let stripped = match value.strip_prefix("0o") {
        Some(v) => v,
        None => value,
    };
    let mode = u32::from_str_radix(stripped, 8)
        .map_err(|_| Error::Argument(format!("invalid octal mode {value:?}")))?;
    if mode & !0o777 != 0 {
        return Err(Error::Argument(format!(
            "file_mode contains special bits: {value:?}"
        )));
    }
    Ok(mode)
}

/// Candidate paths for an optional site-wide registry (read-only layer).
fn system_registry_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(path) = std::env::var("TRUSS_SYSTEM_REGISTRY") {
        if !path.is_empty() {
            out.push(PathBuf::from(path));
        }
    }
    // Common multi-user install prefixes (optional; missing paths are ignored).
    out.push(PathBuf::from("/etc/nixos/truss/registry.json"));
    out.push(PathBuf::from("/etc/truss/registry.json"));
    out.push(PathBuf::from("/usr/local/etc/truss/registry.json"));
    out
}
