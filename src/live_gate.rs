//! Authoritative live-trading certification and execution authorization gate.
//!
//! Safety invariants:
//! - Every production real-money entry point (CLI, `Engine::new`, `ExecutionEngine::submit`,
//!   `HyperliquidAdapter`, `StateManager::emergency_flatten`, native cockpit, and future IPC)
//!   must consult this module.
//! - There is intentionally no `Certified` bypass in production code while net financial
//!   settlement, stream/restart recovery, exchange-native protective orders, authenticated
//!   local control IPC, and watchdog supervision remain incomplete.

use anyhow::{bail, ensure, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveCertificationStatus {
    PendingCertification {
        missing_requirements: &'static [&'static str],
    },
}

pub const PENDING_REQUIREMENTS: &[&str] = &[
    "Version-bound EvidenceRegistry across settlement, recovery, native Testnet stops, IPC/signing, and live watchdog supervision",
    "Historical OOS strategy acceptance (`hydra strategy-accept` over verified public market history)",
    "Functional public-market paper lifecycle (`--paper-service-worker` clean start/ingest/pause/resume/stop/DB check)",
    "Hyperliquid Testnet functional exchange proof (`TriggerOrderAction` reduce-only stops, account precision & reconciliation)",
    "Explicit owner monitored-pilot authorization & hard capital/loss/duration ceilings (separate from unattended AWAKENED)",
];

pub const CURRENT_LIVE_CERTIFICATION: LiveCertificationStatus =
    LiveCertificationStatus::PendingCertification {
        missing_requirements: PENDING_REQUIREMENTS,
    };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionChannel {
    ProductionEngine,
    CliRun,
    StateFlatten,
    VenueAdapterDirect,
    CockpitControl,
    IpcControl,
}

impl ExecutionChannel {
    pub const fn label(self) -> &'static str {
        match self {
            Self::ProductionEngine => "engine::new",
            Self::CliRun => "cli::run",
            Self::StateFlatten => "state::emergency_flatten",
            Self::VenueAdapterDirect => "hyperliquid_adapter::submit",
            Self::CockpitControl => "cockpit::control",
            Self::IpcControl => "ipc::control",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveAuthorizationRequest {
    pub channel: ExecutionChannel,
    pub configured_live_trading: bool,
    pub cli_live_requested: bool,
    pub treasury_authorized: bool,
    pub emergency_suspended: bool,
}

impl LiveAuthorizationRequest {
    /// Inspect local non-secret Treasury profile flags best-effort (fail closed if missing/corrupt).
    pub fn from_treasury_store(
        channel: ExecutionChannel,
        configured_live_trading: bool,
        cli_live_requested: bool,
    ) -> Self {
        let (treasury_authorized, emergency_suspended) =
            match crate::treasury::TreasuryStore::default_store()
                .and_then(|s| s.load_profile().map(|opt| (s, opt)))
            {
                Ok((store, Some(profile))) => (
                    profile.wallet_authorized
                        && profile.risk_acknowledged
                        && store.has_protected_key()
                        && !profile.emergency_suspended,
                    profile.emergency_suspended,
                ),
                _ => (false, false),
            };
        Self {
            channel,
            configured_live_trading,
            cli_live_requested,
            treasury_authorized,
            emergency_suspended,
        }
    }
}

/// Structural separation between production exchange transports and test-only loopback fixtures.
///
/// Outside `#[cfg(test)]`, only `ProductionMainnet` exists, preventing any production path
/// from treating an endpoint as a mock/loopback bypass based solely on string inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VenueTransportClass {
    ProductionMainnet,
    TestnetSandbox,
    #[cfg(test)]
    LoopbackTestFixture,
}

impl VenueTransportClass {
    pub const fn is_production(self) -> bool {
        match self {
            Self::ProductionMainnet => true,
            Self::TestnetSandbox => false,
            #[cfg(test)]
            Self::LoopbackTestFixture => false,
        }
    }

