use crate::error::{Error, Result};
use indexmap::{IndexMap, IndexSet};
use std::path::{Path, PathBuf};

/// The tables Cargo reads dependencies from, both at the top level of a
/// manifest and below a `[target.'cfg(...)']` key.
const DEP_KINDS: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

#[derive(Debug, Clone, Default)]
pub struct UnifyConfig {
    pub allowlist: Vec<String>,
    pub blocklist: Vec<String>,
}

/// Read a list of dependency names from the unify config.
///
/// A key of the wrong shape used to be dropped, which left the default policy
/// free to rewrite a dependency the workspace had explicitly excluded. A
/// configuration this file cannot express is an error, not an empty list.
fn string_array(doc: &toml_edit::DocumentMut, key: &str) -> Result<Vec<String>> {
    let Some(item) = doc.get(key) else {
        return Ok(Vec::new());
    };
    let Some(array) = item.as_array() else {
        return Err(Error::Argument(format!(
            "unify config: '{key}' must be an array of dependency names"
        )));
    };
    array
        .iter()
        .map(|value| {
            value.as_str().map(str::to_string).ok_or_else(|| {
                Error::Argument(format!(
                    "unify config: '{key}' must contain only dependency names, found {value}"
                ))
            })
        })
        .collect()
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

        let config = Self {
            allowlist: string_array(&doc, "allowlist")?,
            blocklist: string_array(&doc, "blocklist")?,
        };
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

    /// True when the configuration takes this dependency out of scope
    /// entirely, whatever its occurrence count.
    pub fn is_excluded(&self, dep_name: &str) -> bool {
        if self.blocklist.iter().any(|d| d == dep_name) {
            return true;
        }
        !self.allowlist.is_empty() && !self.allowlist.iter().any(|d| d == dep_name)
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

/// Where a dependency entry takes its version from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencySource {
    /// A registry version, the only kind a workspace entry can hold.
    Registry,
    /// `dep = { workspace = true }`.
    Workspace,
    /// A `path` or `git` source, which can never be inherited.
    Local,
}

/// How one dependency is declared in one manifest table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencySpec {
    pub version: Option<String>,
    pub features: Vec<String>,
    pub default_features: bool,
    pub optional: bool,
    pub source: DependencySource,
    /// True when the entry carries `package` or `registry`, so the table key is
    /// not the crate it resolves to. Such an entry cannot inherit from
    /// `[workspace.dependencies]` under its own key.
    pub renamed: bool,
}

impl Default for DependencySpec {
    fn default() -> Self {
        Self {
            version: None,
            features: Vec::new(),
            default_features: true,
            optional: false,
            source: DependencySource::Registry,
            renamed: false,
        }
    }
}

impl DependencySpec {
    /// Read a dependency entry written either as `dep = "1"`, as an inline
    /// table `dep = { version = "1" }`, or as a `[dependencies.dep]` section.
    fn parse(item: &toml_edit::Item) -> Option<Self> {
        if let Some(version) = item.as_str() {
            return Some(Self {
                version: Some(version.to_string()),
                ..Self::default()
            });
        }

        let table = item.as_table_like()?;
        let mut features: Vec<String> = table
            .get("features")
            .and_then(toml_edit::Item::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                    .collect()
            })
            .unwrap_or_else(Vec::new);
        features.sort_unstable();
        features.dedup();

        Some(Self {
            version: table
                .get("version")
                .and_then(|v| v.as_str())
                .map(std::string::ToString::to_string),
            features,
            // Defaults stay on unless the entry turns them off.
            default_features: table
                .get("default-features")
                .and_then(toml_edit::Item::as_bool)
                != Some(false),
            optional: table
                .get("optional")
                .and_then(toml_edit::Item::as_bool)
                .is_some_and(|b| b),
            source: if table
                .get("workspace")
                .and_then(toml_edit::Item::as_bool)
                .is_some_and(|b| b)
            {
                DependencySource::Workspace
            } else if table.contains_key("path") || table.contains_key("git") {
                DependencySource::Local
            } else {
                DependencySource::Registry
            },
            renamed: table.contains_key("package") || table.contains_key("registry"),
        })
    }

    /// True when the member already inherits from `[workspace.dependencies]`.
    fn is_workspace(&self) -> bool {
        self.source == DependencySource::Workspace
    }

    /// True when the entry names its own source, so it has nothing to inherit.
    fn is_local(&self) -> bool {
        self.source == DependencySource::Local
    }

    /// True when the entry cannot take part in unification at all.
    fn is_unifiable(&self) -> bool {
        !self.is_local() && !self.renamed
    }

    /// True when inheriting from the workspace would resolve the same features.
    fn features_match(&self, root: &Self) -> bool {
        self.default_features == root.default_features && self.features == root.features
    }
}

/// One dependency as declared by one workspace member.
#[derive(Debug, Clone)]
pub struct DependencyInfo {
    pub name: String,
    /// Directory of the member crate.
    pub member_path: PathBuf,
    /// Path of the member's `Cargo.toml`.
    pub manifest: PathBuf,
    /// Key path of the table holding the entry, such as `["dependencies"]` or
    /// `["target", "cfg(unix)", "dev-dependencies"]`.
    pub section: Vec<String>,
    pub spec: DependencySpec,
}

impl DependencyInfo {
    /// Name of the manifest table the dependency was declared in.
    pub fn section_label(&self) -> String {
        self.section.join(".")
    }
}

#[derive(Debug, Clone)]
pub struct DriftEntry {
    pub member_path: PathBuf,
    pub dependency: String,
    /// Manifest table the dependency was declared in.
    pub section: String,
    pub root_version: Option<String>,
    pub member_version: String,
    pub kind: DriftKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftKind {
    VersionMismatch,
    MissingInRoot,
    NotWorkspaceRef,
    FeaturesDiffers,
}

impl std::fmt::Display for DriftKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::VersionMismatch => "version mismatch",
            Self::MissingInRoot => "missing in workspace root",
            Self::NotWorkspaceRef => "not using workspace reference",
            Self::FeaturesDiffers => "features differ",
        };
        f.write_str(text)
    }
}

/// The `[workspace.dependencies]` entry a plan will write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootDependency {
    pub version: String,
    /// Written as `default-features = false` when the members turned defaults
    /// off. Cargo ignores that key on an inheriting member, so it has to live
    /// on the workspace entry.
    pub default_features: bool,
}

