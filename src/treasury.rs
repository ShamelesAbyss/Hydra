//! Dedicated Leviathan Treasury: OS-protected wallet credential storage,
//! public-address Hyperliquid account verification, truthful portfolio metrics,
//! operational modes, readiness gating, and emergency suspension.
//!
//! Security & financial invariants:
//! - Private signing keys NEVER leave this machine, NEVER enter logs/telemetry/errors,
//!   and are NEVER transmitted over the network (including during account verification).
//! - Threat-model note: Windows DPAPI (`CryptProtectData`) encrypts secrets under the
//!   current OS user identity; same-user OS malware running under the same user SID can
//!   query DPAPI, whereas cross-user access and plain config inspection are blocked.
//! - Callers should obtain an `EngineSignerAuthority` via `open_engine_signer` rather than
//!   handling raw key strings; plaintext export (`load_secret_for_local_signer`) is crate-private.
//! - No automatic withdrawals or arbitrary asset transfers exist anywhere in Hydra.
//! - Live / AWAKENED operation remains strictly gated behind all engineering,
//!   financial settlement, security, and operator acknowledgment requirements.
//! - Unavailable financial figures remain explicitly unavailable; deposits and
//!   withdrawals are never conflated with trading profits.

use crate::{
    account::{standard_perp_equity, verify_signer, UserRole},
    hyperliquid_wire::derive_address,
    ownership::canonical_account,
    telemetry::TelemetrySnapshot,
};
use anyhow::{anyhow, ensure, Context, Result};
use chrono::Utc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
    time::{Duration, Instant},
};

/// Exact verbatim risk acknowledgment required before authorizing live operation.
pub const OFFERING_ACKNOWLEDGMENT: &str =
    "Entrust Leviathan only with capital you are fully prepared to lose. Its seven heads pursue opportunity through computational intelligence, but no strategy guarantees profit.";

const PROFILE_FILE: &str = "treasury-profile.json";
const SECRET_FILE: &str = "treasury-secret.bin";
const MAINNET_INFO_URL: &str = "https://api.hyperliquid.xyz/info";
const MAX_INFO_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Zeroizing secret buffer that never formats its contents in Debug or Display.
pub struct SecretString {
    bytes: Vec<u8>,
}

impl SecretString {
    pub fn new(value: String) -> Self {
        Self {
            bytes: value.into_bytes(),
        }
    }
    pub fn as_str(&self) -> Result<&str> {
        std::str::from_utf8(&self.bytes)
            .map_err(|_| anyhow!("stored secret is not valid UTF-8; key material omitted"))
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        for b in &mut self.bytes {
            // SAFETY: valid aligned mutable pointer to owned Vec element.
            unsafe {
                std::ptr::write_volatile(b, 0);
            }
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
        self.bytes.clear();
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigningAccessScope {
    EngineExclusive,
    UiReadOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectionBackend {
    /// Windows DPAPI (`CryptProtectData`, current OS user scope).
    WindowsDpapiCurrentUser,
    /// POSIX owner-only file permissions (`0700` dir / `0600` file).
    UnixOwnerOnly0600,
}

impl ProtectionBackend {
    pub const fn current() -> Self {
        #[cfg(windows)]
        {
            Self::WindowsDpapiCurrentUser
        }
        #[cfg(not(windows))]
        {
            Self::UnixOwnerOnly0600
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::WindowsDpapiCurrentUser => "Windows DPAPI (OS Current-User Encrypted)",
            Self::UnixOwnerOnly0600 => {
                "Unix Owner-Only Permissions (chmod 0700/0600 — NOT ENCRYPTED AT REST)"
            }
        }
    }
    pub const fn is_encrypted_at_rest(self) -> bool {
        matches!(self, Self::WindowsDpapiCurrentUser)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignerRelationship {
    /// Signing key derives directly to the dedicated trading wallet address.
    MasterWallet,
    /// Signing key derives to a separate address; Hyperliquid must confirm agent approval.
    ApprovedAgent,
}

impl SignerRelationship {
    pub const fn label(self) -> &'static str {
        match self {
            Self::MasterWallet => "DEDICATED MASTER SIGNER",
            Self::ApprovedAgent => "SEPARATE AGENT SIGNER (REQUIRES VENUE APPROVAL)",
        }
    }
}

/// Six operational states for Awakening the Beast.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalMode {
    /// Trading disabled. Safe resting state.
    #[default]
    Dormant,
    /// Market monitoring only; no order signing or submission.
    Observation,
    /// Research and simulated execution only.
    Simulation,
    /// Explicitly authorized live operation; available only after all readiness gates pass.
    Awakened,
    /// Protective restrictions active; new exposure blocked, protective exits remain available.
    Guardian,
    /// Emergency trading suspension active; all new trading activity blocked.
    Suspended,
}

impl OperationalMode {
    pub const ALL: [Self; 6] = [
        Self::Dormant,
        Self::Observation,
        Self::Simulation,
        Self::Awakened,
        Self::Guardian,
        Self::Suspended,
    ];
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dormant => "DORMANT",
            Self::Observation => "OBSERVATION",
            Self::Simulation => "SIMULATION",
            Self::Awakened => "AWAKENED",
            Self::Guardian => "GUARDIAN",
            Self::Suspended => "SUSPENDED",
        }
    }
    pub const fn summary(self) -> &'static str {
        match self {
            Self::Dormant => "Trading disabled. Engine idle; no orders permitted.",
            Self::Observation => "Market monitoring only. No order signing or live exposure.",
            Self::Simulation => "Research and deterministic simulation only; live signing off.",
            Self::Awakened => {
                "Explicitly authorized live operation; locked until all readiness gates pass."
            }
            Self::Guardian => {
                "Protective restrictions active. New entries blocked; protective exits permitted."
            }
            Self::Suspended => {
                "Emergency suspension latched. All new trading activity blocked immediately."
            }
        }
    }
    pub const fn permits_new_live_entries(self) -> bool {
        matches!(self, Self::Awakened)
    }
    pub const fn protective_management_available(self) -> bool {
        matches!(self, Self::Guardian | Self::Suspended | Self::Awakened)
    }
}

/// Persisted non-secret metadata for the configured dedicated wallet.
/// Private keys are NEVER stored in this struct or serialized to JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreasuryProfile {
    pub schema_version: u32,
    pub dedicated_wallet_address: String,
    pub derived_signer_address: String,
    pub signer_relationship: SignerRelationship,
    pub protection_backend: ProtectionBackend,
    pub risk_acknowledged: bool,
    pub risk_acknowledged_at: Option<String>,
    pub risk_acknowledgment_hash: Option<String>,
    pub wallet_authorized: bool,
    pub emergency_suspended: bool,
    pub emergency_suspended_at: Option<String>,
    pub emergency_suspension_reason: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedWalletInput {
    pub dedicated_wallet_address: String,
    pub derived_signer_address: String,
    pub signer_relationship: SignerRelationship,
}

/// Validate a dedicated wallet public address and local private signing key
/// without ever formatting key bytes into errors or logs.
pub fn validate_wallet_inputs(
    wallet_address: &str,
    private_key: &str,
) -> Result<ValidatedWalletInput> {
    let wallet = canonical_account(wallet_address)
        .context("dedicated wallet address must be a valid non-zero 0x Ethereum address")?;
    let trimmed_key = private_key.trim();
    ensure!(
        !trimmed_key.is_empty(),
        "private signing key is required; key material omitted"
    );
    let signer = derive_address(trimmed_key)
        .map_err(|_| anyhow!("invalid secp256k1 private signing key; key material omitted"))?;
    let signer = canonical_account(&signer)?;
    let signer_relationship = if signer == wallet {
        SignerRelationship::MasterWallet
    } else {
        SignerRelationship::ApprovedAgent
    };
    Ok(ValidatedWalletInput {
        dedicated_wallet_address: wallet,
        derived_signer_address: signer,
        signer_relationship,
    })
}

pub fn acknowledgment_digest() -> String {
    hex::encode(Keccak256::digest(OFFERING_ACKNOWLEDGMENT.as_bytes()))
}

/// Mask a public address cleanly (e.g. `0x1234…abcd`) while keeping it verifiable.
pub fn mask_address(address: &str) -> String {
    let trimmed = address.trim();
    if trimmed.len() >= 12 {
        format!("{}…{}", &trimmed[..8], &trimmed[trimmed.len() - 6..])
    } else {
        "unavailable".into()
    }
}

/// Engine-owned signing capability that never exposes plaintext private key bytes to external
/// callers, research modules, or UI components.
pub struct EngineSignerAuthority {
    dedicated_wallet_address: String,
    derived_signer_address: String,
    signer_relationship: SignerRelationship,
    protection_backend: ProtectionBackend,
    signing_key: SecretString,
}

impl fmt::Debug for EngineSignerAuthority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineSignerAuthority")
            .field("dedicated_wallet_address", &self.dedicated_wallet_address)
            .field("derived_signer_address", &self.derived_signer_address)
            .field("signer_relationship", &self.signer_relationship)
            .field("protection_backend", &self.protection_backend)
            .field("signing_key", &"[REDACTED]")
            .finish()
    }
}

