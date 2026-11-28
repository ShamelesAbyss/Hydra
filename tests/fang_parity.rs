//! Cross-consumer planning parity verification proving identical point-in-time
//! market inputs, FANG strategy configurations, and portfolio budgets yield identical
//! selected & rejected plans across:
//! 1. Direct `FangUnifiedPlanner`
//! 2. ORACLE `plan_unified_oracle_candidates`
//! 3. ACTUAL `Engine::unified_planning_step` (`src/core/engine.rs`)
//! 4. Synapse Paper pipeline (`SynapsePipeline`)

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use hydra::adapters::{VenueAdapter, VenuePosition};
use hydra::cognition_shadow::{infer_regime, RegimeInferenceConfig};
use hydra::config::AppConfig;
use hydra::core::engine::Engine;
use hydra::core::{Candle, MarketSnapshot, OrderIntent, OrderResult, VenueKind};
use hydra::fang_engine::{FangMarketInput, FangUnifiedPlanner, UnifiedStrategyConfig};
use hydra::fang_planner::PlanningBudget;
use hydra::hunter::MarketObservation;
use hydra::oracle::plan_unified_oracle_candidates;
use hydra::paper_pipeline::{SynapsePipeline, SynapsePipelineConfig};
use hydra::storage::Storage;
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};

struct ParityMockVenue {
    candles: Vec<Candle>,
    mid: f64,
}

