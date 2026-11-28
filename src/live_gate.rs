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

/// Returns `true` strictly for the official Hyperliquid Testnet origin.
pub fn is_hyperliquid_testnet_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url.trim()) else {
        return false;
    };
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return false;
    }
    matches!(parsed.scheme(), "https" | "wss")
        && parsed
            .host_str()
            .is_some_and(|h| h.eq_ignore_ascii_case("api.hyperliquid-testnet.xyz"))
        && parsed.port().is_none()
}

/// Returns `true` for any exchange endpoint that is not an explicit local loopback test server or Testnet sandbox.
pub fn is_production_exchange_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url.trim()) else {
        return true;
    };
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return true;
    }
    if is_hyperliquid_testnet_url(url) {
        return false;
    }
    let is_loopback_host = matches!(
        parsed.host_str(),
        Some("127.0.0.1") | Some("localhost") | Some("[::1]") | Some("::1")
    );
    !(matches!(parsed.scheme(), "http" | "ws") && is_loopback_host && parsed.port().is_some())
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceProducer {
    LocalVerificationSuite,
    HistoricalLaboratory,
    PublicPaperService,
    HyperliquidTestnetProbe,
    #[default]
    ManualUnattested,
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
    #[serde(default)]
    pub producer: EvidenceProducer,
    #[serde(default)]
    pub artifact_sha256: String,
    #[serde(default)]
    pub strategy_config_sha3: String,
    #[serde(default)]
    pub max_age_ms: Option<i64>,
    #[serde(default)]
    pub revoked: bool,
    pub record_sha3: String,
}

