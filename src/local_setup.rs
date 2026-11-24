//! Explicit local initialization, independent of credentials and exchange I/O.
use anyhow::{Context, Result};
use std::path::Path;

pub fn initialize_history(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .context("create a new history database; existing files are preserved")?;
    drop(file);
    let storage = crate::storage::Storage::open(path)?;
    storage.migrate()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn initialization_is_empty_and_never_overwrites_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        super::initialize_history(&path).unwrap();
        let snapshot = crate::telemetry::load_snapshot(&path, 10).unwrap();
        assert_eq!(snapshot.count("positions").unwrap().value, 0);
        assert!(super::initialize_history(&path).is_err());
        let protected = dir.path().join("existing.txt");
        std::fs::write(&protected, b"preserve me").unwrap();
        assert!(super::initialize_history(&protected).is_err());
        assert_eq!(std::fs::read(protected).unwrap(), b"preserve me");
    }
}
