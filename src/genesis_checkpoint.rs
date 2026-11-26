//! Immutable checkpoint files: complete bytes are linked into place without overwrite.
use crate::genesis::SearchCheckpoint;
use anyhow::{ensure, Context, Result};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

pub fn save_new(path: &Path, checkpoint: &SearchCheckpoint) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(checkpoint)?;
    ensure!(bytes.len() <= 8 * 1024 * 1024, "checkpoint exceeds 8 MiB");
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    ensure!(
        path.file_name().is_some() && parent.is_dir(),
        "checkpoint parent directory must exist"
    );
    let temporary = parent.join(format!(".hydra-genesis-{}.tmp", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        // Atomic non-replacing publication on filesystems supporting hard links.
        // No weaker overwrite/rename fallback exists.
        fs::hard_link(&temporary, path)
            .context("publish checkpoint without overwrite; filesystem must support hard links")?;
        Ok(())
    })();
    let cleanup = fs::remove_file(&temporary);
    result?;
    cleanup.context("checkpoint published but temporary-link cleanup failed")?;
    Ok(())
}