impl EngineSignerAuthority {
    /// Construct from an explicit disposable test/testnet fixture key without persisting on disk.
    pub fn from_test_fixture(wallet_address: &str, private_key: &str) -> Result<Self> {
        let validated = validate_wallet_inputs(wallet_address, private_key)?;
        Ok(Self {
            dedicated_wallet_address: validated.dedicated_wallet_address,
            derived_signer_address: validated.derived_signer_address,
            signer_relationship: validated.signer_relationship,
            protection_backend: ProtectionBackend::current(),
            signing_key: SecretString::new(private_key.trim().to_string()),
        })
    }

    pub fn wallet_address(&self) -> &str {
        &self.dedicated_wallet_address
    }

    pub fn signer_address(&self) -> &str {
        &self.derived_signer_address
    }

    pub fn signer_relationship(&self) -> SignerRelationship {
        self.signer_relationship
    }

    pub fn protection_backend(&self) -> ProtectionBackend {
        self.protection_backend
    }

    /// Sign a 32-byte digest strictly inside the authority boundary; never returns key material.
    pub fn sign_digest(&self, digest: [u8; 32]) -> Result<crate::hyperliquid_wire::WireSignature> {
        crate::hyperliquid_wire::sign_digest(self.signing_key.as_str()?, digest)
    }
}

/// Persistent OS-protected Treasury credential store.
#[derive(Clone, Debug)]
pub struct TreasuryStore {
    root: PathBuf,
}

