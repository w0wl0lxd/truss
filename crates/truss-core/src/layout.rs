//! Multi-crate layout descriptor parsed from a template's `layout.toml`.

use crate::error::{Error, Result};
use crate::pathsafe::normalize_relative_path;
use crate::sync::SyncContext;
use crate::workspace::{MemberKind, add_workspace_member_with_deps, validate_member_name};
use indexmap::{IndexMap, IndexSet};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Layout {
    #[serde(default)]
    pub members: Vec<LayoutMember>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LayoutMember {
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: LayoutMemberKind,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub deps: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutMemberKind {
    Lib,
    Bin,
}

impl From<LayoutMemberKind> for MemberKind {
    fn from(value: LayoutMemberKind) -> Self {
        match value {
            LayoutMemberKind::Lib => Self::Lib,
            LayoutMemberKind::Bin => Self::Bin,
        }
    }
}

fn default_kind() -> LayoutMemberKind {
    LayoutMemberKind::Lib
}

impl Layout {
    /// Parse a `layout.toml` descriptor from a string.
    pub fn parse(content: &str) -> Result<Self> {
        toml_edit::de::from_str(content).map_err(|e| Error::Argument(format!("layout.toml: {e}")))
    }

    /// Return an ordered map of member name -> effective relative path, validating
    /// names, duplicates, and path normalizations along the way.
    pub fn member_paths(&self) -> Result<IndexMap<String, String>> {
        let mut map = IndexMap::new();
        let mut seen_paths = IndexSet::new();

        for member in &self.members {
            validate_member_name(&member.name)?;

            if map.contains_key(&member.name) {
                return Err(Error::Argument(format!(
                    "duplicate member name in layout: {}",
                    member.name
                )));
            }

            let path = match &member.path {
                Some(p) => normalize_relative_path(p)?,
                None => format!("crates/{}", member.name),
            };

            if seen_paths.contains(&path) {
                return Err(Error::Argument(format!(
                    "duplicate member path in layout: {}",
                    path
                )));
            }
            seen_paths.insert(path.clone());
            map.insert(member.name.clone(), path);
        }

        Ok(map)
    }

    /// Validate that every dependency refers to another declared member and is not
    /// a self-reference.
    fn validate_dependencies(&self, paths: &IndexMap<String, String>) -> Result<()> {
        for member in &self.members {
            for dep in &member.deps {
                validate_member_name(dep)?;
                if dep == &member.name {
                    return Err(Error::Argument(format!(
                        "member {} cannot depend on itself",
                        member.name
                    )));
                }
                if !paths.contains_key(dep) {
                    return Err(Error::Argument(format!(
                        "member {} depends on unknown member {}",
                        member.name, dep
                    )));
                }
            }
        }
        Ok(())
    }

    /// Return the relative file paths that `apply` would create for each member.
    pub fn dry_run(&self) -> Result<Vec<String>> {
        let paths = self.member_paths()?;
        self.validate_dependencies(&paths)?;

        let mut out = Vec::new();
        for member in &self.members {
            let member_path = paths
                .get(&member.name)
                .ok_or_else(|| Error::Argument(format!("member {} missing path", member.name)))?;
            let source = match member.kind {
                LayoutMemberKind::Lib => "lib.rs",
                LayoutMemberKind::Bin => "main.rs",
            };
            out.push(format!("{member_path}/Cargo.toml"));
            out.push(format!("{member_path}/src/{source}"));
        }
        Ok(out)
    }

    /// Generate all declared members into the workspace at `root`.
    pub fn apply(&self, root: &Path, ctx: &SyncContext) -> Result<()> {
        let root = root.canonicalize().map_err(Error::Io)?;
        let paths = self.member_paths()?;
        self.validate_dependencies(&paths)?;

        for member in &self.members {
            let member_path = paths
                .get(&member.name)
                .ok_or_else(|| Error::Argument(format!("member {} missing path", member.name)))?;
            let kind = member.kind.into();

            let deps: Vec<(String, String)> = member
                .deps
                .iter()
                .map(|dep| {
                    let dep_path = paths.get(dep).map(String::as_str).ok_or_else(|| {
                        Error::Argument(format!(
                            "member {} missing dependency {}",
                            member.name, dep
                        ))
                    })?;
                    let rel = relative_path(member_path, dep_path);
                    Ok::<_, Error>((dep.clone(), rel))
                })
                .collect::<Result<Vec<_>>>()?;

            add_workspace_member_with_deps(
                &root,
                &member.name,
                kind,
                Some(member_path),
                &deps,
                ctx,
            )?;
        }

        Ok(())
    }
}

/// Compute the relative path from `member_path` to `dep_path`.
///
/// Both paths are normalized relative strings using `/` as the separator and
/// containing no `..` segments. The result is the Cargo path-dependency string
/// that the member crate should use to depend on `dep_path`.
fn relative_path(member_path: &str, dep_path: &str) -> String {
    let member_parts: Vec<&str> = member_path.split('/').collect();
    let dep_parts: Vec<&str> = dep_path.split('/').collect();

    let mut common = 0;
    while member_parts.get(common).is_some() && member_parts.get(common) == dep_parts.get(common) {
        common += 1;
    }

    let up = member_parts.len().saturating_sub(common);
    let mut rel_parts = vec![".."; up];
    if let Some(suffix) = dep_parts.get(common..) {
        rel_parts.extend(suffix.iter().copied());
    }

    rel_parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_layout() {
        let content = r#"
[[members]]
name = "app"
kind = "bin"
"#;
        let layout = Layout::parse(content).unwrap();
        assert_eq!(layout.members.len(), 1);
        assert_eq!(layout.members[0].name, "app");
        assert!(matches!(layout.members[0].kind, LayoutMemberKind::Bin));
    }

    #[test]
    fn computes_relative_paths() {
        assert_eq!(
            relative_path("apps/app", "libs/shared"),
            "../../libs/shared"
        );
        assert_eq!(relative_path("crates/app", "crates/shared"), "../shared");
        assert_eq!(
            relative_path("tools/dev", "libs/shared"),
            "../../libs/shared"
        );
    }

    #[test]
    fn rejects_duplicate_names_and_paths() {
        let layout = Layout {
            members: vec![
                LayoutMember {
                    name: "foo".into(),
                    kind: LayoutMemberKind::Lib,
                    path: Some("crates/bar".into()),
                    deps: vec![],
                },
                LayoutMember {
                    name: "baz".into(),
                    kind: LayoutMemberKind::Lib,
                    path: Some("crates/bar".into()),
                    deps: vec![],
                },
            ],
        };
        assert!(layout.member_paths().is_err());
    }

    #[test]
    fn rejects_unknown_and_self_dependencies() {
        let layout = Layout {
            members: vec![LayoutMember {
                name: "app".into(),
                kind: LayoutMemberKind::Bin,
                path: None,
                deps: vec!["unknown".into()],
            }],
        };
        let paths = layout.member_paths().unwrap();
        assert!(layout.validate_dependencies(&paths).is_err());

        let layout2 = Layout {
            members: vec![LayoutMember {
                name: "app".into(),
                kind: LayoutMemberKind::Bin,
                path: None,
                deps: vec!["app".into()],
            }],
        };
        let paths2 = layout2.member_paths().unwrap();
        assert!(layout2.validate_dependencies(&paths2).is_err());
    }
}
