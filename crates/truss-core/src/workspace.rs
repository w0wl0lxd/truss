//! Workspace member management: add, list, and remove Cargo workspace members.

use crate::error::{Error, Result};
use crate::pathsafe::{ensure_under_root, is_symlink, normalize_relative_path};
use crate::sync::SyncContext;
use crate::template::Engine;
use std::fmt::Write as _;
use std::path::Path;
use toml_edit::{Array, DocumentMut, Item, Value, value};

/// Whether the new member is a library or a binary crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Lib,
    Bin,
}

const MEMBER_CARGO_TOML: &str = r#"[package]
name = "{{ project_name }}"
version.workspace = true
edition.workspace = true
authors.workspace = true{% if license %}
license.workspace = true{% endif %}{% if repository %}
repository.workspace = true{% endif %}

[lints]
workspace = true
"#;

const LIB_RS: &str = r"//! The `{{ project_name }}` crate.

/// Adds two numbers.
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
";

const MAIN_RS: &str = r#"fn main() {
    println!("Hello from {{ project_name }}!");
}
"#;

/// Add a workspace member.
///
/// `member_path` is the relative path stored in `workspace.members`. When
/// `None`, it defaults to `crates/<name>`. Files are only written when the
/// member directory is created by this call; an existing directory is left
/// untouched.
pub fn add_workspace_member(
    root: &Path,
    name: &str,
    kind: MemberKind,
    member_path: Option<&str>,
    ctx: &SyncContext,
) -> Result<()> {
    add_workspace_member_with_deps(root, name, kind, member_path, &[], ctx)
}

/// Add a workspace member with inter-crate path dependencies.
///
/// `deps` is a list of `(crate_name, relative_path)` pairs that are written as
/// `path` dependencies in the member's `Cargo.toml`.
pub fn add_workspace_member_with_deps(
    root: &Path,
    name: &str,
    kind: MemberKind,
    member_path: Option<&str>,
    deps: &[(String, String)],
    ctx: &SyncContext,
) -> Result<()> {
    validate_member_name(name)?;

    let root = root.canonicalize().map_err(Error::Io)?;
    let cargo_path = root.join("Cargo.toml");
    if !cargo_path.is_file() {
        return Err(Error::Argument(format!(
            "workspace root has no Cargo.toml: {}",
            root.display()
        )));
    }

    let member_path = resolve_member_path(name, member_path)?;
    let member_dir = root.join(&member_path);

    ensure_under_root(&root, &member_dir)?;
    if has_symlink_in_path(&member_dir, &root)? {
        return Err(Error::Argument(format!(
            "refusing to follow symlink in member path: {}",
            member_dir.display()
        )));
    }

    let dir_exists = member_dir.try_exists()?;
    if dir_exists && member_dir.is_file() {
        return Err(Error::Argument(format!(
            "member path exists as a file: {}",
            member_dir.display()
        )));
    }

    let mut document = load_manifest(&cargo_path)?;
    append_member_to_manifest(&mut document, &member_path)?;
    std::fs::write(&cargo_path, document.to_string())?;

    if dir_exists || member_dir.try_exists()? {
        return Ok(());
    }

    if let Some(parent) = member_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir(&member_dir)?;

    let engine = Engine::new();
    let member_ctx = ctx.clone().with_project_name(name);
    let mut cargo_toml = MEMBER_CARGO_TOML.to_string();
    if !deps.is_empty() {
        cargo_toml.push_str("\n[dependencies]\n");
        for (dep_name, dep_path) in deps {
            let _ = writeln!(cargo_toml, "{dep_name} = {{ path = \"{dep_path}\" }}");
        }
    }
    let cargo_toml = engine.render_str(&cargo_toml, &member_ctx)?;
    let source_name = source_file_name(kind);
    let source_template = match kind {
        MemberKind::Lib => LIB_RS,
        MemberKind::Bin => MAIN_RS,
    };
    let source = engine.render_str(source_template, &member_ctx)?;

    let src_dir = member_dir.join("src");
    std::fs::create_dir(&src_dir)?;
    std::fs::write(member_dir.join("Cargo.toml"), cargo_toml)?;
    std::fs::write(src_dir.join(source_name), source)?;

    Ok(())
}

/// List the member paths declared in `workspace.members`.
///
/// Returns an empty vector when `workspace.members` is absent. Errors when the
/// root `Cargo.toml` has no `[workspace]` table.
pub fn list_workspace_members(root: &Path) -> Result<Vec<String>> {
    let root = root.canonicalize().map_err(Error::Io)?;
    let cargo_path = root.join("Cargo.toml");
    let document = load_manifest(&cargo_path)?;

    let workspace = document
        .get("workspace")
        .and_then(Item::as_table)
        .ok_or_else(|| Error::Argument("no [workspace] in Cargo.toml".to_string()))?;

    let Some(members_item) = workspace.get("members") else {
        return Ok(Vec::new());
    };

    let members = members_item
        .as_array()
        .ok_or_else(|| Error::Argument("workspace.members is not an array".to_string()))?;

    let mut out = Vec::with_capacity(members.len());
    for v in members {
        if let Some(s) = v.as_str() {
            out.push(s.to_string());
        }
    }
    Ok(out)
}

