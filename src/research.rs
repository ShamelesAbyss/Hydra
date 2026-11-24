//! Bounded JSON inputs for offline research commands.
use anyhow::{ensure, Context, Result};
use serde::de::DeserializeOwned;
use std::{io::Read, path::Path};

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    const MAX_BYTES: u64 = 32 * 1024 * 1024;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .context("open research input")?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "research input exceeds 32 MiB"
    );
    serde_json::from_slice(&bytes).context("parse research JSON")
}
