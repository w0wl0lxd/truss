use crate::error::{Error, Result};
use indexmap::IndexMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct UnifyConfig {
    pub allowlist: Vec<String>,
    pub blocklist: Vec<String>,
}

impl UnifyConfig {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        let doc = content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| Error::Argument(format!("failed to parse unify config: {e}")))?;

        let mut config = Self::default();
        if let Some(table) = doc.get("allowlist").and_then(|a| a.as_array()) {
            for item in table {
                if let Some(s) = item.as_str() {
                    config.allowlist.push(s.to_string());
                }
            }
        }
        if let Some(table) = doc.get("blocklist").and_then(|b| b.as_array()) {
            for item in table {
                if let Some(s) = item.as_str() {
                    config.blocklist.push(s.to_string());
                }
            }
        }
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        for dep in &self.allowlist {
            if self.blocklist.contains(dep) {
                return Err(Error::Argument(format!(
                    "dependency '{dep}' is in both allowlist and blocklist"
                )));
            }
        }
        Ok(())
    }

    pub fn should_unify(&self, dep_name: &str, occurrence_count: usize) -> bool {
        if self.blocklist.iter().any(|d| d == dep_name) {
            return false;
        }
        if !self.allowlist.is_empty() {
            return self.allowlist.iter().any(|d| d == dep_name);
        }
        occurrence_count >= 2
    }
}

#[derive(Debug, Clone)]
pub struct DependencyInfo {
    pub name: String,
    pub version: String,
    pub features: Vec<String>,
    pub default_features: bool,
    pub optional: bool,
    pub source_path: PathBuf,
    pub is_workspace_ref: bool,
}

#[derive(Debug, Clone)]
pub struct DriftEntry {
    pub member_path: PathBuf,
    pub dependency: String,
    pub root_version: Option<String>,
    pub member_version: String,
    pub kind: DriftKind,
}

#[derive(Debug, Clone)]
pub enum DriftKind {
    VersionMismatch,
    MissingInRoot,
    NotWorkspaceRef,
    FeaturesDiffers,
}

#[derive(Debug, Clone)]
pub struct UnifyPlan {
    pub root_additions: IndexMap<String, String>,
    pub root_updates: IndexMap<String, String>,
    pub member_changes: Vec<MemberChange>,
}

#[derive(Debug, Clone)]
pub struct MemberChange {
    pub path: PathBuf,
    pub dependency: String,
    pub change: ChangeKind,
}

#[derive(Debug, Clone)]
pub enum ChangeKind {
    ToWorkspaceRef,
    VersionUpdate(String),
}

#[derive(Debug, Clone, Default)]
pub struct UnifyOptions {
    pub dry_run: bool,
    pub config: UnifyConfig,
}

pub fn check_dependency_drift(workspace_root: &Path) -> Result<Vec<DriftEntry>> {
    let root_manifest = workspace_root.join("Cargo.toml");
    if !root_manifest.exists() {
        return Err(Error::Argument(format!(
            "workspace root not found at {}",
            root_manifest.display()
        )));
    }

    let root_content = std::fs::read_to_string(&root_manifest)?;
    let root_doc = root_content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| Error::Argument(format!("failed to parse root Cargo.toml: {e}")))?;

    let workspace_deps = extract_workspace_dependencies(&root_doc);

    let members = get_workspace_members(&root_doc, workspace_root);
    let mut drift = Vec::new();

    for member_path in &members {
        let member_manifest = member_path.join("Cargo.toml");
        if !member_manifest.exists() {
            continue;
        }

        let member_content = std::fs::read_to_string(&member_manifest)?;
        let member_doc = member_content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| {
                Error::Argument(format!(
                    "failed to parse {}: {e}",
                    member_manifest.display()
                ))
            })?;

        if let Some(deps) = member_doc.get("dependencies").and_then(|d| d.as_table()) {
            for (dep_name, dep_item) in deps {
                if let Some(dep_table) = dep_item.as_table() {
                    let is_workspace = dep_table.get("workspace").and_then(toml_edit::Item::as_bool).is_some_and(|b| b);
                    let version = dep_table.get("version").and_then(|v| v.as_str());

                    if let Some(root_version) = workspace_deps.get(dep_name) {
                        if is_workspace {
                            continue;
                        }

                        if let Some(member_version) = version {
                            if member_version != root_version {
                                drift.push(DriftEntry {
                                    member_path: member_path.clone(),
                                    dependency: dep_name.to_string(),
                                    root_version: Some(root_version.clone()),
                                    member_version: member_version.to_string(),
                                    kind: DriftKind::VersionMismatch,
                                });
                            }
                        } else {
                            drift.push(DriftEntry {
                                member_path: member_path.clone(),
                                dependency: dep_name.to_string(),
                                root_version: Some(root_version.clone()),
                                member_version: "workspace = false".to_string(),
                                kind: DriftKind::NotWorkspaceRef,
                            });
                        }
                    } else if version.is_some() || !is_workspace {
                        drift.push(DriftEntry {
                            member_path: member_path.clone(),
                            dependency: dep_name.to_string(),
                            root_version: None,
                            member_version: version.map(std::string::ToString::to_string).unwrap_or_else(|| "none".to_string()),
                            kind: DriftKind::MissingInRoot,
                        });
                    }
                } else if let Some(version_str) = dep_item.as_str() {
                    if let Some(root_version) = workspace_deps.get(dep_name) {
                        if version_str != root_version {
                            drift.push(DriftEntry {
                                member_path: member_path.clone(),
                                dependency: dep_name.to_string(),
                                root_version: Some(root_version.clone()),
                                member_version: version_str.to_string(),
                                kind: DriftKind::VersionMismatch,
                            });
                        }
                    } else {
                        drift.push(DriftEntry {
                            member_path: member_path.clone(),
                            dependency: dep_name.to_string(),
                            root_version: None,
                            member_version: version_str.to_string(),
                            kind: DriftKind::MissingInRoot,
                        });
                    }
                }
            }
        }
    }

    Ok(drift)
}