impl EvidenceRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn produce_attested(
        producer: EvidenceProducer,
        pillar_index: u8,
        tier: VerificationTier,
        risk_policy_sha3: impl Into<String>,
        environment: EvidenceEnvironment,
        account_mode: impl Into<String>,
        dataset_fingerprint: impl Into<String>,
        artifact_sha256: impl Into<String>,
        strategy_config_sha3: impl Into<String>,
        max_age_ms: Option<i64>,
        recorded_at_ms: i64,
        summary: impl Into<String>,
    ) -> Result<Self> {
        ensure!(
            producer != EvidenceProducer::ManualUnattested,
            "ManualUnattested producer cannot produce attested evidence"
        );
        let artifact_sha256 = artifact_sha256.into();
        let strategy_config_sha3 = strategy_config_sha3.into();
        ensure!(
            !artifact_sha256.trim().is_empty() && !strategy_config_sha3.trim().is_empty(),
            "attested evidence requires non-empty artifact_sha256 and strategy_config_sha3"
        );
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
            producer,
            artifact_sha256,
            strategy_config_sha3,
            max_age_ms,
            revoked: false,
            record_sha3: String::new(),
        };
        rec.record_sha3 = rec.compute_digest();
        Ok(rec)
    }

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
            producer: EvidenceProducer::ManualUnattested,
            artifact_sha256: String::new(),
            strategy_config_sha3: String::new(),
            max_age_ms: None,
            revoked: false,
            record_sha3: String::new(),
        };
        rec.record_sha3 = rec.compute_digest();
        rec
    }

    pub fn compute_digest(&self) -> String {
        use sha3::{Digest, Sha3_256};
        let canonical = format!(
            "v2|{}|{:?}|{}|{}|{}|{:?}|{}|{}|{}|{}|{:?}|{}|{}|{}|{}",
            self.pillar_index,
            self.tier,
            self.software_version,
            self.strategy_registry_version,
            self.risk_policy_sha3,
            self.environment,
            self.account_mode,
            self.dataset_fingerprint,
            self.recorded_at_ms,
            self.summary,
            self.producer,
            self.artifact_sha256,
            self.strategy_config_sha3,
            self.max_age_ms.unwrap_or(-1),
            self.revoked,
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
        let now_ms = record
            .max_age_ms
            .map(|_| chrono::Utc::now().timestamp_millis());
        Self::verify_record_at(record, expected_env, expected_risk_sha3, now_ms)
    }

    pub fn verify_record_at(
        record: &EvidenceRecord,
        expected_env: Option<EvidenceEnvironment>,
        expected_risk_sha3: &str,
        now_ms: Option<i64>,
    ) -> bool {
        if record.revoked || record.producer == EvidenceProducer::ManualUnattested {
            return false;
        }
        if record.artifact_sha256.trim().is_empty() || record.strategy_config_sha3.trim().is_empty()
        {
            return false;
        }
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
        if let (Some(now), Some(max_age)) = (now_ms, record.max_age_ms) {
            if now < record.recorded_at_ms || now.saturating_sub(record.recorded_at_ms) > max_age {
                return false;
            }
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

/// Execution scope a pilot grant may authorize. Only `ProductionMainnet` grants can authorize
/// real-money mainnet entries, and they can only be minted by
/// [`authorize_restricted_mainnet_pilot`] from trusted evidence + local owner consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotGrantScope {
    /// Offline/loopback fixture enforcement only (structurally cannot authorize production).
    OfflineFixture,
    /// Hyperliquid Testnet sandbox.
    TestnetSandbox,
    /// Real-money Hyperliquid mainnet restricted monitored pilot.
    ProductionMainnet,
}

/// Ephemeral in-memory restricted monitored pilot authorization grant.
/// Non-forgeable (all fields private, never auto-persisted or deserializable from disk).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MonitoredPilotGrant {
    session_id: String,
    scope: PilotGrantScope,
    limits: MonitoredPilotLimits,
    owner_consent_statement: String,
    account_address: String,
    signer_address: String,
    strategy_config_sha3: String,
    risk_policy_sha3: String,
    build_identity: String,
    granted_at_ms: i64,
    session_expires_at_ms: i64,
    supervision_expires_at_ms: i64,
    last_renewed_at_ms: i64,
    evidence_digest: String,
}

impl MonitoredPilotGrant {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn scope(&self) -> PilotGrantScope {
        self.scope
    }

    pub fn account_address(&self) -> &str {
        &self.account_address
    }

    pub fn signer_address(&self) -> &str {
        &self.signer_address
    }

    pub fn strategy_config_sha3(&self) -> &str {
        &self.strategy_config_sha3
    }

    pub fn risk_policy_sha3(&self) -> &str {
        &self.risk_policy_sha3
    }

    pub fn build_identity(&self) -> &str {
        &self.build_identity
    }

    pub fn limits(&self) -> &MonitoredPilotLimits {
        &self.limits
    }

    pub fn owner_consent_statement(&self) -> &str {
        &self.owner_consent_statement
    }

    pub fn granted_at_ms(&self) -> i64 {
        self.granted_at_ms
    }

    pub fn session_expires_at_ms(&self) -> i64 {
        self.session_expires_at_ms
    }

    pub fn supervision_expires_at_ms(&self) -> i64 {
        self.supervision_expires_at_ms
    }

    pub fn last_renewed_at_ms(&self) -> i64 {
        self.last_renewed_at_ms
    }

    pub fn evidence_digest(&self) -> &str {
        &self.evidence_digest
    }

    pub fn session_expired(&self, now_ms: i64) -> bool {
        now_ms < self.granted_at_ms || now_ms >= self.session_expires_at_ms
    }

    pub fn supervision_expired(&self, now_ms: i64) -> bool {
        now_ms < self.granted_at_ms || now_ms >= self.supervision_expires_at_ms
    }

    pub fn is_active_at(&self, now_ms: i64) -> bool {
        now_ms >= self.granted_at_ms
            && !self.supervision_expired(now_ms)
            && !self.session_expired(now_ms)
    }

    pub fn renew_supervision(&mut self, now_ms: i64) -> Result<()> {
        ensure!(
            now_ms >= self.last_renewed_at_ms,
            "monotonic clock required for pilot supervision renewal"
        );
        ensure!(
            !self.session_expired(now_ms),
            "pilot session duration expired; full re-authorization required"
        );
        let lease_ms = (self.limits.operator_supervision_lease_secs as i64).saturating_mul(1_000);
        self.last_renewed_at_ms = now_ms;
        self.supervision_expires_at_ms = now_ms
            .saturating_add(lease_ms)
            .min(self.session_expires_at_ms);
        Ok(())
    }
}

/// Centralized monitored pilot execution authorization barrier checked immediately before
/// any pilot admission or order submission.
#[allow(clippy::too_many_arguments)]
pub fn require_authorized_pilot_execution(
    grant: &MonitoredPilotGrant,
    now_ms: i64,
    emergency_suspended: bool,
    daily_pnl_usd: rust_decimal::Decimal,
    session_pnl_usd: rust_decimal::Decimal,
    symbol: &str,
    strategy_id: &str,
    requested_notional_usd: rust_decimal::Decimal,
    current_exposure_usd: rust_decimal::Decimal,
) -> Result<()> {
    ensure!(
        !emergency_suspended,
        "pilot execution blocked: emergency suspension active"
    );
    ensure!(
        grant.is_active_at(now_ms),
        "pilot execution blocked: supervision lease or session duration expired"
    );
    ensure!(
        -daily_pnl_usd < grant.limits.max_daily_loss_usd,
        "pilot execution blocked: daily loss ceiling reached"
    );
    ensure!(
        -session_pnl_usd < grant.limits.max_session_loss_usd,
        "pilot execution blocked: session loss ceiling reached"
    );
    ensure!(
        grant
            .limits
            .allowed_instruments
            .iter()
            .any(|s| s.eq_ignore_ascii_case(symbol)),
        "pilot execution blocked: symbol '{symbol}' not authorized"
    );
    ensure!(
        strategy_id == grant.limits.qualified_strategy_id,
        "pilot execution blocked: strategy '{strategy_id}' does not match qualified '{}'",
        grant.limits.qualified_strategy_id
    );
    ensure!(
        requested_notional_usd > rust_decimal::Decimal::ZERO
            && requested_notional_usd <= grant.limits.max_order_notional_usd,
        "pilot execution blocked: requested notional {requested_notional_usd} exceeds cap {}",
        grant.limits.max_order_notional_usd
    );
    ensure!(
        current_exposure_usd + requested_notional_usd <= grant.limits.max_aggregate_exposure_usd,
        "pilot execution blocked: aggregate exposure exceeds cap {}",
        grant.limits.max_aggregate_exposure_usd
    );
    Ok(())
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
        session_id: uuid::Uuid::new_v4().to_string(),
        scope: PilotGrantScope::OfflineFixture,
        account_address: limits.wallet_address.trim().to_ascii_lowercase(),
        signer_address: String::new(),
        strategy_config_sha3: String::new(),
        risk_policy_sha3: expected_risk_sha3.to_string(),
        build_identity: crate::trusted_evidence::build_identity(),
        limits,
        owner_consent_statement: owner_consent_statement.trim().to_string(),
        granted_at_ms: now_ms,
        session_expires_at_ms: now_ms.saturating_add(session_ms),
        supervision_expires_at_ms: now_ms.saturating_add(lease_ms.min(session_ms)),
        last_renewed_at_ms: now_ms,
        evidence_digest,
    })
}

