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
    }
}