pub fn unify_dependencies(workspace_root: &Path, options: &UnifyOptions) -> Result<UnifyPlan> {
    let root_manifest = workspace_root.join("Cargo.toml");
    if !root_manifest.exists() {
        return Err(Error::Argument(format!(
            "workspace root not found at {}",
            root_manifest.display()
        )));
    }

    let config_path = workspace_root.join(".truss/unify.toml");
    let config = if config_path.exists() {
        UnifyConfig::load(&config_path)?
    } else {
        options.config.clone()
    };

    let root_content = std::fs::read_to_string(&root_manifest)?;
    let root_doc = root_content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| Error::Argument(format!("failed to parse root Cargo.toml: {e}")))?;

    let members = get_workspace_members(&root_doc, workspace_root);
    let mut dep_occurrences: IndexMap<String, usize> = IndexMap::new();
    let mut member_deps: Vec<(PathBuf, String, DependencyInfo)> = Vec::new();

    for member_path in &members {
        let member_manifest = member_path.join("Cargo.toml");
        if !member_manifest.exists() {
            continue;
        }

        let member_content = std::fs::read_to_string(&member_manifest)?;
        let member_doc = member_content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| {
                Error::Argument(format!(
                    "failed to parse {}: {e}",
                    member_manifest.display()
                ))
            })?;

        if let Some(deps) = member_doc.get("dependencies").and_then(|d| d.as_table()) {
            for (dep_name, dep_item) in deps {
                let (version, is_workspace) = if let Some(dep_table) = dep_item.as_table() {
                    let is_ws = dep_table.get("workspace").and_then(toml_edit::Item::as_bool).is_some_and(|b| b);
                    let ver = dep_table.get("version").and_then(|v| v.as_str());
                    (ver.map(std::string::ToString::to_string), is_ws)
                } else {
                    (dep_item.as_str().map(std::string::ToString::to_string), false)
                };

                if let Some(ver) = version {
                    *dep_occurrences.entry(dep_name.to_string()).or_insert(0) += 1;
                    member_deps.push((
                        member_path.clone(),
                        dep_name.to_string(),
                        DependencyInfo {
                            name: dep_name.to_string(),
                            version: ver.clone(),
                            features: Vec::new(),
                            default_features: true,
                            optional: false,
                            source_path: member_manifest.clone(),
                            is_workspace_ref: is_workspace,
                        },
                    ));
                }
            }
        }
    }

    let mut plan = UnifyPlan {
        root_additions: IndexMap::new(),
        root_updates: IndexMap::new(),
        member_changes: Vec::new(),
    };

    let existing_workspace_deps = extract_workspace_dependencies(&root_doc);

    for (dep_name, &count) in &dep_occurrences {
        if !config.should_unify(dep_name, count) {
            continue;
        }

        let dep_infos: Vec<_> = member_deps
            .iter()
            .filter(|(_, name, _)| name == dep_name)
            .collect();

        if dep_infos.is_empty() {
            continue;
        }

        let versions: Vec<_> = dep_infos.iter().map(|(_, _, info)| &info.version).collect();

        if versions.windows(2).any(|w| w.first() != w.last()) {
            return Err(Error::UnificationConflict(format!(
                "dependency '{dep_name}' has conflicting versions: {versions:?}"
            )));
        }

        let unified_version = versions.first().ok_or_else(|| {
            Error::UnificationConflict(format!("no version found for dependency '{dep_name}'"))
        })?;

        if let Some(existing_version) = existing_workspace_deps.get(dep_name) {
            if existing_version.as_str() != *unified_version {
                plan.root_updates.insert(dep_name.clone(), (*unified_version).clone());
            }
        } else {
            plan.root_additions.insert(dep_name.clone(), (*unified_version).clone());
        }

        for (member_path, _, info) in dep_infos {
            if !info.is_workspace_ref {
                plan.member_changes.push(MemberChange {
                    path: member_path.clone(),
                    dependency: dep_name.clone(),
                    change: ChangeKind::ToWorkspaceRef,
                });
            }
        }
    }

    if !options.dry_run {
        apply_plan(workspace_root, &plan)?;
    }

    Ok(plan)
}