/// Remove a workspace member.
///
/// `name_or_path` is the member path as recorded in `workspace.members`. If it
/// contains no path separator, it defaults to `crates/<name_or_path>`.
/// When `delete` is true, the member directory is removed after validation.
pub fn remove_workspace_member(root: &Path, name_or_path: &str, delete: bool) -> Result<()> {
    if name_or_path.is_empty() {
        return Err(Error::Argument(
            "member name or path cannot be empty".to_string(),
        ));
    }

    let root = root.canonicalize().map_err(Error::Io)?;
    let cargo_path = root.join("Cargo.toml");
    let mut document = load_manifest(&cargo_path)?;

    let normalized = normalize_member_path(name_or_path)?;
    let member_path = if normalized.chars().any(std::path::is_separator) {
        normalized
    } else {
        format!("crates/{normalized}")
    };

    let member_dir = root.join(&member_path);
    let dir_exists = if delete {
        ensure_under_root(&root, &member_dir)?;
        if has_symlink_in_path(&member_dir, &root)? {
            return Err(Error::Argument(format!(
                "refusing to follow symlink in member path: {}",
                member_dir.display()
            )));
        }
        let exists = member_dir.try_exists()?;
        if exists && (is_symlink(&member_dir)? || !member_dir.is_dir()) {
            return Err(Error::Argument(format!(
                "member path is not a directory: {}",
                member_dir.display()
            )));
        }
        exists
    } else {
        false
    };

    remove_member_from_manifest(&mut document, &member_path)?;
    std::fs::write(&cargo_path, document.to_string())?;

    if delete && dir_exists {
        std::fs::remove_dir_all(&member_dir)?;
    }

    Ok(())
}

fn resolve_member_path(name: &str, member_path: Option<&str>) -> Result<String> {
    if let Some(path) = member_path {
        normalize_member_path(path)
    } else {
        Ok(format!("crates/{name}"))
    }
}

fn normalize_member_path(path: &str) -> Result<String> {
    // Treat backslashes the same as forward slashes so the same relative path
    // works on Windows and is stored consistently in workspace.members.
    let normalized = path.replace('\\', "/");
    normalize_relative_path(&normalized)
}

fn load_manifest(cargo_path: &Path) -> Result<DocumentMut> {
    let manifest = std::fs::read_to_string(cargo_path)?;
    manifest.parse::<DocumentMut>().map_err(Error::Toml)
}

fn append_member_to_manifest(document: &mut DocumentMut, member_path: &str) -> Result<()> {
    let workspace = document
        .get_mut("workspace")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| Error::Argument("no [workspace] in Cargo.toml".to_string()))?;

    let members = match workspace.get_mut("members") {
        Some(item) => item
            .as_array_mut()
            .ok_or_else(|| Error::Argument("workspace.members is not an array".to_string()))?,
        None => {
            workspace["members"] = value(Value::Array(Array::new()));
            workspace
                .get_mut("members")
                .and_then(Item::as_array_mut)
                .ok_or_else(|| Error::Argument("failed to create workspace.members".to_string()))?
        }
    };

    if !members.iter().any(|v| v.as_str() == Some(member_path)) {
        members.push(member_path);
    }

    Ok(())
}

fn remove_member_from_manifest(document: &mut DocumentMut, member_path: &str) -> Result<()> {
    let workspace = document
        .get_mut("workspace")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| Error::Argument("no [workspace] in Cargo.toml".to_string()))?;

    let members = workspace
        .get_mut("members")
        .and_then(Item::as_array_mut)
        .ok_or_else(|| Error::Argument("workspace.members not found".to_string()))?;

    let index = members
        .iter()
        .enumerate()
        .find_map(|(i, v)| (v.as_str() == Some(member_path)).then_some(i))
        .ok_or_else(|| Error::Argument(format!("member not found: {member_path}")))?;

    members.remove(index);
    Ok(())
}

fn source_file_name(kind: MemberKind) -> &'static str {
    match kind {
        MemberKind::Lib => "lib.rs",
        MemberKind::Bin => "main.rs",
    }
}

pub(crate) fn validate_member_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::Argument("member name cannot be empty".to_string()));
    }
    if name.len() > 64 {
        return Err(Error::Argument(format!(
            "member name exceeds 64 characters: {name}"
        )));
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(Error::Argument("member name cannot be empty".to_string()));
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return Err(Error::Argument(format!(
            "member name must start with a letter or underscore: {name}"
        )));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(Error::Argument(format!(
            "member name contains invalid characters: {name}"
        )));
    }

    const RESERVED: &[&str] = &[
        "aux", "con", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
        "nul", "prn", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
    ];
    if RESERVED.contains(&name.to_ascii_lowercase().as_str()) {
        return Err(Error::Argument(format!(
            "member name is a reserved Windows name: {name}"
        )));
    }

    Ok(())
}

fn has_symlink_in_path(path: &Path, root: &Path) -> Result<bool> {
    let mut current = Some(path);
    while let Some(p) = current {
        if p.as_os_str().is_empty() || p == root {
            break;
        }
        if is_symlink(p)? {
            return Ok(true);
        }
        current = p.parent();
    }
    Ok(false)
}
