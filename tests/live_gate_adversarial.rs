//! Adversarial regression tests verifying that no execution entry point can
//! bypass the authoritative `PendingCertification` gate for real-money trading.

use async_trait::async_trait;
use chrono::Utc;
use hydra::adapters::{hyperliquid::HyperliquidAdapter, VenueAdapter, VenuePosition};
use hydra::config::AppConfig;
use hydra::core::{MarketSnapshot, OrderIntent, OrderResult, Side, VenueKind};
use hydra::execution::ExecutionEngine;
use hydra::live_gate::{
    require_certified_live_execution, ExecutionChannel, LiveAuthorizationRequest,
    VenueTransportClass,
};
use hydra::storage::Storage;
use hydra::treasury::{GateState, ReadinessReport};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct ProductionEndpointStub {
    submitted: Arc<AtomicBool>,
}

#[async_trait]
impl VenueAdapter for ProductionEndpointStub {
    fn kind(&self) -> VenueKind {
        VenueKind::Hyperliquid
    }

    fn is_production_venue(&self) -> bool {
        true
    }

    async fn ping(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn snapshot(&self, symbol: &str, _lookback: usize) -> anyhow::Result<MarketSnapshot> {
        Ok(MarketSnapshot {
            venue: "hyperliquid".into(),
            symbol: symbol.into(),
            mid: 65_000.0,
            bid: Some(64_995.0),
            ask: Some(65_005.0),
            spread_bps: Some(1.5),
            l2_depth_usd: None,
            funding_bps_1h: None,
            last_update: Utc::now(),
            observed_exchange_ts_ms: None,
            candles: vec![],
            microstructure: None,
        })
    }

    async fn open_positions(&self) -> anyhow::Result<Vec<VenuePosition>> {
        Ok(vec![])
    }

    async fn submit_order(&self, _intent: &OrderIntent) -> anyhow::Result<OrderResult> {
        self.submitted.store(true, Ordering::SeqCst);
        panic!("production submit_order must never be reached while PendingCertification");
    }

    async fn close_position(
        &self,
        _symbol: &str,
        _size: f64,
        _side_is_buy_to_close: bool,
    ) -> anyhow::Result<OrderResult> {
        self.submitted.store(true, Ordering::SeqCst);
        panic!("production close_position must never be reached while PendingCertification");
    }

    async fn account_equity_usd(&self) -> anyhow::Result<f64> {
        Ok(10_000.0)
    }
}

fn sample_intent() -> OrderIntent {
    OrderIntent {
        id: uuid::Uuid::new_v4().to_string(),
        venue: "hyperliquid".into(),
        symbol: "BTC".into(),
        side: Side::Long,
        size: 0.01,
        limit_price: Some(65_000.0),
        reduce_only: false,
        stop_price: None,
        take_profit: None,
        source_signal: None,
        created_at: Utc::now(),
    }
}

#[test]
fn gate_blocks_every_channel_even_if_all_caller_flags_are_set() {
    for channel in [
        ExecutionChannel::ProductionEngine,
        ExecutionChannel::CliRun,
        ExecutionChannel::StateFlatten,
        ExecutionChannel::VenueAdapterDirect,
        ExecutionChannel::CockpitControl,
        ExecutionChannel::IpcControl,
    ] {
        let req = LiveAuthorizationRequest {
            channel,
            configured_live_trading: true,
            cli_live_requested: true,
            treasury_authorized: true,
            emergency_suspended: false,
        };
        let err = require_certified_live_execution(&req)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("PendingCertification"),
            "channel {channel:?} escaped with {err}"
        );
    }

    let report = ReadinessReport::evaluate(None, true, None, None);
    assert!(!report.can_awaken);
    assert!(report
        .checks
        .iter()
        .any(|c| c.state == GateState::PendingCertification));
}

#[tokio::test]
async fn live_adapter_constructor_and_engine_and_execution_all_refuse_without_network() {
    let mut cfg = AppConfig::load("config/strategies.toml", None).unwrap();
    cfg.live_trading = true;
    let err = match HyperliquidAdapter::new(&cfg).await {
        Ok(_) => panic!("HyperliquidAdapter::new with live_trading=true must fail closed"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("PendingCertification"), "got: {err}");

    let tmp = tempfile::tempdir().unwrap();
    let storage = Storage::open(tmp.path().join("gate-test.db")).unwrap();
    storage.migrate().unwrap();

    let err = match hydra::core::engine::Engine::new(
        cfg.clone(),
        storage.clone(),
        hydra::cli::Venue::Hyperliquid,
        vec!["BTC".into()],
        true,
        None,
    )
    .await
    {
        Ok(_) => panic!("Engine::new with live_trading=true must fail closed"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("PendingCertification"), "got: {err}");

    let submitted = Arc::new(AtomicBool::new(false));
    let venue = ProductionEndpointStub {
        submitted: submitted.clone(),
    };
    let mut exec = ExecutionEngine::new(storage, true);
    let err = exec
        .submit(&sample_intent(), &venue)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("PendingCertification"), "got: {err}");
    assert!(!submitted.load(Ordering::SeqCst));
    assert!(VenueTransportClass::ProductionMainnet.is_production());
}