/// Canonical consent text the owner must type locally (shown verbatim in the cockpit/CLI).
pub fn restricted_pilot_consent_text(limits: &MonitoredPilotLimits) -> String {
    format!(
        "I AUTHORIZE A RESTRICTED MONITORED REAL-MONEY PILOT ON {} WITH MAX CAPITAL {} USD, MAX ORDER {} USD, MAX EXPOSURE {} USD, MAX LEVERAGE {}X, MAX DAILY LOSS {} USD, MAX SESSION LOSS {} USD AND I ACCEPT TOTAL LOSS OF THIS CAPITAL",
        limits.wallet_address.trim().to_ascii_lowercase(),
        limits.max_allocated_capital_usd.normalize(),
        limits.max_order_notional_usd.normalize(),
        limits.max_aggregate_exposure_usd.normalize(),
        limits.max_leverage.normalize(),
        limits.max_daily_loss_usd.normalize(),
        limits.max_session_loss_usd.normalize(),
    )
}

/// Inputs for the single trusted real-money restricted pilot minting path.
pub struct MainnetPilotRequest<'a> {
    pub trust_store: &'a crate::trusted_evidence::TrustStore,
    pub strategy_config_sha3: &'a str,
    pub risk_policy_sha3: &'a str,
    pub limits: MonitoredPilotLimits,
    pub owner_consent_statement: &'a str,
    /// Account the running production adapter is bound to.
    pub adapter_account: &'a str,
    /// Signer address of the engine-owned `EngineSignerAuthority`.
    pub engine_signer: &'a str,
    pub treasury_profile: Option<&'a crate::treasury::TreasuryProfile>,
    pub treasury_key_present: bool,
    pub emergency_suspended: bool,
    pub now_ms: i64,
}

