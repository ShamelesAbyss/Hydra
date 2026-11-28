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
    "Authoritative round-trip entry/exit fee, rebate, and funding net settlement",
    "Complete WebSocket/REST stream consistency and restart order-recovery proof",
    "Exchange-native protective stop-loss and margin-tier validation",
    "OS-scoped authenticated engine control IPC separating UI from signing keys",
    "External watchdog supervision and extended paper-trading certification evidence",
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
    #[cfg(test)]
    LoopbackTestFixture,
}

impl VenueTransportClass {
    pub const fn is_production(self) -> bool {
        match self {
            Self::ProductionMainnet => true,
            #[cfg(test)]
            Self::LoopbackTestFixture => false,
        }
    }
}

/// Returns `true` for any exchange endpoint that is not an explicit local loopback test server.
pub fn is_production_exchange_url(url: &str) -> bool {
    let normalized = url.trim().to_ascii_lowercase();
    !(normalized.starts_with("http://127.0.0.1:")
        || normalized.starts_with("http://localhost:")
        || normalized.starts_with("http://[::1]:")
        || normalized.starts_with("ws://127.0.0.1:")
        || normalized.starts_with("ws://localhost:")
        || normalized.starts_with("ws://[::1]:"))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PillarStatus {
    OfflineFixtureVerified,
    PendingExternalValidation,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PillarAssessment {
    pub index: u8,
    pub pillar: &'static str,
    pub status: PillarStatus,
    pub evidence: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CertificationChecklist {
    pub live_trading_authorized: bool,
    pub overall_status: &'static str,
    pub pillars: Vec<PillarAssessment>,
}

/// Evaluates current evidence-backed status across all 5 certification pillars.
/// Real-money execution stays strictly locked (`live_trading_authorized: false`) until
/// Pillar 5 external watchdog supervision and multi-day qualification finish & certify.
pub fn evaluate_certification_readiness() -> CertificationChecklist {
    CertificationChecklist {
        live_trading_authorized: false,
        overall_status: "PendingCertification",
        pillars: vec![
            PillarAssessment {
                index: 1,
                pillar: "Authoritative net financial settlement",
                status: PillarStatus::OfflineFixtureVerified,
                evidence: "src/settlement.rs (FIFO partial fills, fees/rebates, position funding, external cash flow separation, exactly-once learning)",
            },
            PillarAssessment {
                index: 2,
                pillar: "Exchange and restart order recovery",
                status: PillarStatus::OfflineFixtureVerified,
                evidence: "src/execution_recovery.rs (RestWsConsistency, partial fills, cloid dedup, blind-retry prohibition, ownership reconciliation)",
            },
            PillarAssessment {
                index: 3,
                pillar: "Native protective orders and sovereign risk",
                status: PillarStatus::OfflineFixtureVerified,
                evidence: "src/protective_orders.rs & src/aegis_sovereign.rs (reduce-only stops under blocked entries, fail-closed sovereign risk)",
            },
            PillarAssessment {
                index: 4,
                pillar: "Authenticated engine IPC and credential isolation",
                status: PillarStatus::OfflineFixtureVerified,
                evidence: "src/engine_ipc.rs & src/treasury.rs (token/nonce/freshness IPC, DPAPI encryption requirement, UiReadOnly scope denial)",
            },
            PillarAssessment {
                index: 5,
                pillar: "External watchdog supervision and fault-injection resilience",
                status: PillarStatus::OfflineFixtureVerified,
                evidence: "src/watchdog.rs & src/qualification.rs (OS-locked supervisor, PRAGMA quick_check, disk budget, 10-scenario fault-injection battery)",
            },
            PillarAssessment {
                index: 6,
                pillar: "Extended multi-week public paper qualification and owner release-signing keys",
                status: PillarStatus::PendingExternalValidation,
                evidence: "READINESS_V2.md: requires real multi-week elapsed public-market soak evidence & pinned PINNED_RELEASE_SIGNING_PUBKEY_HEX before live pilot",
            },
        ],
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
