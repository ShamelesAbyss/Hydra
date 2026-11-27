//! LEVIATHAN EVOLUTION — secure, truthful native update manager for HYDRA.
//!
//! Security & operational invariants:
//! - Uses the canonical GitHub Releases API (`/repos/ShamelesAbyss/Hydra/releases`)
//!   and NEVER misuses `/releases/latest` to retrieve prereleases.
//! - Verifies downloaded artifacts with pure-Rust FIPS 180-4 SHA-256 before staging.
//! - Supports cryptographic `secp256k1` signed-manifest provenance verification.
//! - Automatic installation stays strictly disabled until pinned release-signing
//!   infrastructure is configured and verified.
//! - Enforces a trading-safe maintenance gate: never installs or restarts while
//!   live/guardian trading is active, positions are open, or execution intents are
//!   unresolved, and never auto-resumes live trading after restart.
//! - Never modifies `%LOCALAPPDATA%\Hydra\data\`, Treasury credentials, risk policies,
//!   or financial history.

use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::Duration;

use crate::telemetry::TelemetrySnapshot;
use crate::treasury::OperationalMode;

pub const DEFAULT_GITHUB_RELEASES_API: &str =
    "https://api.github.com/repos/ShamelesAbyss/Hydra/releases?per_page=25";
pub const CANONICAL_RELEASES_PAGE: &str = "https://github.com/ShamelesAbyss/Hydra/releases";
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
pub const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;

/// Pinned compressed secp256k1 release-signing public key (33-byte hex).
/// Remains `None` until dedicated offline/CI release-signing key infrastructure
/// is configured. Automatic installation is disabled while `None`.
pub const PINNED_RELEASE_SIGNING_PUBKEY_HEX: Option<&str> = None;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateChannel {
    /// Only non-prerelease stable releases.
    Stable,
    /// Development prereleases (`alpha`, `beta`, `rc`) and newer stable releases.
    Development,
}