/// The ONLY constructor of `ProductionMainnet` grants. Deny-by-default: requires trusted
/// (MAC-authenticated, artifact-verified, build-bound, unexpired) historical OOS, public-paper and
/// Testnet execution evidence for the exact deployed strategy configuration, explicit verbatim
/// local owner consent, matching DPAPI-protected Treasury wallet/signer and adapter account,
/// validated owner ceilings and no active suspension.
pub fn authorize_restricted_mainnet_pilot(
    req: MainnetPilotRequest<'_>,
) -> Result<MonitoredPilotGrant> {
    let mut reasons: Vec<String> = Vec::new();
    if let Err(e) = req.limits.validate() {
        reasons.push(format!("invalid owner ceilings: {e}"));
    }
    if req.emergency_suspended {
        reasons.push("emergency suspension active".into());
    }
    if req.strategy_config_sha3.len() != 64 {
        reasons.push("deployed strategy configuration digest missing".into());
    }
    if req.risk_policy_sha3.trim().is_empty() {
        reasons.push("risk policy digest missing".into());
    }
    let binding = crate::trusted_evidence::DeploymentBinding::current(req.strategy_config_sha3);
    let readiness = req.trust_store.readiness(&binding, req.now_ms);
    for st in &readiness.statuses {
        if !st.satisfied {
            reasons.push(format!("{:?}: {}", st.kind, st.reason));
        }
    }
    let wallet = req.limits.wallet_address.trim().to_ascii_lowercase();
    match req.treasury_profile {
        Some(p) => {
            if !(p.wallet_authorized && p.risk_acknowledged && req.treasury_key_present) {
                reasons.push(
                    "Treasury wallet not authorized / risk not acknowledged / key absent".into(),
                );
            }
            if p.emergency_suspended {
                reasons.push("Treasury emergency suspension active".into());
            }
            if !p.protection_backend.is_encrypted_at_rest() {
                reasons.push("Treasury credential backend is not encrypted at rest".into());
            }
            if !p.dedicated_wallet_address.eq_ignore_ascii_case(&wallet) {
                reasons.push("pilot wallet does not match Treasury dedicated wallet".into());
            }
            if !p
                .derived_signer_address
                .eq_ignore_ascii_case(req.engine_signer)
            {
                reasons.push("engine signer does not match Treasury signer".into());
            }
        }
        None => reasons.push("no Treasury profile configured".into()),
    }
    if !req.adapter_account.eq_ignore_ascii_case(&wallet) {
        reasons.push("production adapter account does not match pilot wallet".into());
    }
    if req.owner_consent_statement.trim() != restricted_pilot_consent_text(&req.limits) {
        reasons.push("owner consent statement does not match the required verbatim text".into());
    }
    ensure!(
        reasons.is_empty(),
        "restricted mainnet pilot refused: {}",
        reasons.join("; ")
    );
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    for st in &readiness.statuses {
        hasher.update(st.record_id.as_deref().unwrap_or("").as_bytes());
    }
    let session_ms = (req.limits.pilot_session_duration_secs as i64).saturating_mul(1_000);
    let lease_ms = (req.limits.operator_supervision_lease_secs as i64).saturating_mul(1_000);
    Ok(MonitoredPilotGrant {
        session_id: uuid::Uuid::new_v4().to_string(),
        scope: PilotGrantScope::ProductionMainnet,
        account_address: wallet,
        signer_address: req.engine_signer.to_ascii_lowercase(),
        strategy_config_sha3: req.strategy_config_sha3.to_string(),
        risk_policy_sha3: req.risk_policy_sha3.to_string(),
        build_identity: crate::trusted_evidence::build_identity(),
        owner_consent_statement: req.owner_consent_statement.trim().to_string(),
        limits: req.limits,
        granted_at_ms: req.now_ms,
        session_expires_at_ms: req.now_ms.saturating_add(session_ms),
        supervision_expires_at_ms: req.now_ms.saturating_add(lease_ms.min(session_ms)),
        last_renewed_at_ms: req.now_ms,
        evidence_digest: hex::encode(hasher.finalize()),
    })
}

