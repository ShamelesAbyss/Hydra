//! Trusted evidence store for restricted pilot authorization (`v2.0.0 | THE AWAKENING`).
//!
//! Replaces the previous self-described `EvidenceRecord` trust decision. Evidence is only
//! accepted when ALL of the following hold:
//! - It was produced by an in-process Hydra producer that computed the result itself
//!   (historical OOS acceptance, public-paper lifecycle attestation, Testnet scenario), never
//!   from a user-typed JSON registry.
//! - The raw producer artifact bytes are stored beside the record and their SHA3-256 is
//!   recomputed at verification time (`artifact_sha3_256`, accurately named SHA3, not SHA-256).
//! - The record carries a keyed SHA3-256 MAC (prefix-key sponge construction) computed with a
//!   local 256-bit trust key generated on first use and stored under Windows DPAPI
//!   (current-user scope) or `0600` owner-only permissions on Unix. No secret is embedded in the
//!   public binary.
//! - Kind-specific producer/environment semantics, exact build identity (version + git commit),
//!   strategy registry version, exact strategy configuration digest, mandatory bounded expiry and
//!   an independent revocation list are enforced, and the artifact contents are re-parsed to
//!   confirm they actually support the claimed verdict.
//!
//! Threat model (documented honestly): same-user malware that can call DPAPI as the operator
//! can read the trust key and forge records, edit datasets before attestation, or delete the
//! revocation list. Cross-user access, plain file edits, re-hashing, producer relabeling, mock
//! fixtures, stale/future records and unrelated imported results are rejected.

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::path::{Path, PathBuf};

pub const TRUSTED_EVIDENCE_SCHEMA_VERSION: u32 = 1;
pub const MAX_CLOCK_SKEW_MS: i64 = 60_000;

/// Exact executable build identity: `<cargo version>+<git commit>[-dirty]`.
pub fn build_identity() -> String {
    let dirty = if env!("HYDRA_BUILD_DIRTY") == "dirty" {
        "-dirty"
    } else {
        ""
    };
    format!(
        "{}+{}{}",
        env!("CARGO_PKG_VERSION"),
        env!("HYDRA_BUILD_COMMIT"),
        dirty
    )
}

pub fn build_commit() -> &'static str {
    env!("HYDRA_BUILD_COMMIT")
}

pub fn sha3_hex(bytes: &[u8]) -> String {
    hex::encode(Sha3_256::digest(bytes))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustedEvidenceKind {
    HistoricalOosQualification,
    PublicPaperLifecycle,
    TestnetExecutionProof,
}

impl TrustedEvidenceKind {
    pub const ALL: [Self; 3] = [
        Self::HistoricalOosQualification,
        Self::PublicPaperLifecycle,
        Self::TestnetExecutionProof,
    ];

    pub const fn permitted_producer(self) -> crate::live_gate::EvidenceProducer {
        use crate::live_gate::EvidenceProducer as P;
        match self {
            Self::HistoricalOosQualification => P::HistoricalLaboratory,
            Self::PublicPaperLifecycle => P::PublicPaperService,
            Self::TestnetExecutionProof => P::HyperliquidTestnetProbe,
        }
    }

    pub const fn permitted_environment(self) -> crate::live_gate::EvidenceEnvironment {
        use crate::live_gate::EvidenceEnvironment as E;
        match self {
            Self::HistoricalOosQualification => E::OfflineMock,
            Self::PublicPaperLifecycle => E::PublicPaper,
            Self::TestnetExecutionProof => E::HyperliquidTestnet,
        }
    }

    /// Mandatory bounded validity window.
    pub const fn max_validity_ms(self) -> i64 {
        match self {
            Self::HistoricalOosQualification => 14 * 86_400_000,
            Self::PublicPaperLifecycle => 7 * 86_400_000,
            Self::TestnetExecutionProof => 7 * 86_400_000,
        }
    }

    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::HistoricalOosQualification => "historical_oos",
            Self::PublicPaperLifecycle => "public_paper",
            Self::TestnetExecutionProof => "testnet_execution",
        }
    }

    /// Whether the exact strategy configuration digest must match the deployment.
    pub const fn binds_strategy_config(self) -> bool {
        matches!(self, Self::HistoricalOosQualification)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedEvidenceRecord {
    pub schema_version: u32,
    pub record_id: String,
    pub kind: TrustedEvidenceKind,
    pub producer: crate::live_gate::EvidenceProducer,
    pub environment: crate::live_gate::EvidenceEnvironment,
    pub build_identity: String,
    pub registry_version: String,
    /// Exact strategy configuration digest (`UnifiedStrategyConfig::config_sha3`) or `"n/a"`.
    pub strategy_config_sha3: String,
    pub dataset_fingerprint: String,
    pub artifact_sha3_256: String,
    pub produced_at_ms: i64,
    pub expires_at_ms: i64,
    pub passed: bool,
    pub summary: String,
    pub mac_sha3: String,
}

impl TrustedEvidenceRecord {
    fn canonical_unmacd(&self) -> Result<Vec<u8>> {
        let mut clone = self.clone();
        clone.mac_sha3 = String::new();
        Ok(serde_json::to_vec(&clone)?)
    }
}

/// What the running deployment expects evidence to be bound to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentBinding {
    pub build_identity: String,
    pub registry_version: String,
    pub strategy_config_sha3: String,
}

