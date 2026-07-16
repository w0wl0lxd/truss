use crate::error::{Error, Result};
use crate::layout::{Layout, LayoutMember, LayoutMemberKind};
use crate::pathsafe::{ensure_under_root, is_symlink, validate_relative_path};
use indexmap::IndexMap;
use std::path::Path;
use toml_edit::DocumentMut;

#[derive(Debug, Clone, Default)]
pub struct ExtractOptions {
    pub force: bool,
    pub skip_prompts: bool,
    pub extra_values: IndexMap<String, String>,
}

pub fn extract_pack(source: &Path, pack: &Path, options: &ExtractOptions) -> Result<()> {
    if !source.try_exists()? {
        return Err(Error::Argument(format!(
            "source does not exist: {}",
            source.display()
        )));
    }
    if !source.is_dir() {
        return Err(Error::Argument(format!(
            "source is not a directory: {}",
            source.display()
        )));
    }
    if source == pack {
        return Err(Error::Argument(
            "source and pack destination must be different directories".into(),
        ));
    }

    let source_canon = source.canonicalize().map_err(Error::Io)?;
    if pack.try_exists()? {
        if !options.force {
            return Err(Error::Argument(format!(
                "pack destination already exists; use --force to overwrite: {}",
                pack.display()
            )));
        }
        let pack_canon = pack.canonicalize().map_err(Error::Io)?;
        if pack_canon == source_canon {
            return Err(Error::Argument(
                "source and pack destination are the same directory".into(),
            ));
        }
        if pack_canon.starts_with(&source_canon) {
            return Err(Error::Argument(
                "pack destination cannot be inside the source directory".into(),
            ));
        }
        std::fs::remove_dir_all(&pack_canon)?;
    }

    let values = discover_values(&source_canon, options)?;
    let sorted = sorted_values(&values);

    std::fs::create_dir_all(pack)?;

    let mut stack = vec![source_canon.clone()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)? {
            let entry = entry?;
            let path = entry.path();
            if is_symlink(&path)? {
                continue;
            }
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                if should_skip_dir(&path, &source_canon) {
                    continue;
                }
                stack.push(path);
            } else if file_type.is_file() {
                if should_skip_file(&path, &source_canon) {
                    continue;
                }
                let rel = path
                    .strip_prefix(&source_canon)
                    .map_err(|e| Error::Argument(e.to_string()))?;
                let rel = normalize_snapshot_path(rel);
                validate_relative_path(&rel)?;

                let mut rel_replaced = rel.clone();
                let content = std::fs::read(&path)?;
                let is_text = is_text(&content);

                for (literal, placeholder) in &sorted {
                    rel_replaced = rel_replaced.replace(literal, placeholder);
                }
                validate_relative_path(&rel_replaced)?;
                let output_path = pack.join(&rel_replaced);
                ensure_under_root(pack, &output_path)?;

                let final_content = if is_text {
                    let mut text = String::from_utf8(content).map_err(Error::Utf8)?;
                    for (literal, placeholder) in &sorted {
                        text = text.replace(literal, placeholder);
                    }
                    text.into_bytes()
                } else {
                    content
                };

                if let Some(parent) = output_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&output_path, final_content)?;
                copy_mode(&path, &output_path)?;
            }
        }
    }

    if let Some(layout) = discover_layout(&source_canon, &values)? {
        let layout_path = pack.join("layout.toml");
        ensure_under_root(pack, &layout_path)?;
        let layout_toml = serialize_layout(&layout);
        std::fs::write(&layout_path, layout_toml)?;
    }

    if !options.skip_prompts {
        // Built-in context variables are supplied by `truss new` flags, so an
        // extracted pack does not need prompts for them. An empty manifest is
        // written to reserve the file for future pack-specific prompts.
        let manifest_path = pack.join("truss.toml");
        ensure_under_root(pack, &manifest_path)?;
        std::fs::write(&manifest_path, "[prompts]\n")?;
    }

    Ok(())
}