impl TreasuryStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn default_store() -> Result<Self> {
        #[cfg(windows)]
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .context("LOCALAPPDATA required for Treasury credential vault")?;
        #[cfg(not(windows))]
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .context("XDG_STATE_HOME or HOME required for Treasury credential vault")?;
        Ok(Self::new(base.join("Hydra").join("treasury")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn profile_path(&self) -> PathBuf {
        self.root.join(PROFILE_FILE)
    }

    fn secret_path(&self) -> PathBuf {
        self.root.join(SECRET_FILE)
    }

    pub fn has_protected_key(&self) -> bool {
        self.secret_path().is_file()
    }

    pub fn load_profile(&self) -> Result<Option<TreasuryProfile>> {
        let path = self.profile_path();
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path).context("read treasury profile metadata")?;
        let profile: TreasuryProfile =
            serde_json::from_str(&raw).context("parse treasury profile metadata")?;
        ensure!(
            profile.schema_version == 1,
            "unsupported treasury profile schema version {}",
            profile.schema_version
        );
        canonical_account(&profile.dedicated_wallet_address)?;
        canonical_account(&profile.derived_signer_address)?;
        Ok(Some(profile))
    }

    /// Save validated dedicated wallet credentials into OS-protected storage.
    pub fn save_offering(
        &self,
        wallet_address: &str,
        private_key: &str,
        risk_acknowledged: bool,
        wallet_authorized: bool,
    ) -> Result<TreasuryProfile> {
        let validated = validate_wallet_inputs(wallet_address, private_key)?;
        let normalized_key = private_key
            .trim()
            .strip_prefix("0x")
            .unwrap_or(private_key.trim());
        let protected = protect_bytes(normalized_key.as_bytes())?;

        self.ensure_dir()?;
        write_atomic(&self.secret_path(), &protected, true)?;

        let now = Utc::now().to_rfc3339();
        let existing_suspended = self
            .load_profile()
            .ok()
            .flatten()
            .map(|p| {
                (
                    p.emergency_suspended,
                    p.emergency_suspended_at,
                    p.emergency_suspension_reason,
                )
            })
            .unwrap_or((false, None, None));

        let authorized = wallet_authorized && risk_acknowledged && !existing_suspended.0;
        let profile = TreasuryProfile {
            schema_version: 1,
            dedicated_wallet_address: validated.dedicated_wallet_address,
            derived_signer_address: validated.derived_signer_address,
            signer_relationship: validated.signer_relationship,
            protection_backend: ProtectionBackend::current(),
            risk_acknowledged,
            risk_acknowledged_at: risk_acknowledged.then(|| now.clone()),
            risk_acknowledgment_hash: risk_acknowledged.then(acknowledgment_digest),
            wallet_authorized: authorized,
            emergency_suspended: existing_suspended.0,
            emergency_suspended_at: existing_suspended.1,
            emergency_suspension_reason: existing_suspended.2,
            updated_at: now,
        };
        self.write_profile(&profile)?;
        Ok(profile)
    }

    /// Update authorization, risk acknowledgment, or emergency suspension flags
    /// without re-entering the private key.
    pub fn update_flags(
        &self,
        risk_acknowledged: bool,
        wallet_authorized: bool,
        emergency_suspended: bool,
        suspension_reason: Option<String>,
    ) -> Result<TreasuryProfile> {
        let mut profile = self
            .load_profile()?
            .ok_or_else(|| anyhow!("no dedicated wallet configured in Treasury"))?;
        let now = Utc::now().to_rfc3339();
        profile.risk_acknowledged = risk_acknowledged;
        if risk_acknowledged {
            if profile.risk_acknowledged_at.is_none() {
                profile.risk_acknowledged_at = Some(now.clone());
            }
            profile.risk_acknowledgment_hash = Some(acknowledgment_digest());
        } else {
            profile.risk_acknowledged_at = None;
            profile.risk_acknowledgment_hash = None;
        }
        profile.emergency_suspended = emergency_suspended;
        if emergency_suspended {
            profile.emergency_suspended_at = Some(now.clone());
            profile.emergency_suspension_reason =
                suspension_reason.or_else(|| Some("Operator emergency suspension".into()));
            profile.wallet_authorized = false;
        } else {
            profile.emergency_suspended_at = None;
            profile.emergency_suspension_reason = None;
            profile.wallet_authorized = wallet_authorized && risk_acknowledged;
        }
        profile.updated_at = now;
        self.write_profile(&profile)?;
        Ok(profile)
    }

    /// Decrypt the stored signing key locally to verify vault integrity and signer match.
    /// Never returns raw key material to the caller.
    pub fn verify_vault_integrity(&self) -> Result<String> {
        let profile = self
            .load_profile()?
            .ok_or_else(|| anyhow!("no treasury profile configured"))?;
        let secret = self.load_secret_for_local_signer()?;
        let derived = derive_address(secret.as_str()?).map_err(|_| {
            anyhow!("stored signing key failed local validation; key material omitted")
        })?;
        let derived = canonical_account(&derived)?;
        ensure!(
            derived == profile.derived_signer_address,
            "stored key signer does not match recorded treasury profile"
        );
        Ok(derived)
    }

    /// Engine-only local secret load (crate-internal). Prefer `open_engine_signer`.
    pub(crate) fn load_secret_for_local_signer(&self) -> Result<SecretString> {
        let path = self.secret_path();
        ensure!(
            path.is_file(),
            "no protected signing key stored in Treasury"
        );
        let blob = fs::read(&path).context("read protected treasury secret")?;
        let plain = unprotect_bytes(&blob)?;
        let s = String::from_utf8(plain)
            .map_err(|_| anyhow!("decrypted credential is invalid UTF-8; key material omitted"))?;
        Ok(SecretString::new(s))
    }

    /// Requires encrypted-at-rest OS credential backend before granting live signing access.
    pub fn require_encrypted_credential_backend(&self) -> Result<ProtectionBackend> {
        let backend = ProtectionBackend::current();
        ensure!(
            backend.is_encrypted_at_rest(),
            "credential backend '{}' does not encrypt signing keys at rest; live authorization denied",
            backend.label()
        );
        Ok(backend)
    }

    /// Opens an engine-owned `EngineSignerAuthority` wrapping signing capabilities without
    /// ever returning raw private key bytes to external callers or UI components.
    pub fn open_engine_signer(
        &self,
        scope: SigningAccessScope,
        require_authorized: bool,
    ) -> Result<EngineSignerAuthority> {
        ensure!(
            scope == SigningAccessScope::EngineExclusive,
            "EngineSignerAuthority denied for non-engine scope {:?}",
            scope
        );
        let backend = self.require_encrypted_credential_backend()?;
        let profile = self
            .load_profile()?
            .ok_or_else(|| anyhow!("no treasury profile configured"))?;
        ensure!(
            !profile.emergency_suspended,
            "treasury emergency suspension active; signer unavailable"
        );
        if require_authorized {
            ensure!(
                profile.wallet_authorized && profile.risk_acknowledged,
                "dedicated treasury wallet is not authorized or risk acknowledgment is missing"
            );
        }
        let secret = self.load_secret_for_local_signer()?;
        let derived = derive_address(secret.as_str()?).map_err(|_| {
            anyhow!("stored signing key failed local validation; key material omitted")
        })?;
        let derived = canonical_account(&derived)?;
        ensure!(
            derived == profile.derived_signer_address,
            "stored key signer does not match recorded treasury profile"
        );
        Ok(EngineSignerAuthority {
            dedicated_wallet_address: profile.dedicated_wallet_address,
            derived_signer_address: derived,
            signer_relationship: profile.signer_relationship,
            protection_backend: backend,
            signing_key: secret,
        })
    }

    /// Enforces `EngineExclusive` access control for decryption; GUI/research/external callers must use `UiReadOnly`.
    pub fn load_secret_with_scope(&self, scope: SigningAccessScope) -> Result<SecretString> {
        ensure!(
            scope == SigningAccessScope::EngineExclusive,
            "signing key decryption denied for non-engine scope {:?}; private key omitted",
            scope
        );
        self.require_encrypted_credential_backend()?;
        self.load_secret_for_local_signer()
    }

    /// Explicitly revokes live wallet authorization while preserving auditability & stored address identity.
    pub fn revoke_live_authorization(&self, reason: &str) -> Result<TreasuryProfile> {
        ensure!(
            !reason.trim().is_empty(),
            "revocation reason cannot be empty"
        );
        let mut profile = self
            .load_profile()?
            .ok_or_else(|| anyhow!("no treasury profile configured"))?;
        profile.wallet_authorized = false;
        profile.emergency_suspension_reason = Some(format!("REVOKED: {}", reason.trim()));
        profile.updated_at = Utc::now().to_rfc3339();
        self.write_profile(&profile)?;
        Ok(profile)
    }

    /// Replaces the local protected signing key and validates wallet ownership without exposing key material.
    pub fn replace_signing_key_audited(
        &self,
        wallet_address: &str,
        new_private_key: &str,
    ) -> Result<TreasuryProfile> {
        let existing = self
            .load_profile()?
            .ok_or_else(|| anyhow!("no treasury profile configured to replace"))?;
        self.save_offering(
            wallet_address,
            new_private_key,
            existing.risk_acknowledged,
            false, // Always requires fresh re-authorization after key rotation
        )
    }

    /// Securely overwrite and remove stored credentials and profile metadata.
    pub fn remove_credentials(&self) -> Result<()> {
        let secret = self.secret_path();
        if secret.exists() {
            if let Ok(meta) = fs::metadata(&secret) {
                let len = (meta.len() as usize).clamp(64, 16_384);
                if let Ok(mut f) = OpenOptions::new().write(true).open(&secret) {
                    let _ = f.write_all(&vec![0u8; len]);
                    let _ = f.sync_all();
                }
            }
            fs::remove_file(&secret).context("remove protected signing key")?;
        }
        let profile = self.profile_path();
        if profile.exists() {
            fs::remove_file(&profile).context("remove treasury profile")?;
        }
        Ok(())
    }

    fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.root).context("create treasury vault directory")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    fn write_profile(&self, profile: &TreasuryProfile) -> Result<()> {
        self.ensure_dir()?;
        let json = serde_json::to_vec_pretty(profile)?;
        write_atomic(&self.profile_path(), &json, false)
    }
}

fn write_atomic(path: &Path, bytes: &[u8], restricted: bool) -> Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .context("create temporary treasury file")?;
        #[cfg(unix)]
        if restricted {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        let _ = restricted;
        f.write_all(bytes)
            .context("write temporary treasury file")?;
        f.sync_all().context("sync temporary treasury file")?;
    }
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp, path).context("commit treasury file atomically")?;
    Ok(())
}

