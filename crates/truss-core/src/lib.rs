pub mod error;
pub mod registry;
pub mod sync;
pub mod template;

pub use error::{Error, Result};
pub use registry::Registry;

use std::path::Path;

pub fn new_workspace(path: &Path, template_dir: Option<&Path>) -> Result<()> {
    let files = match template_dir {
        Some(dir) => template::read_directory_files(dir),
        None => template::default_files(),
    }?;

    std::fs::create_dir_all(path)?;

    for (name, content) in files {
        let file_path = path.join(&name);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, content.as_bytes())?;
    }

    Ok(())
}

pub fn sync_workspace(path: &Path, entry: Option<&str>) -> Result<()> {
    sync::sync_workspace(path, entry)
}

pub fn check_workspace(path: &Path, entry: Option<&str>) -> Result<()> {
    sync::check_workspace(path, entry)
}
