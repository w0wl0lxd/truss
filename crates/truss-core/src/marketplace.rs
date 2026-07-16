use crate::error::{Error, Result};
use crate::registry::{Kind, RegistryEntry};
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceIndex {
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<MarketplaceEntry>,
}

impl MarketplaceIndex {
    pub fn load(source: &str) -> Result<Self> {
        let content = if source.starts_with("https://") {
            fetch_http(source)?
        } else if source.starts_with("file://") {
            let path = match source.strip_prefix("file://") {
                Some(p) => p,
                None => source,
            };
            std::fs::read_to_string(path)?
        } else {
            std::fs::read_to_string(source)?
        };

        let index: Self = serde_json::from_str(&content).map_err(Error::Json)?;
        Ok(index)
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

                let matches_tag = tag.is_none_or(|t| {
                    entry.tags.iter().any(|tag| {
                        tag.eq_ignore_ascii_case(t)
                    })
                });

                matches_keyword && matches_tag
            })
            .collect()
    }

    pub fn find(&self, name: &str) -> Option<&MarketplaceEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    pub fn add_entry(&mut self, entry: MarketplaceEntry) {
        self.entries.push(entry);
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

    let body = response
        .into_string()
        .map_err(|e| Error::Network(format!("failed to read response body: {e}")))?;
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