fn discover_values(source: &Path, options: &ExtractOptions) -> Result<IndexMap<String, String>> {
    let mut values = IndexMap::new();

    let cargo_path = source.join("Cargo.toml");
    if cargo_path.try_exists()? {
        let manifest = std::fs::read_to_string(&cargo_path)?;
        let document = manifest.parse::<DocumentMut>()?;
        let workspace_package = document
            .get("workspace")
            .and_then(toml_edit::Item::as_table_like)
            .and_then(|workspace| workspace.get("package"))
            .and_then(toml_edit::Item::as_table_like);
        let package = document
            .get("package")
            .and_then(toml_edit::Item::as_table_like);

        if let Some(name) = metadata_string(workspace_package, package, "name") {
            values.insert("project_name".into(), name);
        }
        if let Some(author) = metadata_author(workspace_package, package) {
            values.insert("author".into(), author);
        }
        if let Some(license) = metadata_string(workspace_package, package, "license") {
            values.insert("license".into(), license);
        }
        if let Some(repository) = metadata_string(workspace_package, package, "repository") {
            values.insert("repository".into(), repository);
        }
        if let Some(edition) = metadata_string(workspace_package, package, "edition") {
            values.insert("edition".into(), edition);
        }
    }

    if !values.contains_key("project_name") {
        let fallback = source
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().to_string());
        values.insert("project_name".into(), fallback);
    }

    for (k, v) in &options.extra_values {
        values.insert(k.clone(), v.clone());
    }

    Ok(values)
}

fn sorted_values(values: &IndexMap<String, String>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = values
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| (v.clone(), placeholder(k)))
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.0.len()));
    out
}

fn placeholder(name: &str) -> String {
    format!("{{{{ {name} }}}}")
}

fn metadata_string(
    workspace_package: Option<&dyn toml_edit::TableLike>,
    package: Option<&dyn toml_edit::TableLike>,
    key: &str,
) -> Option<String> {
    workspace_package
        .and_then(|table| table.get(key))
        .or_else(|| package.and_then(|table| table.get(key)))?
        .as_str()
        .map(String::from)
}

fn metadata_author(
    workspace_package: Option<&dyn toml_edit::TableLike>,
    package: Option<&dyn toml_edit::TableLike>,
) -> Option<String> {
    workspace_package
        .and_then(|table| table.get("authors"))
        .or_else(|| package.and_then(|table| table.get("authors")))
        .and_then(toml_edit::Item::as_array)
        .and_then(|authors| authors.get(0))
        .and_then(|author| author.as_str())
        .map(str::to_string)
}