#[derive(Debug, Default)]
struct PilotAuthorityState {
    grant: Option<MonitoredPilotGrant>,
    revoked_reason: Option<String>,
    daily_pnl_usd: rust_decimal::Decimal,
    session_pnl_usd: rust_decimal::Decimal,
    current_exposure_usd: rust_decimal::Decimal,
    effective_equity_usd: Option<rust_decimal::Decimal>,
    high_water_equity_usd: Option<rust_decimal::Decimal>,
    deployed_strategy_config_sha3: Option<String>,
    deployed_risk_policy_sha3: Option<String>,
}

/// Single shared authority shared by the engine loop, the durable executor, the production
/// adapter final signing boundary and authenticated owner IPC. Revocation/suspension is
/// immediately visible to every mutation boundary (no stale per-batch authorization).
#[derive(Debug, Clone, Default)]
pub struct PilotAuthorityCell {
    inner: std::sync::Arc<std::sync::Mutex<PilotAuthorityState>>,
}

impl PilotAuthorityCell {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attach(&self, grant: MonitoredPilotGrant) {
        let mut g = self.inner.lock().unwrap();
        g.grant = Some(grant);
        g.revoked_reason = None;
    }

    pub fn revoke(&self, reason: &str) {
        let mut g = self.inner.lock().unwrap();
        if g.grant.take().is_some() || g.revoked_reason.is_none() {
            g.revoked_reason = Some(reason.to_string());
        }
    }

    pub fn snapshot(&self) -> Option<MonitoredPilotGrant> {
        self.inner.lock().unwrap().grant.clone()
    }

    pub fn active_session_id(&self) -> Option<String> {
        self.inner
            .lock()
            .unwrap()
            .grant
            .as_ref()
            .map(|g| g.session_id.clone())
    }

    pub fn revoked_reason(&self) -> Option<String> {
        self.inner.lock().unwrap().revoked_reason.clone()
    }

    pub fn update_pnl(&self, daily: rust_decimal::Decimal, session: rust_decimal::Decimal) {
        let mut g = self.inner.lock().unwrap();
        g.daily_pnl_usd = daily;
        g.session_pnl_usd = session;
    }

    pub fn update_runtime_context(
        &self,
        daily: rust_decimal::Decimal,
        session: rust_decimal::Decimal,
        current_exposure: rust_decimal::Decimal,
        effective_equity: Option<rust_decimal::Decimal>,
        high_water_equity: Option<rust_decimal::Decimal>,
    ) {
        let mut g = self.inner.lock().unwrap();
        g.daily_pnl_usd = daily;
        g.session_pnl_usd = session;
        g.current_exposure_usd = current_exposure;
        g.effective_equity_usd = effective_equity;
        g.high_water_equity_usd = high_water_equity;
    }

    pub fn set_deployed_digests(
        &self,
        strategy_config_sha3: impl Into<String>,
        risk_policy_sha3: impl Into<String>,
    ) {
        let mut g = self.inner.lock().unwrap();
        g.deployed_strategy_config_sha3 = Some(strategy_config_sha3.into());
        g.deployed_risk_policy_sha3 = Some(risk_policy_sha3.into());
    }

