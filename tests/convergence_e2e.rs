//! Full Project Convergence (`v1.16.0`) Real-Engine End-to-End Lifecycle & Denial Suite.
//!
//! Drives the ACTUAL `Engine` (`src/core/engine.rs`) against a stateful mock exchange adapter:
//! observation -> shared FANG plan -> AEGIS sovereign admission -> precision constraints ->
//! durable intent -> fill ack -> native reduce-only protective stop -> durable SQLite settlement
//! -> at-most-once learning -> hard crash & restart recovery -> authenticated IPC EmergencySuspend
//! (blocking entries while keeping orphan stop cleanup active) -> fail-closed missing-input denials
//! and ORACLE/GENESIS durable research events & fingerprint validation.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use hydra::adapters::account_evidence::AccountExposureEvidence;
use hydra::adapters::{OrderReceipt, VenueAdapter, VenuePosition};
use hydra::aegis::AccountSnapshot;
use hydra::config::AppConfig;
use hydra::core::engine::Engine;
use hydra::core::{Candle, MarketSnapshot, OrderIntent, OrderResult, Side, VenueKind};
use hydra::engine_ipc::{
    EngineControlAuthority, EngineControlCommand, EngineIpcClient, EngineIpcServer,
};
use hydra::execution_recovery::{
    ExchangeOrderSnapshot, ExecutionJournalRecord, InstrumentOrderConstraints, OrderLifecycleState,
};
use hydra::fang_engine::UnifiedStrategyConfig;
use hydra::genesis_nsga2::{persist_genesis_research_event, run_nsga2_campaign, Nsga2Options};
use hydra::ledger::{ExchangeFill, FillSide, FundingPayment, PnlConvention, TokenAmount};
use hydra::oracle_research::{
    persist_oracle_research_event, simulate_portfolio, ExecutionModelConfig, MultiMarketDataset,
};
use hydra::protective_orders::ProtectiveOrderSpec;
use hydra::settlement::{FundingCoverageWindow, SettlementStatus, SettlementStore};
use hydra::storage::Storage;
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct MockVenueState {
    candles: Vec<Candle>,
    mid: f64,
    spread_bps: Option<f64>,
    equity_usd: f64,
    next_oid: u64,
    positions: Vec<VenuePosition>,
    open_orders: Vec<ExchangeOrderSnapshot>,
    stops: Vec<ProtectiveOrderSpec>,
    filled_intents: Vec<OrderIntent>,
}

#[derive(Clone)]
struct StatefulMockVenue {
    st: Arc<Mutex<MockVenueState>>,
}

