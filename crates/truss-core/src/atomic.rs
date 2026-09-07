//! Durable file replacement.

use crate::error::Result;
use std::path::Path;

/// Replace a file's contents in one step.
///
/// `std::fs::write` truncates the target and then writes into it, so an
/// interrupted process or a full disk leaves a half-written file where a
/// complete one used to be. For the registry and the marketplace index that
/// means every later command fails to parse what it loads. Writing a sibling
/// temporary file and renaming it over the target never exposes a partial
/// state: the rename is atomic within a directory, and readers see either the
/// old file or the new one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;

    // Same directory as the target: a rename across filesystems fails.
    let temp = parent.join(format!(
        ".{}.tmp{}",
        path.file_name()
            .map_or_else(|| "file".into(), |n| n.to_string_lossy()),
        std::process::id()
    ));

    let result = (|| -> Result<()> {
        use std::io::Write;
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        // The rename orders against the data only once it has reached the
        // filesystem; without this the new name can survive a crash while its
        // contents do not.
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)?;
        Ok(())
    })();

    if result.is_err() {
        // A leftover temporary file would accumulate on every failure.
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.json");
        std::fs::write(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }

    #[test]
    fn creates_the_parent_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("index.json");
        write_atomic(&path, b"body").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"body");
    }

    #[test]
    fn leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.json");
        write_atomic(&path, b"body").unwrap();
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["index.json".to_string()]);
    }
}
