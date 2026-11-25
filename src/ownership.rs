//! Operating-system ownership locks coordinate engines using different databases.
use anyhow::{ensure, Context, Result};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

pub fn canonical_account(address: &str) -> Result<String> {
    let address = address.trim();
    ensure!(
        address.len() == 42
            && address.starts_with("0x")
            && address[2..].bytes().all(|b| b.is_ascii_hexdigit()),
        "account must be a 20-byte 0x-prefixed Ethereum address"
    );
    ensure!(
        address[2..].bytes().any(|b| b != b'0'),
        "zero trading account is invalid"
    );
    Ok(address.to_ascii_lowercase())
}

pub struct AccountOwnership {
    _file: File,
}
impl AccountOwnership {
    /// Root must be an operator-owned directory shared by all Hydra databases.
    pub fn acquire(root: &Path, account: &str) -> Result<Self> {
        let account = canonical_account(account)?;
        std::fs::create_dir_all(root).context("create account lock directory")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(format!("hyperliquid-mainnet-{account}.lock")))?;
        file.try_lock().map_err(|_| anyhow::anyhow!("trading account is already owned or cannot be exclusively locked; stop its existing engine before another controller"))?;
        // Never unlink a lock file: inode replacement would let a second owner in.
        Ok(Self { _file: file })
    }

    pub fn default_root() -> Result<PathBuf> {
        #[cfg(windows)]
        let root = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .context("LOCALAPPDATA required for account ownership")?;
        #[cfg(not(windows))]
        let root = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .context("XDG_STATE_HOME or HOME required for account ownership")?;
        Ok(root.join("Hydra").join("account-locks"))
    }
}

/// Signer derivation is local only. Never format key parsing errors or key material.
pub fn configured_account(cfg: &crate::config::AppConfig) -> Result<Option<String>> {
    let signer = cfg
        .hl_private_key
        .as_ref()
        .map(|key| {
            crate::hyperliquid_wire::derive_address(key)
                .map_err(|_| anyhow::anyhow!("invalid signing key; key material omitted"))
        })
        .transpose()?;
    let account = cfg.hl_account_address.as_deref().or(signer.as_deref());
    account.map(canonical_account).transpose()
}