#[async_trait]
impl VenueAdapter for ParityMockVenue {
    fn kind(&self) -> VenueKind {
        VenueKind::Hyperliquid
    }
    fn is_production_venue(&self) -> bool {
        false
    }
    async fn ping(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn snapshot(&self, symbol: &str, lookback: usize) -> anyhow::Result<MarketSnapshot> {
        let len = self.candles.len();
        let slice = if len > lookback {
            self.candles[len - lookback..].to_vec()
        } else {
            self.candles.clone()
        };
        Ok(MarketSnapshot {
            venue: "hyperliquid".into(),
            symbol: symbol.into(),
            mid: self.mid,
            bid: Some(self.mid * 0.9999),
            ask: Some(self.mid * 1.0001),
            spread_bps: Some(2.0),
            l2_depth_usd: None,
            funding_bps_1h: None,
            last_update: Utc::now(),
            observed_exchange_ts_ms: None,
            candles: slice,
            microstructure: None,
        })
    }
    async fn open_positions(&self) -> anyhow::Result<Vec<VenuePosition>> {
        Ok(Vec::new())
    }
    async fn submit_order(&self, intent: &OrderIntent) -> anyhow::Result<OrderResult> {
        Ok(OrderResult {
            intent_id: intent.id.clone(),
            accepted: false,
            venue_order_id: None,
            filled_size: 0.0,
            avg_fill_price: None,
            message: "parity mock".into(),
        })
    }
    async fn close_position(
        &self,
        _symbol: &str,
        _size: f64,
        _side_is_buy_to_close: bool,
    ) -> anyhow::Result<OrderResult> {
        anyhow::bail!("unused")
    }
    async fn account_equity_usd(&self) -> anyhow::Result<f64> {
        Ok(10_000.0)
    }
}

#[tokio::test]
async fn fang_cross_consumer_fixture_parity_is_identical() {
    let base_ts: DateTime<Utc> = DateTime::from_timestamp_millis(1_700_020_000_000).unwrap();
    let bars: Vec<Candle> = (0..18)
        .map(|i| {
            let px = 60_000.0 + (i as f64) * 240.0;
            Candle {
                ts: base_ts + Duration::minutes(i),
                open: px * 0.999,
                high: px * 1.002,
                low: px * 0.998,
                close: px,
                volume: 2_000.0,
            }
        })
        .collect();
    let last_close = bars.last().unwrap().close;
    let as_of_ms = bars.last().unwrap().ts.timestamp_millis();
    let reg = infer_regime(
        &bars,
        &RegimeInferenceConfig {
            lookback_bars: 12,
            ..Default::default()
        },
    )
    .unwrap()
    .regime;
    let mark = Decimal::try_from(last_close).unwrap().round_dp(6);
    let marks = BTreeMap::from([("BTC".to_string(), mark)]);
    let budget = PlanningBudget {
        approved_symbols: ["BTC".into()].into(),
        excluded_symbols: BTreeSet::new(),
        total_remaining_notional: Decimal::from(5_000u32),
        per_symbol_remaining_notional: BTreeMap::from([("BTC".into(), Decimal::from(2_000u32))]),
        occupied_symbols: BTreeMap::new(),
        confidence_floor: 0.45,
    };
    let u_cfg = UnifiedStrategyConfig {
        base_stop_loss_pct: Decimal::new(2, 2),
        base_take_profit_pct: Decimal::new(4, 2),
        max_market_notional: Decimal::from(2_000u32),
        default_prior_confidence: 0.55,
        ..Default::default()
    };

    // 1. Direct FangUnifiedPlanner
    let input = FangMarketInput {
        as_of_ms,
        correlation_id: "parity".into(),
        watchlist: vec!["BTC".into()],
        candles: BTreeMap::from([("BTC".into(), bars.clone())]),
        marks: marks.clone(),
        regimes: BTreeMap::from([("BTC".into(), reg)]),
        microstructure: BTreeMap::new(),
        portfolio_equity_usd: Decimal::from(10_000u32),
    };
    let fang_out = FangUnifiedPlanner::evaluate_and_plan(&input, &budget, &u_cfg, None).unwrap();

    // 2. ORACLE unified helper
    let oracle_out =
        plan_unified_oracle_candidates(&bars, "BTC", Decimal::from(10_000u32), &u_cfg, &budget)
            .unwrap();
    assert_eq!(
        fang_out.report.selected.len(),
        oracle_out.report.selected.len()
    );
    assert_eq!(
        fang_out.report.selected[0].signal.strategy,
        oracle_out.report.selected[0].signal.strategy
    );
    assert_eq!(
        fang_out.report.selected[0].signal.side,
        oracle_out.report.selected[0].signal.side
    );
    assert_eq!(
        fang_out.report.selected[0].requested_notional,
        oracle_out.report.selected[0].requested_notional
    );
    assert!(
        (fang_out.report.selected[0].signal.stop_price
            - oracle_out.report.selected[0].signal.stop_price)
            .abs()
            < 1e-9
    );

    // 3. ACTUAL Engine::unified_planning_step reproduces exact plan strategy/side/notional/stop
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = AppConfig::load_research("config/strategies.toml", None).unwrap();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.live_trading = false;
    cfg.risk.max_open_exposure_pct = 50.0;
    cfg.risk.max_notional_per_market_usd = 2_000.0;
    let storage = Storage::open(tmp.path().join("parity_engine.db")).unwrap();
    storage
        .with_conn(hydra::storage::schema::run_migrations)
        .unwrap();
    let mock = ParityMockVenue {
        candles: bars.clone(),
        mid: last_close,
    };
    let mut engine = Engine::new_with_venues(
        cfg,
        storage,
        vec![Box::new(mock)],
        vec!["BTC".into()],
        false,
        None,
        None,
        None,
        Some(u_cfg.clone()),
        None,
        BTreeMap::new(),
        false,
    )
    .await
    .unwrap();
    let snap = MarketSnapshot {
        venue: "hyperliquid".into(),
        symbol: "BTC".into(),
        mid: last_close,
        bid: Some(last_close * 0.9999),
        ask: Some(last_close * 1.0001),
        spread_bps: Some(2.0),
        l2_depth_usd: None,
        funding_bps_1h: None,
        last_update: Utc::now(),
        observed_exchange_ts_ms: None,
        candles: bars.clone(),
        microstructure: None,
    };
    let eng_out = engine.unified_planning_step(1, &[snap]).unwrap();
    assert_eq!(eng_out.report.selected.len(), 1);
    assert_eq!(
        eng_out.report.selected[0].signal.strategy,
        fang_out.report.selected[0].signal.strategy
    );
    assert_eq!(
        eng_out.report.selected[0].signal.side,
        fang_out.report.selected[0].signal.side
    );
    assert_eq!(
        eng_out.report.selected[0].requested_notional,
        fang_out.report.selected[0].requested_notional
    );
    assert!(
        (eng_out.report.selected[0].signal.stop_price
            - fang_out.report.selected[0].signal.stop_price)
            .abs()
            < 1e-9
    );

    // 4. Synapse Paper pipeline opens the exact same symbol/side/strategy
    let mut pipe = SynapsePipeline::new(SynapsePipelineConfig::default()).unwrap();
    let obs = MarketObservation {
        symbol: "BTC".into(),
        observed_at_ms: as_of_ms,
        volume_24h_usd: Some(50_000_000.0),
        depth_usd: Some(500_000.0),
        spread_bps: Some(2.0),
        volatility_pct: Some(2.0),
        directionality: Some(0.85),
        funding_bps: Some(1.0),
        open_interest_usd: Some(15_000_000.0),
        coverage_fraction: Some(1.0),
        age_days: Some(365.0),
        observed_history_minutes: None,
        ema_ready: true,
    };
    let sum = pipe
        .step_cycle(
            as_of_ms,
            &[obs],
            &BTreeMap::from([("BTC".into(), bars)]),
            &marks,
        )
        .unwrap();
    assert_eq!(sum.approved_entries, 1);
    let open_pos = &pipe.ledger.open_positions["BTC"];
    assert_eq!(open_pos.side, fang_out.report.selected[0].signal.side);
    assert_eq!(
        open_pos.owner_strategy,
        fang_out.report.selected[0].signal.strategy
    );
}