#[async_trait]
impl VenueAdapter for StatefulMockVenue {
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
        let g = self.st.lock().unwrap();
        let len = g.candles.len();
        let slice = if len > lookback {
            g.candles[len - lookback..].to_vec()
        } else {
            g.candles.clone()
        };
        Ok(MarketSnapshot {
            venue: "hyperliquid".into(),
            symbol: symbol.into(),
            mid: g.mid,
            bid: g.spread_bps.map(|_| g.mid * 0.9999),
            ask: g.spread_bps.map(|_| g.mid * 1.0001),
            spread_bps: g.spread_bps,
            l2_depth_usd: g.spread_bps.map(|_| 500_000.0),
            funding_bps_1h: g.spread_bps.map(|_| 0.0),
            last_update: Utc::now(),
            observed_exchange_ts_ms: None,
            candles: slice,
            microstructure: None,
        })
    }

    async fn open_positions(&self) -> anyhow::Result<Vec<VenuePosition>> {
        Ok(self.st.lock().unwrap().positions.clone())
    }

    async fn submit_order(&self, intent: &OrderIntent) -> anyhow::Result<OrderResult> {
        Ok(self.submit_order_with_evidence(intent).await?.result)
    }

    async fn submit_order_with_evidence(
        &self,
        intent: &OrderIntent,
    ) -> anyhow::Result<OrderReceipt> {
        let mut g = self.st.lock().unwrap();
        g.next_oid += 1;
        let oid = g.next_oid;
        let fill_px = intent.limit_price.unwrap_or(g.mid);
        if !intent.reduce_only {
            let signed_sz = if intent.side.is_buy() {
                intent.size
            } else {
                -intent.size
            };
            g.positions.push(VenuePosition {
                symbol: intent.symbol.clone(),
                size: signed_sz,
                entry_price: fill_px,
                unrealized_pnl: 0.0,
                leverage: 1.0,
            });
        }
        g.filled_intents.push(intent.clone());
        Ok(OrderReceipt {
            result: OrderResult {
                intent_id: intent.id.clone(),
                accepted: true,
                venue_order_id: Some(oid.to_string()),
                filled_size: intent.size,
                avg_fill_price: Some(fill_px),
                message: "mock filled".into(),
            },
            precise_fill: None,
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
        Ok(self.st.lock().unwrap().equity_usd)
    }

    async fn account_exposure_evidence(&self) -> anyhow::Result<AccountExposureEvidence> {
        let g = self.st.lock().unwrap();
        let mut ex_by_mkt = BTreeMap::new();
        for p in &g.positions {
            if let Ok(ntl) = Decimal::try_from((p.size.abs() * p.entry_price).max(0.0)) {
                *ex_by_mkt.entry(p.symbol.clone()).or_insert(Decimal::ZERO) += ntl;
            }
        }
        let eq = Decimal::try_from(g.equity_usd).unwrap_or(Decimal::from(10_000u32));
        let now_ms = Utc::now().timestamp_millis();
        Ok(AccountExposureEvidence {
            account: "0x1111111111111111111111111111111111111111".into(),
            venue: "hyperliquid".into(),
            environment: "mainnet".into(),
            perp_dex: String::new(),
            account_mode: "disabled".into(),
            acquired_started_ms: now_ms,
            acquired_finished_ms: now_ms,
            exchange_time_ms: now_ms,
            snapshot: AccountSnapshot {
                revision: now_ms,
                observed_at_ms: now_ms,
                equity: eq,
                exposure_by_market: ex_by_mkt,
            },
            positions: Vec::new(),
            open_orders: Vec::new(),
            withdrawable_collateral: None,
            total_margin_used: None,
        })
    }

    async fn open_exchange_orders(&self) -> anyhow::Result<Vec<ExchangeOrderSnapshot>> {
        Ok(self.st.lock().unwrap().open_orders.clone())
    }

    async fn list_protective_stops(&self) -> anyhow::Result<Vec<ProtectiveOrderSpec>> {
        Ok(self.st.lock().unwrap().stops.clone())
    }

    async fn place_protective_stop(
        &self,
        spec: &ProtectiveOrderSpec,
    ) -> anyhow::Result<ProtectiveOrderSpec> {
        anyhow::ensure!(spec.reduce_only, "stop must be reduce_only");
        let mut g = self.st.lock().unwrap();
        g.next_oid += 1;
        let mut placed = spec.clone();
        placed.exchange_oid = Some(g.next_oid);
        g.stops.push(placed.clone());
        Ok(placed)
    }

    async fn cancel_protective_stop(
        &self,
        symbol: &str,
        exchange_oid: Option<u64>,
        cloid: &str,
    ) -> anyhow::Result<()> {
        let mut g = self.st.lock().unwrap();
        g.stops.retain(|s| {
            !(s.symbol.eq_ignore_ascii_case(symbol)
                && (exchange_oid.is_some() && s.exchange_oid == exchange_oid || s.cloid == cloid))
        });
        Ok(())
    }

    async fn instrument_constraints(
        &self,
    ) -> anyhow::Result<BTreeMap<String, InstrumentOrderConstraints>> {
        Ok(BTreeMap::from([(
            "BTC".into(),
            InstrumentOrderConstraints {
                symbol: "BTC".into(),
                tick_size: Decimal::new(1, 1),
                lot_size: Decimal::new(1, 4),
                min_notional_usd: Decimal::from(10u32),
                max_leverage: Decimal::from(20u32),
            },
        )]))
    }
}

fn trending_btc_candles() -> Vec<Candle> {
    let base_ts: DateTime<Utc> = DateTime::from_timestamp_millis(1_700_050_000_000).unwrap();
    (0..18)
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
        .collect()
}

#[tokio::test]
async fn full_converged_engine_lifecycle_settlement_crash_recovery_and_authenticated_suspend() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("converged_engine.db");
    let bars = trending_btc_candles();
    let mid = bars.last().unwrap().close;

    let mock_state = Arc::new(Mutex::new(MockVenueState {
        candles: bars.clone(),
        mid,
        spread_bps: Some(2.0),
        equity_usd: 10_000.0,
        next_oid: 100,
        ..Default::default()
    }));
    let venue = StatefulMockVenue {
        st: mock_state.clone(),
    };

    let mut cfg = AppConfig::load_research("config/strategies.toml", None).unwrap();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.live_trading = true;
    cfg.hl_account_address = Some("0x1111111111111111111111111111111111111111".into());
    cfg.risk.max_open_exposure_pct = 50.0;
    cfg.risk.max_notional_per_market_usd = 2_000.0;
    cfg.risk.max_slippage_bps = 30.0;

    let storage = Storage::open(&db_path).unwrap();
    storage
        .with_conn(hydra::storage::schema::run_migrations)
        .unwrap();
    storage
        .bind_trading_account("0x1111111111111111111111111111111111111111", false)
        .unwrap();
    let s_store = SettlementStore::open(
        storage.clone(),
        "0x1111111111111111111111111111111111111111",
    )
    .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let auth = EngineControlAuthority::new("conv-engine", stop.clone(), Some(storage.clone()));
    let mut ipc_srv = EngineIpcServer::start_with_authority(tmp.path(), auth.clone()).unwrap();

    let constraints = BTreeMap::from([(
        "BTC".into(),
        InstrumentOrderConstraints {
            symbol: "BTC".into(),
            tick_size: Decimal::new(1, 1),
            lot_size: Decimal::new(1, 4),
            min_notional_usd: Decimal::from(10u32),
            max_leverage: Decimal::from(20u32),
        },
    )]);

    let mut engine = Engine::new_with_venues(
        cfg.clone(),
        storage.clone(),
        vec![Box::new(venue.clone())],
        vec!["BTC".into()],
        false,
        None,
        Some(auth.clone()),
        None,
        Some(UnifiedStrategyConfig::default()),
        Some(s_store.clone()),
        constraints.clone(),
        false,
    )
    .await
    .unwrap();

    // Tick 1: shared FANG plan -> AEGIS sovereign admission -> rounded execution -> fill -> native stop!
    let s1 = engine.step_once(1).await.unwrap();
    assert_eq!(s1.planned_candidates, 1);
    assert_eq!(s1.accepted_entries, 1);
    assert!(!engine.halted_unprotected());
    {
        let g = mock_state.lock().unwrap();
        assert_eq!(g.filled_intents.len(), 1);
        assert_eq!(g.stops.len(), 1);
        assert!(g.stops[0].reduce_only);
        assert_eq!(g.stops[0].protective_side, Side::Short);
    }

    // Durable financial settlement & at-most-once learning on real Storage handle
    let filled_qty = mock_state.lock().unwrap().filled_intents[0].size;
    let qty_dec = Decimal::try_from(filled_qty).unwrap();
    let px_dec = Decimal::try_from(mid).unwrap();
    s_store
        .record_funding_coverage(&FundingCoverageWindow {
            symbol: "BTC".into(),
            start_ms: 1_000,
            end_ms: 20_000,
            complete: true,
        })
        .unwrap();
    s_store
        .record_fill(
            &ExchangeFill {
                fill_id: "conv-in-1".into(),
                order_id: 101,
                client_order_id: None,
                symbol: "BTC".into(),
                side: FillSide::Buy,
                quantity: qty_dec,
                price: px_dec,
                closed_pnl_usdc: None,
                pnl_convention: PnlConvention::BeforeFees,
                fee: Some(TokenAmount {
                    token: "USDC".into(),
                    amount: Decimal::from(2u32),
                }),
                timestamp_ms: 5_000,
            },
            Some("adaptive_ema_anchor"),
        )
        .unwrap();
    s_store
        .record_funding(&FundingPayment {
            funding_id: "conv-fund-1".into(),
            symbol: "BTC".into(),
            payment: Some(TokenAmount {
                token: "USDC".into(),
                amount: Decimal::ONE,
            }),
            timestamp_ms: 10_000,
        })
        .unwrap();
    s_store
        .record_fill(
            &ExchangeFill {
                fill_id: "conv-out-1".into(),
                order_id: 102,
                client_order_id: None,
                symbol: "BTC".into(),
                side: FillSide::Sell,
                quantity: qty_dec,
                price: px_dec + Decimal::from(500u32),
                closed_pnl_usdc: None,
                pnl_convention: PnlConvention::BeforeFees,
                fee: Some(TokenAmount {
                    token: "USDC".into(),
                    amount: Decimal::from(2u32),
                }),
                timestamp_ms: 15_000,
            },
            None,
        )
        .unwrap();
    let (rts, learn1) = s_store.recompute_and_commit().unwrap();
    assert_eq!(rts.len(), 1);
    assert_eq!(rts[0].status, SettlementStatus::VerifiedSettled);
    assert_eq!(learn1.len(), 1);
    let (_rts2, learn2) = s_store.recompute_and_commit().unwrap();
    assert!(learn2.is_empty());

    // Simulate process crash & restart on same SQLite DB + ambiguous unacknowledged submission
    drop(engine);
    let storage2 = Storage::open(&db_path).unwrap();
    let mut engine2 = Engine::new_with_venues(
        cfg,
        storage2,
        vec![Box::new(venue)],
        vec!["BTC".into()],
        false,
        None,
        Some(auth.clone()),
        None,
        Some(UnifiedStrategyConfig::default()),
        None,
        constraints,
        false,
    )
    .await
    .unwrap();

    engine2.inject_journal_record_for_recovery(ExecutionJournalRecord {
        cloid: "ambiguous-eth-submission".into(),
        exchange_oid: None,
        symbol: "ETH".into(),
        side: Side::Long,
        owner_strategy: "adaptive_ema_anchor".into(),
        requested_qty: Decimal::ONE,
        limit_price: None,
        filled_qty: Decimal::ZERO,
        state: OrderLifecycleState::Submitted,
        submitted_at_ms: 10_000,
    });
    let rec = engine2.reconcile_startup_and_recovery().await.unwrap();
    assert_eq!(rec.unresolved_holds, 1);
    assert!(engine2.halted_unresolved());

    let s2 = engine2.step_once(2).await.unwrap();
    assert!(s2.halts_blocked_entries);
    assert_eq!(s2.accepted_entries, 0);

    // Trigger authenticated IPC EmergencySuspend -> add orphan SOL stop -> engine MUST clean orphan while SUSPENDED!
    let susp = EngineIpcClient::send_command(
        tmp.path(),
        "conv-engine",
        1,
        EngineControlCommand::EmergencySuspend {
            reason: "convergence e2e suspend".into(),
        },
    )
    .unwrap();
    assert!(susp.ok && susp.emergency_suspended);
    assert_eq!(susp.mode, "SUSPENDED");
    {
        let mut g = mock_state.lock().unwrap();
        g.stops.push(ProtectiveOrderSpec {
            symbol: "SOL".into(),
            owner_strategy: "old".into(),
            position_side: Side::Long,
            protective_side: Side::Short,
            qty: Decimal::from(5u32),
            trigger_price: Decimal::from(100u32),
            reduce_only: true,
            cloid: "orphan-sol".into(),
            exchange_oid: Some(999),
        });
    }

    let s3 = engine2.step_once(3).await.unwrap();
    assert!(s3.halts_blocked_entries);
    assert_eq!(s3.accepted_entries, 0);
    // Orphan SOL stop removed cleanly even under emergency suspension while BTC stop remains intact
    {
        let g = mock_state.lock().unwrap();
        assert!(!g.stops.iter().any(|s| s.symbol == "SOL"));
        assert!(g.stops.iter().any(|s| s.symbol == "BTC" && s.reduce_only));
    }

    let shut = EngineIpcClient::send_command(
        tmp.path(),
        "conv-engine",
        2,
        EngineControlCommand::GracefulShutdown,
    )
    .unwrap();
    assert!(shut.ok);
    assert!(engine2.stop_flag().load(Ordering::SeqCst));
    ipc_srv.stop();
}