impl std::fmt::Display for RootDependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.default_features {
            write!(f, "\"{}\"", self.version)
        } else {
            write!(
                f,
                "{{ version = \"{}\", default-features = false }}",
                self.version
            )
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnifyPlan {
    pub root_additions: IndexMap<String, RootDependency>,
    pub root_updates: IndexMap<String, RootDependency>,
    pub member_changes: Vec<MemberChange>,
}

#[derive(Debug, Clone)]
pub struct MemberChange {
    /// Path of the member's `Cargo.toml`.
    pub path: PathBuf,
    /// Key path of the table holding the entry.
    pub section: Vec<String>,
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

fn read_manifest(path: &Path) -> Result<toml_edit::DocumentMut> {
    let content = std::fs::read_to_string(path)?;
    content
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| Error::Argument(format!("failed to parse {}: {e}", path.display())))
}

fn read_root_manifest(workspace_root: &Path) -> Result<toml_edit::DocumentMut> {
    let root_manifest = workspace_root.join("Cargo.toml");
    if !root_manifest.is_file() {
        return Err(Error::Argument(format!(
            "workspace root not found at {}",
            root_manifest.display()
        )));
    }
    read_manifest(&root_manifest)
}

/// Every dependency table in one manifest, paired with the key path that
/// reaches it.
fn dependency_sections(
    doc: &toml_edit::DocumentMut,
) -> Vec<(Vec<String>, &dyn toml_edit::TableLike)> {
    let mut sections: Vec<(Vec<String>, &dyn toml_edit::TableLike)> = Vec::new();

    for kind in DEP_KINDS {
        if let Some(table) = doc.get(kind).and_then(toml_edit::Item::as_table_like) {
            sections.push((vec![kind.to_string()], table));
        }
    }

    if let Some(targets) = doc.get("target").and_then(toml_edit::Item::as_table_like) {
        for (cfg, item) in targets.iter() {
            let Some(cfg_table) = item.as_table_like() else {
                continue;
            };
            for kind in DEP_KINDS {
                if let Some(table) = cfg_table.get(kind).and_then(toml_edit::Item::as_table_like) {
                    sections.push((
                        vec!["target".to_string(), cfg.to_string(), kind.to_string()],
                        table,
                    ));
                }
            }
        }
    }

    sections
}

/// Every dependency declared by every workspace member, across every dependency
/// table.
fn collect_member_dependencies(
    root_doc: &toml_edit::DocumentMut,
    workspace_root: &Path,
) -> Result<Vec<DependencyInfo>> {
    let mut collected = Vec::new();

    for member_path in get_workspace_members(root_doc, workspace_root)? {
        let manifest = member_path.join("Cargo.toml");
        let member_doc = read_manifest(&manifest)?;

        for (section, table) in dependency_sections(&member_doc) {
            for (name, item) in table.iter() {
                let Some(spec) = DependencySpec::parse(item) else {
                    continue;
                };
                collected.push(DependencyInfo {
                    name: name.to_string(),
                    member_path: member_path.clone(),
                    manifest: manifest.clone(),
                    section: section.clone(),
                    spec,
                });
            }
        }
    }

    Ok(collected)
}

/// Read `.truss/unify.toml` when the workspace has one, else keep `fallback`.
fn load_unify_config(workspace_root: &Path, fallback: &UnifyConfig) -> Result<UnifyConfig> {
    let config_path = workspace_root.join(".truss/unify.toml");
    if config_path.exists() {
        UnifyConfig::load(&config_path)
    } else {
        Ok(fallback.clone())
    }
}

/// How many distinct members declare each dependency.
///
/// A member that already inherits still counts: one inherited use plus one
/// explicit use is exactly the drift unification exists to remove. One crate
/// that names a dependency in `[dependencies]` and again in `[dev-dependencies]`
/// is still one member, so counting declarations let a single crate reach the
/// threshold on its own.
fn dependency_occurrences(member_deps: &[DependencyInfo]) -> IndexMap<String, usize> {
    let mut dep_members: IndexMap<String, IndexSet<PathBuf>> = IndexMap::new();
    for dep in member_deps {
        if !dep.spec.is_unifiable() || (!dep.spec.is_workspace() && dep.spec.version.is_none()) {
            continue;
        }
        dep_members
            .entry(dep.name.clone())
            .or_default()
            .insert(dep.member_path.clone());
    }
    dep_members
        .into_iter()
        .map(|(name, members)| (name, members.len()))
        .collect()
}

pub fn check_dependency_drift(workspace_root: &Path) -> Result<Vec<DriftEntry>> {
    let root_doc = read_root_manifest(workspace_root)?;
    let workspace_deps = extract_workspace_dependencies(&root_doc);
    let member_deps = collect_member_dependencies(&root_doc, workspace_root)?;
    // The check has to agree with `unify`, or CI reports drift on exactly the
    // dependencies the workspace configured it to leave alone.
    let config = load_unify_config(workspace_root, &UnifyConfig::default())?;

    // The same count `unify` uses, so the check reports only what `unify` would
    // act on. Reporting a dependency one member declares meant `truss unify`
    // could finish and `truss unify --check` still fail on it.
    let occurrences = dependency_occurrences(&member_deps);

    let mut drift = Vec::new();
    for dep in member_deps {
        // A `package`/`registry` entry resolves to a crate other than its key,
        // so it can never inherit under that key.
        if !dep.spec.is_unifiable() {
            continue;
        }
        if config.is_excluded(&dep.name) {
            continue;
        }
        // A member that says `workspace = true` with no root entry does not
        // build whatever the count is, so that case is checked below and is
        // deliberately not subject to the threshold.
        // A dependency the root already declares is in scope whatever the
        // count: the workspace has decided it belongs there, so a member that
        // still declares it explicitly is drift by anyone's reading.
        if !dep.spec.is_workspace()
            && !workspace_deps.contains_key(&dep.name)
            && !config.should_unify(&dep.name, occurrences.get(&dep.name).map_or(0, |n| *n))
        {
            continue;
        }

        // A member that says `workspace = true` with no root entry does not
        // build. Skipping it reported a clean workspace for a broken manifest.
        if dep.spec.is_workspace() {
            if workspace_deps.contains_key(&dep.name) {
                continue;
            }
            drift.push(DriftEntry {
                member_path: dep.member_path.clone(),
                dependency: dep.name.clone(),
                section: dep.section_label(),
                root_version: None,
                member_version: "workspace".to_string(),
                kind: DriftKind::MissingInRoot,
            });
            continue;
        }

        let kind = match workspace_deps.get(&dep.name) {
            None => DriftKind::MissingInRoot,
            Some(root) => {
                if versions_differ(dep.spec.version.as_deref(), root.version.as_deref()) {
                    DriftKind::VersionMismatch
                } else if dep.spec.features_match(root) {
                    DriftKind::NotWorkspaceRef
                } else {
                    DriftKind::FeaturesDiffers
                }
            }
        };

        drift.push(DriftEntry {
            member_path: dep.member_path.clone(),
            dependency: dep.name.clone(),
            section: dep.section_label(),
            root_version: workspace_deps
                .get(&dep.name)
                .and_then(|root| root.version.clone()),
            member_version: dep
                .spec
                .version
                .clone()
                .unwrap_or_else(|| "none".to_string()),
            kind,
        });
    }

    Ok(drift)
}

/// Compare two version requirements. `semver` canonicalises them first, so
/// `1` and `^1` count as the same requirement.
fn versions_differ(member: Option<&str>, root: Option<&str>) -> bool {
    match (member, root) {
        (Some(member), Some(root)) => !same_requirement(member, root),
        _ => false,
    }
}

fn same_requirement(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (
        semver::VersionReq::parse(a).ok(),
        semver::VersionReq::parse(b).ok(),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

pub fn unify_dependencies(workspace_root: &Path, options: &UnifyOptions) -> Result<UnifyPlan> {
    let root_doc = read_root_manifest(workspace_root)?;

    let config = load_unify_config(workspace_root, &options.config)?;

    let member_deps = collect_member_dependencies(&root_doc, workspace_root)?;
    let existing_workspace_deps = extract_workspace_dependencies(&root_doc);

    // A member that already inherits still counts: one inherited use plus one
    // explicit use is exactly the drift this command exists to remove.
    // One crate that uses a dependency in [dependencies] and again in
    // [dev-dependencies] is still one member. Counting declarations made a
    // single crate reach the threshold on its own.
    let dep_occurrences = dependency_occurrences(&member_deps);

    let mut plan = UnifyPlan {
        root_additions: IndexMap::new(),
        root_updates: IndexMap::new(),
        member_changes: Vec::new(),
    };

    for (dep_name, &count) in &dep_occurrences {
        if !config.should_unify(dep_name, count) {
            continue;
        }

        let explicit: Vec<&DependencyInfo> = member_deps
            .iter()
            .filter(|dep| {
                &dep.name == dep_name && !dep.spec.is_workspace() && dep.spec.is_unifiable()
            })
            .collect();

        // Every member already inherits this dependency.
        if explicit.is_empty() {
            continue;
        }

        let root_dependency = unified_root_entry(dep_name, &explicit)?;

        if let Some(existing) = existing_workspace_deps.get(dep_name) {
            // Making a member inherit gives it whatever the root entry says.
            // A root entry that names its own source, or that enables features
            // the members do not ask for, would change what those members
            // resolve to -- silently, and for every other inheritor too.
            if existing.is_local() || existing.renamed {
                return Err(Error::UnificationConflict(format!(
                    "dependency '{dep_name}' is declared in [workspace.dependencies] with its own \
                     source, so members cannot inherit it as a registry dependency"
                )));
            }
            // Cargo makes an inheriting member's features additive to the
            // root's, and the rewrite keeps the member's own list, so the
            // member ends up with the union. That is behaviour-preserving
            // exactly when the root asks for nothing the member did not
            // already ask for. Members need not agree with each other, and
            // they need not match the root: a root entry with no features at
            // all is always safe to adopt.
            for dep in &explicit {
                let added: Vec<&str> = existing
                    .features
                    .iter()
                    .filter(|feature| !dep.spec.features.contains(feature))
                    .map(String::as_str)
                    .collect();
                if !added.is_empty() {
                    return Err(Error::UnificationConflict(format!(
                        "dependency '{dep_name}' is declared in [workspace.dependencies] with \
                         features {:?}; inheriting would add {added:?} to {}, which did not ask \
                         for them",
                        existing.features,
                        dep.member_path.display()
                    )));
                }
            }

            let matches_root = existing
                .version
                .as_ref()
                .is_some_and(|version| same_requirement(version, &root_dependency.version))
                && existing.default_features == root_dependency.default_features;

            if !matches_root {
                // Rewriting the root entry changes the version of every member
                // that already inherits it, so refuse rather than do that
                // silently.
                if member_deps
                    .iter()
                    .any(|dep| &dep.name == dep_name && dep.spec.is_workspace())
                {
                    return Err(Error::UnificationConflict(format!(
                        "dependency '{dep_name}' is inherited at workspace version {} but declared explicitly as {root_dependency}",
                        existing.version.as_deref().map_or("none", |v| v)
                    )));
                }
                plan.root_updates
                    .insert(dep_name.clone(), root_dependency.clone());
            }
        } else {
            plan.root_additions
                .insert(dep_name.clone(), root_dependency.clone());
        }

        for dep in explicit {
            plan.member_changes.push(MemberChange {
                path: dep.manifest.clone(),
                section: dep.section.clone(),
                dependency: dep_name.clone(),
                change: ChangeKind::ToWorkspaceRef,
            });
        }
    }

    if !options.dry_run {
        apply_plan(workspace_root, &plan)?;
    }

    Ok(plan)
}

/// Fold every explicit declaration of one dependency into the single entry the
/// workspace root will carry.
fn unified_root_entry(dep_name: &str, explicit: &[&DependencyInfo]) -> Result<RootDependency> {
    let versions: Vec<&str> = explicit
        .iter()
        .filter_map(|dep| dep.spec.version.as_deref())
        .collect();

    let Some(unified_version) = versions.first() else {
        return Err(Error::UnificationConflict(format!(
            "dependency '{dep_name}' is declared without a version"
        )));
    };

    if let Some(other) = versions
        .iter()
        .find(|version| !same_requirement(version, unified_version))
    {
        return Err(Error::UnificationConflict(format!(
            "dependency '{dep_name}' has conflicting versions: {unified_version} and {other}"
        )));
    }

    // An edition-2024 package overrides this key; an earlier edition ignores it
    // and takes the workspace value. The root can therefore carry only one
    // value, so the members have to agree on it.
    let defaults: Vec<bool> = explicit
        .iter()
        .map(|dep| dep.spec.default_features)
        .collect();
    if defaults.windows(2).any(|w| w.first() != w.last()) {
        return Err(Error::UnificationConflict(format!(
            "dependency '{dep_name}' disagrees on default-features across members"
        )));
    }

    Ok(RootDependency {
        version: (*unified_version).to_string(),
        default_features: defaults.first().copied().is_none_or(|enabled| enabled),
    })
}

/// Walk a key path down to the table it names.
fn table_at_mut<'a>(
    item: &'a mut toml_edit::Item,
    path: &[String],
) -> Option<&'a mut dyn toml_edit::TableLike> {
    match path.split_first() {
        None => item.as_table_like_mut(),
        Some((head, rest)) => table_at_mut(item.as_table_like_mut()?.get_mut(head)?, rest),
    }
}

fn root_dependency_item(entry: &RootDependency) -> toml_edit::Item {
    if entry.default_features {
        return toml_edit::Item::Value(toml_edit::Value::from(entry.version.as_str()));
    }
    let mut inline = toml_edit::InlineTable::new();
    inline.insert("version", toml_edit::Value::from(entry.version.as_str()));
    inline.insert("default-features", toml_edit::Value::from(false));
    toml_edit::Item::Value(toml_edit::Value::InlineTable(inline))
}

/// Write the root entry, keeping any keys the existing entry already carries.
fn write_root_dependency(deps: &mut dyn toml_edit::TableLike, name: &str, entry: &RootDependency) {
    let Some(existing) = deps.get_mut(name) else {
        deps.insert(name, root_dependency_item(entry));
        return;
    };

    // A plain string entry carries nothing worth keeping.
    if existing.is_str() || existing.as_table_like().is_none() {
        *existing = root_dependency_item(entry);
        return;
    }

    if let Some(table) = existing.as_table_like_mut() {
        table.insert(
            "version",
            toml_edit::Item::Value(toml_edit::Value::from(entry.version.as_str())),
        );
        if entry.default_features {
            table.remove("default-features");
        } else {
            table.insert(
                "default-features",
                toml_edit::Item::Value(toml_edit::Value::from(false)),
            );
        }
    }
}

fn apply_plan(workspace_root: &Path, plan: &UnifyPlan) -> Result<()> {
    // Render every manifest before writing any of them, so a failure part way
    // through cannot leave the workspace half unified.
    let mut writes: Vec<(PathBuf, String)> = Vec::new();

    // One document per manifest. The workspace root can also be a member, and
    // two documents for one path would mean two writes, the second discarding
    // the first one's edits.
    let mut docs: IndexMap<PathBuf, toml_edit::DocumentMut> = IndexMap::new();

    let root_manifest = workspace_root.join("Cargo.toml");
    if !plan.root_additions.is_empty() || !plan.root_updates.is_empty() {
        let root_doc = match docs.entry(root_manifest.clone()) {
            indexmap::map::Entry::Occupied(entry) => entry.into_mut(),
            indexmap::map::Entry::Vacant(entry) => entry.insert(read_manifest(&root_manifest)?),
        };
        let workspace = root_doc
            .as_table_mut()
            .entry("workspace")
            .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
        let Some(workspace) = workspace.as_table_like_mut() else {
            return Err(Error::Argument(
                "[workspace] in the root Cargo.toml is not a table".to_string(),
            ));
        };
        if !workspace.contains_key("dependencies") {
            // A standard table, which is what Cargo and every template write.
            workspace.insert(
                "dependencies",
                toml_edit::Item::Table(toml_edit::Table::new()),
            );
        }
        let Some(deps) = workspace
            .get_mut("dependencies")
            .and_then(toml_edit::Item::as_table_like_mut)
        else {
            return Err(Error::Argument(
                "[workspace.dependencies] in the root Cargo.toml is not a table".to_string(),
            ));
        };

        for (dep_name, entry) in &plan.root_additions {
            write_root_dependency(deps, dep_name, entry);
        }
        for (dep_name, entry) in &plan.root_updates {
            write_root_dependency(deps, dep_name, entry);
        }
    }

    for change in &plan.member_changes {
        if !docs.contains_key(&change.path) {
            docs.insert(change.path.clone(), read_manifest(&change.path)?);
        }
        let Some(doc) = docs.get_mut(&change.path) else {
            continue;
        };
        let Some(deps) = table_at_mut(doc.as_item_mut(), &change.section) else {
            continue;
        };
        let Some(dep_item) = deps.get_mut(&change.dependency) else {
            continue;
        };

        match &change.change {
            ChangeKind::ToWorkspaceRef => rewrite_as_workspace_ref(dep_item),
            ChangeKind::VersionUpdate(version) => {
                if let Some(table) = dep_item.as_table_like_mut() {
                    table.insert(
                        "version",
                        toml_edit::Item::Value(toml_edit::Value::from(version.as_str())),
                    );
                } else {
                    *dep_item = toml_edit::Item::Value(toml_edit::Value::from(version.as_str()));
                }
            }
        }
    }
    for (path, doc) in docs {
        writes.push((path, doc.to_string()));
    }

    // Rendering every manifest first rules out a mid-plan rendering failure,
    // but the writes themselves can still fail -- a full disk, a read-only
    // file. Keep each original so a failed write restores what it replaced.
    let mut restore: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for (path, content) in writes {
        let original = std::fs::read(&path)?;
        if let Err(error) = std::fs::write(&path, content) {
            for (done, bytes) in restore.iter().rev() {
                // A restore that fails leaves that file changed. Report the
                // original failure, which is the one the user can act on.
                let _ = std::fs::write(done, bytes);
            }
            let _ = std::fs::write(&path, &original);
            return Err(Error::Io(error));
        }
        restore.push((path, original));
    }

    Ok(())
}

/// Turn a member's explicit dependency into `{ workspace = true }`, keeping the
/// keys Cargo still honours on an inheriting member.
fn rewrite_as_workspace_ref(dep_item: &mut toml_edit::Item) {
    if dep_item.as_table_like().is_none() {
        let mut inline = toml_edit::InlineTable::new();
        inline.insert("workspace", toml_edit::Value::from(true));
        *dep_item = toml_edit::Item::Value(toml_edit::Value::InlineTable(inline));
        return;
    }

    if let Some(table) = dep_item.as_table_like_mut() {
        table.remove("version");
        // `default-features` stays. An edition-2024 package overrides the
        // workspace value with its own, so removing the key would change what
        // the member resolves to. Earlier editions ignore it and take the root
        // value, which `unified_root_entry` already made match. Keeping the key
        // is correct under both, and the spec requires the member to keep it.
        table.insert(
            "workspace",
            toml_edit::Item::Value(toml_edit::Value::from(true)),
        );
    }
    // Removing a key leaves its spacing behind, so tidy the entry.
    if let Some(inline) = dep_item.as_inline_table_mut() {
        inline.fmt();
    }
}

fn extract_workspace_dependencies(
    doc: &toml_edit::DocumentMut,
) -> IndexMap<String, DependencySpec> {
    let mut deps = IndexMap::new();
    let Some(workspace) = doc
        .get("workspace")
        .and_then(toml_edit::Item::as_table_like)
    else {
        return deps;
    };
    let Some(workspace_deps) = workspace
        .get("dependencies")
        .and_then(toml_edit::Item::as_table_like)
    else {
        return deps;
    };
    for (key, value) in workspace_deps.iter() {
        if let Some(spec) = DependencySpec::parse(value) {
            deps.insert(key.to_string(), spec);
        }
    }
    deps
}

fn string_list(item: Option<&toml_edit::Item>) -> Vec<String> {
    match item {
        Some(item) if item.is_array() => item.as_array().map_or_else(Vec::new, |array| {
            array
                .iter()
                .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                .collect()
        }),
        Some(item) => item
            .as_str()
            .map(std::string::ToString::to_string)
            .into_iter()
            .collect(),
        None => Vec::new(),
    }
}

fn get_workspace_members(doc: &toml_edit::DocumentMut, root: &Path) -> Result<Vec<PathBuf>> {
    let Some(workspace) = doc
        .get("workspace")
        .and_then(toml_edit::Item::as_table_like)
    else {
        return Ok(Vec::new());
    };

    // Cargo reads an exclusion the same way it reads a member, so a pattern
    // like `crates/*` excludes every crate under `crates`. Comparing the
    // pattern as a literal path let those crates be rewritten anyway.
    let mut excluded_paths: Vec<PathBuf> = Vec::new();
    let mut excluded_globs: Vec<globset::GlobMatcher> = Vec::new();
    for pattern in string_list(workspace.get("exclude")) {
        if pattern.contains(['*', '?', '[']) {
            excluded_globs.push(build_member_glob(&pattern)?);
        } else {
            excluded_paths.push(root.join(&pattern));
        }
    }

    let mut members: Vec<PathBuf> = Vec::new();
    // A root manifest that also carries [package] is a member of its own
    // workspace, exactly as Cargo reads it. Leaving it out let the root
    // package's dependencies escape both the drift check and the occurrence
    // count that decides what gets unified.
    if doc.get("package").is_some() {
        members.push(root.to_path_buf());
    }
    for pattern in string_list(workspace.get("members")) {
        if pattern.contains(['*', '?', '[']) {
            expand_member_glob(root, &pattern, &mut members)?;
        } else {
            let member_path = root.join(&pattern);
            // A member is a path from the manifest, so `..` or an absolute
            // path in it would make unification rewrite a manifest outside the
            // workspace it was pointed at.
            crate::pathsafe::ensure_under_root(root, &member_path)?;
            if !member_path.join("Cargo.toml").is_file() {
                // Cargo itself refuses to load such a workspace. Skipping it
                // silently would report a clean check for a crate nobody read.
                return Err(Error::Argument(format!(
                    "workspace member '{pattern}' has no Cargo.toml"
                )));
            }
            members.push(member_path);
        }
    }

    members.retain(|member| {
        if excluded_paths.contains(member) {
            return false;
        }
        let Ok(relative) = member.strip_prefix(root) else {
            return true;
        };
        !excluded_globs.iter().any(|glob| glob.is_match(relative))
    });
    members.sort();
    members.dedup();
    Ok(members)
}

/// Compile one Cargo member or exclude pattern.
///
/// `literal_separator` keeps `*` inside one path segment, the way Cargo reads a
/// member glob.
fn build_member_glob(pattern: &str) -> Result<globset::GlobMatcher> {
    globset::GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(|e| {
            Error::Argument(format!(
                "workspace member pattern '{pattern}' is not a valid glob: {e}"
            ))
        })
}

/// Expand a Cargo member glob such as `crates/*` into the crate directories it
/// names, skipping `target` and hidden directories the way Cargo does.
///
/// Every failure is reported. Skipping an unreadable directory silently let
/// `truss unify --check` report a clean workspace after reading only part of
/// it, which is the one answer this command must never give wrongly.
fn expand_member_glob(root: &Path, pattern: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    let matcher = build_member_glob(pattern)?;

    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| {
            Error::Argument(format!(
                "cannot read '{}' while expanding workspace member pattern '{pattern}': {e}",
                dir.display()
            ))
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| {
                Error::Argument(format!(
                    "cannot read an entry of '{}' while expanding workspace member pattern \
                     '{pattern}': {e}",
                    dir.display()
                ))
            })?;
            let file_type = entry.file_type().map_err(|e| {
                Error::Argument(format!(
                    "cannot read the type of '{}' while expanding workspace member pattern \
                     '{pattern}': {e}",
                    entry.path().display()
                ))
            })?;
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "target" {
                continue;
            }

            let path = entry.path();
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            if matcher.is_match(relative) && path.join("Cargo.toml").is_file() {
                out.push(path.clone());
            }
            // No depth cap: Cargo places no limit on how deep a member sits,
            // and a cap silently dropped everything below it.
            pending.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a workspace on disk. Each member is `(relative dir, dependency
    /// table body)`.
    fn workspace(root_body: &str, members: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("Cargo.toml"), root_body).expect("write root");
        for (relative, body) in members {
            let member_dir = dir.path().join(relative);
            std::fs::create_dir_all(&member_dir).expect("create member");
            let name = relative.rsplit('/').next().unwrap_or(relative);
            std::fs::write(
                member_dir.join("Cargo.toml"),
                format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n{body}"),
            )
            .expect("write member");
        }
        dir
    }

    fn read(dir: &Path, relative: &str) -> String {
        std::fs::read_to_string(dir.join(relative)).expect("read manifest")
    }

    fn unify(dir: &Path) -> Result<UnifyPlan> {
        unify_dependencies(dir, &UnifyOptions::default())
    }

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

    /// `[workspace.dependencies]` is a standard table, not an inline one.
    #[test]
    fn standard_workspace_dependency_table_is_read() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\"]\n\n[workspace.dependencies]\nserde = \"1\"\n",
            &[("a", "[dependencies]\nserde = \"1\"\n")],
        );
        let drift = check_dependency_drift(dir.path()).expect("drift");
        assert_eq!(drift.len(), 1);
        let entry = drift.first().expect("entry");
        assert_eq!(entry.root_version.as_deref(), Some("1"));
        assert_eq!(entry.kind, DriftKind::NotWorkspaceRef);
    }

    /// A member repeating the root version is still outside inheritance.
    #[test]
    fn matching_explicit_version_is_reported_as_drift() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\"]\n\n[workspace.dependencies]\nserde = \"1\"\n",
            &[("a", "[dependencies]\nserde = { version = \"1\" }\n")],
        );
        let drift = check_dependency_drift(dir.path()).expect("drift");
        assert_eq!(
            drift.first().map(|entry| entry.kind),
            Some(DriftKind::NotWorkspaceRef)
        );
    }

    /// `dep = { version = "1" }` is an inline table, which `as_table` misses.
    #[test]
    fn inline_dependency_specification_is_scanned() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[
                ("a", "[dependencies]\nserde = { version = \"1\" }\n"),
                ("b", "[dependencies]\nserde = { version = \"1\" }\n"),
            ],
        );
        let drift = check_dependency_drift(dir.path()).expect("drift");
        assert_eq!(drift.len(), 2);
        assert_eq!(
            drift.first().map(|entry| entry.kind),
            Some(DriftKind::MissingInRoot)
        );
    }

    #[test]
    fn a_semver_equivalent_requirement_is_not_a_mismatch() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\"]\n\n[workspace.dependencies]\nserde = \"^1\"\n",
            &[("a", "[dependencies]\nserde = \"1\"\n")],
        );
        assert_eq!(
            check_dependency_drift(dir.path())
                .expect("drift")
                .first()
                .map(|entry| entry.kind),
            Some(DriftKind::NotWorkspaceRef)
        );
    }

    #[test]
    fn a_feature_difference_is_reported() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\"]\n\n[workspace.dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\n",
            &[("a", "[dependencies]\nserde = { version = \"1\" }\n")],
        );
        assert_eq!(
            check_dependency_drift(dir.path())
                .expect("drift")
                .first()
                .map(|entry| entry.kind),
            Some(DriftKind::FeaturesDiffers)
        );
    }

    #[test]
    fn dev_and_build_dependencies_are_scanned() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[
                (
                    "a",
                    "[dev-dependencies]\ntempfile = \"3\"\n\n[build-dependencies]\ncc = \"1\"\n",
                ),
                (
                    "b",
                    "[dev-dependencies]\ntempfile = \"3\"\n\n[build-dependencies]\ncc = \"1\"\n",
                ),
            ],
        );
        let drift = check_dependency_drift(dir.path()).expect("drift");
        let mut sections: Vec<&str> = drift.iter().map(|entry| entry.section.as_str()).collect();
        sections.sort_unstable();
        sections.dedup();
        assert_eq!(sections, vec!["build-dependencies", "dev-dependencies"]);
    }

    #[test]
    fn target_dependencies_are_scanned() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[
                ("a", "[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n"),
                ("b", "[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n"),
            ],
        );
        let drift = check_dependency_drift(dir.path()).expect("drift");
        assert_eq!(
            drift.first().map(|entry| entry.section.as_str()),
            Some("target.cfg(unix).dependencies")
        );
    }

    /// A path dependency names a source, so it can never be inherited.
    #[test]
    fn a_path_dependency_is_not_drift() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[
                ("a", "[dependencies]\nb = { path = \"../b\" }\n"),
                ("b", ""),
            ],
        );
        assert!(
            check_dependency_drift(dir.path())
                .expect("drift")
                .is_empty()
        );
    }

    #[test]
    fn member_globs_are_expanded() {
        let dir = workspace(
            "[workspace]\nmembers = [\"crates/*\"]\n",
            &[
                ("crates/a", "[dependencies]\nserde = \"1\"\n"),
                ("crates/b", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        assert_eq!(check_dependency_drift(dir.path()).expect("drift").len(), 2);
    }

    #[test]
    fn an_excluded_member_is_skipped() {
        let dir = workspace(
            "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/b\"]\n",
            &[
                ("crates/a", "[dependencies]\nserde = \"1\"\n"),
                ("crates/c", "[dependencies]\nserde = \"1\"\n"),
                ("crates/b", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        let drift = check_dependency_drift(dir.path()).expect("drift");
        assert_eq!(drift.len(), 2, "{drift:?}");
        assert!(
            !drift
                .iter()
                .any(|entry| entry.member_path.ends_with("crates/b")),
            "the excluded member must not be scanned: {drift:?}"
        );
    }

    /// The whole point of the command: the root gains the dependency and each
    /// member manifest is rewritten in place.
    #[test]
    fn unify_writes_the_root_and_the_members() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[
                ("a", "[dependencies]\nserde = \"1\"\n"),
                ("b", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        let plan = unify(dir.path()).expect("unify");
        assert_eq!(plan.member_changes.len(), 2);

        let root = read(dir.path(), "Cargo.toml");
        assert!(root.contains("[workspace.dependencies]"), "{root}");
        assert!(root.contains("serde = \"1\""), "{root}");

        let member = read(dir.path(), "a/Cargo.toml");
        assert!(member.contains("workspace = true"), "{member}");
        assert!(!member.contains("\"1\""), "{member}");
    }

    /// The root table already exists as a standard table, so the new entry has
    /// to land in it rather than in a fresh inline table.
    #[test]
    fn unify_extends_an_existing_root_table() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n\n[workspace.dependencies]\nthiserror = \"2\"\n",
            &[
                ("a", "[dependencies]\nserde = \"1\"\n"),
                ("b", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        unify(dir.path()).expect("unify");
        let root = read(dir.path(), "Cargo.toml");
        assert!(root.contains("thiserror = \"2\""), "{root}");
        assert!(root.contains("serde = \"1\""), "{root}");
        assert!(!root.contains("dependencies = {"), "{root}");
    }

    /// One inherited use plus one explicit use meets the default threshold.
    #[test]
    fn a_workspace_reference_counts_toward_the_threshold() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n\n[workspace.dependencies]\nserde = \"1\"\n",
            &[
                ("a", "[dependencies]\nserde = { workspace = true }\n"),
                ("b", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        let plan = unify(dir.path()).expect("unify");
        assert_eq!(plan.member_changes.len(), 1);
        assert!(read(dir.path(), "b/Cargo.toml").contains("workspace = true"));
    }

    #[test]
    fn features_survive_the_rewrite() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[
                (
                    "a",
                    "[dependencies]\nserde = { version = \"1\", features = [\"derive\"], optional = true }\n",
                ),
                (
                    "b",
                    "[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\n",
                ),
            ],
        );
        unify(dir.path()).expect("unify");
        let member = read(dir.path(), "a/Cargo.toml");
        assert!(member.contains("features = [\"derive\"]"), "{member}");
        assert!(member.contains("optional = true"), "{member}");
        assert!(member.contains("workspace = true"), "{member}");
        // The `[package]` version stays; the dependency version goes.
        assert!(!member.contains("version = \"1\""), "{member}");
    }

    /// An edition-2024 package overrides the workspace `default-features` with
    /// its own, so the member keeps the key. The root carries the agreed value
    /// too, which is what an earlier edition reads.
    #[test]
    fn default_features_reach_the_root_and_stay_in_the_member() {
        let body = "[dependencies]\nserde = { version = \"1\", default-features = false }\n";
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[("a", body), ("b", body)],
        );
        unify(dir.path()).expect("unify");
        let root = read(dir.path(), "Cargo.toml");
        assert!(root.contains("default-features = false"), "{root}");
        let member = read(dir.path(), "a/Cargo.toml");
        assert!(
            member.contains("default-features = false"),
            "the member must keep its own default-features: {member}"
        );
    }

    #[test]
    fn disagreeing_default_features_are_a_conflict() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[
                (
                    "a",
                    "[dependencies]\nserde = { version = \"1\", default-features = false }\n",
                ),
                ("b", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        assert!(unify(dir.path()).is_err());
    }

    #[test]
    fn conflicting_versions_are_rejected() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[
                ("a", "[dependencies]\nserde = \"1\"\n"),
                ("b", "[dependencies]\nserde = \"2\"\n"),
            ],
        );
        assert!(unify(dir.path()).is_err());
    }

    /// Bumping the root version would silently change the version an already
    /// inheriting member resolves.
    #[test]
    fn a_root_bump_under_an_inheriting_member_is_rejected() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n\n[workspace.dependencies]\nserde = \"1\"\n",
            &[
                ("a", "[dependencies]\nserde = { workspace = true }\n"),
                ("b", "[dependencies]\nserde = \"2\"\n"),
            ],
        );
        assert!(unify(dir.path()).is_err());
    }

    #[test]
    fn a_dry_run_leaves_every_manifest_alone() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[
                ("a", "[dependencies]\nserde = \"1\"\n"),
                ("b", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        let options = UnifyOptions {
            dry_run: true,
            config: UnifyConfig::default(),
        };
        let plan = unify_dependencies(dir.path(), &options).expect("unify");
        assert_eq!(plan.root_additions.len(), 1);
        assert!(!read(dir.path(), "Cargo.toml").contains("serde"));
        assert!(read(dir.path(), "a/Cargo.toml").contains("serde = \"1\""));
    }

    #[test]
    fn drift_kind_renders_a_label() {
        assert_eq!(DriftKind::VersionMismatch.to_string(), "version mismatch");
        assert_eq!(DriftKind::FeaturesDiffers.to_string(), "features differ");
    }

    /// `serde = { version = "1", package = "serde_core" }` resolves to a crate
    /// other than its key, so it cannot inherit under that key. Rewriting it
    /// to `workspace = true` produced a manifest that resolves the wrong crate.
    #[test]
    fn a_renamed_dependency_is_left_alone() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[
                (
                    "a",
                    "[dependencies]\nserde = { version = \"1\", package = \"serde_core\" }\n",
                ),
                (
                    "b",
                    "[dependencies]\nserde = { version = \"1\", package = \"serde_core\" }\n",
                ),
            ],
        );
        assert!(
            check_dependency_drift(dir.path())
                .expect("drift")
                .is_empty(),
            "a renamed dependency is not drift"
        );
        let plan = unify(dir.path()).expect("unify");
        assert!(
            plan.member_changes.is_empty() && plan.root_additions.is_empty(),
            "a renamed dependency must not be unified"
        );
        assert!(read(dir.path(), "a/Cargo.toml").contains("package = \"serde_core\""));
    }

    /// A member path is read from the manifest. `..` in it would make
    /// unification rewrite a Cargo.toml outside the workspace it was given.
    #[test]
    fn a_member_path_may_not_escape_the_workspace() {
        let dir = workspace("[workspace]\nmembers = [\"../outside\"]\n", &[]);
        let outside = dir.path().parent().expect("parent").join("outside");
        std::fs::create_dir_all(&outside).expect("mkdir outside");
        std::fs::write(
            outside.join("Cargo.toml"),
            "[package]\nname = \"outside\"\nversion = \"0.1.0\"\n",
        )
        .expect("write outside");

        let err = check_dependency_drift(dir.path())
            .expect_err("a member outside the workspace must be rejected")
            .to_string();
        assert!(!err.is_empty(), "the error must say what was rejected");
        std::fs::remove_dir_all(&outside).ok();
    }

    /// Cargo refuses to load a workspace whose member has no manifest.
    /// Skipping it silently reported a clean check for a crate nobody read.
    #[test]
    fn a_member_without_a_manifest_is_an_error() {
        let dir = workspace("[workspace]\nmembers = [\"a\", \"ghost\"]\n", &[("a", "")]);
        std::fs::create_dir_all(dir.path().join("ghost")).expect("mkdir ghost");
        let err = check_dependency_drift(dir.path())
            .expect_err("a member without a manifest must be reported")
            .to_string();
        assert!(err.contains("ghost"), "unexpected error: {err}");
    }

    /// Cargo reads an exclusion as a pattern, so `crates/b*` excludes those
    /// crates. Comparing it as a literal path let them be rewritten anyway.
    #[test]
    fn a_wildcard_exclusion_is_honoured() {
        let dir = workspace(
            "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/b*\"]\n",
            &[
                ("crates/a", "[dependencies]\nserde = \"1\"\n"),
                ("crates/c", "[dependencies]\nserde = \"1\"\n"),
                ("crates/beta", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        let drift = check_dependency_drift(dir.path()).expect("drift");
        assert_eq!(
            drift.len(),
            2,
            "only the unexcluded members count: {drift:?}"
        );
        assert!(
            !drift
                .iter()
                .any(|entry| entry.member_path.ends_with("crates/beta")),
            "the excluded member must not be scanned: {drift:?}"
        );
    }

    /// One crate that uses a dependency in [dependencies] and again in
    /// [dev-dependencies] is still one member, so it must not reach the
    /// two-member unification threshold on its own.
    #[test]
    fn one_member_using_a_dependency_twice_is_not_unified() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\"]\n",
            &[(
                "a",
                "[dependencies]\nserde = \"1\"\n\n[dev-dependencies]\nserde = \"1\"\n",
            )],
        );
        let plan = unify(dir.path()).expect("unify");
        assert!(
            plan.root_additions.is_empty() && plan.member_changes.is_empty(),
            "a single member must not trigger workspace unification: {plan:?}"
        );
    }

    /// `workspace = true` with no matching root entry does not build. Skipping
    /// it reported a clean workspace for a manifest Cargo rejects.
    #[test]
    fn an_inherited_dependency_missing_from_the_root_is_drift() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\"]\n",
            &[("a", "[dependencies]\nserde = { workspace = true }\n")],
        );
        let drift = check_dependency_drift(dir.path()).expect("drift");
        assert_eq!(drift.len(), 1, "{drift:?}");
        assert_eq!(drift[0].kind, DriftKind::MissingInRoot);
        assert_eq!(drift[0].dependency, "serde");
    }

    /// The check has to agree with `unify`, or CI reports drift on exactly the
    /// dependencies the workspace configured it to leave alone.
    #[test]
    fn the_drift_check_honours_a_blocklist() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[
                ("a", "[dependencies]\nserde = \"1\"\n"),
                ("b", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        assert_eq!(check_dependency_drift(dir.path()).expect("drift").len(), 2);

        std::fs::create_dir_all(dir.path().join(".truss")).expect("mkdir .truss");
        std::fs::write(
            dir.path().join(".truss/unify.toml"),
            "blocklist = [\"serde\"]\n",
        )
        .expect("write config");
        assert!(
            check_dependency_drift(dir.path())
                .expect("drift")
                .is_empty(),
            "a blocked dependency is not drift"
        );
    }

    /// Inheriting replaces a member's features with the root entry's. A root
    /// that enables features the members never asked for would change what
    /// they resolve to, silently.
    #[test]
    fn a_root_entry_with_extra_features_is_a_conflict() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n\n[workspace.dependencies]\n             serde = { version = \"1\", features = [\"derive\"] }\n",
            &[
                ("a", "[dependencies]\nserde = \"1\"\n"),
                ("b", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        let err = unify(dir.path())
            .expect_err("inheriting extra features must be refused")
            .to_string();
        assert!(err.contains("features"), "unexpected error: {err}");
        assert!(
            read(dir.path(), "a/Cargo.toml").contains("serde = \"1\""),
            "a refused plan must leave the member alone"
        );
    }

    /// A write can still fail after earlier ones succeeded -- a read-only file,
    /// a full disk. Leaving the root rewritten and the members untouched gives
    /// a workspace that no longer builds.
    #[cfg(unix)]
    #[test]
    fn a_failed_write_restores_every_manifest() {
        use std::os::unix::fs::PermissionsExt;

        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[
                ("a", "[dependencies]\nserde = \"1\"\n"),
                ("b", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        let root_before = read(dir.path(), "Cargo.toml");
        let a_before = read(dir.path(), "a/Cargo.toml");

        // The last manifest the plan writes cannot be replaced.
        let blocked = dir.path().join("b/Cargo.toml");
        let mut perms = std::fs::metadata(&blocked).expect("metadata").permissions();
        perms.set_mode(0o444);
        std::fs::set_permissions(&blocked, perms).expect("chmod");

        let failed = unify(dir.path()).is_err();

        let mut perms = std::fs::metadata(&blocked).expect("metadata").permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&blocked, perms).expect("chmod back");

        assert!(failed, "the unwritable manifest must fail the command");
        assert_eq!(
            read(dir.path(), "Cargo.toml"),
            root_before,
            "the root must be restored"
        );
        assert_eq!(
            read(dir.path(), "a/Cargo.toml"),
            a_before,
            "an already-written member must be restored"
        );
    }

    /// A root entry that names its own source cannot be inherited as a
    /// registry dependency.
    #[test]
    fn a_root_entry_with_its_own_source_is_a_conflict() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n\n[workspace.dependencies]\n             serde = { path = \"../vendor/serde\" }\n",
            &[
                ("a", "[dependencies]\nserde = \"1\"\n"),
                ("b", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        let err = unify(dir.path())
            .expect_err("inheriting a path dependency must be refused")
            .to_string();
        assert!(err.contains("source"), "unexpected error: {err}");
    }
    // Cargo unions an inheriting member's features with the root's, and the
    // rewrite keeps the member's own list. Adopting an existing root entry is
    // therefore safe whenever every root feature is one the member already
    // asked for -- members need not agree with each other.
    #[test]
    fn an_existing_root_is_adopted_when_its_features_are_a_subset() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n\n[workspace.dependencies]\n\
             serde = { version = \"1\", features = [\"derive\"] }\n",
            &[
                (
                    "a",
                    "[dependencies]\nserde = { version = \"1\", features = [\"derive\", \"std\"] }\n",
                ),
                (
                    "b",
                    "[dependencies]\nserde = { version = \"1\", features = [\"derive\", \"rc\"] }\n",
                ),
            ],
        );
        unify(dir.path()).expect("members that already ask for the root features must unify");
        assert!(read(dir.path(), "a/Cargo.toml").contains("workspace = true"));
        assert!(read(dir.path(), "b/Cargo.toml").contains("workspace = true"));
    }

    // Inheriting would add a feature the member never asked for, which changes
    // what it resolves to.
    #[test]
    fn an_existing_root_is_refused_when_it_adds_a_feature() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n\n[workspace.dependencies]\n\
             serde = { version = \"1\", features = [\"derive\"] }\n",
            &[
                ("a", "[dependencies]\nserde = { version = \"1\" }\n"),
                ("b", "[dependencies]\nserde = { version = \"1\" }\n"),
            ],
        );
        let err = unify(dir.path()).unwrap_err().to_string();
        assert!(err.contains("derive"), "unexpected error: {err}");
    }

    // A key of the wrong shape used to be dropped, leaving the default policy
    // free to rewrite a dependency the workspace had explicitly excluded.
    #[test]
    fn a_malformed_unify_config_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("unify.toml");

        std::fs::write(&path, "blocklist = \"serde\"\n").expect("write");
        let err = UnifyConfig::load(&path).unwrap_err().to_string();
        assert!(err.contains("must be an array"), "unexpected error: {err}");

        std::fs::write(&path, "allowlist = [\"serde\", 3]\n").expect("write");
        let err = UnifyConfig::load(&path).unwrap_err().to_string();
        assert!(
            err.contains("only dependency names"),
            "unexpected error: {err}"
        );
    }

    // A root manifest that also carries [package] is a member of its own
    // workspace. Its dependencies used to escape both the count and the
    // rewrite, and the root's two roles have to share one write.
    #[test]
    fn the_root_package_is_a_member_of_its_own_workspace() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\"]\n\n[package]\nname = \"root\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nserde = \"1\"\n",
        )
        .expect("write root");
        let member = dir.path().join("a");
        std::fs::create_dir_all(&member).expect("mkdir");
        std::fs::write(
            member.join("Cargo.toml"),
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \"1\"\n",
        )
        .expect("write member");

        // Two members declare serde: the root package and `a`.
        let drift = check_dependency_drift(dir.path()).expect("drift");
        assert_eq!(
            drift.len(),
            2,
            "the root package must be scanned: {drift:?}"
        );

        unify(dir.path()).expect("unify");
        let root = read(dir.path(), "Cargo.toml");
        assert!(
            root.contains("[workspace.dependencies]"),
            "the workspace entry must survive: {root}"
        );
        assert!(
            root.contains("serde = { workspace = true }") || root.contains("serde.workspace"),
            "the root package's own dependency must be rewritten in the same file: {root}"
        );
        assert!(read(dir.path(), "a/Cargo.toml").contains("workspace = true"));
    }

    // Skipping a member silently let `--check` report a clean workspace after
    // reading only part of it.
    #[test]
    fn an_invalid_member_glob_is_an_error() {
        let dir = workspace(
            "[workspace]\nmembers = [\"crates/[\"]\n",
            &[("crates/a", "[dependencies]\nserde = \"1\"\n")],
        );
        let err = check_dependency_drift(dir.path()).unwrap_err().to_string();
        assert!(err.contains("not a valid glob"), "unexpected error: {err}");
    }

    // The walk had a fixed depth cap, so a member below it was dropped without
    // a word.
    #[test]
    fn a_deeply_nested_member_is_found() {
        let deep = "a/b/c/d/e/f/g/h/i/leaf";
        let dir = workspace(
            "[workspace]\nmembers = [\"**/leaf\", \"shallow\"]\n",
            &[
                (deep, "[dependencies]\nserde = \"1\"\n"),
                ("shallow", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        let drift = check_dependency_drift(dir.path()).expect("drift");
        assert!(
            drift.iter().any(|entry| entry.member_path.ends_with(deep)),
            "a member ten levels down must still be scanned: {drift:?}"
        );
    }

    // `truss unify` then `truss unify --check` has to agree, or CI reports
    // drift the tool refuses to fix.
    #[test]
    fn check_reports_nothing_that_unify_would_leave_alone() {
        let dir = workspace(
            "[workspace]\nmembers = [\"a\", \"b\"]\n",
            &[
                // Two members share serde; only `a` uses rand.
                ("a", "[dependencies]\nserde = \"1\"\nrand = \"0.8\"\n"),
                ("b", "[dependencies]\nserde = \"1\"\n"),
            ],
        );
        unify(dir.path()).expect("unify");
        let drift = check_dependency_drift(dir.path()).expect("drift");
        assert!(
            drift.is_empty(),
            "check must be clean after unify: {drift:?}"
        );
    }
}