impl DeploymentBinding {
    pub fn current(strategy_config_sha3: impl Into<String>) -> Self {
        Self {
            build_identity: build_identity(),
            registry_version: crate::fang_engine::REGISTRY_VERSION.to_string(),
            strategy_config_sha3: strategy_config_sha3.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceKindStatus {
    pub kind: TrustedEvidenceKind,
    pub satisfied: bool,
    pub record_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedReadiness {
    pub build_identity: String,
    pub strategy_config_sha3: String,
    pub statuses: Vec<EvidenceKindStatus>,
    pub all_satisfied: bool,
}

/// Local authenticated evidence bundle (`<root>/trust.key`, `<root>/<kind>/<id>/{record,artifact}.json`).
#[derive(Debug, Clone)]
pub struct TrustStore {
    root: PathBuf,
}

impl TrustStore {
    pub fn at_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default_root() -> Result<PathBuf> {
        #[cfg(windows)]
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .context("LOCALAPPDATA required for trusted evidence store")?;
        #[cfg(not(windows))]
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .context("XDG_STATE_HOME or HOME required for trusted evidence store")?;
        Ok(base.join("Hydra").join("evidence"))
    }

    pub fn open_default() -> Result<Self> {
        Ok(Self::at_root(Self::default_root()?))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn ensure_dir(path: &Path) -> Result<()> {
        std::fs::create_dir_all(path)
            .with_context(|| format!("create evidence directory {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    fn key_path(&self) -> PathBuf {
        self.root.join("trust.key")
    }

    /// Loads the local trust key, generating it on first use (never exported).
    fn trust_key(&self, create: bool) -> Result<Vec<u8>> {
        let path = self.key_path();
        if path.is_file() {
            let blob = std::fs::read(&path).context("read evidence trust key")?;
            let key = crate::treasury::unprotect_bytes(&blob)?;
            ensure!(key.len() == 32, "evidence trust key corrupt");
            return Ok(key);
        }
        ensure!(
            create,
            "evidence trust key absent; no trusted evidence exists"
        );
        Self::ensure_dir(&self.root)?;
        let mut key = Vec::with_capacity(32);
        key.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        key.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        let mixed = Sha3_256::digest(&key).to_vec();
        let protected = crate::treasury::protect_bytes(&mixed)?;
        crate::treasury::write_atomic_restricted(&path, &protected)?;
        Ok(mixed)
    }

    fn mac(key: &[u8], canonical: &[u8]) -> String {
        let mut h = Sha3_256::new();
        h.update(b"hydra-trusted-evidence-mac-v1|");
        h.update(key);
        h.update(b"|");
        h.update(canonical);
        hex::encode(h.finalize())
    }

    fn revocation_path(&self) -> PathBuf {
        self.root.join("revoked.json")
    }

    pub fn revoked_ids(&self) -> Result<std::collections::BTreeSet<String>> {
        let p = self.revocation_path();
        if !p.is_file() {
            return Ok(Default::default());
        }
        Ok(serde_json::from_slice(&std::fs::read(&p)?)?)
    }

    /// Owner revocation, independent of any self-declared record field.
    pub fn revoke(&self, record_id: &str) -> Result<()> {
        Self::ensure_dir(&self.root)?;
        let mut ids = self.revoked_ids()?;
        ids.insert(record_id.to_string());
        crate::treasury::write_atomic_restricted(
            &self.revocation_path(),
            &serde_json::to_vec_pretty(&ids)?,
        )
    }

    /// Crate-private producer boundary: only in-process Hydra producers that computed the
    /// verdict themselves may record trusted evidence.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record(
        &self,
        kind: TrustedEvidenceKind,
        artifact_bytes: &[u8],
        strategy_config_sha3: &str,
        dataset_fingerprint: &str,
        passed: bool,
        summary: &str,
        now_ms: i64,
    ) -> Result<TrustedEvidenceRecord> {
        ensure!(!artifact_bytes.is_empty(), "evidence artifact is empty");
        ensure!(
            artifact_bytes.len() <= 8 * 1024 * 1024,
            "evidence artifact exceeds 8 MiB"
        );
        ensure!(
            !dataset_fingerprint.trim().is_empty(),
            "dataset fingerprint required"
        );
        if kind.binds_strategy_config() {
            ensure!(
                strategy_config_sha3.len() == 64,
                "historical evidence requires exact strategy config digest"
            );
        }
        let key = self.trust_key(true)?;
        let record_id = format!(
            "{}-{}-{}",
            kind.dir_name(),
            now_ms,
            &uuid::Uuid::new_v4().simple().to_string()[..12]
        );
        let mut rec = TrustedEvidenceRecord {
            schema_version: TRUSTED_EVIDENCE_SCHEMA_VERSION,
            record_id: record_id.clone(),
            kind,
            producer: kind.permitted_producer(),
            environment: kind.permitted_environment(),
            build_identity: build_identity(),
            registry_version: crate::fang_engine::REGISTRY_VERSION.to_string(),
            strategy_config_sha3: if kind.binds_strategy_config() {
                strategy_config_sha3.to_string()
            } else {
                "n/a".into()
            },
            dataset_fingerprint: dataset_fingerprint.to_string(),
            artifact_sha3_256: sha3_hex(artifact_bytes),
            produced_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(kind.max_validity_ms()),
            passed,
            summary: summary.chars().take(512).collect(),
            mac_sha3: String::new(),
        };
        rec.mac_sha3 = Self::mac(&key, &rec.canonical_unmacd()?);
        let dir = self.root.join(kind.dir_name()).join(&record_id);
        Self::ensure_dir(&dir)?;
        crate::treasury::write_atomic_restricted(&dir.join("artifact.json"), artifact_bytes)?;
        crate::treasury::write_atomic_restricted(
            &dir.join("record.json"),
            &serde_json::to_vec_pretty(&rec)?,
        )?;
        Ok(rec)
    }

    fn load_bundle(dir: &Path) -> Result<(TrustedEvidenceRecord, Vec<u8>)> {
        let rec: TrustedEvidenceRecord =
            serde_json::from_slice(&std::fs::read(dir.join("record.json"))?)?;
        let art = std::fs::read(dir.join("artifact.json"))?;
        Ok((rec, art))
    }

    /// Full verification of one bundle against the running deployment binding.
    pub fn verify_bundle(
        &self,
        rec: &TrustedEvidenceRecord,
        artifact: &[u8],
        binding: &DeploymentBinding,
        now_ms: i64,
    ) -> Result<()> {
        ensure!(
            rec.schema_version == TRUSTED_EVIDENCE_SCHEMA_VERSION,
            "unsupported evidence schema"
        );
        let key = self.trust_key(false)?;
        let expected_mac = Self::mac(&key, &rec.canonical_unmacd()?);
        ensure!(
            constant_time_eq(expected_mac.as_bytes(), rec.mac_sha3.as_bytes()),
            "evidence MAC invalid (edited, re-hashed or foreign record)"
        );
        ensure!(
            sha3_hex(artifact) == rec.artifact_sha3_256,
            "artifact bytes do not match recorded SHA3-256"
        );
        ensure!(
            rec.producer == rec.kind.permitted_producer()
                && rec.environment == rec.kind.permitted_environment(),
            "producer/environment not permitted for evidence kind"
        );
        ensure!(
            rec.build_identity == binding.build_identity,
            "evidence build identity '{}' does not match running build '{}'",
            rec.build_identity,
            binding.build_identity
        );
        ensure!(
            rec.registry_version == binding.registry_version,
            "strategy registry version mismatch"
        );
        if rec.kind.binds_strategy_config() {
            ensure!(
                rec.strategy_config_sha3 == binding.strategy_config_sha3,
                "evidence strategy config digest does not match deployed configuration"
            );
        }
        ensure!(
            rec.produced_at_ms > 0
                && rec.produced_at_ms <= now_ms.saturating_add(MAX_CLOCK_SKEW_MS),
            "evidence is future-dated"
        );
        ensure!(
            rec.expires_at_ms > now_ms
                && rec.expires_at_ms.saturating_sub(rec.produced_at_ms)
                    <= rec.kind.max_validity_ms(),
            "evidence expired or has unbounded validity"
        );
        ensure!(
            !self.revoked_ids()?.contains(&rec.record_id),
            "evidence revoked by owner"
        );
        ensure!(rec.passed, "evidence records a failing or partial result");
        Self::verify_artifact_semantics(rec, artifact)
    }

    /// Re-parse the artifact and confirm it genuinely supports a passing verdict.
    fn verify_artifact_semantics(rec: &TrustedEvidenceRecord, artifact: &[u8]) -> Result<()> {
        let v: serde_json::Value =
            serde_json::from_slice(artifact).context("artifact is not valid JSON")?;
        match rec.kind {
            TrustedEvidenceKind::HistoricalOosQualification => {
                ensure!(
                    v.pointer("/verdict/verdict").and_then(|x| x.as_str())
                        == Some("eligible_for_monitored_pilot"),
                    "historical artifact verdict is not eligible_for_monitored_pilot"
                );
                ensure!(
                    v.get("config_sha3").and_then(|x| x.as_str())
                        == Some(rec.strategy_config_sha3.as_str()),
                    "historical artifact config digest mismatch"
                );
                ensure!(
                    v.get("dataset_sha3_256").and_then(|x| x.as_str())
                        == Some(rec.dataset_fingerprint.as_str()),
                    "historical artifact dataset fingerprint mismatch"
                );
            }
            TrustedEvidenceKind::PublicPaperLifecycle => {
                ensure!(
                    v.get("provenance").and_then(|x| x.as_str()) == Some("public_paper"),
                    "paper artifact is not a public-market paper session"
                );
                ensure!(
                    v.get("cycle_count").and_then(|x| x.as_u64()).unwrap_or(0) >= 3,
                    "paper artifact has fewer than 3 committed cycles"
                );
                ensure!(
                    v.get("status").and_then(|x| x.as_str()) == Some("stopped")
                        && v.get("fault_reason").is_some_and(|x| x.is_null())
                        && v.get("quick_check").and_then(|x| x.as_str()) == Some("ok"),
                    "paper artifact lifecycle not cleanly stopped with healthy database"
                );
            }
            TrustedEvidenceKind::TestnetExecutionProof => {
                ensure!(
                    v.get("environment").and_then(|x| x.as_str()) == Some("HyperliquidTestnet"),
                    "testnet artifact environment mismatch"
                );
                for flag in [
                    "scenario_passed",
                    "entry_fill_verified",
                    "stop_readback_verified",
                    "restart_reconciliation_verified",
                    "close_verified",
                    "stop_cancel_verified",
                    "zero_residual_exposure",
                ] {
                    ensure!(
                        v.get(flag).and_then(|x| x.as_bool()) == Some(true),
                        "testnet artifact flag '{flag}' not true"
                    );
                }
            }
        }
        Ok(())
    }

    /// Latest valid record of `kind` for the deployment, with the precise reason when absent.
    pub fn latest_valid(
        &self,
        kind: TrustedEvidenceKind,
        binding: &DeploymentBinding,
        now_ms: i64,
    ) -> EvidenceKindStatus {
        let dir = self.root.join(kind.dir_name());
        let mut best: Option<(i64, String)> = None;
        let mut last_reason = format!("no {} evidence recorded", kind.dir_name());
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if !p.is_dir() {
                    continue;
                }
                match Self::load_bundle(&p) {
                    Ok((rec, art)) => match self.verify_bundle(&rec, &art, binding, now_ms) {
                        Ok(()) => {
                            if best.as_ref().is_none_or(|(ts, _)| rec.produced_at_ms > *ts) {
                                best = Some((rec.produced_at_ms, rec.record_id.clone()));
                            }
                        }
                        Err(err) => last_reason = format!("{}: {err}", rec.record_id),
                    },
                    Err(err) => last_reason = format!("unreadable bundle {}: {err}", p.display()),
                }
            }
        }
        match best {
            Some((_, id)) => EvidenceKindStatus {
                kind,
                satisfied: true,
                record_id: Some(id),
                reason: "verified".into(),
            },
            None => EvidenceKindStatus {
                kind,
                satisfied: false,
                record_id: None,
                reason: last_reason,
            },
        }
    }

    /// Single authoritative trusted-evidence readiness verdict shared by CLI and cockpit.
    pub fn readiness(&self, binding: &DeploymentBinding, now_ms: i64) -> TrustedReadiness {
        let statuses: Vec<EvidenceKindStatus> = TrustedEvidenceKind::ALL
            .iter()
            .map(|k| self.latest_valid(*k, binding, now_ms))
            .collect();
        let all_satisfied = statuses.iter().all(|s| s.satisfied);
        TrustedReadiness {
            build_identity: binding.build_identity.clone(),
            strategy_config_sha3: binding.strategy_config_sha3.clone(),
            statuses,
            all_satisfied,
        }
    }

    /// Records evidence and returns an error unless the producer verdict actually passed.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_checked(
        &self,
        kind: TrustedEvidenceKind,
        artifact_bytes: &[u8],
        strategy_config_sha3: &str,
        dataset_fingerprint: &str,
        passed: bool,
        summary: &str,
        now_ms: i64,
    ) -> Result<TrustedEvidenceRecord> {
        let rec = self.record(
            kind,
            artifact_bytes,
            strategy_config_sha3,
            dataset_fingerprint,
            passed,
            summary,
            now_ms,
        )?;
        if !passed {
            bail!(
                "{} evidence recorded as FAILED/partial ({}); it can never authorize production",
                kind.dir_name(),
                rec.record_id
            );
        }
        Ok(rec)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hist_artifact(cfg: &str, ds: &str, eligible: bool) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "verdict": {"verdict": if eligible {"eligible_for_monitored_pilot"} else {"rejected_keep_disabled"}},
            "config_sha3": cfg,
            "dataset_sha3_256": ds,
        }))
        .unwrap()
    }