#[cfg(windows)]
mod dpapi {
    use anyhow::{bail, Result};
    use std::ffi::c_void;
    use std::ptr;

    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            p_data_in: *const DataBlob,
            sz_data_descr: *const u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut c_void,
            p_prompt_struct: *const c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            p_data_in: *const DataBlob,
            ppsz_data_descr: *mut *mut u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut c_void,
            p_prompt_struct: *const c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(h_mem: *mut c_void) -> *mut c_void;
    }

    const ENTROPY: &[u8] = b"HYDRA-PROJECT-LEVIATHAN-TREASURY-V1";

    pub fn protect(plain: &[u8]) -> Result<Vec<u8>> {
        let input = DataBlob {
            cb_data: u32::try_from(plain.len())?,
            pb_data: plain.as_ptr() as *mut u8,
        };
        let entropy = DataBlob {
            cb_data: ENTROPY.len() as u32,
            pb_data: ENTROPY.as_ptr() as *mut u8,
        };
        let mut output = DataBlob {
            cb_data: 0,
            pb_data: ptr::null_mut(),
        };
        // SAFETY: Valid pointers for duration of Win32 call; output buffer freed via LocalFree.
        let ok = unsafe {
            CryptProtectData(
                &input,
                ptr::null(),
                &entropy,
                ptr::null_mut(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 || output.pb_data.is_null() {
            bail!("Windows DPAPI encryption failed");
        }
        // SAFETY: output.pb_data is allocated by CryptProtectData with length output.cb_data.
        let result = unsafe {
            let slice = std::slice::from_raw_parts(output.pb_data, output.cb_data as usize);
            let vec = slice.to_vec();
            LocalFree(output.pb_data as *mut c_void);
            vec
        };
        Ok(result)
    }

    pub fn unprotect(cipher: &[u8]) -> Result<Vec<u8>> {
        let input = DataBlob {
            cb_data: u32::try_from(cipher.len())?,
            pb_data: cipher.as_ptr() as *mut u8,
        };
        let entropy = DataBlob {
            cb_data: ENTROPY.len() as u32,
            pb_data: ENTROPY.as_ptr() as *mut u8,
        };
        let mut output = DataBlob {
            cb_data: 0,
            pb_data: ptr::null_mut(),
        };
        // SAFETY: Valid pointers for duration of Win32 call; output scrubbed and freed via LocalFree.
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                &entropy,
                ptr::null_mut(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 || output.pb_data.is_null() {
            bail!("Windows DPAPI decryption failed");
        }
        // SAFETY: output.pb_data is valid for output.cb_data bytes; scrub before LocalFree.
        let result = unsafe {
            let slice = std::slice::from_raw_parts_mut(output.pb_data, output.cb_data as usize);
            let vec = slice.to_vec();
            for b in slice.iter_mut() {
                ptr::write_volatile(b, 0);
            }
            LocalFree(output.pb_data as *mut c_void);
            vec
        };
        Ok(result)
    }
}

fn protect_bytes(plain: &[u8]) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        dpapi::protect(plain)
    }
    #[cfg(not(windows))]
    {
        Ok(plain.to_vec())
    }
}

fn unprotect_bytes(stored: &[u8]) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        dpapi::unprotect(stored)
    }
    #[cfg(not(windows))]
    {
        Ok(stored.to_vec())
    }
}

/// Result of a genuine public-address Hyperliquid `/info` verification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HyperliquidVerification {
    pub verified_at: String,
    pub endpoint: String,
    pub dedicated_wallet_address: String,
    pub derived_signer_address: String,
    pub signer_relationship: SignerRelationship,
    pub account_mode: String,
    pub account_equity_usd: Decimal,
    pub withdrawable_collateral_usd: Decimal,
    pub margin_used_usd: Decimal,
    pub open_positions_count: usize,
    pub unrealized_pnl_usd: Decimal,
    pub latency_ms: u64,
    pub diagnostics: Vec<String>,
}

/// Verify a configured dedicated wallet against Hyperliquid's public read-only
/// `/info` endpoint using ONLY public addresses. Private keys never touch the network.
pub fn verify_hyperliquid_account(
    wallet_address: &str,
    signer_address: &str,
) -> Result<HyperliquidVerification> {
    verify_hyperliquid_account_at_endpoint(MAINNET_INFO_URL, wallet_address, signer_address)
}

/// Parameterized verification helper supporting offline loopback integration tests.
pub fn verify_hyperliquid_account_at_endpoint(
    info_url: &str,
    wallet_address: &str,
    signer_address: &str,
) -> Result<HyperliquidVerification> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("create bounded Hyperliquid verification runtime")?;
    rt.block_on(verify_hyperliquid_account_async(
        info_url,
        wallet_address,
        signer_address,
    ))
}