fn is_text(bytes: &[u8]) -> bool {
    !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

fn normalize_snapshot_path(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

fn should_skip_dir(dir: &Path, source: &Path) -> bool {
    if dir == source {
        return false;
    }
    if let Some(name) = dir.file_name() {
        let n = name.to_string_lossy();
        if n == ".git" || n == ".truss" || n == "target" {
            return true;
        }
    }
    false
}

fn should_skip_file(file: &Path, source: &Path) -> bool {
    if file == source.join("truss.toml") {
        return true;
    }
    false
}

fn copy_mode(src: &Path, dst: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(src)?;
        let mode = meta.permissions().mode() & 0o777;
        let mut perms = std::fs::metadata(dst)?.permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(dst, perms)?;
    }
    let _ = (src, dst);
    Ok(())
}

fn discover_layout(source: &Path, _values: &IndexMap<String, String>) -> Result<Option<Layout>> {
    let cargo_path = source.join("Cargo.toml");
    if !cargo_path.try_exists()? {
        return Ok(None);
    }
    let manifest = std::fs::read_to_string(&cargo_path)?;
    let document = manifest.parse::<DocumentMut>()?;
    let Some(workspace) = document
        .get("workspace")
        .and_then(toml_edit::Item::as_table_like)
    else {
        return Ok(None);
    };
    let Some(members_item) = workspace.get("members") else {
        return Ok(None);
    };
    let Some(members_array) = members_item.as_array() else {
        return Ok(None);
    };

    let mut layout_members = Vec::new();
    for item in members_array {
        let Some(member_path) = item.as_str() else {
            continue;
        };
        let resolved = source.join(member_path);
        let name = member_path
            .split('/')
            .next_back()
            .map_or(member_path, |s| s)
            .to_string();
        let kind = detect_member_kind(&resolved)?;
        layout_members.push(LayoutMember {
            name,
            kind,
            path: Some(member_path.to_string()),
            deps: Vec::new(),
        });
    }

    if layout_members.is_empty() {
        return Ok(None);
    }

    Ok(Some(Layout {
        members: layout_members,
    }))
}

fn detect_member_kind(member_dir: &Path) -> Result<LayoutMemberKind> {
    let cargo = member_dir.join("Cargo.toml");
    if !cargo.try_exists()? {
        return Ok(LayoutMemberKind::Bin);
    }
    let content = std::fs::read_to_string(&cargo)?;
    let document = content.parse::<DocumentMut>()?;
    if document.get("lib").is_some() {
        return Ok(LayoutMemberKind::Lib);
    }
    if let Some(bin) = document.get("bin") {
        if bin.is_array() {
            return Ok(LayoutMemberKind::Bin);
        }
    }
    // Default: a package with no explicit [lib] is a binary unless it has a src/lib.rs.
    if member_dir.join("src/lib.rs").try_exists()? {
        return Ok(LayoutMemberKind::Lib);
    }
    Ok(LayoutMemberKind::Bin)
}

fn serialize_layout(layout: &Layout) -> String {
    let mut lines = Vec::new();
    for member in &layout.members {
        lines.push("[[members]]".to_string());
        lines.push(format!("name = \"{}\"", escape_toml(&member.name)));
        lines.push(format!("kind = \"{}\"", member_kind_str(member.kind)));
        if let Some(path) = &member.path {
            lines.push(format!("path = \"{}\"", escape_toml(path)));
        }
    }
    lines.join("\n") + "\n"
}

fn member_kind_str(kind: LayoutMemberKind) -> &'static str {
    match kind {
        LayoutMemberKind::Lib => "lib",
        LayoutMemberKind::Bin => "bin",
    }
}

fn escape_toml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn opts() -> ExtractOptions {
        ExtractOptions {
            force: true,
            ..ExtractOptions::default()
        }
    }

    #[test]
    fn extract_replaces_project_name_in_content_and_path() {
        let source = TempDir::new().unwrap();
        std::fs::write(
            source.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .unwrap();
        std::fs::create_dir(source.path().join("demo")).unwrap();
        std::fs::write(
            source.path().join("demo/main.rs"),
            "fn main() { println!(\"demo\"); }",
        )
        .unwrap();

        let pack = TempDir::new().unwrap();
        extract_pack(source.path(), pack.path(), &opts()).unwrap();

        assert!(pack.path().join("{{ project_name }}/main.rs").is_file());
        let content =
            std::fs::read_to_string(pack.path().join("{{ project_name }}/main.rs")).unwrap();
        assert!(content.contains("{{ project_name }}"));
    }

    #[test]
    fn extract_preserves_binary_files() {
        let source = TempDir::new().unwrap();
        std::fs::write(
            source.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .unwrap();
        std::fs::write(source.path().join("image.bin"), vec![0u8, 1, 2, 3]).unwrap();

        let pack = TempDir::new().unwrap();
        extract_pack(source.path(), pack.path(), &opts()).unwrap();

        let bytes = std::fs::read(pack.path().join("image.bin")).unwrap();
        assert_eq!(bytes, vec![0u8, 1, 2, 3]);
    }

    #[test]
    fn extract_rejects_same_source_and_pack() {
        let source = TempDir::new().unwrap();
        std::fs::write(
            source.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .unwrap();
        let result = extract_pack(source.path(), source.path(), &opts());
        assert!(result.is_err());
    }
}
