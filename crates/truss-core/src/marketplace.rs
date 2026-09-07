use crate::error::{Error, Result};
use crate::registry::{Kind, RegistryEntry};
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceEntry {
    pub name: String,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub source: String,
    pub kind: Kind,
    #[serde(default)]
    #[serde(rename = "ref")]
    pub pointer: Option<String>,
    #[serde(default)]
    pub subfolder: Option<String>,
    #[serde(default)]
    pub version: String,
}

impl MarketplaceEntry {
    pub fn to_registry_entry(&self) -> RegistryEntry {
        RegistryEntry {
            name: self.name.clone(),
            source: self.source.clone(),
            kind: self.kind.clone(),
            targets: Vec::new(),
            pointer: self.pointer.clone(),
            subfolder: self.subfolder.clone(),
            file_mode: None,
            auth_env: None,
            ssh_key: None,
            // Installing through the marketplace is what stamps this; see
            // `Registry::add`. A hand-written registry entry never carries it.
            marketplace: false,
            marketplace_version: (!self.version.is_empty()).then(|| self.version.clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceIndex {
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<MarketplaceEntry>,
    /// True when the index was fetched over the network. Never serialized: it
    /// describes where this copy came from, not the index itself.
    #[serde(skip)]
    pub remote: bool,
}

/// Set to `1` to accept a marketplace index served over plain HTTP.
const INSECURE_MARKETPLACE_ENV: &str = "TRUSS_ALLOW_INSECURE_MARKETPLACE";

/// True when the operator has accepted plain-HTTP marketplace indexes.
fn insecure_marketplace_allowed() -> bool {
    std::env::var(INSECURE_MARKETPLACE_ENV).is_ok_and(|v| v == "1")
}

/// Refuse an index body larger than this. `fetch_http` buffers before parsing,
/// so without a cap a marketplace server can exhaust client memory.
const MAX_INDEX_BYTES: u64 = 8 * 1024 * 1024;

impl MarketplaceIndex {
    pub fn load(source: &str) -> Result<Self> {
        let remote = source.starts_with("https://") || source.starts_with("http://");

        let content = if remote {
            if source.starts_with("http://") && !insecure_marketplace_allowed() {
                // A warning does not stop the install. The listings decide
                // which repositories are cloned and which template files run
                // hooks on this machine, so anyone on the network path chooses
                // what executes. Refuse, and make the operator opt in.
                return Err(Error::Validation(format!(
                    "marketplace index {source} is served over plain HTTP, so anyone on the \
                     network path can choose the templates this machine installs. Use https://, \
                     or set {INSECURE_MARKETPLACE_ENV}=1 to accept that risk."
                )));
            }
            fetch_http(source)?
        } else if let Some(path) = source.strip_prefix("file://") {
            std::fs::read_to_string(path)?
        } else {
            std::fs::read_to_string(source)?
        };

        let mut index: Self = serde_json::from_str(&content).map_err(Error::Json)?;
        index.remote = remote;
        index.validate()?;
        Ok(index)
    }

    /// Reject entries a remote index must not be allowed to publish.
    ///
    /// A `dir` or `file` entry names a path on the machine that installs it. If
    /// a remote index could set one, it would decide which local directory gets
    /// read as template content, and the contents would land in the generated
    /// project. Only a local index may point at local paths.
    /// Reject a listing that could not be installed, before any of it reaches
    /// the registry. Index contents are untrusted input.
    fn validate(&self) -> Result<()> {
        let mut seen: IndexSet<&str> = IndexSet::new();
        for entry in &self.entries {
            if entry.name.trim().is_empty() {
                return Err(Error::Validation(
                    "marketplace index contains an entry with an empty name".to_string(),
                ));
            }
            if !seen.insert(entry.name.as_str()) {
                return Err(Error::Validation(format!(
                    "marketplace index lists {:?} more than once",
                    entry.name
                )));
            }
            if entry.source.trim().is_empty() {
                return Err(Error::Validation(format!(
                    "marketplace entry {:?} has an empty source",
                    entry.name
                )));
            }
            if matches!(entry.kind, Kind::Git) {
                crate::git::GitUrl::parse(&entry.source).map_err(|e| {
                    Error::Validation(format!(
                        "marketplace entry {:?} has an unusable git source: {e}",
                        entry.name
                    ))
                })?;
            }
            // `to_registry_entry` carries no targets and no file mode, so a
            // `file` listing can never satisfy `Registry::add`, and `json` is
            // rejected there outright. Listing either advertises a template
            // that fails at install time, so refuse it at the index.
            if !matches!(entry.kind, Kind::Git | Kind::Dir) {
                return Err(Error::Validation(format!(
                    "marketplace entry {:?} declares kind {:?}; a marketplace may only list \
                     git or dir templates",
                    entry.name, entry.kind
                )));
            }
            // A dir or file entry names a path on the installing machine, which
            // a remote index has no business choosing.
            if self.remote && !matches!(entry.kind, Kind::Git) {
                return Err(Error::Validation(format!(
                    "remote marketplace entry {:?} declares kind {:?}; a remote index may only list git templates",
                    entry.name, entry.kind
                )));
            }
        }
        Ok(())
    }

    pub fn search(&self, keyword: &str, tag: Option<&str>) -> Vec<&MarketplaceEntry> {
        let keyword_lower = keyword.to_ascii_lowercase();
        self.entries
            .iter()
            .filter(|entry| {
                let matches_keyword = entry.name.to_ascii_lowercase().contains(&keyword_lower)
                    || entry
                        .description
                        .to_ascii_lowercase()
                        .contains(&keyword_lower)
                    || entry
                        .tags
                        .iter()
                        .any(|t| t.to_ascii_lowercase().contains(&keyword_lower));

                let matches_tag =
                    tag.is_none_or(|t| entry.tags.iter().any(|tag| tag.eq_ignore_ascii_case(t)));

                matches_keyword && matches_tag
            })
            .collect()
    }

    pub fn find(&self, name: &str) -> Option<&MarketplaceEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// Add an entry, replacing any existing one with the same name.
    ///
    /// `find` returns the first match, so appending a duplicate would leave the
    /// stale listing in charge and show the template twice when browsing.
    pub fn add_entry(&mut self, entry: MarketplaceEntry) {
        match self.entries.iter_mut().find(|e| e.name == entry.name) {
            Some(existing) => *existing = entry,
            None => self.entries.push(entry),
        }
    }
}

fn fetch_http(url: &str) -> Result<String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .build();

    let response = agent
        .get(url)
        .call()
        .map_err(|e| Error::Network(format!("failed to fetch {url}: {e}")))?;

    let status = response.status();
    if !(200..300).contains(&status) {
        return Err(Error::Network(format!(
            "HTTP error fetching {url}: {status}"
        )));
    }

    use std::io::Read;

    let mut body = String::new();
    // `take` caps what is read, so an oversized or endless body is refused
    // instead of buffered.
    let mut reader = response.into_reader().take(MAX_INDEX_BYTES + 1);
    reader
        .read_to_string(&mut body)
        .map_err(|e| Error::Network(format!("failed to read response body: {e}")))?;

    if body.len() as u64 > MAX_INDEX_BYTES {
        return Err(Error::Network(format!(
            "marketplace index at {url} exceeds the {MAX_INDEX_BYTES} byte limit"
        )));
    }
    Ok(body)
}

pub fn marketplace_index_path() -> Result<std::path::PathBuf> {
    directories::BaseDirs::new()
        .map(|b| b.config_dir().join("truss").join("marketplace.json"))
        .ok_or(Error::ProjectDir)
}

pub fn default_marketplace_source() -> String {
    if let Ok(url) = std::env::var("TRUSS_MARKETPLACE_INDEX") {
        if !url.is_empty() {
            return url;
        }
    }

    match marketplace_index_path() {
        Ok(path) if path.exists() => path.display().to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_by_keyword() {
        let index = MarketplaceIndex {
            version: 1,
            remote: false,
            entries: vec![
                MarketplaceEntry {
                    name: "web-service".to_string(),
                    description: "A web service template".to_string(),
                    author: "test".to_string(),
                    tags: vec!["web".to_string(), "rust".to_string()],
                    source: "https://example.com/web".to_string(),
                    kind: Kind::Git,
                    pointer: None,
                    subfolder: None,
                    version: "1.0.0".to_string(),
                },
                MarketplaceEntry {
                    name: "cli-tool".to_string(),
                    description: "A CLI tool template".to_string(),
                    author: "test".to_string(),
                    tags: vec!["cli".to_string(), "rust".to_string()],
                    source: "https://example.com/cli".to_string(),
                    kind: Kind::Git,
                    pointer: None,
                    subfolder: None,
                    version: "1.0.0".to_string(),
                },
            ],
        };

        let results = index.search("web", None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "web-service");
    }

    #[test]
    fn test_search_by_tag() {
        let index = MarketplaceIndex {
            version: 1,
            remote: false,
            entries: vec![
                MarketplaceEntry {
                    name: "web-service".to_string(),
                    description: "A web service template".to_string(),
                    author: "test".to_string(),
                    tags: vec!["web".to_string(), "rust".to_string()],
                    source: "https://example.com/web".to_string(),
                    kind: Kind::Git,
                    pointer: None,
                    subfolder: None,
                    version: "1.0.0".to_string(),
                },
                MarketplaceEntry {
                    name: "cli-tool".to_string(),
                    description: "A CLI tool template".to_string(),
                    author: "test".to_string(),
                    tags: vec!["cli".to_string(), "rust".to_string()],
                    source: "https://example.com/cli".to_string(),
                    kind: Kind::Git,
                    pointer: None,
                    subfolder: None,
                    version: "1.0.0".to_string(),
                },
            ],
        };

        let results = index.search("", Some("rust"));
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_find_by_name() {
        let index = MarketplaceIndex {
            version: 1,
            remote: false,
            entries: vec![MarketplaceEntry {
                name: "web-service".to_string(),
                description: "A web service template".to_string(),
                author: "test".to_string(),
                tags: vec!["web".to_string()],
                source: "https://example.com/web".to_string(),
                kind: Kind::Git,
                pointer: None,
                subfolder: None,
                version: "1.0.0".to_string(),
            }],
        };

        let entry = index.find("web-service");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().name, "web-service");

        let missing = index.find("missing");
        assert!(missing.is_none());
    }

    #[test]
    fn test_to_registry_entry() {
        let entry = MarketplaceEntry {
            name: "web-service".to_string(),
            description: "A web service template".to_string(),
            author: "test".to_string(),
            tags: vec!["web".to_string()],
            source: "https://example.com/web".to_string(),
            kind: Kind::Git,
            pointer: Some("main".to_string()),
            subfolder: Some("template".to_string()),
            version: "1.0.0".to_string(),
        };

        let registry_entry = entry.to_registry_entry();
        assert_eq!(registry_entry.name, "web-service");
        assert_eq!(registry_entry.source, "https://example.com/web");
        assert_eq!(registry_entry.kind, Kind::Git);
        assert_eq!(registry_entry.pointer, Some("main".to_string()));
        assert_eq!(registry_entry.subfolder, Some("template".to_string()));
    }

    #[test]
    fn test_load_from_json() {
        let json = r#"{
            "version": 1,
            "entries": [
                {
                    "name": "test",
                    "description": "Test template",
                    "author": "test",
                    "tags": ["test"],
                    "source": "https://example.com/test",
                    "kind": "git",
                    "ref": "main",
                    "subfolder": null,
                    "version": "1.0.0"
                }
            ]
        }"#;

        let index: MarketplaceIndex = serde_json::from_str(json).unwrap();
        assert_eq!(index.version, 1);
        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.entries[0].name, "test");
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;

    fn entry(name: &str, kind: Kind) -> MarketplaceEntry {
        MarketplaceEntry {
            name: name.into(),
            description: "d".into(),
            author: "a".into(),
            tags: vec![],
            source: "https://example.com/repo.git".into(),
            kind,
            pointer: None,
            subfolder: None,
            version: "1.0.0".into(),
        }
    }

    #[test]
    fn a_remote_index_may_not_list_local_paths() {
        // A dir entry names a path on the installing machine, so a remote index
        // could otherwise choose which local directory is read as template
        // content and copied into the generated project.
        let index = MarketplaceIndex {
            version: 1,
            entries: vec![entry("local", Kind::Dir)],
            remote: true,
        };
        let err = index.validate().unwrap_err().to_string();
        assert!(err.contains("local"), "unexpected error: {err}");

        let index = MarketplaceIndex {
            version: 1,
            entries: vec![entry("remote", Kind::Git)],
            remote: true,
        };
        index.validate().expect("git entries are allowed");
    }

    #[test]
    fn a_local_index_may_list_local_paths() {
        let index = MarketplaceIndex {
            version: 1,
            entries: vec![entry("local", Kind::Dir)],
            remote: false,
        };
        index
            .validate()
            .expect("a local index may point at local paths");
    }

    #[test]
    fn republishing_replaces_the_existing_listing() {
        let mut index = MarketplaceIndex {
            version: 1,
            entries: vec![],
            remote: false,
        };
        index.add_entry(entry("pack", Kind::Git));

        let mut updated = entry("pack", Kind::Git);
        updated.source = "https://example.com/new.git".into();
        index.add_entry(updated);

        assert_eq!(index.entries.len(), 1, "a republish must not duplicate");
        assert_eq!(
            index.find("pack").expect("entry").source,
            "https://example.com/new.git",
            "find must return the new listing, not the stale one"
        );
    }
    fn index_json(kind: &str) -> String {
        format!(
            r#"{{"version":1,"entries":[{{"name":"t","description":"d","author":"a",
               "tags":[],"source":"/tmp/x","kind":"{kind}","ref":null,"subfolder":null,
               "version":"1.0.0"}}]}}"#
        )
    }

    // `to_registry_entry` carries no targets, so `Registry::add` rejects a file
    // listing, and it rejects json outright. Both would advertise a template
    // that can never install.
    #[test]
    fn validate_rejects_uninstallable_kinds() {
        for kind in ["file", "json"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("index.json");
            std::fs::write(&path, index_json(kind)).unwrap();
            let err = MarketplaceIndex::load(path.to_str().unwrap())
                .expect_err("an uninstallable kind must be refused")
                .to_string();
            assert!(err.contains("git or dir"), "unexpected error: {err}");
        }
        // `dir` is installable, so it still loads.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.json");
        std::fs::write(&path, index_json("dir")).unwrap();
        MarketplaceIndex::load(path.to_str().unwrap()).expect("dir must still load");
    }

    // A plain-HTTP index chooses which repositories are cloned and which
    // template hooks run, so a warning is not enough.
    #[test]
    fn load_refuses_a_plain_http_index() {
        // No request is made: the scheme is refused before the fetch.
        let err = MarketplaceIndex::load("http://example.invalid/index.json")
            .expect_err("plain HTTP must be refused")
            .to_string();
        assert!(err.contains("plain HTTP"), "unexpected error: {err}");
        assert!(
            err.contains(INSECURE_MARKETPLACE_ENV),
            "the error must name the opt-in: {err}"
        );
    }
}
