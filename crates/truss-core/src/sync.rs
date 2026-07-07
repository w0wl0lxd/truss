use crate::error::{Error, Result};
use indexmap::IndexMap;
use std::path::Path;

#[derive(Debug, Default)]
pub struct SyncContext {
    pub entries: IndexMap<String, String>,
}

impl SyncContext {
    pub fn new() -> Self {
        Self::default()
    }
}

pub fn sync_workspace(path: &Path, entry: Option<&str>) -> Result<()> {
    let manifest_path = path.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest_path)?;
    let mut doc = text.parse::<toml_edit::DocumentMut>()?;

    let workspace = doc
        .as_table_mut()
        .get_mut("workspace")
        .and_then(toml_edit::Item::as_table_mut)
        .ok_or_else(|| Error::Argument("workspace table missing".to_string()))?;

    let members = workspace
        .get_mut("members")
        .and_then(toml_edit::Item::as_array_mut)
        .ok_or_else(|| Error::Argument("workspace.members array missing".to_string()))?;

    if let Some(entry) = entry {
        let already_present = members.iter().all(|v| v.as_str() != Some(entry));
        if already_present {
            members.push(entry);
        }
    }

    std::fs::write(&manifest_path, doc.to_string())?;
    Ok(())
}

pub fn check_workspace(path: &Path, entry: Option<&str>) -> Result<()> {
    let manifest_path = path.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest_path)?;
    let doc = text.parse::<toml_edit::DocumentMut>()?;

    let workspace = doc
        .as_table()
        .get("workspace")
        .and_then(toml_edit::Item::as_table)
        .ok_or_else(|| Error::Validation("workspace table missing".to_string()))?;

    let members = workspace
        .get("members")
        .and_then(toml_edit::Item::as_array)
        .ok_or_else(|| Error::Validation("workspace.members array missing".to_string()))?;

    if let Some(entry) = entry {
        let present = members.iter().any(|v| v.as_str() == Some(entry));
        if !present {
            return Err(Error::Validation(format!("entry {entry:?} not in workspace")));
        }
    }

    let flake_path = path.join("flake.nix");
    if !flake_path.try_exists()? {
        return Err(Error::Validation("flake.nix missing".to_string()));
    }

    let _ = serde_json::to_string(&serde_json::json!({
        "workspace": path.display().to_string(),
        "entry": entry,
    }))
    .map_err(Error::Json)?;

    Ok(())
}