    pub const fn is_testnet(self) -> bool {
        matches!(self, Self::TestnetSandbox)
    }
}

/// Returns `true` for the official Hyperliquid Testnet origin.
pub fn is_hyperliquid_testnet_url(url: &str) -> bool {
    let normalized = url.trim().to_ascii_lowercase();
    normalized.starts_with("https://api.hyperliquid-testnet.xyz")
        || normalized.starts_with("wss://api.hyperliquid-testnet.xyz")
}

/// Returns `true` for any exchange endpoint that is not an explicit local loopback test server or Testnet sandbox.
pub fn is_production_exchange_url(url: &str) -> bool {
    let normalized = url.trim().to_ascii_lowercase();
    !(normalized.starts_with("http://127.0.0.1:")
        || normalized.starts_with("http://localhost:")
        || normalized.starts_with("http://[::1]:")
        || normalized.starts_with("ws://127.0.0.1:")
        || normalized.starts_with("ws://localhost:")
        || normalized.starts_with("ws://[::1]:")
        || is_hyperliquid_testnet_url(&normalized))
}

/// Authoritative fail-closed gate for any real-money production execution path.
pub fn require_certified_live_execution(req: &LiveAuthorizationRequest) -> Result<()> {
    ensure!(
        !req.cli_live_requested || req.configured_live_trading,
        "[{}] --live requested without ENABLE_LIVE_TRADING=true",
        req.channel.label()
    );
    ensure!(
        req.configured_live_trading,
        "[{}] live execution disabled (ENABLE_LIVE_TRADING=false)",
        req.channel.label()
    );
    ensure!(
        !req.emergency_suspended,
        "[{}] emergency suspension is active; real-money execution blocked",
        req.channel.label()
    );
    match CURRENT_LIVE_CERTIFICATION {
        LiveCertificationStatus::PendingCertification {
            missing_requirements,
        } => {
            ensure!(
                req.treasury_authorized,
                "[{}] PendingCertification and Treasury wallet authorization is incomplete: {}",
                req.channel.label(),
                missing_requirements.join("; ")
            );
            bail!(
                "[{}] live real-money execution blocked by PendingCertification: {}",
                req.channel.label(),
                missing_requirements.join("; ")
            );
        }
    }
}

pub fn certification_summary() -> String {
    match CURRENT_LIVE_CERTIFICATION {
        LiveCertificationStatus::PendingCertification {
            missing_requirements,
        } => format!(
            "PendingCertification ({} incomplete gates: {})",
            missing_requirements.len(),
            missing_requirements.join("; ")
        ),
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum VerificationTier {
    NotImplemented,
    ImplementedUnverified,
    ComponentVerified,
    IntegratedVerified,
    RealExchangeTestVerified,
    QualifiedStrategyVerified,
    MonitoredPilotEligible,
    UnattendedCertified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceEnvironment {
    OfflineMock,
    PublicPaper,
    HyperliquidTestnet,
    HyperliquidMainnet,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvidenceRecord {
    pub pillar_index: u8,
    pub tier: VerificationTier,
    pub software_version: String,
    pub strategy_registry_version: String,
    pub risk_policy_sha3: String,
    pub environment: EvidenceEnvironment,
    pub account_mode: String,
    pub dataset_fingerprint: String,
    pub recorded_at_ms: i64,
    pub summary: String,
    pub record_sha3: String,
}

impl EvidenceRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn signed_new(
        pillar_index: u8,
        tier: VerificationTier,
        risk_policy_sha3: impl Into<String>,
        environment: EvidenceEnvironment,
        account_mode: impl Into<String>,
        dataset_fingerprint: impl Into<String>,
        recorded_at_ms: i64,
        summary: impl Into<String>,
    ) -> Self {
        let mut rec = Self {
            pillar_index,
            tier,
            software_version: env!("CARGO_PKG_VERSION").to_string(),
            strategy_registry_version: crate::fang_engine::REGISTRY_VERSION.to_string(),
            risk_policy_sha3: risk_policy_sha3.into(),
            environment,
            account_mode: account_mode.into(),
            dataset_fingerprint: dataset_fingerprint.into(),
            recorded_at_ms,
            summary: summary.into(),
            record_sha3: String::new(),
        };
        rec.record_sha3 = rec.compute_digest();
        rec
    }

    pub fn compute_digest(&self) -> String {
        use sha3::{Digest, Sha3_256};
        let canonical = format!(
            "v1|{}|{:?}|{}|{}|{}|{:?}|{}|{}|{}|{}",
            self.pillar_index,
            self.tier,
            self.software_version,
            self.strategy_registry_version,
            self.risk_policy_sha3,
            self.environment,
            self.account_mode,
            self.dataset_fingerprint,
            self.recorded_at_ms,
            self.summary
        );
        let mut h = Sha3_256::new();
        h.update(canonical.as_bytes());
        hex::encode(h.finalize())
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EvidenceRegistry {
    pub records: Vec<EvidenceRecord>,
}

impl EvidenceRegistry {
    pub fn verify_record(
        record: &EvidenceRecord,
        expected_env: Option<EvidenceEnvironment>,
        expected_risk_sha3: &str,
    ) -> bool {
        if record.software_version != env!("CARGO_PKG_VERSION") {
            return false;
        }
        if record.strategy_registry_version != crate::fang_engine::REGISTRY_VERSION {
            return false;
        }
        if record.risk_policy_sha3.is_empty() || record.risk_policy_sha3 != expected_risk_sha3 {
            return false;
        }
        if let Some(env) = expected_env {
            if record.environment != env {
                return false;
            }
        }
        if record.dataset_fingerprint.trim().is_empty() || record.recorded_at_ms <= 0 {
            return false;
        }
        record.record_sha3 == record.compute_digest()
    }

    pub fn latest_valid_for_pillar(
        &self,
        pillar_index: u8,
        expected_env: Option<EvidenceEnvironment>,
        expected_risk_sha3: &str,
    ) -> Option<&EvidenceRecord> {
        self.records
            .iter()
            .filter(|r| {
                r.pillar_index == pillar_index
                    && Self::verify_record(r, expected_env, expected_risk_sha3)
            })
            .max_by_key(|r| r.recorded_at_ms)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PillarStatus {
    OfflineFixtureVerified,
    EvidenceVerified,
    PendingExternalValidation,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PillarAssessment {
    pub index: u8,
    pub pillar: &'static str,
    pub status: PillarStatus,
    pub tier: VerificationTier,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CertificationChecklist {
    pub live_trading_authorized: bool,
    pub overall_status: &'static str,
    pub pillars: Vec<PillarAssessment>,
}

/// Evaluates current evidence-backed status across all 6 certification pillars
/// without hardcoded `OfflineFixtureVerified` assumptions. Callers without a
/// version-bound `EvidenceRegistry` see honest `PendingExternalValidation` gaps.
pub fn evaluate_certification_readiness() -> CertificationChecklist {
    evaluate_certification_readiness_with_registry(None, "")
}

pub fn evaluate_certification_readiness_with_registry(
    registry: Option<&EvidenceRegistry>,
    expected_risk_sha3: &str,
) -> CertificationChecklist {
    let specs: [(u8, &'static str, &'static str); 6] = [
        (
            1,
            "Authoritative net financial settlement",
            "Missing current version-bound SettlementStore / ledger reconciliation EvidenceRecord",
        ),
        (
            2,
            "Exchange and restart order recovery",
            "Missing current version-bound REST/WS recovery & restart EvidenceRecord",
        ),
        (
            3,
            "Native protective orders and sovereign risk",
            "Missing current version-bound native reduce_only stop & Testnet EvidenceRecord",
        ),
        (
            4,
            "Authenticated engine IPC and credential isolation",
            "Missing current version-bound EngineControlAuthority / EngineSignerAuthority EvidenceRecord",
        ),
        (
            5,
            "External watchdog supervision and fault-injection resilience",
            "Missing current version-bound WatchdogSupervisor / sentinel_faults EvidenceRecord",
        ),
        (
            6,
            "Accelerated Awakening evidence: historical OOS strategy qualification, public paper lifecycle & Testnet proof",
            "Accelerated Awakening requires verified historical OOS replay, public paper lifecycle, Hyperliquid Testnet functional proof & explicit owner pilot ceilings (plus pinned PINNED_RELEASE_SIGNING_PUBKEY_HEX for auto-update authenticity)",
        ),
    ];

    let pillars = specs
        .into_iter()
        .map(|(index, pillar, missing_msg)| {
            if let Some(rec) =
                registry.and_then(|r| r.latest_valid_for_pillar(index, None, expected_risk_sha3))
            {
                let verified_external =
                    index < 6 || rec.tier >= VerificationTier::RealExchangeTestVerified;
                PillarAssessment {
                    index,
                    pillar,
                    status: if verified_external {
                        PillarStatus::EvidenceVerified
                    } else {
                        PillarStatus::PendingExternalValidation
                    },
                    tier: rec.tier,
                    evidence: format!(
                        "{} [env={:?} v={} digest={}]",
                        rec.summary,
                        rec.environment,
                        rec.software_version,
                        &rec.record_sha3[..12.min(rec.record_sha3.len())]
                    ),
                }
            } else {
                PillarAssessment {
                    index,
                    pillar,
                    status: PillarStatus::PendingExternalValidation,
                    tier: if index < 6 {
                        VerificationTier::ImplementedUnverified
                    } else {
                        VerificationTier::NotImplemented
                    },
                    evidence: missing_msg.to_string(),
                }
            }
        })
        .collect();

    CertificationChecklist {
        live_trading_authorized: false,
        overall_status: "PendingCertification",
        pillars,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certification_gate_refuses_even_if_all_caller_flags_are_enabled() {
        let req = LiveAuthorizationRequest {
            channel: ExecutionChannel::ProductionEngine,
            configured_live_trading: true,
            cli_live_requested: true,
            treasury_authorized: true,
            emergency_suspended: false,
        };
        let err = require_certified_live_execution(&req)
            .unwrap_err()
            .to_string();
        assert!(err.contains("PendingCertification"), "got: {err}");
    }

    #[test]
    fn loopback_endpoints_are_not_misclassified_as_production() {
        assert!(!is_production_exchange_url("http://127.0.0.1:8080"));
        assert!(!is_production_exchange_url("ws://127.0.0.1:9001"));
        assert!(!is_production_exchange_url("http://localhost:3000"));
        assert!(is_production_exchange_url("https://api.hyperliquid.xyz"));
        assert!(is_production_exchange_url("wss://api.hyperliquid.xyz/ws"));
        assert!(VenueTransportClass::ProductionMainnet.is_production());
        assert!(!VenueTransportClass::LoopbackTestFixture.is_production());
    }
}
