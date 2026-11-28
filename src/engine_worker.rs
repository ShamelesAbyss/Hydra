//! Shared headless Engine Worker runner (`--engine-worker`, A08).
//!
//! Used by both `hydra-cockpit --engine-worker` (self-contained standalone Windows binary) and
//! `hydra engine-worker` CLI supervision. Never opens a graphical window and never auto-resumes
//! live entries after a crash or restart.

use crate::adapters::{VenueAdapter, VenuePosition};
use crate::config::AppConfig;
use crate::core::engine::Engine;
use crate::core::{Candle, MarketSnapshot, OrderIntent, OrderResult, VenueKind};
use crate::engine_ipc::{EngineControlAuthority, EngineIpcServer};
use crate::execution_recovery::InstrumentOrderConstraints;
use crate::fang_engine::UnifiedStrategyConfig;
use crate::storage::Storage;
use anyhow::Result;
use chrono::{Duration as ChronoDuration, Utc};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[derive(Debug, Clone)]
pub struct EngineWorkerConfig {
    pub root: PathBuf,
    pub db_path: PathBuf,
    pub engine_id: String,
    pub symbols: Vec<String>,
    pub poll_interval_ms: u64,
    pub max_ticks: Option<u64>,
    pub strategy_config: Option<PathBuf>,
    /// Strictly non-production read-only fixture venue for offline watchdog/IPC drills.
    pub offline_fixture: bool,
}

/// Non-production read-only observation venue (never places orders, no network).
struct OfflineObservationVenue;

#[async_trait::async_trait]
impl VenueAdapter for OfflineObservationVenue {
    fn kind(&self) -> VenueKind {
        VenueKind::Hyperliquid
    }
    fn is_production_venue(&self) -> bool {
        false
    }
    async fn ping(&self) -> Result<()> {
        Ok(())
    }
    async fn snapshot(&self, symbol: &str, _lookback: usize) -> Result<MarketSnapshot> {
        let now = Utc::now();
        let candles: Vec<Candle> = (0..18)
            .map(|i| {
                let px = 50_000.0 + (i as f64) * 5.0;
                Candle {
                    ts: now - ChronoDuration::minutes(18 - i),
                    open: px,
                    high: px * 1.001,
                    low: px * 0.999,
                    close: px,
                    volume: 100.0,
                }
            })
            .collect();
        Ok(MarketSnapshot {
            venue: "hyperliquid".into(),
            symbol: symbol.into(),
            mid: 50_085.0,
            bid: Some(50_080.0),
            ask: Some(50_090.0),
            spread_bps: Some(2.0),
            l2_depth_usd: Some(50_000.0),
            funding_bps_1h: Some(0.5),
            last_update: now,
            observed_exchange_ts_ms: Some(now.timestamp_millis()),
            candles,
            microstructure: None,
        })
    }
    async fn open_positions(&self) -> Result<Vec<VenuePosition>> {
        Ok(Vec::new())
    }
    async fn submit_order(&self, intent: &OrderIntent) -> Result<OrderResult> {
        Ok(OrderResult {
            intent_id: intent.id.clone(),
            accepted: false,
            venue_order_id: None,
            filled_size: 0.0,
            avg_fill_price: None,
            message: "offline observation fixture never submits orders".into(),
        })
    }
    async fn close_position(
        &self,
        _symbol: &str,
        _size: f64,
        _side_is_buy_to_close: bool,
    ) -> Result<OrderResult> {
        anyhow::bail!("offline observation fixture never closes positions")
    }
    async fn account_equity_usd(&self) -> Result<f64> {
        Ok(10_000.0)
    }
    async fn instrument_constraints(&self) -> Result<BTreeMap<String, InstrumentOrderConstraints>> {
        Ok(BTreeMap::new())
    }
}

pub async fn run_headless_engine_worker(
    worker_cfg: EngineWorkerConfig,
    stop: Arc<AtomicBool>,
) -> Result<u64> {
    std::fs::create_dir_all(&worker_cfg.root)?;
    let storage = Storage::open(&worker_cfg.db_path)?;
    storage.migrate()?;

    let mut cfg = AppConfig::load_research("config/strategies.toml", None)
        .or_else(|_| AppConfig::from_embedded_defaults(&worker_cfg.root))?;
    // Fail-closed: headless worker starts in OBSERVATION with live_trading disabled.
    cfg.live_trading = false;
    cfg.poll_interval_ms = worker_cfg.poll_interval_ms.clamp(200, 30_000);
    cfg.data_dir = worker_cfg.root.clone();
    cfg.db_path = worker_cfg.db_path.clone();

    let u_cfg = match worker_cfg.strategy_config.as_ref() {
        Some(p) => Some(UnifiedStrategyConfig::load_artifact(p)?),
        None => None,
    };

    let authority =
        EngineControlAuthority::new(&worker_cfg.engine_id, stop.clone(), Some(storage.clone()));
    {
        let mut st = authority.state.lock().unwrap();
        if !st.emergency_suspended {
            st.mode = "OBSERVATION".into();
        }
    }
    authority.persist_state();

    let mut ipc_server =
        EngineIpcServer::start_with_authority(&worker_cfg.root, authority.clone())?;

    let venues: Vec<Box<dyn VenueAdapter>> = if worker_cfg.offline_fixture {
        vec![Box::new(OfflineObservationVenue)]
    } else {
        vec![crate::adapters::make_venue(VenueKind::Hyperliquid, &cfg).await?]
    };
    let symbols = if worker_cfg.symbols.is_empty() {
        vec!["BTC".to_string(), "ETH".to_string()]
    } else {
        worker_cfg.symbols.clone()
    };

    let mut engine = Engine::new_with_venues(
        cfg,
        storage,
        venues,
        symbols,
        false,
        None,
        Some(authority),
        None,
        u_cfg,
        None,
        BTreeMap::new(),
        false,
    )
    .await?;

    let poll_ms = worker_cfg.poll_interval_ms.clamp(200, 30_000);
    let mut tick: u64 = 0;
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        tick = tick.saturating_add(1);
        let _ = engine.step_once(tick).await;
        if let Some(max_t) = worker_cfg.max_ticks {
            if tick >= max_t {
                break;
            }
        }
        let waited_step = Duration::from_millis(50);
        let target = Duration::from_millis(poll_ms);
        let mut elapsed = Duration::ZERO;
        while elapsed < target && !stop.load(Ordering::SeqCst) {
            sleep(waited_step).await;
            elapsed += waited_step;
        }
    }
    ipc_server.stop();
    let _ = Decimal::ZERO;
    Ok(tick)
}