#[tokio::test]
async fn denial_paths_missing_sovereign_inputs_and_oracle_genesis_fingerprinting_durability() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("denial_and_research.db");
    let bars = trending_btc_candles();
    let mid = bars.last().unwrap().close;

    // Missing orderbook depth (`spread_bps: None`) must FAIL CLOSED in AEGIS Sovereign Risk!
    let mock_state = Arc::new(Mutex::new(MockVenueState {
        candles: bars.clone(),
        mid,
        spread_bps: None,
        equity_usd: 10_000.0,
        next_oid: 1,
        ..Default::default()
    }));
    let venue = StatefulMockVenue { st: mock_state };

    let mut cfg = AppConfig::load_research("config/strategies.toml", None).unwrap();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.live_trading = true;
    cfg.hl_account_address = Some("0x1111111111111111111111111111111111111111".into());
    let storage = Storage::open(&db_path).unwrap();
    storage
        .with_conn(hydra::storage::schema::run_migrations)
        .unwrap();
    storage
        .bind_trading_account("0x1111111111111111111111111111111111111111", false)
        .unwrap();

    let mut engine = Engine::new_with_venues(
        cfg,
        storage.clone(),
        vec![Box::new(venue)],
        vec!["BTC".into()],
        false,
        None,
        None,
        None,
        None,
        None,
        BTreeMap::new(),
        false,
    )
    .await
    .unwrap();

    let step = engine.step_once(1).await.unwrap();
    assert_eq!(step.accepted_entries, 0);
    assert!(step.risk_rejected >= 1);

    // Durable ORACLE & GENESIS research events & strict experiment fingerprint rejection
    let mut long_bars = Vec::new();
    let t0 = 1_700_000_000_000_i64;
    let mut px = 50_000.0;
    for i in 0..70i64 {
        px *= if i % 6 == 0 { 0.997 } else { 1.003 };
        long_bars.push(Candle {
            ts: chrono::DateTime::from_timestamp_millis(t0 + i * 3_600_000).unwrap(),
            open: px * 0.999,
            high: px * 1.005,
            low: px * 0.995,
            close: px,
            volume: 900.0,
        });
    }
    let ds = MultiMarketDataset {
        bars_by_symbol: BTreeMap::from([("BTC".into(), long_bars)]),
        specs: BTreeMap::new(),
        funding_bps_1h: BTreeMap::new(),
        outage_windows: vec![],
    };
    let u_cfg = UnifiedStrategyConfig::default();
    let ex_cfg = ExecutionModelConfig::default();
    let fp = ds.experiment_fingerprint(&u_cfg, &ex_cfg).unwrap();
    let port_rep = simulate_portfolio(&ds, &u_cfg, &ex_cfg).unwrap();
    persist_oracle_research_event(&storage, "exp-conv-1", "default", &fp, &port_rep).unwrap();

    let opts = Nsga2Options {
        population: 4,
        generations: 1,
        max_workers: 2,
        ..Default::default()
    };
    let gen_rep = run_nsga2_campaign(&ds, 40, 4, &opts, None).unwrap();
    assert!(!gen_rep.checkpoint.experiment_fingerprint.is_empty());
    persist_genesis_research_event(&storage, "exp-conv-1", &gen_rep).unwrap();

    let events = storage.list_research_events("exp-conv-1").unwrap();
    assert_eq!(events.len(), 2);
    assert!(events
        .iter()
        .all(|e| e.activation_state.contains("non_executable")));

    // Mutating optimizer seed invalidates checkpoint experiment_fingerprint on resume
    let bad_opts = Nsga2Options {
        seed: 999,
        generations: 2,
        ..opts
    };
    assert!(run_nsga2_campaign(&ds, 40, 4, &bad_opts, Some(&gen_rep.checkpoint)).is_err());

    // Missing `instrument_max_leverage` (no synthetic 20x fallback) MUST also fail closed in AEGIS!
    let mock_no_lev = Arc::new(Mutex::new(MockVenueState {
        candles: trending_btc_candles(),
        mid: 64_000.0,
        spread_bps: Some(2.0),
        equity_usd: 10_000.0,
        next_oid: 1,
        ..Default::default()
    }));
    struct NoConstraintsVenue(Arc<Mutex<MockVenueState>>);
    #[async_trait::async_trait]
    impl VenueAdapter for NoConstraintsVenue {
        fn kind(&self) -> hydra::core::VenueKind {
            hydra::core::VenueKind::Hyperliquid
        }
        fn is_production_venue(&self) -> bool {
            false
        }
        async fn ping(&self) -> anyhow::Result<()> {
            Ok(())
        }
        async fn snapshot(&self, symbol: &str, lookback: usize) -> anyhow::Result<MarketSnapshot> {
            StatefulMockVenue { st: self.0.clone() }
                .snapshot(symbol, lookback)
                .await
        }
        async fn open_positions(&self) -> anyhow::Result<Vec<VenuePosition>> {
            Ok(vec![])
        }
        async fn submit_order(&self, intent: &OrderIntent) -> anyhow::Result<OrderResult> {
            StatefulMockVenue { st: self.0.clone() }
                .submit_order(intent)
                .await
        }
        async fn close_position(&self, _: &str, _: f64, _: bool) -> anyhow::Result<OrderResult> {
            anyhow::bail!("unused")
        }
        async fn account_equity_usd(&self) -> anyhow::Result<f64> {
            Ok(10_000.0)
        }
    }
    let tmp_nolev = tempfile::tempdir().unwrap();
    let mut cfg_nl = AppConfig::load_research("config/strategies.toml", None).unwrap();
    cfg_nl.data_dir = tmp_nolev.path().to_path_buf();
    cfg_nl.live_trading = false;
    let db_nl = Storage::open(tmp_nolev.path().join("nolev.db")).unwrap();
    db_nl
        .with_conn(hydra::storage::schema::run_migrations)
        .unwrap();
    let mut eng_nl = Engine::new_with_venues(
        cfg_nl,
        db_nl,
        vec![Box::new(NoConstraintsVenue(mock_no_lev.clone()))],
        vec!["BTC".into()],
        false,
        None,
        None,
        None,
        None,
        None,
        BTreeMap::new(),
        false,
    )
    .await
    .unwrap();
    let nl_step = eng_nl.step_once(1).await.unwrap();
    assert_eq!(nl_step.accepted_entries, 0);
    assert!(nl_step.risk_rejected >= 1);
    assert!(mock_no_lev.lock().unwrap().filled_intents.is_empty());
}