async fn verify_hyperliquid_account_async(
    info_url: &str,
    wallet_address: &str,
    signer_address: &str,
) -> Result<HyperliquidVerification> {
    let account = canonical_account(wallet_address)?;
    let signer = canonical_account(signer_address)?;
    let started = Instant::now();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("build Hyperliquid info client")?;

    async fn post_json(
        client: &reqwest::Client,
        info_url: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let resp = client
            .post(info_url)
            .json(&payload)
            .send()
            .await
            .context("query Hyperliquid read-only /info endpoint")?;
        let status = resp.status();
        ensure!(
            status.is_success(),
            "Hyperliquid /info returned HTTP {}",
            status.as_u16()
        );
        let bytes = resp.bytes().await.context("read Hyperliquid /info body")?;
        ensure!(
            bytes.len() <= MAX_INFO_RESPONSE_BYTES,
            "Hyperliquid /info response exceeded safety size bound"
        );
        serde_json::from_slice(&bytes).context("parse Hyperliquid /info JSON")
    }

    let account_role: UserRole = serde_json::from_value(
        post_json(
            &client,
            info_url,
            serde_json::json!({
                "type": "userRole",
                "user": account,
            }),
        )
        .await?,
    )
    .context("decode Hyperliquid account userRole")?;

    let signer_role: UserRole = if signer == account {
        UserRole {
            role: account_role.role.clone(),
            data: None,
        }
    } else {
        serde_json::from_value(
            post_json(
                &client,
                info_url,
                serde_json::json!({
                    "type": "userRole",
                    "user": signer,
                }),
            )
            .await?,
        )
        .context("decode Hyperliquid signer userRole")?
    };

    verify_signer(&signer, &account, &signer_role, &account_role)?;

    let abstraction_val = post_json(
        &client,
        info_url,
        serde_json::json!({
            "type": "userAbstraction",
            "user": account,
        }),
    )
    .await?;
    let account_mode = abstraction_val
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            abstraction_val
                .get("mode")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
        .ok_or_else(|| anyhow!("Hyperliquid userAbstraction response missing mode"))?;

    let ch_state = post_json(
        &client,
        info_url,
        serde_json::json!({
            "type": "clearinghouseState",
            "user": account,
        }),
    )
    .await?;
    let margin = ch_state
        .get("marginSummary")
        .ok_or_else(|| anyhow!("clearinghouseState missing marginSummary"))?;
    let equity_str = margin
        .get("accountValue")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("clearinghouseState missing accountValue"))?;
    // Enforce Hydra's existing standard-perp collateral mode check.
    let _ = standard_perp_equity(&account_mode, equity_str)?;
    let account_equity_usd =
        Decimal::from_str(equity_str).context("parse Hyperliquid accountValue decimal")?;

    let withdrawable_str = ch_state
        .get("withdrawable")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("clearinghouseState missing withdrawable collateral"))?;
    let withdrawable_collateral_usd =
        Decimal::from_str(withdrawable_str).context("parse withdrawable collateral decimal")?;

    let margin_used_str = margin
        .get("totalMarginUsed")
        .and_then(|v| v.as_str())
        .unwrap_or("0");
    let margin_used_usd =
        Decimal::from_str(margin_used_str).context("parse totalMarginUsed decimal")?;

    let mut open_positions_count = 0usize;
    let mut unrealized_pnl_usd = Decimal::ZERO;
    if let Some(positions) = ch_state.get("assetPositions").and_then(|v| v.as_array()) {
        for entry in positions {
            let Some(pos) = entry.get("position") else {
                continue;
            };
            let szi = pos
                .get("szi")
                .and_then(|v| v.as_str())
                .and_then(|s| Decimal::from_str(s).ok())
                .unwrap_or(Decimal::ZERO);
            if szi != Decimal::ZERO {
                open_positions_count += 1;
                if let Some(upnl) = pos
                    .get("unrealizedPnl")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Decimal::from_str(s).ok())
                {
                    unrealized_pnl_usd = unrealized_pnl_usd
                        .checked_add(upnl)
                        .context("unrealized PnL overflow")?;
                }
            }
        }
    }

    let signer_relationship = if signer == account {
        SignerRelationship::MasterWallet
    } else {
        SignerRelationship::ApprovedAgent
    };
    let mut diagnostics = vec![
        format!(
            "Verified public identity via read-only /info ({} ms).",
            started.elapsed().as_millis()
        ),
        "Private signing key remained strictly local and was never transmitted.".into(),
        "Standard Hyperliquid perp collateral mode confirmed (spot balances excluded).".into(),
    ];
    if account_equity_usd == Decimal::ZERO {
        diagnostics.push("Account equity is currently $0.00; fund this dedicated wallet independently on Hyperliquid before live authorization.".into());
    }

    Ok(HyperliquidVerification {
        verified_at: Utc::now().to_rfc3339(),
        endpoint: info_url.to_owned(),
        dedicated_wallet_address: account,
        derived_signer_address: signer,
        signer_relationship,
        account_mode,
        account_equity_usd,
        withdrawable_collateral_usd,
        margin_used_usd,
        open_positions_count,
        unrealized_pnl_usd,
        latency_ms: started.elapsed().as_millis() as u64,
        diagnostics,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateState {
    Passed,
    Missing,
    Blocked,
    PendingCertification,
}

impl GateState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Passed => "PASSED",
            Self::Missing => "REQUIRED",
            Self::Blocked => "BLOCKED",
            Self::PendingCertification => "PENDING CERTIFICATION",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadinessCheck {
    pub name: &'static str,
    pub state: GateState,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadinessReport {
    pub checks: Vec<ReadinessCheck>,
    pub can_awaken: bool,
}

impl ReadinessReport {
    /// Evaluate every prerequisite required before `OperationalMode::Awakened` can be permitted.
    pub fn evaluate(
        profile: Option<&TreasuryProfile>,
        has_protected_key: bool,
        verification: Option<&HyperliquidVerification>,
        snapshot: Option<&TelemetrySnapshot>,
    ) -> Self {
        let mut checks = Vec::with_capacity(7);

        let vault_ok = profile.is_some() && has_protected_key;
        checks.push(ReadinessCheck {
            name: "Dedicated Wallet & OS-Protected Key",
            state: if vault_ok {
                GateState::Passed
            } else {
                GateState::Missing
            },
            detail: if let Some(p) = profile {
                if has_protected_key {
                    format!(
                        "{} stored via {}",
                        mask_address(&p.dedicated_wallet_address),
                        p.protection_backend.label()
                    )
                } else {
                    "Profile metadata exists, but protected signing key file is missing.".into()
                }
            } else {
                "Complete The Offering with a dedicated trading wallet.".into()
            },
        });

        let ack_ok = profile.is_some_and(|p| p.risk_acknowledged);
        checks.push(ReadinessCheck {
            name: "Risk Acknowledgment Signed",
            state: if ack_ok {
                GateState::Passed
            } else {
                GateState::Missing
            },
            detail: if ack_ok {
                "Operator explicitly acknowledged capital-loss risk.".into()
            } else {
                "Operator must acknowledge The Offering risk disclosure.".into()
            },
        });

        let auth_ok = profile.is_some_and(|p| p.wallet_authorized);
        checks.push(ReadinessCheck {
            name: "Explicit Wallet Authorization",
            state: if auth_ok {
                GateState::Passed
            } else {
                GateState::Missing
            },
            detail: if auth_ok {
                "Operator authorization flag enabled for dedicated wallet.".into()
            } else {
                "Wallet authorization is disabled.".into()
            },
        });

        let verify_ok = verification.is_some_and(|v| {
            profile.is_some_and(|p| {
                v.dedicated_wallet_address == p.dedicated_wallet_address
                    && v.derived_signer_address == p.derived_signer_address
            })
        });
        checks.push(ReadinessCheck {
            name: "Hyperliquid Account Verification",
            state: if verify_ok {
                GateState::Passed
            } else {
                GateState::Missing
            },
            detail: if let Some(v) = verification.filter(|_| verify_ok) {
                format!(
                    "Verified standard perp account ({} mode, equity ${}, collateral ${}).",
                    v.account_mode, v.account_equity_usd, v.withdrawable_collateral_usd
                )
            } else {
                "Run read-only Hyperliquid account verification for the active wallet.".into()
            },
        });

        let suspended = profile.is_some_and(|p| p.emergency_suspended);
        let breaker_latched =
            snapshot.is_some_and(|s| s.risk_accounts.iter().any(|a| a.breaker_reason.is_some()));
        let unresolved_intents = snapshot
            .map(|s| {
                s.intents
                    .iter()
                    .filter(|i| {
                        matches!(
                            i.state.as_str(),
                            "submitted" | "unresolved" | "unknown" | "ambiguous"
                        )
                    })
                    .count()
            })
            .unwrap_or(0);
        let safe_state = !suspended && !breaker_latched && unresolved_intents == 0;
        checks.push(ReadinessCheck {
            name: "Safeguard & Unresolved-Event Clear",
            state: if safe_state {
                GateState::Passed
            } else {
                GateState::Blocked
            },
            detail: if suspended {
                "Emergency suspension is latched.".into()
            } else if breaker_latched {
                "Persistent AEGIS risk breaker is latched.".into()
            } else if unresolved_intents > 0 {
                format!(
                    "{unresolved_intents} unresolved execution intent(s) require reconciliation."
                )
            } else {
                "No emergency suspension, risk breaker, or unresolved intent observed locally."
                    .into()
            },
        });

        // Permanent certification gate: unattended autonomous live operation stays
        // disabled until settlement, full recovery, control IPC, and watchdog pass.
        checks.push(ReadinessCheck {
            name: "Financial Settlement & Unattended Certification",
            state: GateState::PendingCertification,
            detail: crate::live_gate::certification_summary(),
        });

        let can_awaken = checks.iter().all(|c| c.state == GateState::Passed);
        Self { checks, can_awaken }
    }
}

/// Truthful classification of every Treasury financial metric.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricProvenance {
    /// Verified from fresh read-only Hyperliquid clearinghouse snapshot.
    VerifiedVenue,
    /// Observed from local read-only SQLite history (not current venue truth).
    RecordedLocal,
    /// Not yet available or uncertified; never guessed or fabricated.
    Unavailable,
}

impl MetricProvenance {
    pub const fn badge(self) -> &'static str {
        match self {
            Self::VerifiedVenue => "VERIFIED",
            Self::RecordedLocal => "LOCAL HISTORY",
            Self::Unavailable => "UNAVAILABLE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreasuryMetric {
    pub label: &'static str,
    pub value: String,
    pub provenance: MetricProvenance,
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct TreasuryPortfolioSummary {
    pub metrics: Vec<TreasuryMetric>,
    pub unresolved_count: usize,
}

impl TreasuryPortfolioSummary {
    /// Assemble all required Treasury metrics without inventing unavailable values
    /// and without ever conflating deposits/withdrawals with trading profits.
    pub fn build(
        verification: Option<&HyperliquidVerification>,
        snapshot: Option<&TelemetrySnapshot>,
    ) -> Self {
        let mut metrics = Vec::with_capacity(12);

        // 1. Starting capital (distinct from trading profits).
        metrics.push(TreasuryMetric {
            label: "Starting Capital",
            value: "unavailable".into(),
            provenance: MetricProvenance::Unavailable,
            note: "Requires verified deposit/withdrawal ledger baseline; never guessed from equity snapshots.".into(),
        });

        // 2. Current account equity.
        if let Some(v) = verification {
            metrics.push(TreasuryMetric {
                label: "Current Account Equity",
                value: format!("${:.2}", v.account_equity_usd),
                provenance: MetricProvenance::VerifiedVenue,
                note: format!(
                    "Standard perp equity from Hyperliquid /info at {}.",
                    v.verified_at
                ),
            });
        } else if let Some(last) = snapshot.and_then(|s| s.equity.last()) {
            metrics.push(TreasuryMetric {
                label: "Current Account Equity",
                value: format!("${:.2} (hist)", last.recorded_equity),
                provenance: MetricProvenance::RecordedLocal,
                note: format!(
                    "Last recorded SQLite snapshot at {}; not live venue balance.",
                    last.at
                ),
            });
        } else {
            metrics.push(TreasuryMetric {
                label: "Current Account Equity",
                value: "unavailable".into(),
                provenance: MetricProvenance::Unavailable,
                note: "Verify Hyperliquid account or open local history.".into(),
            });
        }

        // 3. Available trading collateral.
        if let Some(v) = verification {
            metrics.push(TreasuryMetric {
                label: "Available Collateral",
                value: format!("${:.2}", v.withdrawable_collateral_usd),
                provenance: MetricProvenance::VerifiedVenue,
                note: "Withdrawable / free perp collateral reported by Hyperliquid.".into(),
            });
        } else {
            metrics.push(TreasuryMetric {
                label: "Available Collateral",
                value: "unavailable".into(),
                provenance: MetricProvenance::Unavailable,
                note: "Requires fresh Hyperliquid account verification.".into(),
            });
        }

        // 4. Capital allocated to positions.
        if let Some(v) = verification {
            metrics.push(TreasuryMetric {
                label: "Capital Allocated",
                value: format!("${:.2} ({} pos)", v.margin_used_usd, v.open_positions_count),
                provenance: MetricProvenance::VerifiedVenue,
                note: "Total margin allocated across open perp positions.".into(),
            });
        } else if let Some(notional) = snapshot
            .and_then(|s| s.risk_accounts.first())
            .and_then(|a| a.reserved_notional.as_deref())
        {
            metrics.push(TreasuryMetric {
                label: "Capital Allocated",
                value: format!("${notional} (reserved)"),
                provenance: MetricProvenance::RecordedLocal,
                note: "Local AEGIS reserved notional; not current venue margin used.".into(),
            });
        } else {
            metrics.push(TreasuryMetric {
                label: "Capital Allocated",
                value: "unavailable".into(),
                provenance: MetricProvenance::Unavailable,
                note: "No verified margin usage or active AEGIS reservation observed.".into(),
            });
        }

        // 5. Unrealized PnL.
        if let Some(v) = verification {
            metrics.push(TreasuryMetric {
                label: "Unrealized PnL",
                value: format!("${:.2}", v.unrealized_pnl_usd),
                provenance: MetricProvenance::VerifiedVenue,
                note: "Mark-to-market unrealized PnL across verified open perp positions.".into(),
            });
        } else {
            metrics.push(TreasuryMetric {
                label: "Unrealized PnL",
                value: "unavailable".into(),
                provenance: MetricProvenance::Unavailable,
                note: "Live marks unavailable without fresh venue verification.".into(),
            });
        }

        // 6. Verified realized net PnL (strictly uncertified until complete fee/funding settlement).
        metrics.push(TreasuryMetric {
            label: "Verified Realized Net PnL",
            value: "unavailable".into(),
            provenance: MetricProvenance::Unavailable,
            note: "Authoritative round-trip net PnL remains uncertified until complete entry-fee and funding attribution is proven.".into(),
        });

        // 7. Trading fees (per-currency without summing distinct tokens).
        if let Some(data) = snapshot.filter(|s| s.has_table("ledger_fills")) {
            let mut by_ccy = BTreeMap::<String, Decimal>::new();
            let mut missing_fee = false;
            for f in &data.fills {
                match (&f.fee_amount, &f.fee_currency) {
                    (Some(amt), Some(ccy)) => {
                        if let Ok(d) = Decimal::from_str(amt) {
                            let entry = by_ccy.entry(ccy.clone()).or_insert(Decimal::ZERO);
                            *entry = entry.saturating_add(d);
                        } else {
                            missing_fee = true;
                        }
                    }
                    _ => missing_fee = true,
                }
            }
            if data.fills.is_empty() {
                metrics.push(TreasuryMetric {
                    label: "Trading Fees",
                    value: "0 fills recorded".into(),
                    provenance: MetricProvenance::RecordedLocal,
                    note: "Bounded local ledger_fills table is present with zero rows.".into(),
                });
            } else if missing_fee {
                metrics.push(TreasuryMetric {
                    label: "Trading Fees",
                    value: "partial / unknown".into(),
                    provenance: MetricProvenance::RecordedLocal,
                    note: "Some recorded fills have unknown fee amounts; missing fees are never guessed as zero.".into(),
                });
            } else {
                let summary = by_ccy
                    .into_iter()
                    .map(|(ccy, amt)| format!("{amt:.4} {ccy}"))
                    .collect::<Vec<_>>()
                    .join(" · ");
                metrics.push(TreasuryMetric {
                    label: "Trading Fees",
                    value: summary,
                    provenance: MetricProvenance::RecordedLocal,
                    note: "Summed per currency over bounded local ledger_fills window.".into(),
                });
            }
        } else {
            metrics.push(TreasuryMetric {
                label: "Trading Fees",
                value: "unavailable".into(),
                provenance: MetricProvenance::Unavailable,
                note: "Authoritative ledger_fills table unavailable.".into(),
            });
        }

        // 8. Funding payments.
        if let Some(count) = snapshot.and_then(|s| s.count("ledger_funding")) {
            metrics.push(TreasuryMetric {
                label: "Funding Payments",
                value: format!("{}{} rows", count.value, if count.capped { "+" } else { "" }),
                provenance: MetricProvenance::RecordedLocal,
                note: "Recorded ledger_funding count; full period settlement coverage remains pending.".into(),
            });
        } else {
            metrics.push(TreasuryMetric {
                label: "Funding Payments",
                value: "unavailable".into(),
                provenance: MetricProvenance::Unavailable,
                note: "Authoritative ledger_funding table unavailable.".into(),
            });
        }

        // 9. Drawdown (computed strictly when same-scope peak & equity exist).
        let drawdown_metric = if let Some((v, acct)) = verification.and_then(|v| {
            snapshot
                .and_then(|s| {
                    s.risk_accounts
                        .iter()
                        .find(|a| a.account == v.dedicated_wallet_address)
                })
                .map(|acct| (v, acct))
        }) {
            if let Ok(peak) = Decimal::from_str(&acct.high_water_equity) {
                if peak > Decimal::ZERO {
                    let dd = ((peak - v.account_equity_usd).max(Decimal::ZERO) / peak)
                        * Decimal::from(100);
                    TreasuryMetric {
                        label: "Drawdown",
                        value: format!("{dd:.2}% (peak ${peak})"),
                        provenance: MetricProvenance::RecordedLocal,
                        note: "Compared verified equity against persistent AEGIS high-water equity for this account.".into(),
                    }
                } else {
                    TreasuryMetric {
                        label: "Drawdown",
                        value: "unavailable".into(),
                        provenance: MetricProvenance::Unavailable,
                        note: "Persistent high-water peak is zero.".into(),
                    }
                }
            } else {
                TreasuryMetric {
                    label: "Drawdown",
                    value: "unavailable".into(),
                    provenance: MetricProvenance::Unavailable,
                    note: "Could not parse persisted high-water peak.".into(),
                }
            }
        } else {
            TreasuryMetric {
                label: "Drawdown",
                value: "unavailable".into(),
                provenance: MetricProvenance::Unavailable,
                note: "Requires both verified equity and matching-account AEGIS high-water equity."
                    .into(),
            }
        };
        metrics.push(drawdown_metric);

        // 10. Deposits & Withdrawals separation (distinct from trading profits).
        metrics.push(TreasuryMetric {
            label: "Deposits / Withdrawals",
            value: "separated / pending".into(),
            provenance: MetricProvenance::Unavailable,
            note: "External capital transfers are tracked separately and never counted as trading profit. Automatic withdrawals are permanently disabled.".into(),
        });

        // 11. Active risk limits.
        if let Some(data) = snapshot.filter(|s| !s.risk_accounts.is_empty()) {
            let breakers = data
                .risk_accounts
                .iter()
                .filter(|a| a.breaker_reason.is_some())
                .count();
            let reservations: usize = data
                .risk_accounts
                .iter()
                .map(|a| a.active_reservations.value)
                .sum();
            metrics.push(TreasuryMetric {
                label: "Active Risk Limits",
                value: if breakers > 0 {
                    format!("{breakers} BREAKER(S) · {reservations} res")
                } else {
                    format!("{} acct · {reservations} res", data.risk_accounts.len())
                },
                provenance: MetricProvenance::RecordedLocal,
                note: "Persistent AEGIS account policies and active exposure reservations.".into(),
            });
        } else {
            metrics.push(TreasuryMetric {
                label: "Active Risk Limits",
                value: "unavailable".into(),
                provenance: MetricProvenance::Unavailable,
                note: "No persisted AEGIS risk accounts observed in local snapshot.".into(),
            });
        }

        // 12. Unresolved financial events.
        let unresolved_intents = snapshot
            .map(|s| {
                s.intents
                    .iter()
                    .filter(|i| {
                        matches!(
                            i.state.as_str(),
                            "submitted" | "unresolved" | "unknown" | "ambiguous"
                        )
                    })
                    .count()
            })
            .unwrap_or(0);
        let breakers = snapshot
            .map(|s| {
                s.risk_accounts
                    .iter()
                    .filter(|a| a.breaker_reason.is_some())
                    .count()
            })
            .unwrap_or(0);
        let unresolved_count = unresolved_intents + breakers;
        metrics.push(TreasuryMetric {
            label: "Unresolved Financial Events",
            value: if snapshot.is_some() {
                format!("{unresolved_count} observed")
            } else {
                "unavailable".into()
            },
            provenance: if snapshot.is_some() {
                MetricProvenance::RecordedLocal
            } else {
                MetricProvenance::Unavailable
            },
            note: "Unresolved execution intents and active AEGIS breaker holds. Unknown net settlement remains blocked.".into(),
        });

        Self {
            metrics,
            unresolved_count,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityStage {
    Implemented,
    Partial,
    Pending,
}

impl CapabilityStage {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Implemented => "IMPLEMENTED",
            Self::Partial => "PARTIAL",
            Self::Pending => "PENDING",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AutonomousCapability {
    pub name: &'static str,
    pub head: &'static str,
    pub stage: CapabilityStage,
    pub detail: &'static str,
}

/// Truthful engineering inventory of the Autonomous Leviathan architecture.
pub const AUTONOMOUS_CAPABILITIES: &[AutonomousCapability] = &[
    AutonomousCapability {
        name: "Continuous market monitoring",
        head: "HUNTER",
        stage: CapabilityStage::Partial,
        detail: "Headless engine & TUI monitor configured symbols; autonomous acquisition pending.",
    },
    AutonomousCapability {
        name: "Autonomous opportunity discovery",
        head: "HUNTER",
        stage: CapabilityStage::Partial,
        detail: "Offline Radar ranking & hysteresis implemented; live market rotation pending.",
    },
    AutonomousCapability {
        name: "Strategy evaluation & selection",
        head: "FANG",
        stage: CapabilityStage::Partial,
        detail: "Existing strategies & offline deterministic planner exist; shared parity pending.",
    },
    AutonomousCapability {
        name: "Independent risk assessment",
        head: "AEGIS",
        stage: CapabilityStage::Implemented,
        detail: "Persistent high-water peaks, drawdown breakers & atomic exposure reservations.",
    },
    AutonomousCapability {
        name: "Authorized order signing & execution",
        head: "NEXUS",
        stage: CapabilityStage::Partial,
        detail: "Native secp256k1 IOC signing & durable nonces exist; live defaults disabled.",
    },
    AutonomousCapability {
        name: "Position management",
        head: "NEXUS",
        stage: CapabilityStage::Partial,
        detail: "Entry context & exact residual proofs exist; full fixed-point core pending.",
    },
    AutonomousCapability {
        name: "Protective exits",
        head: "AEGIS",
        stage: CapabilityStage::Partial,
        detail: "Isolated exit loop exists; native venue protective orders pending certification.",
    },
    AutonomousCapability {
        name: "Financial reconciliation",
        head: "NEXUS",
        stage: CapabilityStage::Partial,
        detail: "Deduplicated fill/funding ledger exists; complete net settlement remains open.",
    },
    AutonomousCapability {
        name: "Persistent operational history",
        head: "HEART",
        stage: CapabilityStage::Implemented,
        detail: "Transactional SQLite WAL history, durable intent journal & read-only telemetry.",
    },
    AutonomousCapability {
        name: "Recovery after network interruptions",
        head: "NEXUS",
        stage: CapabilityStage::Partial,
        detail: "Unresolved orders stay held; full WebSocket resubscription & replay pending.",
    },
    AutonomousCapability {
        name: "Recovery after restarts",
        head: "NEXUS",
        stage: CapabilityStage::Partial,
        detail: "Narrow known-receipt recovery & account locks exist; broad replay pending.",
    },
    AutonomousCapability {
        name: "Watchdog supervision",
        head: "HEART",
        stage: CapabilityStage::Pending,
        detail: "Independent headless engine runs without GUI; external watchdog daemon pending.",
    },
    AutonomousCapability {
        name: "Configurable notifications",
        head: "HEART",
        stage: CapabilityStage::Pending,
        detail: "Structured logs & TUI/cockpit alerts exist; external alert channels pending.",
    },
    AutonomousCapability {
        name: "Emergency trading suspension",
        head: "AEGIS",
        stage: CapabilityStage::Implemented,
        detail: "Immediate local suspension latch blocks new entries; authenticated IPC pending.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    // Well-known deterministic test private key (never holds real funds).
    const TEST_KEY: &str = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn secret_string_redacts_debug_and_validates_wallet_identity() {
        let secret = SecretString::new(TEST_KEY.into());
        assert_eq!(format!("{secret:?}"), "[REDACTED]");
        let derived = derive_address(TEST_KEY).unwrap();
        let master = validate_wallet_inputs(&derived, TEST_KEY).unwrap();
        assert_eq!(master.signer_relationship, SignerRelationship::MasterWallet);

        let other = "0x1111111111111111111111111111111111111111";
        let agent = validate_wallet_inputs(other, TEST_KEY).unwrap();
        assert_eq!(agent.signer_relationship, SignerRelationship::ApprovedAgent);

        let err = validate_wallet_inputs(other, "bad-secret-material-xyz").unwrap_err();
        assert!(!format!("{err:#}").contains("bad-secret-material-xyz"));
    }

    #[test]
    fn os_protected_treasury_roundtrip_update_and_secure_removal() {
        let dir = tempfile::tempdir().unwrap();
        let store = TreasuryStore::new(dir.path().join("treasury"));
        let derived = derive_address(TEST_KEY).unwrap();

        let saved = store.save_offering(&derived, TEST_KEY, true, true).unwrap();
        assert!(saved.risk_acknowledged);
        assert!(saved.wallet_authorized);
        assert!(!saved.emergency_suspended);
        assert!(store.has_protected_key());
        assert_eq!(store.verify_vault_integrity().unwrap(), derived);

        // Profile JSON must never contain the private signing key.
        let profile_raw =
            fs::read_to_string(dir.path().join("treasury").join(PROFILE_FILE)).unwrap();
        assert!(!profile_raw.contains("0123456789abcdef0123456789abcdef"));

        // Emergency suspension immediately revokes wallet authorization.
        let suspended = store
            .update_flags(true, true, true, Some("Manual halt".into()))
            .unwrap();
        assert!(suspended.emergency_suspended);
        assert!(!suspended.wallet_authorized);

        // Removal wipes both files cleanly.
        store.remove_credentials().unwrap();
        assert!(!store.has_protected_key());
        assert!(store.load_profile().unwrap().is_none());
    }

    #[test]
    fn readiness_gate_keeps_awakened_locked_while_certification_is_pending() {
        let dir = tempfile::tempdir().unwrap();
        let store = TreasuryStore::new(dir.path().join("treasury"));
        let derived = derive_address(TEST_KEY).unwrap();
        let profile = store.save_offering(&derived, TEST_KEY, true, true).unwrap();
        let verification = HyperliquidVerification {
            verified_at: Utc::now().to_rfc3339(),
            endpoint: "http://127.0.0.1/info".into(),
            dedicated_wallet_address: derived.clone(),
            derived_signer_address: derived,
            signer_relationship: SignerRelationship::MasterWallet,
            account_mode: "disabled".into(),
            account_equity_usd: Decimal::from(500),
            withdrawable_collateral_usd: Decimal::from(450),
            margin_used_usd: Decimal::from(50),
            open_positions_count: 1,
            unrealized_pnl_usd: Decimal::from(12),
            latency_ms: 14,
            diagnostics: vec![],
        };
        let report = ReadinessReport::evaluate(Some(&profile), true, Some(&verification), None);
        assert!(!report.can_awaken);
        assert!(report
            .checks
            .iter()
            .any(|c| c.state == GateState::PendingCertification));
    }

    #[test]
    fn loopback_hyperliquid_verification_checks_role_mode_and_collateral_without_sending_key() {
        let derived = derive_address(TEST_KEY).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap();
                let req = String::from_utf8_lossy(&buf[..n]);
                // Private key material must never appear in any request.
                assert!(!req.contains("0123456789abcdef0123456789abcdef"));
                let body = if req.contains("userRole") {
                    r#"{"role":"user"}"#
                } else if req.contains("userAbstraction") {
                    r#""disabled""#
                } else {
                    r#"{"marginSummary":{"accountValue":"1250.50","totalMarginUsed":"120.25"},"withdrawable":"1130.25","assetPositions":[{"position":{"szi":"0.5","unrealizedPnl":"18.75"}}]}"#
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(resp.as_bytes()).unwrap();
            }
        });

        let url = format!("http://{addr}/info");
        let verified = verify_hyperliquid_account_at_endpoint(&url, &derived, &derived).unwrap();
        server.join().unwrap();

        assert_eq!(verified.account_mode, "disabled");
        assert_eq!(
            verified.account_equity_usd,
            Decimal::from_str("1250.50").unwrap()
        );
        assert_eq!(
            verified.withdrawable_collateral_usd,
            Decimal::from_str("1130.25").unwrap()
        );
        assert_eq!(
            verified.margin_used_usd,
            Decimal::from_str("120.25").unwrap()
        );
        assert_eq!(verified.open_positions_count, 1);
        assert_eq!(
            verified.unrealized_pnl_usd,
            Decimal::from_str("18.75").unwrap()
        );
    }

    #[test]
    fn treasury_portfolio_never_fabricates_unavailable_or_net_pnl_values() {
        let summary = TreasuryPortfolioSummary::build(None, None);
        assert_eq!(summary.metrics.len(), 12);
        assert!(summary
            .metrics
            .iter()
            .all(|m| m.provenance == MetricProvenance::Unavailable));
        let net_pnl = summary
            .metrics
            .iter()
            .find(|m| m.label == "Verified Realized Net PnL")
            .unwrap();
        assert_eq!(net_pnl.value, "unavailable");
        let deposits = summary
            .metrics
            .iter()
            .find(|m| m.label == "Deposits / Withdrawals")
            .unwrap();
        assert!(deposits.note.contains("never counted as trading profit"));
    }

    #[test]
    fn signing_access_scope_and_authorization_revocation_isolate_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let store = TreasuryStore::new(dir.path().to_path_buf());
        let derived = derive_address(TEST_KEY).unwrap();
        let p = store.save_offering(&derived, TEST_KEY, true, true).unwrap();
        assert!(p.wallet_authorized);

        // UiReadOnly callers are refused decryption cleanly
        let ui_err = store
            .load_secret_with_scope(SigningAccessScope::UiReadOnly)
            .unwrap_err()
            .to_string();
        assert!(ui_err.contains("SigningAccessScope") || ui_err.contains("denied"));
        assert!(!ui_err.contains(&TEST_KEY[2..]));

        // Revocation disables authorization without deleting identity
        let revoked = store.revoke_live_authorization("operator audit").unwrap();
        assert!(!revoked.wallet_authorized);
        assert!(revoked
            .emergency_suspension_reason
            .as_deref()
            .unwrap_or("")
            .contains("REVOKED"));

        // Key rotation resets authorization to false until explicit operator re-approval
        let rotated = store
            .replace_signing_key_audited(&derived, TEST_KEY)
            .unwrap();
        assert!(!rotated.wallet_authorized);

        if cfg!(windows) {
            let sec = store
                .load_secret_with_scope(SigningAccessScope::EngineExclusive)
                .unwrap();
            assert_eq!(format!("{sec:?}"), "[REDACTED]");

            // Re-authorize wallet cleanly and verify EngineSignerAuthority signs without exporting key bytes.
            let _ = store.update_flags(true, true, false, None).unwrap();
            let signer = store
                .open_engine_signer(SigningAccessScope::EngineExclusive, true)
                .unwrap();
            let dbg = format!("{signer:?}");
            assert!(dbg.contains("[REDACTED]"));
            assert!(!dbg.contains(&TEST_KEY[2..]));
            let sig = signer.sign_digest([0x11; 32]).unwrap();
            assert_eq!(sig.r.len(), 66);
        }

        // UiReadOnly is rejected by open_engine_signer too, and from_test_fixture signs safely.
        assert!(store
            .open_engine_signer(SigningAccessScope::UiReadOnly, false)
            .is_err());
        let fx = EngineSignerAuthority::from_test_fixture(&derived, TEST_KEY).unwrap();
        assert_eq!(fx.signer_address(), derived);
        assert!(fx.sign_digest([0x22; 32]).is_ok());
    }
}
