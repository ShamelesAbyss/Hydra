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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveExecutionTier {
    Dormant,
    RestrictedMonitoredPilot,
    UnattendedAwakened,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PilotOrderType {
    LimitPostOnly,
    LimitIoc,
    NativeReduceOnlyStop,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MonitoredPilotLimits {
    pub wallet_address: String,
    pub allowed_instruments: std::collections::BTreeSet<String>,
    pub qualified_strategy_id: String,
    pub strategy_registry_version: String,
    pub max_allocated_capital_usd: rust_decimal::Decimal,
    pub max_order_notional_usd: rust_decimal::Decimal,
    pub max_aggregate_exposure_usd: rust_decimal::Decimal,
    pub max_leverage: rust_decimal::Decimal,
    pub max_daily_loss_usd: rust_decimal::Decimal,
    pub max_session_loss_usd: rust_decimal::Decimal,
    pub max_drawdown_ratio: rust_decimal::Decimal,
    pub permitted_order_types: std::collections::BTreeSet<PilotOrderType>,
    pub pilot_session_duration_secs: u64,
    pub operator_supervision_lease_secs: u64,
}

impl MonitoredPilotLimits {
    pub fn validate(&self) -> Result<()> {
        use rust_decimal::Decimal;
        let w = self.wallet_address.trim();
        let hex_part = w.strip_prefix("0x").unwrap_or("");
        ensure!(
            w.starts_with("0x")
                && hex_part.len() == 40
                && hex_part.chars().all(|c| c.is_ascii_hexdigit()),
            "monitored pilot requires valid dedicated EVM wallet address (0x + 40 hex chars)"
        );
        ensure!(
            !self.allowed_instruments.is_empty()
                && self
                    .allowed_instruments
                    .iter()
                    .all(|s| !s.trim().is_empty()),
            "allowed_instruments must be non-empty"
        );
        ensure!(
            !self.qualified_strategy_id.trim().is_empty(),
            "qualified_strategy_id must be specified"
        );
        ensure!(
            self.strategy_registry_version == crate::fang_engine::REGISTRY_VERSION,
            "strategy_registry_version mismatch: got '{}', expected '{}'",
            self.strategy_registry_version,
            crate::fang_engine::REGISTRY_VERSION
        );
        ensure!(
            self.max_allocated_capital_usd > Decimal::ZERO,
            "max_allocated_capital_usd must be > 0"
        );
        ensure!(
            self.max_order_notional_usd > Decimal::ZERO
                && self.max_order_notional_usd <= self.max_allocated_capital_usd,
            "max_order_notional_usd must be in (0, max_allocated_capital_usd]"
        );
        ensure!(
            self.max_leverage > Decimal::ZERO && self.max_leverage <= Decimal::from(5u32),
            "monitored pilot max_leverage must be in (0, 5]"
        );
        let max_lev_notional = self.max_allocated_capital_usd * self.max_leverage;
        ensure!(
            self.max_aggregate_exposure_usd > Decimal::ZERO
                && self.max_aggregate_exposure_usd <= max_lev_notional,
            "max_aggregate_exposure_usd must be in (0, allocated * leverage]"
        );
        ensure!(
            self.max_daily_loss_usd > Decimal::ZERO
                && self.max_daily_loss_usd <= self.max_allocated_capital_usd,
            "max_daily_loss_usd must be in (0, max_allocated_capital_usd]"
        );
        ensure!(
            self.max_session_loss_usd > Decimal::ZERO
                && self.max_session_loss_usd <= self.max_daily_loss_usd,
            "max_session_loss_usd must be in (0, max_daily_loss_usd]"
        );
        ensure!(
            self.max_drawdown_ratio > Decimal::ZERO
                && self.max_drawdown_ratio <= Decimal::new(20, 2),
            "monitored pilot max_drawdown_ratio must be in (0, 0.20]"
        );
        ensure!(
            !self.permitted_order_types.is_empty()
                && self
                    .permitted_order_types
                    .contains(&PilotOrderType::NativeReduceOnlyStop),
            "permitted_order_types must include NativeReduceOnlyStop"
        );
        ensure!(
            (1..=86_400).contains(&self.pilot_session_duration_secs),
            "pilot_session_duration_secs must be in 1..=86400"
        );
        ensure!(
            (30..=1_800).contains(&self.operator_supervision_lease_secs),
            "operator_supervision_lease_secs must be in 30..=1800"
        );
        Ok(())
    }
}

/// Ephemeral in-memory restricted monitored pilot authorization grant.
/// Never auto-persisted for automatic restart resumption.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MonitoredPilotGrant {
    pub limits: MonitoredPilotLimits,
    pub owner_consent_statement: String,
    pub granted_at_ms: i64,
    pub session_expires_at_ms: i64,
    pub supervision_expires_at_ms: i64,
    pub evidence_digest: String,
}

impl MonitoredPilotGrant {
    pub fn session_expired(&self, now_ms: i64) -> bool {
        now_ms >= self.session_expires_at_ms
    }

    pub fn supervision_expired(&self, now_ms: i64) -> bool {
        now_ms >= self.supervision_expires_at_ms
    }

    pub fn is_active_at(&self, now_ms: i64) -> bool {
        !self.supervision_expired(now_ms) && !self.session_expired(now_ms)
    }

    pub fn renew_supervision(&mut self, now_ms: i64) -> Result<()> {
        ensure!(
            !self.session_expired(now_ms),
            "pilot session duration expired; full re-authorization required"
        );
        let lease_ms = (self.limits.operator_supervision_lease_secs as i64).saturating_mul(1_000);
        self.supervision_expires_at_ms = now_ms
            .saturating_add(lease_ms)
            .min(self.session_expires_at_ms);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MonitoredPilotEligibilityReport {
    pub eligible: bool,
    pub execution_tier: LiveExecutionTier,
    pub unattended_awakened_eligible: bool,
    pub blocking_reasons: Vec<String>,
    pub verified_pillars: Vec<u8>,
}

pub fn evaluate_monitored_pilot_eligibility(
    registry: &EvidenceRegistry,
    expected_risk_sha3: &str,
    limits: Option<&MonitoredPilotLimits>,
    treasury_authorized: bool,
    emergency_suspended: bool,
) -> MonitoredPilotEligibilityReport {
    let mut blocking_reasons = Vec::new();
    let mut verified_pillars = Vec::new();

    for p in 1..=5u8 {
        match registry.latest_valid_for_pillar(p, None, expected_risk_sha3) {
            Some(rec) if rec.tier >= VerificationTier::IntegratedVerified => {
                verified_pillars.push(p);
            }
            Some(rec) => blocking_reasons.push(format!(
                "Pillar {p} tier {:?} below IntegratedVerified",
                rec.tier
            )),
            None => blocking_reasons.push(format!(
                "Pillar {p} missing current version-bound EvidenceRecord"
            )),
        }
    }

    let valid_p6: Vec<&EvidenceRecord> = registry
        .records
        .iter()
        .filter(|r| {
            r.pillar_index == 6 && EvidenceRegistry::verify_record(r, None, expected_risk_sha3)
        })
        .collect();
    let has_historical = valid_p6
        .iter()
        .any(|r| r.tier >= VerificationTier::QualifiedStrategyVerified);
    let has_testnet = valid_p6.iter().any(|r| {
        r.environment == EvidenceEnvironment::HyperliquidTestnet
            && r.tier >= VerificationTier::RealExchangeTestVerified
    });
    let has_public_paper = valid_p6.iter().any(|r| {
        r.environment == EvidenceEnvironment::PublicPaper
            && r.tier >= VerificationTier::IntegratedVerified
    });

    if !has_historical {
        blocking_reasons.push(
            "Missing qualified historical OOS strategy evidence (`hydra strategy-accept` QualifiedStrategyVerified)"
                .into(),
        );
    }
    if !has_testnet {
        blocking_reasons.push(
            "Missing Hyperliquid Testnet functional exchange proof (RealExchangeTestVerified)"
                .into(),
        );
    }
    if !has_public_paper {
        blocking_reasons.push(
            "Missing live public-market paper lifecycle evidence (PublicPaper IntegratedVerified)"
                .into(),
        );
    }
    if has_historical && has_testnet && has_public_paper {
        verified_pillars.push(6);
    }

    match limits {
        Some(lim) => {
            if let Err(err) = lim.validate() {
                blocking_reasons.push(format!("Invalid MonitoredPilotLimits: {err}"));
            }
        }
        None => blocking_reasons.push("Missing explicit owner MonitoredPilotLimits".into()),
    }

    if !treasury_authorized {
        blocking_reasons.push("Treasury wallet not authorized with DPAPI-protected key".into());
    }
    if emergency_suspended {
        blocking_reasons.push("Engine emergency suspension is active".into());
    }

    let eligible = blocking_reasons.is_empty();
    MonitoredPilotEligibilityReport {
        eligible,
        execution_tier: if eligible {
            LiveExecutionTier::RestrictedMonitoredPilot
        } else {
            LiveExecutionTier::Dormant
        },
        unattended_awakened_eligible: false,
        blocking_reasons,
        verified_pillars,
    }
}

pub fn authorize_monitored_pilot_session(
    registry: &EvidenceRegistry,
    expected_risk_sha3: &str,
    limits: MonitoredPilotLimits,
    owner_consent_statement: &str,
    treasury_authorized: bool,
    emergency_suspended: bool,
    now_ms: i64,
) -> Result<MonitoredPilotGrant> {
    let report = evaluate_monitored_pilot_eligibility(
        registry,
        expected_risk_sha3,
        Some(&limits),
        treasury_authorized,
        emergency_suspended,
    );
    ensure!(
        report.eligible,
        "monitored pilot refused: {}",
        report.blocking_reasons.join("; ")
    );
    ensure!(
        owner_consent_statement.trim().len() >= 16,
        "explicit owner consent statement (>= 16 chars) required for monitored pilot"
    );

    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    for rec in &registry.records {
        if EvidenceRegistry::verify_record(rec, None, expected_risk_sha3) {
            hasher.update(rec.record_sha3.as_bytes());
        }
    }
    let evidence_digest = hex::encode(hasher.finalize());
    let session_ms = (limits.pilot_session_duration_secs as i64).saturating_mul(1_000);
    let lease_ms = (limits.operator_supervision_lease_secs as i64).saturating_mul(1_000);

    Ok(MonitoredPilotGrant {
        limits,
        owner_consent_statement: owner_consent_statement.trim().to_string(),
        granted_at_ms: now_ms,
        session_expires_at_ms: now_ms.saturating_add(session_ms),
        supervision_expires_at_ms: now_ms.saturating_add(lease_ms.min(session_ms)),
        evidence_digest,
    })
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