    /// Owner supervision renewal applied directly to the live grant; the result reflects actual
    /// acceptance (session match, unexpired session, monotonic clock).
    pub fn renew(&self, session_id: &str, now_ms: i64) -> Result<i64> {
        let mut g = self.inner.lock().unwrap();
        let grant = g
            .grant
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no active pilot grant"))?;
        ensure!(
            !session_id.trim().is_empty() && grant.session_id == session_id,
            "pilot session id mismatch"
        );
        grant.renew_supervision(now_ms)?;
        Ok(grant.supervision_expires_at_ms)
    }
}

/// Final entry (exposure-increasing) mutation context evaluated at `Utc::now()` immediately
/// before signing and again before transmission.
#[derive(Debug, Clone)]
pub struct EntryMutationContext<'a> {
    pub production: bool,
    pub account: &'a str,
    pub signer: &'a str,
    pub symbol: &'a str,
    pub strategy_id: &'a str,
    pub order_type: PilotOrderType,
    pub requested_leverage: rust_decimal::Decimal,
    pub requested_notional_usd: rust_decimal::Decimal,
    pub current_exposure_usd: rust_decimal::Decimal,
    pub emergency_suspended: bool,
}

/// Deny-by-default authority check for any exposure-increasing production mutation.
/// Non-production (Testnet/offline) transports only enforce an attached grant if present.
pub fn require_entry_mutation_authority(
    cell: Option<&PilotAuthorityCell>,
    ctx: &EntryMutationContext<'_>,
    now_ms: i64,
) -> Result<()> {
    use rust_decimal::Decimal;
    ensure!(
        !ctx.emergency_suspended,
        "entry denied: emergency suspension active"
    );
    let state = cell.map(|c| {
        let g = c.inner.lock().unwrap();
        (
            g.grant.clone(),
            g.daily_pnl_usd,
            g.session_pnl_usd,
            g.revoked_reason.clone(),
            g.current_exposure_usd,
            g.effective_equity_usd,
            g.high_water_equity_usd,
            g.deployed_strategy_config_sha3.clone(),
            g.deployed_risk_policy_sha3.clone(),
        )
    });
    let (
        grant,
        daily,
        session,
        revoked,
        cell_exposure,
        equity_opt,
        hwm_opt,
        strat_sha_opt,
        risk_sha_opt,
    ) = match state {
        Some(s) => s,
        None if ctx.production => {
            bail!("entry denied: no pilot authority attached to production path")
        }
        None => return Ok(()),
    };
    let Some(grant) = grant else {
        if ctx.production {
            bail!(
                "entry denied: no active restricted pilot grant{}",
                revoked
                    .map(|r| format!(" (revoked: {r})"))
                    .unwrap_or_default()
            );
        }
        return Ok(());
    };
    let eff_exposure = ctx.current_exposure_usd.max(cell_exposure);
    ensure!(
        grant.limits.permitted_order_types.contains(&ctx.order_type),
        "entry denied: order type {:?} not permitted by owner pilot ceilings",
        ctx.order_type
    );
    ensure!(
        ctx.requested_leverage > Decimal::ZERO
            && ctx.requested_leverage <= grant.limits.max_leverage,
        "entry denied: leverage {} exceeds owner ceiling {}",
        ctx.requested_leverage,
        grant.limits.max_leverage
    );
    let max_alloc_notional = grant.limits.max_allocated_capital_usd * grant.limits.max_leverage;
    ensure!(
        eff_exposure + ctx.requested_notional_usd <= max_alloc_notional,
        "entry denied: post-order exposure {} exceeds allocated capital * max_leverage ({max_alloc_notional})",
        eff_exposure + ctx.requested_notional_usd
    );
    if let Some(eq) = equity_opt {
        ensure!(
            eq > Decimal::ZERO,
            "entry denied: non-positive account equity"
        );
        let denom = eq.min(grant.limits.max_allocated_capital_usd);
        ensure!(
            denom > Decimal::ZERO
                && (eff_exposure + ctx.requested_notional_usd) / denom <= grant.limits.max_leverage,
            "entry denied: effective pilot leverage exceeds max_leverage {}",
            grant.limits.max_leverage
        );
        if let Some(hwm) = hwm_opt {
            if hwm > Decimal::ZERO && eq < hwm {
                let dd = (hwm - eq) / hwm;
                ensure!(
                    dd < grant.limits.max_drawdown_ratio,
                    "entry denied: drawdown {dd} reached pilot max_drawdown_ratio {}",
                    grant.limits.max_drawdown_ratio
                );
            }
        }
    }
    if ctx.production {
        ensure!(
            grant.scope == PilotGrantScope::ProductionMainnet,
            "entry denied: grant scope {:?} cannot authorize production",
            grant.scope
        );
        ensure!(
            grant.account_address.eq_ignore_ascii_case(ctx.account),
            "entry denied: grant account does not match production account"
        );
        ensure!(
            grant.signer_address.eq_ignore_ascii_case(ctx.signer),
            "entry denied: grant signer does not match engine signer"
        );
        ensure!(
            grant.build_identity == crate::trusted_evidence::build_identity(),
            "entry denied: grant bound to a different executable build"
        );
        ensure!(
            strat_sha_opt.as_deref() == Some(grant.strategy_config_sha3.as_str()),
            "entry denied: grant strategy configuration digest mismatch or unbound"
        );
        ensure!(
            risk_sha_opt.as_deref() == Some(grant.risk_policy_sha3.as_str()),
            "entry denied: grant risk policy digest mismatch or unbound"
        );
    }
    require_authorized_pilot_execution(
        &grant,
        now_ms,
        ctx.emergency_suspended,
        daily,
        session,
        ctx.symbol,
        ctx.strategy_id,
        ctx.requested_notional_usd,
        eff_exposure,
    )
}