impl UpdateChannel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stable => "STABLE",
            Self::Development => "DEVELOPMENT (PRERELEASE)",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Stable => "Stable certified releases only (none published yet).",
            Self::Development => "Alpha/Beta/RC engineering prereleases and newer stable releases.",
        }
    }

    pub fn default_for_installed() -> Self {
        let installed = installed_version();
        if installed.is_prerelease() {
            Self::Development
        } else {
            Self::Stable
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreId {
    Numeric(u64),
    AlphaNum(String),
}

impl Ord for PreId {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Numeric(a), Self::Numeric(b)) => a.cmp(b),
            // SemVer 2.0.0 §11.4.3: numeric identifiers always have lower precedence than non-numeric.
            (Self::Numeric(_), Self::AlphaNum(_)) => Ordering::Less,
            (Self::AlphaNum(_), Self::Numeric(_)) => Ordering::Greater,
            (Self::AlphaNum(a), Self::AlphaNum(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for PreId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Vec<PreId>,
}

impl SemVer {
    pub fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        let without_v = trimmed.strip_prefix('v').unwrap_or(trimmed);
        let without_build = without_v.split('+').next().unwrap_or(without_v);
        let (core, pre_part) = match without_build.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (without_build, None),
        };
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() != 3 {
            bail!("Invalid SemVer '{raw}': expected major.minor.patch");
        }
        let major = parts[0]
            .parse::<u64>()
            .with_context(|| format!("Invalid major version in '{raw}'"))?;
        let minor = parts[1]
            .parse::<u64>()
            .with_context(|| format!("Invalid minor version in '{raw}'"))?;
        let patch = parts[2]
            .parse::<u64>()
            .with_context(|| format!("Invalid patch version in '{raw}'"))?;
        let mut pre = Vec::new();
        if let Some(p) = pre_part {
            if p.is_empty() {
                bail!("Invalid SemVer '{raw}': empty prerelease identifier");
            }
            for seg in p.split('.') {
                if seg.is_empty() {
                    bail!("Invalid SemVer '{raw}': empty prerelease segment");
                }
                if seg.chars().all(|c| c.is_ascii_digit()) {
                    pre.push(PreId::Numeric(seg.parse::<u64>().with_context(|| {
                        format!("Invalid numeric prerelease '{seg}'")
                    })?));
                } else {
                    pre.push(PreId::AlphaNum(seg.to_string()));
                }
            }
        }
        Ok(Self {
            major,
            minor,
            patch,
            pre,
        })
    }

    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-")?;
            for (i, seg) in self.pre.iter().enumerate() {
                if i > 0 {
                    write!(f, ".")?;
                }
                match seg {
                    PreId::Numeric(n) => write!(f, "{n}")?,
                    PreId::AlphaNum(s) => write!(f, "{s}")?,
                }
            }
        }
        Ok(())
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                // SemVer 2.0.0 §11.3: normal version has higher precedence than prerelease.
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (a, b) in self.pre.iter().zip(other.pre.iter()) {
                        let c = a.cmp(b);
                        if c != Ordering::Equal {
                            return c;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            })
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn installed_version() -> SemVer {
    SemVer::parse(env!("CARGO_PKG_VERSION")).unwrap_or(SemVer {
        major: 1,
        minor: 7,
        patch: 0,
        pre: vec![PreId::AlphaNum("unknown".into())],
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformTarget {
    pub os: &'static str,
    pub arch: &'static str,
}

impl PlatformTarget {
    pub fn current() -> Self {
        Self {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
        }
    }

    pub fn display_label(self) -> String {
        format!("{}-{}", self.os, self.arch)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub draft: bool,
    pub prerelease: bool,
    pub published_at: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone)]
pub struct ReleaseCandidate {
    pub version: SemVer,
    pub tag_name: String,
    pub title: String,
    pub prerelease: bool,
    pub published_at: String,
    pub html_url: String,
    pub release_notes: String,
    pub primary_asset: Option<GitHubAsset>,
    pub checksum_asset: Option<GitHubAsset>,
    pub signed_manifest_asset: Option<GitHubAsset>,
    pub signature_asset: Option<GitHubAsset>,
}

#[derive(Debug, Clone)]
pub struct UpdateCheckReport {
    pub installed: SemVer,
    pub channel: UpdateChannel,
    pub platform: PlatformTarget,
    pub checked_at: String,
    pub candidate: Option<ReleaseCandidate>,
    pub is_update_available: bool,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProvenanceStatus {
    /// Cryptographic secp256k1 signature verified against pinned release key.
    Verified {
        signer_pubkey_prefix: String,
        manifest_sha256: String,
    },
    /// No pinned release-signing public key is configured in this build yet.
    Unconfigured,
    /// Pinned key exists, but the release does not publish a signed manifest.
    Unsigned,
    /// Cryptographic signature or manifest content verification failed.
    Invalid(String),
}

impl ProvenanceStatus {
    pub fn is_cryptographically_verified(&self) -> bool {
        matches!(self, Self::Verified { .. })
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Verified { .. } => "SIGNED & VERIFIED",
            Self::Unconfigured => "SIGNING KEY UNCONFIGURED (AUTO-INSTALL LOCKED)",
            Self::Unsigned => "UNSIGNED RELEASE (AUTO-INSTALL LOCKED)",
            Self::Invalid(_) => "SIGNATURE INVALID (BLOCKED)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedReleaseManifest {
    pub schema_version: u32,
    pub version: String,
    pub tag: String,
    pub asset_name: String,
    pub asset_size: u64,
    pub asset_sha256: String,
    pub published_at: String,
}

#[derive(Debug, Clone)]
pub struct MaintenanceGate {
    pub safe_to_install: bool,
    pub blockers: Vec<String>,
}

impl MaintenanceGate {
    /// Evaluates whether applying/restarting an update is strictly safe right now.
    /// Never allows interrupting live/guardian trading, open positions, or unresolved intents.
    pub fn evaluate(mode: OperationalMode, snapshot: Option<&TelemetrySnapshot>) -> Self {
        let mut blockers = Vec::new();
        if matches!(mode, OperationalMode::Awakened | OperationalMode::Guardian) {
            blockers.push(format!(
                "Operational mode is {} — switch to DORMANT or SUSPENDED before updating.",
                mode.label()
            ));
        }
        if let Some(snap) = snapshot {
            if !snap.positions.is_empty() {
                blockers.push(format!(
                    "{} open position row(s) recorded — close or reconcile positions before applying an update.",
                    snap.positions.len()
                ));
            }
            let unresolved = snap
                .intents
                .iter()
                .filter(|i| !matches!(i.state.as_str(), "applied" | "rejected" | "cancelled"))
                .count();
            if unresolved > 0 {
                blockers.push(format!(
                    "{unresolved} unresolved execution intent(s) in journal — never interrupt pending execution."
                ));
            }
        }
        Self {
            safe_to_install: blockers.is_empty(),
            blockers,
        }
    }
}

#[derive(Debug, Default)]
pub struct DownloadProgress {
    pub downloaded_bytes: AtomicU64,
    pub total_bytes: AtomicU64,
    pub cancelled: AtomicBool,
}

impl DownloadProgress {
    pub fn cancel(&self) {
        self.cancelled.store(true, AtomicOrdering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(AtomicOrdering::SeqCst)
    }

    pub fn fraction(&self) -> f32 {
        let total = self.total_bytes.load(AtomicOrdering::Relaxed);
        let done = self.downloaded_bytes.load(AtomicOrdering::Relaxed);
        if total == 0 {
            0.0
        } else {
            (done as f64 / total as f64).clamp(0.0, 1.0) as f32
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedUpdate {
    pub version: String,
    pub tag_name: String,
    pub asset_name: String,
    pub staged_path: PathBuf,
    pub sha256: String,
    pub expected_sha256: String,
    pub sha256_verified: bool,
    pub size_bytes: u64,
    pub provenance: ProvenanceStatus,
    pub staged_at: String,
}

impl StagedUpdate {
    /// Automatic in-place installation is strictly permitted ONLY when both
    /// SHA-256 integrity AND cryptographic signed provenance succeeded.
    pub fn auto_install_allowed(&self) -> bool {
        self.sha256_verified && self.provenance.is_cryptographically_verified()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRecord {
    pub created_at: String,
    pub installed_version: String,
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticEntry {
    pub timestamp: String,
    pub stage: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateDiagnostics {
    pub entries: Vec<DiagnosticEntry>,
    pub backups: Vec<BackupRecord>,
}

/// Resolves `%LOCALAPPDATA%\Hydra\updates` (or XDG equivalent), completely separate
/// from `%LOCALAPPDATA%\Hydra\data\` and Treasury credential files.
pub fn updates_root_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            if !local.trim().is_empty() {
                return PathBuf::from(local).join("Hydra").join("updates");
            }
        }
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("hydra").join("updates");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("hydra")
                .join("updates");
        }
    }
    PathBuf::from("hydra-updates")
}

pub fn load_diagnostics(root: &Path) -> UpdateDiagnostics {
    let path = root.join("diagnostics.json");
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn record_diagnostic(root: &Path, stage: &str, ok: bool, message: impl Into<String>) {
    let _ = fs::create_dir_all(root);
    let mut diag = load_diagnostics(root);
    diag.entries.push(DiagnosticEntry {
        timestamp: Utc::now().to_rfc3339(),
        stage: stage.to_string(),
        ok,
        message: message.into(),
    });
    if diag.entries.len() > 64 {
        let excess = diag.entries.len() - 64;
        diag.entries.drain(0..excess);
    }
    if let Ok(json) = serde_json::to_string_pretty(&diag) {
        let _ = fs::write(root.join("diagnostics.json"), json);
    }
}

/// Validates a filename or archive entry path strictly against traversal attacks.
pub fn validate_safe_filename(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 180 {
        bail!("Invalid asset filename length");
    }
    if name.contains('\0')
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains(':')
    {
        bail!("Rejected unsafe asset filename '{name}'");
    }
    let path = Path::new(name);
    let mut count = 0usize;
    for comp in path.components() {
        match comp {
            Component::Normal(_) => count += 1,
            _ => bail!("Rejected non-normal path component in '{name}'"),
        }
    }
    if count != 1 {
        bail!("Asset filename must be a single path segment");
    }
    Ok(())
}

/// Validates that a download URL points only to canonical GitHub release hosts
/// (or loopback when `allow_loopback` is true for offline testing).
pub fn validate_download_url(url: &str, allow_loopback: bool) -> Result<()> {
    let parsed = reqwest::Url::parse(url).with_context(|| format!("Invalid URL '{url}'"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("URL missing host"))?
        .to_ascii_lowercase();
    let scheme = parsed.scheme();
    if allow_loopback && (host == "127.0.0.1" || host == "localhost") && scheme == "http" {
        return Ok(());
    }
    if scheme != "https" {
        bail!("Refusing non-HTTPS release download URL '{url}'");
    }
    let allowed = matches!(
        host.as_str(),
        "github.com"
            | "api.github.com"
            | "objects.githubusercontent.com"
            | "release-assets.githubusercontent.com"
            | "github-releases.githubusercontent.com"
    );
    if !allowed {
        bail!("Refusing untrusted download host '{host}'");
    }
    Ok(())
}

/// Selects the right platform/architecture primary asset and integrity files from a release.
pub fn select_release_assets(
    release: &GitHubRelease,
    platform: PlatformTarget,
) -> (
    Option<GitHubAsset>,
    Option<GitHubAsset>,
    Option<GitHubAsset>,
    Option<GitHubAsset>,
) {
    let primary = match (platform.os, platform.arch) {
        ("windows", "x86_64") => release
            .assets
            .iter()
            .find(|a| {
                a.name
                    .eq_ignore_ascii_case("HYDRA-Leviathan-Windows-x64.exe")
            })
            .or_else(|| {
                release
                    .assets
                    .iter()
                    .find(|a| a.name.ends_with("x86_64-pc-windows-msvc.zip"))
            })
            .cloned(),
        ("linux", "x86_64") => release
            .assets
            .iter()
            .find(|a| a.name.ends_with("x86_64-unknown-linux-gnu.tar.gz"))
            .cloned(),
        _ => None,
    };

    let checksum = release
        .assets
        .iter()
        .find(|a| a.name.ends_with("-SHA256SUMS.txt") && !a.name.contains("-linux-"))
        .or_else(|| {
            release
                .assets
                .iter()
                .find(|a| a.name.ends_with("SHA256SUMS.txt"))
        })
        .cloned();

    let signed_manifest = release
        .assets
        .iter()
        .find(|a| a.name.ends_with("-signed-manifest.json"))
        .cloned();

    let signature = release
        .assets
        .iter()
        .find(|a| a.name.ends_with("-signed-manifest.sig"))
        .cloned();

    (primary, checksum, signed_manifest, signature)
}

/// Filters GitHub releases by channel and selects the highest SemVer eligible candidate.
pub fn select_newest_release(
    releases: &[GitHubRelease],
    channel: UpdateChannel,
    platform: PlatformTarget,
) -> Option<ReleaseCandidate> {
    let mut best: Option<ReleaseCandidate> = None;
    for rel in releases {
        if rel.draft {
            continue;
        }
        let Ok(ver) = SemVer::parse(&rel.tag_name) else {
            continue;
        };
        let is_pre = rel.prerelease || ver.is_prerelease();
        if channel == UpdateChannel::Stable && is_pre {
            continue;
        }
        let (primary_asset, checksum_asset, signed_manifest_asset, signature_asset) =
            select_release_assets(rel, platform);
        let candidate = ReleaseCandidate {
            version: ver,
            tag_name: rel.tag_name.clone(),
            title: rel
                .name
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("HYDRA {}", rel.tag_name)),
            prerelease: is_pre,
            published_at: rel.published_at.clone().unwrap_or_else(|| "unknown".into()),
            html_url: rel.html_url.clone(),
            release_notes: rel
                .body
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "No release notes supplied.".into()),
            primary_asset,
            checksum_asset,
            signed_manifest_asset,
            signature_asset,
        };
        let replace = match &best {
            None => true,
            Some(current) => candidate.version > current.version,
        };
        if replace {
            best = Some(candidate);
        }
    }
    best
}

/// Fetches releases from GitHub Releases API (or loopback test server) and evaluates updates.
pub fn check_for_updates(
    api_url: &str,
    channel: UpdateChannel,
    installed: SemVer,
    platform: PlatformTarget,
) -> Result<UpdateCheckReport> {
    let allow_loopback =
        api_url.starts_with("http://127.0.0.1") || api_url.starts_with("http://localhost");
    validate_download_url(api_url, allow_loopback)?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build evolution tokio runtime")?;

    let releases: Vec<GitHubRelease> = rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(format!("HYDRA-Leviathan-Evolution/{installed}"))
            .build()
            .context("build HTTP client")?;
        let resp = client
            .get(api_url)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .context("query GitHub Releases API")?;
        let status = resp.status();
        if !status.is_success() {
            bail!("GitHub Releases API returned HTTP {status}");
        }
        let bytes = resp.bytes().await.context("read release API payload")?;
        if bytes.len() > 4 * 1024 * 1024 {
            bail!("GitHub Releases API response exceeded 4 MiB bound");
        }
        serde_json::from_slice(&bytes).context("decode GitHub Releases JSON")
    })?;

    let candidate = select_newest_release(&releases, channel, platform);
    let is_update_available = candidate
        .as_ref()
        .map(|c| c.version > installed)
        .unwrap_or(false);

    let summary = match &candidate {
        None if channel == UpdateChannel::Stable => {
            "No stable release published yet. Switch to Development channel for alpha/beta updates."
                .to_string()
        }
        None => "No eligible releases found on the selected channel.".to_string(),
        Some(c) if c.version > installed => {
            format!(
                "Update available: v{} → {} ({})",
                installed,
                c.tag_name,
                if c.prerelease { "prerelease" } else { "stable" }
            )
        }
        Some(c) => format!(
            "Up to date (installed v{}, newest {} on {} channel).",
            installed,
            c.tag_name,
            channel.label()
        ),
    };

    Ok(UpdateCheckReport {
        installed,
        channel,
        platform,
        checked_at: Utc::now().to_rfc3339(),
        candidate,
        is_update_available,
        summary,
    })
}

/// Parses a standard `SHA256SUMS.txt` manifest and extracts the expected lowercase hex
/// digest for `asset_name`.
pub fn parse_sha256_manifest(manifest_text: &str, asset_name: &str) -> Result<String> {
    validate_safe_filename(asset_name)?;
    for line in manifest_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(hash) = parts.next() else {
            continue;
        };
        let Some(file) = parts.next() else {
            continue;
        };
        let clean_file = file.trim_start_matches('*');
        if clean_file == asset_name {
            if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
                bail!("Malformed SHA-256 digest for '{asset_name}' in manifest");
            }
            return Ok(hash.to_ascii_lowercase());
        }
    }
    bail!("Asset '{asset_name}' not found in SHA-256 checksum manifest");
}

/// Verifies a detached `secp256k1` ECDSA signature over `manifest_bytes` against a pinned
/// compressed public key, and checks that the manifest matches the downloaded package.
pub fn verify_signed_manifest(
    manifest_bytes: &[u8],
    signature_hex: &str,
    pinned_pubkey_hex: Option<&str>,
    expected_version: &str,
    expected_asset_name: &str,
    expected_size: u64,
    computed_asset_sha256: &str,
) -> ProvenanceStatus {
    let Some(pubkey_hex) = pinned_pubkey_hex else {
        return ProvenanceStatus::Unconfigured;
    };

    let Ok(pubkey_bytes) = hex::decode(pubkey_hex.trim()) else {
        return ProvenanceStatus::Invalid(
            "Pinned release-signing public key is invalid hex".into(),
        );
    };
    let Ok(pubkey) = secp256k1::PublicKey::from_slice(&pubkey_bytes) else {
        return ProvenanceStatus::Invalid(
            "Pinned release-signing public key is invalid secp256k1".into(),
        );
    };
    let Ok(sig_bytes) = hex::decode(signature_hex.trim()) else {
        return ProvenanceStatus::Invalid("Release signature is invalid hex".into());
    };
    let sig = secp256k1::ecdsa::Signature::from_compact(&sig_bytes)
        .or_else(|_| secp256k1::ecdsa::Signature::from_der(&sig_bytes));
    let Ok(sig) = sig else {
        return ProvenanceStatus::Invalid("Release signature failed ECDSA decoding".into());
    };

    let manifest_digest = sha256_raw(manifest_bytes);
    let msg = secp256k1::Message::from_digest(manifest_digest);
    if secp256k1::ecdsa::verify(&sig, msg, &pubkey).is_err() {
        return ProvenanceStatus::Invalid("ECDSA signature verification failed".into());
    }

    let manifest: SignedReleaseManifest = match serde_json::from_slice(manifest_bytes) {
        Ok(m) => m,
        Err(e) => return ProvenanceStatus::Invalid(format!("Malformed signed manifest JSON: {e}")),
    };
    if manifest.schema_version != 1 {
        return ProvenanceStatus::Invalid(format!(
            "Unsupported signed manifest schema version {}",
            manifest.schema_version
        ));
    }
    if manifest.version != expected_version {
        return ProvenanceStatus::Invalid(format!(
            "Signed manifest version '{}' does not match '{}'",
            manifest.version, expected_version
        ));
    }
    if manifest.asset_name != expected_asset_name {
        return ProvenanceStatus::Invalid(format!(
            "Signed manifest asset '{}' does not match '{}'",
            manifest.asset_name, expected_asset_name
        ));
    }
    if manifest.asset_size != expected_size {
        return ProvenanceStatus::Invalid(format!(
            "Signed manifest size {} does not match downloaded size {}",
            manifest.asset_size, expected_size
        ));
    }
    if !manifest
        .asset_sha256
        .eq_ignore_ascii_case(computed_asset_sha256)
    {
        return ProvenanceStatus::Invalid("Signed manifest SHA-256 mismatch".into());
    }

    let prefix: String = pubkey_hex.trim().chars().take(16).collect();
    ProvenanceStatus::Verified {
        signer_pubkey_prefix: format!("{prefix}…"),
        manifest_sha256: hex::encode(manifest_digest),
    }
}

/// Downloads the candidate primary asset + SHA-256 manifest (+ optional signed manifest),
/// verifies integrity byte-for-byte, and stages safely into `<updates_root>/staging/<version>/`.
pub fn download_and_stage_release(
    candidate: &ReleaseCandidate,
    updates_root: &Path,
    pinned_pubkey_hex: Option<&str>,
    progress: Arc<DownloadProgress>,
    allow_loopback: bool,
) -> Result<StagedUpdate> {
    let asset = candidate.primary_asset.as_ref().ok_or_else(|| {
        anyhow!(
            "Release {} has no compatible package for this platform",
            candidate.tag_name
        )
    })?;
    let checksum_asset = candidate.checksum_asset.as_ref().ok_or_else(|| {
        anyhow!(
            "Release {} has no SHA-256 checksum manifest — refusing unverified download",
            candidate.tag_name
        )
    })?;

    validate_safe_filename(&asset.name)?;
    validate_safe_filename(&checksum_asset.name)?;
    validate_download_url(&asset.browser_download_url, allow_loopback)?;
    validate_download_url(&checksum_asset.browser_download_url, allow_loopback)?;
    if asset.size > MAX_PACKAGE_BYTES {
        bail!(
            "Release asset '{}' ({} bytes) exceeds 128 MiB safety cap",
            asset.name,
            asset.size
        );
    }

    let version_str = candidate.version.to_string();
    validate_safe_filename(&version_str)?;
    let stage_dir = updates_root.join("staging").join(&version_str);
    fs::create_dir_all(&stage_dir).context("create staging directory")?;
    let partial_path = stage_dir.join(format!("{}.part", asset.name));
    let final_path = stage_dir.join(&asset.name);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build download runtime")?;

    let (expected_sha, signed_pair) = rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent(format!("HYDRA-Leviathan-Evolution/{}", installed_version()))
            .build()
            .context("build download HTTP client")?;

        // 1. Fetch checksum manifest first.
        let sum_resp = client
            .get(&checksum_asset.browser_download_url)
            .send()
            .await
            .context("download SHA-256 manifest")?;
        if !sum_resp.status().is_success() {
            bail!(
                "Checksum manifest download failed: HTTP {}",
                sum_resp.status()
            );
        }
        let sum_bytes = sum_resp.bytes().await.context("read checksum manifest")?;
        if sum_bytes.len() > MAX_MANIFEST_BYTES {
            bail!("Checksum manifest exceeded 256 KiB cap");
        }
        let sum_text = String::from_utf8(sum_bytes.to_vec()).context("checksum manifest UTF-8")?;
        let expected_sha = parse_sha256_manifest(&sum_text, &asset.name)?;

        // 2. Stream primary package with progress + cancellation.
        let mut resp = client
            .get(&asset.browser_download_url)
            .send()
            .await
            .context("download release package")?;
        if !resp.status().is_success() {
            bail!("Release package download failed: HTTP {}", resp.status());
        }
        let total = resp.content_length().unwrap_or(asset.size);
        if total > MAX_PACKAGE_BYTES {
            bail!("Download Content-Length ({total} bytes) exceeds 128 MiB cap");
        }
        progress.total_bytes.store(total, AtomicOrdering::Relaxed);
        progress.downloaded_bytes.store(0, AtomicOrdering::Relaxed);

        let mut file = fs::File::create(&partial_path).context("create partial download file")?;
        let mut downloaded: u64 = 0;
        while let Some(chunk) = resp.chunk().await.context("read download chunk")? {
            if progress.is_cancelled() {
                drop(file);
                let _ = fs::remove_file(&partial_path);
                bail!("Download cancelled by operator");
            }
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            if downloaded > MAX_PACKAGE_BYTES {
                drop(file);
                let _ = fs::remove_file(&partial_path);
                bail!("Downloaded bytes exceeded 128 MiB safety cap");
            }
            file.write_all(&chunk).context("write download chunk")?;
            progress
                .downloaded_bytes
                .store(downloaded, AtomicOrdering::Relaxed);
        }
        file.flush().context("flush partial download")?;

        // 3. Optionally fetch signed manifest + signature if published.
        let mut signed_pair: Option<(Vec<u8>, String)> = None;
        if let (Some(m_asset), Some(s_asset)) =
            (&candidate.signed_manifest_asset, &candidate.signature_asset)
        {
            if validate_download_url(&m_asset.browser_download_url, allow_loopback).is_ok()
                && validate_download_url(&s_asset.browser_download_url, allow_loopback).is_ok()
            {
                if let (Ok(mr), Ok(sr)) = (
                    client.get(&m_asset.browser_download_url).send().await,
                    client.get(&s_asset.browser_download_url).send().await,
                ) {
                    if mr.status().is_success() && sr.status().is_success() {
                        if let (Ok(mb), Ok(sb)) = (mr.bytes().await, sr.bytes().await) {
                            if mb.len() <= MAX_MANIFEST_BYTES && sb.len() <= 4096 {
                                if let Ok(sig_txt) = String::from_utf8(sb.to_vec()) {
                                    signed_pair = Some((mb.to_vec(), sig_txt));
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok::<(String, Option<(Vec<u8>, String)>), anyhow::Error>((expected_sha, signed_pair))
    })?;

    // Verify SHA-256 of the downloaded file before promoting to final staged path.
    let computed_sha = sha256_file(&partial_path)?;
    let actual_size = fs::metadata(&partial_path)?.len();
    if computed_sha != expected_sha {
        let _ = fs::remove_file(&partial_path);
        record_diagnostic(
            updates_root,
            "verify_sha256",
            false,
            format!(
                "SHA-256 mismatch for {}: expected {}, got {}",
                asset.name, expected_sha, computed_sha
            ),
        );
        bail!(
            "SHA-256 verification failed for {}: expected {}, computed {}",
            asset.name,
            expected_sha,
            computed_sha
        );
    }

    if final_path.exists() {
        let _ = fs::remove_file(&final_path);
    }
    fs::rename(&partial_path, &final_path).context("promote verified file into staging")?;

    // Evaluate cryptographic signed provenance.
    let provenance = match (pinned_pubkey_hex, signed_pair) {
        (None, _) => ProvenanceStatus::Unconfigured,
        (Some(_), None) => ProvenanceStatus::Unsigned,
        (Some(pk), Some((m_bytes, sig_hex))) => verify_signed_manifest(
            &m_bytes,
            &sig_hex,
            Some(pk),
            &version_str,
            &asset.name,
            actual_size,
            &computed_sha,
        ),
    };

    let staged = StagedUpdate {
        version: version_str,
        tag_name: candidate.tag_name.clone(),
        asset_name: asset.name.clone(),
        staged_path: final_path,
        sha256: computed_sha.clone(),
        expected_sha256: expected_sha,
        sha256_verified: true,
        size_bytes: actual_size,
        provenance,
        staged_at: Utc::now().to_rfc3339(),
    };

    if let Ok(meta) = serde_json::to_string_pretty(&staged) {
        let _ = fs::write(stage_dir.join("staged-meta.json"), meta);
    }
    record_diagnostic(
        updates_root,
        "stage_package",
        true,
        format!(
            "Staged {} ({} bytes, sha256={}…, provenance={})",
            staged.asset_name,
            staged.size_bytes,
            &staged.sha256[..16],
            staged.provenance.label()
        ),
    );
    Ok(staged)
}

/// Creates a verified backup of the target executable before replacement.
pub fn backup_executable(
    current_exe: &Path,
    installed_ver: &str,
    updates_root: &Path,
) -> Result<BackupRecord> {
    if !current_exe.exists() {
        bail!(
            "Current executable '{}' does not exist",
            current_exe.display()
        );
    }
    let file_name = current_exe
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Invalid executable path"))?;
    validate_safe_filename(file_name)?;
    let ts = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let backup_dir = updates_root
        .join("backups")
        .join(format!("{ts}-{installed_ver}"));
    fs::create_dir_all(&backup_dir).context("create backup directory")?;
    let backup_path = backup_dir.join(file_name);
    fs::copy(current_exe, &backup_path).context("copy executable to backup")?;
    let orig_sha = sha256_file(current_exe)?;
    let back_sha = sha256_file(&backup_path)?;
    if orig_sha != back_sha {
        let _ = fs::remove_file(&backup_path);
        bail!("Backup SHA-256 verification failed");
    }

    let record = BackupRecord {
        created_at: Utc::now().to_rfc3339(),
        installed_version: installed_ver.to_string(),
        original_path: current_exe.to_path_buf(),
        backup_path,
        sha256: back_sha,
    };

    let mut diag = load_diagnostics(updates_root);
    diag.backups.push(record.clone());
    if diag.backups.len() > 8 {
        let excess = diag.backups.len() - 8;
        diag.backups.drain(0..excess);
    }
    diag.entries.push(DiagnosticEntry {
        timestamp: Utc::now().to_rfc3339(),
        stage: "backup".into(),
        ok: true,
        message: format!("Backed up v{} ({}…)", installed_ver, &record.sha256[..16]),
    });
    let _ = fs::create_dir_all(updates_root);
    if let Ok(json) = serde_json::to_string_pretty(&diag) {
        let _ = fs::write(updates_root.join("diagnostics.json"), json);
    }
    Ok(record)
}

/// Restores a verified backup onto `destination`, checking SHA-256 integrity.
pub fn rollback_from_backup(
    backup: &BackupRecord,
    destination: &Path,
    updates_root: &Path,
) -> Result<()> {
    if !backup.backup_path.exists() {
        bail!(
            "Backup file '{}' no longer exists",
            backup.backup_path.display()
        );
    }
    let current_backup_sha = sha256_file(&backup.backup_path)?;
    if current_backup_sha != backup.sha256 {
        bail!("Backup file integrity check failed; refusing corrupt rollback");
    }
    let tmp = destination.with_extension("rollback.tmp");
    fs::copy(&backup.backup_path, &tmp).context("copy backup to temp destination")?;
    let tmp_sha = sha256_file(&tmp)?;
    if tmp_sha != backup.sha256 {
        let _ = fs::remove_file(&tmp);
        bail!("Copied rollback binary failed SHA-256 verification");
    }
    fs::rename(&tmp, destination).context("atomically restore backup executable")?;
    record_diagnostic(
        updates_root,
        "rollback",
        true,
        format!(
            "Restored v{} backup to {}",
            backup.installed_version,
            destination.display()
        ),
    );
    Ok(())
}

/// Validates all gates and prepares (but does NOT bypass signing or maintenance gates)
/// an atomic replacement plan for a staged, cryptographically verified executable.
pub fn prepare_verified_restart_script(
    staged: &StagedUpdate,
    gate: &MaintenanceGate,
    current_exe: &Path,
    updates_root: &Path,
) -> Result<PathBuf> {
    if !gate.safe_to_install {
        bail!(
            "Maintenance gate blocked update: {}",
            gate.blockers.join(" | ")
        );
    }
    if !staged.auto_install_allowed() {
        bail!(
            "Automatic installation is locked: provenance is {}",
            staged.provenance.label()
        );
    }
    if !staged.staged_path.exists() {
        bail!("Staged package missing at {}", staged.staged_path.display());
    }
    let recheck_sha = sha256_file(&staged.staged_path)?;
    if recheck_sha != staged.sha256 {
        bail!("Staged package SHA-256 changed since staging; aborting installation");
    }

    let backup = backup_executable(current_exe, &installed_version().to_string(), updates_root)?;
    let script_path = updates_root.join("apply-verified-update.ps1");
    let pid = std::process::id();
    let script = format!(
        "$ErrorActionPreference = 'Stop'\r\n\
         $targetPid = {pid}\r\n\
         for ($i = 0; $i -lt 60; $i++) {{\r\n\
           if (-not (Get-Process -Id $targetPid -ErrorAction SilentlyContinue)) {{ break }}\r\n\
           Start-Sleep -Milliseconds 500\r\n\
         }}\r\n\
         $staged = '{staged}'\r\n\
         $dest = '{dest}'\r\n\
         $backup = '{backup}'\r\n\
         try {{\r\n\
           Copy-Item -Path $staged -Destination $dest -Force\r\n\
           Start-Process -FilePath $dest\r\n\
         }} catch {{\r\n\
           if (Test-Path $backup) {{ Copy-Item -Path $backup -Destination $dest -Force }}\r\n\
           throw\r\n\
         }}\r\n",
        staged = staged.staged_path.display().to_string().replace('\'', "''"),
        dest = current_exe.display().to_string().replace('\'', "''"),
        backup = backup.backup_path.display().to_string().replace('\'', "''"),
    );
    fs::write(&script_path, script).context("write safe restart updater script")?;
    record_diagnostic(
        updates_root,
        "prepare_restart",
        true,
        format!(
            "Prepared verified restart handoff script at {}",
            script_path.display()
        ),
    );
    Ok(script_path)
}

// ============================================================================
// Pure-Rust FIPS 180-4 SHA-256 (zero external crate additions)
// ============================================================================

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

struct Sha256State {
    h: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total_bytes: u64,
}

impl Sha256State {
    fn new() -> Self {
        Self {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buf: [0u8; 64],
            buf_len: 0,
            total_bytes: 0,
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[4 * i],
                block[4 * i + 1],
                block[4 * i + 2],
                block[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(h);
    }

    fn update(&mut self, mut data: &[u8]) {
        self.total_bytes = self.total_bytes.saturating_add(data.len() as u64);
        if self.buf_len > 0 {
            let need = 64 - self.buf_len;
            let take = need.min(data.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            data = &data[take..];
            if self.buf_len == 64 {
                let block = self.buf;
                self.compress(&block);
                self.buf_len = 0;
            }
        }
        while data.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[..64]);
            self.compress(&block);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_bytes.wrapping_mul(8);
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;
        if self.buf_len > 56 {
            for b in &mut self.buf[self.buf_len..64] {
                *b = 0;
            }
            let block = self.buf;
            self.compress(&block);
            self.buf = [0u8; 64];
            self.buf_len = 0;
        }
        for b in &mut self.buf[self.buf_len..56] {
            *b = 0;
        }
        self.buf[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buf;
        self.compress(&block);

        let mut out = [0u8; 32];
        for (i, word) in self.h.iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

pub fn sha256_raw(data: &[u8]) -> [u8; 32] {
    let mut s = Sha256State::new();
    s.update(data);
    s.finalize()
}

pub fn sha256_bytes(data: &[u8]) -> String {
    hex::encode(sha256_raw(data))
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("open '{}' for SHA-256", path.display()))?;
    let mut state = Sha256State::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        state.update(&buf[..n]);
    }
    Ok(hex::encode(state.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn sha256_matches_nist_fips_180_4_vectors() {
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_bytes(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn semver_ordering_handles_prereleases_and_numeric_segments() {
        let a9 = SemVer::parse("v1.7.0-alpha.9").unwrap();
        let a27 = SemVer::parse("v1.7.0-alpha.27").unwrap();
        let a28 = SemVer::parse("1.7.0-alpha.28").unwrap();
        let beta1 = SemVer::parse("v1.7.0-beta.1").unwrap();
        let stable = SemVer::parse("v1.7.0").unwrap();
        let next = SemVer::parse("v1.8.0-alpha.1").unwrap();

        assert!(a9 < a27);
        assert!(a27 < a28);
        assert!(a28 < beta1);
        assert!(beta1 < stable);
        assert!(stable < next);
        assert!(a28.is_prerelease());
        assert!(!stable.is_prerelease());
    }

    #[test]
    fn channel_selection_excludes_prereleases_on_stable_and_skips_drafts() {
        let releases = vec![
            GitHubRelease {
                tag_name: "v1.7.0-alpha.29".into(),
                name: Some("Draft".into()),
                body: None,
                draft: true,
                prerelease: true,
                published_at: None,
                html_url: "https://github.com/ShamelesAbyss/Hydra/releases/tag/v1.7.0-alpha.29"
                    .into(),
                assets: vec![],
            },
            GitHubRelease {
                tag_name: "v1.7.0-alpha.28".into(),
                name: Some("Alpha 28".into()),
                body: Some("Notes".into()),
                draft: false,
                prerelease: true,
                published_at: Some("2026-11-27T00:00:00Z".into()),
                html_url: "https://github.com/ShamelesAbyss/Hydra/releases/tag/v1.7.0-alpha.28"
                    .into(),
                assets: vec![
                    GitHubAsset {
                        name: "HYDRA-Leviathan-Windows-x64.exe".into(),
                        size: 22_000_000,
                        browser_download_url: "https://github.com/ShamelesAbyss/Hydra/releases/download/v1.7.0-alpha.28/HYDRA-Leviathan-Windows-x64.exe".into(),
                    },
                    GitHubAsset {
                        name: "hydra-1.7.0-alpha.28-SHA256SUMS.txt".into(),
                        size: 256,
                        browser_download_url: "https://github.com/ShamelesAbyss/Hydra/releases/download/v1.7.0-alpha.28/hydra-1.7.0-alpha.28-SHA256SUMS.txt".into(),
                    },
                ],
            },
        ];
        let win = PlatformTarget {
            os: "windows",
            arch: "x86_64",
        };
        assert!(select_newest_release(&releases, UpdateChannel::Stable, win).is_none());
        let dev = select_newest_release(&releases, UpdateChannel::Development, win).unwrap();
        assert_eq!(dev.tag_name, "v1.7.0-alpha.28");
        assert_eq!(
            dev.primary_asset.unwrap().name,
            "HYDRA-Leviathan-Windows-x64.exe"
        );
    }

    #[test]
    fn signed_manifest_verification_checks_secp256k1_signature_and_rejects_tampering() {
        let sk = secp256k1::SecretKey::from_secret_bytes([0x42u8; 32]).unwrap();
        let pk = secp256k1::PublicKey::from_secret_key(&sk);
        let pk_hex = hex::encode(pk.serialize());

        let payload = b"HYDRA_SIGNED_BINARY_FIXTURE";
        let payload_sha = sha256_bytes(payload);
        let manifest = SignedReleaseManifest {
            schema_version: 1,
            version: "1.7.0-alpha.28".into(),
            tag: "v1.7.0-alpha.28".into(),
            asset_name: "HYDRA-Leviathan-Windows-x64.exe".into(),
            asset_size: payload.len() as u64,
            asset_sha256: payload_sha.clone(),
            published_at: "2026-11-27T00:00:00Z".into(),
        };
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let msg = secp256k1::Message::from_digest(sha256_raw(&manifest_bytes));
        let sig = secp256k1::ecdsa::sign(msg, &sk);
        let sig_hex = hex::encode(sig.serialize_compact());

        // Without pinned key -> Unconfigured (auto-install locked).
        assert_eq!(
            verify_signed_manifest(
                &manifest_bytes,
                &sig_hex,
                None,
                "1.7.0-alpha.28",
                "HYDRA-Leviathan-Windows-x64.exe",
                payload.len() as u64,
                &payload_sha,
            ),
            ProvenanceStatus::Unconfigured
        );

        // With pinned key -> Verified.
        let ok = verify_signed_manifest(
            &manifest_bytes,
            &sig_hex,
            Some(&pk_hex),
            "1.7.0-alpha.28",
            "HYDRA-Leviathan-Windows-x64.exe",
            payload.len() as u64,
            &payload_sha,
        );
        assert!(ok.is_cryptographically_verified());

        // Tampered payload hash -> Invalid.
        let bad = verify_signed_manifest(
            &manifest_bytes,
            &sig_hex,
            Some(&pk_hex),
            "1.7.0-alpha.28",
            "HYDRA-Leviathan-Windows-x64.exe",
            payload.len() as u64,
            &sha256_bytes(b"TAMPERED"),
        );
        assert!(matches!(bad, ProvenanceStatus::Invalid(_)));
    }

    #[test]
    fn maintenance_gate_blocks_active_modes_and_unsafe_paths_are_rejected() {
        assert!(MaintenanceGate::evaluate(OperationalMode::Dormant, None).safe_to_install);
        assert!(MaintenanceGate::evaluate(OperationalMode::Suspended, None).safe_to_install);
        assert!(!MaintenanceGate::evaluate(OperationalMode::Awakened, None).safe_to_install);
        assert!(!MaintenanceGate::evaluate(OperationalMode::Guardian, None).safe_to_install);

        assert!(validate_safe_filename("HYDRA-Leviathan-Windows-x64.exe").is_ok());
        assert!(validate_safe_filename("../evil.exe").is_err());
        assert!(validate_safe_filename("sub/evil.exe").is_err());
        assert!(validate_safe_filename("C:\\evil.exe").is_err());
        assert!(validate_download_url("https://evil.example.com/a.exe", false).is_err());
        assert!(validate_download_url(
            "https://github.com/ShamelesAbyss/Hydra/releases/download/v1.7.0-alpha.28/HYDRA-Leviathan-Windows-x64.exe",
            false
        )
        .is_ok());
    }

    #[test]
    fn loopback_download_staging_backup_and_rollback_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let updates_root = dir.path().join("updates");

        let payload = b"HYDRA-LOOPBACK-RELEASE-BINARY-V28";
        let payload_sha = sha256_bytes(payload);
        let sums = format!("{payload_sha}  HYDRA-Leviathan-Windows-x64.exe\n");

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let sums_clone = sums.clone();
        let payload_vec = payload.to_vec();

        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut req_buf = [0u8; 2048];
                let n = stream.read(&mut req_buf).unwrap();
                let req = String::from_utf8_lossy(&req_buf[..n]);
                let (body, ctype) = if req.contains("SHA256SUMS.txt") {
                    (sums_clone.as_bytes().to_vec(), "text/plain")
                } else {
                    (payload_vec.clone(), "application/octet-stream")
                };
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(header.as_bytes()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });

        let candidate = ReleaseCandidate {
            version: SemVer::parse("1.7.0-alpha.28").unwrap(),
            tag_name: "v1.7.0-alpha.28".into(),
            title: "Alpha 28".into(),
            prerelease: true,
            published_at: "2026-11-27T00:00:00Z".into(),
            html_url: "https://github.com/ShamelesAbyss/Hydra/releases/tag/v1.7.0-alpha.28".into(),
            release_notes: "Test release".into(),
            primary_asset: Some(GitHubAsset {
                name: "HYDRA-Leviathan-Windows-x64.exe".into(),
                size: payload.len() as u64,
                browser_download_url: format!("http://{addr}/HYDRA-Leviathan-Windows-x64.exe"),
            }),
            checksum_asset: Some(GitHubAsset {
                name: "hydra-1.7.0-alpha.28-SHA256SUMS.txt".into(),
                size: sums.len() as u64,
                browser_download_url: format!("http://{addr}/hydra-1.7.0-alpha.28-SHA256SUMS.txt"),
            }),
            signed_manifest_asset: None,
            signature_asset: None,
        };

        let progress = Arc::new(DownloadProgress::default());
        let staged = download_and_stage_release(&candidate, &updates_root, None, progress, true)
            .expect("stage loopback release");
        server.join().unwrap();

        assert!(staged.sha256_verified);
        assert_eq!(staged.sha256, payload_sha);
        assert!(!staged.auto_install_allowed());
        assert_eq!(staged.provenance, ProvenanceStatus::Unconfigured);

        // Verify backup + rollback integrity.
        let fake_exe = dir.path().join("HYDRA-Leviathan-Windows-x64.exe");
        fs::write(&fake_exe, b"OLD_VERSION_27").unwrap();
        let backup = backup_executable(&fake_exe, "1.7.0-alpha.27", &updates_root).unwrap();
        fs::write(&fake_exe, b"CORRUPTED_OR_NEW").unwrap();
        rollback_from_backup(&backup, &fake_exe, &updates_root).unwrap();
        assert_eq!(fs::read(&fake_exe).unwrap(), b"OLD_VERSION_27");
    }
}