fn apply_plan(workspace_root: &Path, plan: &UnifyPlan) -> Result<()> {
    let root_manifest = workspace_root.join("Cargo.toml");
    let mut root_content = std::fs::read_to_string(&root_manifest)?;
    let mut root_doc = root_content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| Error::Argument(format!("failed to parse root Cargo.toml: {e}")))?;

    if let Some(workspace) = root_doc.get_mut("workspace").and_then(|w| w.as_table_mut()) {
        if !workspace.contains_key("dependencies") {
            let inline_table = toml_edit::InlineTable::new();
            workspace["dependencies"] = toml_edit::Item::Value(toml_edit::Value::InlineTable(inline_table));
        }

        if let Some(deps) = workspace.get_mut("dependencies").and_then(|d| d.as_inline_table_mut()) {
            for (dep_name, version) in &plan.root_additions {
                deps[dep_name] = toml_edit::Value::from(version.as_str());
            }
            for (dep_name, version) in &plan.root_updates {
                if let Some(existing) = deps.get_mut(dep_name) {
                    *existing = toml_edit::Value::from(version.as_str());
                }
            }
        }
    }

    root_content = root_doc.to_string();
    std::fs::write(&root_manifest, root_content)?;

    for change in &plan.member_changes {
        let member_manifest = &change.path;
        let mut member_content = std::fs::read_to_string(member_manifest)?;
        let mut member_doc = member_content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| Error::Argument(format!("failed to parse {}: {e}", member_manifest.display())))?;

        if let Some(deps) = member_doc.get_mut("dependencies").and_then(|d| d.as_table_mut()) {
            if let Some(dep_item) = deps.get_mut(&change.dependency) {
                if let Some(dep_table) = dep_item.as_table_mut() {
                    dep_table.remove("version");
                    dep_table["workspace"] = toml_edit::Item::Value(toml_edit::Value::from(true));
                } else if dep_item.is_str() {
                    let mut new_table = toml_edit::Table::new();
                    new_table.insert("workspace", toml_edit::Item::Value(toml_edit::Value::from(true)));
                    deps[&change.dependency] = toml_edit::Item::Table(new_table);
                }
            }
        }

        member_content = member_doc.to_string();
        std::fs::write(member_manifest, member_content)?;
    }

    Ok(())
}

fn extract_workspace_dependencies(doc: &toml_edit::DocumentMut) -> IndexMap<String, String> {
    let mut deps = IndexMap::new();
    if let Some(workspace) = doc.get("workspace").and_then(|w| w.as_table()) {
        if let Some(workspace_deps) = workspace.get("dependencies").and_then(|d| d.as_inline_table()) {
            for (key, value) in workspace_deps {
                if let Some(version) = value.as_str() {
                    deps.insert(key.to_string(), version.to_string());
                }
            }
        }
    }
    deps
}

fn get_workspace_members(doc: &toml_edit::DocumentMut, root: &Path) -> Vec<PathBuf> {
    let mut members = Vec::new();
    if let Some(workspace) = doc.get("workspace").and_then(|w| w.as_table()) {
        if let Some(members_item) = workspace.get("members") {
            if let Some(members_array) = members_item.as_array() {
                for member in members_array {
                    if let Some(member_str) = member.as_str() {
                        let member_path = root.join(member_str);
                        if member_path.exists() {
                            members.push(member_path);
                        }
                    }
                }
            } else if let Some(member_str) = members_item.as_str() {
                let member_path = root.join(member_str);
                if member_path.exists() {
                    members.push(member_path);
                }
            }
        }
    }
    members
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unify_config_default() {
        let config = UnifyConfig::default();
        assert!(config.allowlist.is_empty());
        assert!(config.blocklist.is_empty());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_unify_config_validation() {
        let config = UnifyConfig {
            allowlist: vec!["tokio".to_string()],
            blocklist: vec!["tokio".to_string()],
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_should_unify_without_config() {
        let config = UnifyConfig::default();
        assert!(config.should_unify("serde", 2));
        assert!(!config.should_unify("serde", 1));
    }

    #[test]
    fn test_should_unify_with_allowlist() {
        let config = UnifyConfig {
            allowlist: vec!["tokio".to_string()],
            blocklist: vec![],
        };
        assert!(config.should_unify("tokio", 1));
        assert!(!config.should_unify("serde", 5));
    }

    #[test]
    fn test_should_unify_with_blocklist() {
        let config = UnifyConfig {
            allowlist: vec![],
            blocklist: vec!["internal".to_string()],
        };
        assert!(!config.should_unify("internal", 5));
        assert!(config.should_unify("serde", 2));
    }
}