/// Narrow risk-reducing authority (reduce-only exits, reduce-only native stops, cancellation of
/// identified protective/entry orders). Never authorizes exposure increase, reversal, transfers or
/// withdrawals. Remains available during suspension and after grant expiry/restart.
pub fn require_risk_reduction_authority(
    channel: ExecutionChannel,
    configured_live: bool,
) -> Result<()> {
    ensure!(
        configured_live,
        "[{}] risk-reducing production action requires ENABLE_LIVE_TRADING=true",
        channel.label()
    );
    let store = crate::treasury::TreasuryStore::default_store()?;
    let profile = store
        .load_profile()?
        .ok_or_else(|| anyhow::anyhow!("[{}] no Treasury profile", channel.label()))?;
    ensure!(
        profile.wallet_authorized && profile.risk_acknowledged && store.has_protected_key(),
        "[{}] Treasury signer not authorized for risk-reducing actions",
        channel.label()
    );
    Ok(())
}

/// Production live runtime construction: requires configured live trading and an authorized
/// Treasury signer, but confers NO entry authority (entries additionally require an attached
/// `ProductionMainnet` grant at every mutation boundary). Suspension still permits construction
/// so verified positions can receive protective management after a restart.
pub fn require_live_runtime_construction(req: &LiveAuthorizationRequest) -> Result<()> {
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
    require_risk_reduction_authority(req.channel, req.configured_live_trading)
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
        assert!(!is_hyperliquid_testnet_url(
            "https://api.hyperliquid-testnet.xyz.evil.com"
        ));
        assert!(is_production_exchange_url(
            "https://api.hyperliquid-testnet.xyz.evil.com"
        ));
        assert!(!is_hyperliquid_testnet_url(
            "https://api.hyperliquid-testnet.xyz@evil.com"
        ));
        assert!(is_production_exchange_url(
            "https://api.hyperliquid-testnet.xyz@evil.com"
        ));
        assert!(is_production_exchange_url("http://localhost.evil.com:8080"));
        assert!(is_hyperliquid_testnet_url(
            "https://api.hyperliquid-testnet.xyz/info"
        ));
        assert!(VenueTransportClass::ProductionMainnet.is_production());
        assert!(!VenueTransportClass::LoopbackTestFixture.is_production());
    }
}