    #[test]
    fn trusted_evidence_rejects_edits_relabels_wrong_config_stale_revoked_and_failed() {
        let dir = tempfile::tempdir().unwrap();
        let store = TrustStore::at_root(dir.path());
        let cfg = "a".repeat(64);
        let binding = DeploymentBinding::current(cfg.clone());
        let now = 1_800_000_000_000i64;
        assert!(!store.readiness(&binding, now).all_satisfied);

        let art = hist_artifact(&cfg, "ds1", true);
        let rec = store
            .record_checked(
                TrustedEvidenceKind::HistoricalOosQualification,
                &art,
                &cfg,
                "ds1",
                true,
                "fixture",
                now,
            )
            .unwrap();
        store.verify_bundle(&rec, &art, &binding, now + 1).unwrap();
        assert!(
            store
                .latest_valid(
                    TrustedEvidenceKind::HistoricalOosQualification,
                    &binding,
                    now + 1
                )
                .satisfied
        );

        // Edited + re-hashed artifact rejected (MAC binds artifact digest).
        let forged = hist_artifact(&cfg, "ds1", true);
        let mut edited = rec.clone();
        edited.summary = "edited".into();
        assert!(store
            .verify_bundle(&edited, &forged, &binding, now + 1)
            .is_err());
        let mut rehash = rec.clone();
        let other = hist_artifact(&cfg, "ds2", true);
        rehash.artifact_sha3_256 = sha3_hex(&other);
        assert!(store
            .verify_bundle(&rehash, &other, &binding, now + 1)
            .is_err());
        // Producer relabel rejected.
        let mut relabel = rec.clone();
        relabel.producer = crate::live_gate::EvidenceProducer::LocalVerificationSuite;
        assert!(store
            .verify_bundle(&relabel, &art, &binding, now + 1)
            .is_err());
        // Substituted strategy configuration rejected.
        let other_binding = DeploymentBinding::current("b".repeat(64));
        assert!(store
            .verify_bundle(&rec, &art, &other_binding, now + 1)
            .is_err());
        // Future-dated and expired rejected.
        assert!(store
            .verify_bundle(&rec, &art, &binding, now - 10 * MAX_CLOCK_SKEW_MS)
            .is_err());
        assert!(store
            .verify_bundle(&rec, &art, &binding, now + 15 * 86_400_000)
            .is_err());
        // Foreign trust key rejected.
        let other_dir = tempfile::tempdir().unwrap();
        let foreign = TrustStore::at_root(other_dir.path());
        foreign
            .record(
                TrustedEvidenceKind::PublicPaperLifecycle,
                b"{}",
                "n/a",
                "x",
                false,
                "",
                now,
            )
            .unwrap();
        assert!(foreign
            .verify_bundle(&rec, &art, &binding, now + 1)
            .is_err());
        // Revocation honored independently of record fields.
        store.revoke(&rec.record_id).unwrap();
        assert!(store.verify_bundle(&rec, &art, &binding, now + 1).is_err());

        // Failing producer verdict can never authorize, even with a valid MAC.
        let bad = hist_artifact(&cfg, "ds3", false);
        assert!(store
            .record_checked(
                TrustedEvidenceKind::HistoricalOosQualification,
                &bad,
                &cfg,
                "ds3",
                false,
                "rejected",
                now,
            )
            .is_err());
        // Passed flag with non-eligible artifact contents is rejected semantically.
        let lying = store
            .record(
                TrustedEvidenceKind::HistoricalOosQualification,
                &bad,
                &cfg,
                "ds3",
                true,
                "lying",
                now,
            )
            .unwrap();
        assert!(store
            .verify_bundle(&lying, &bad, &binding, now + 1)
            .is_err());
    }
}
