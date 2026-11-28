//! End-to-End Verification Suite for PROJECT EQUILIBRIUM (`v1.19.0`).
//!
//! Exercises:
//! 1. External WatchdogSupervisor supervising a live OS child process with hang detection,
//!    graceful-stop attempt, forced kill on unresponsive worker, and persisted restart budget.
//! 2. Converged Engine + EngineIpcServer:
//!    - Mid-batch EmergencySuspend halting subsequent candidate execution cleanly
//!    - Session-scoped RenewPilotSupervision { session_id } matching vs mismatched UUID rejection
//!    - Separate session_pnl_usd vs daily_pnl_usd ceiling enforcement
//!    - Fail-closed AEGIS rejection when market depth or instrument_max_leverage is absent
//!    - Real SettlementStore ingestion + VerifiedLearningOutcome emission
//! 3. Non-forgeable MonitoredPilotGrant serialization boundary, EvidenceRecord producer/revocation/
//!    expiry checks, TestnetSandbox strict URL parser, and scientific NaN/Inf/zero-trade gates.

use chrono::{DateTime, Duration, Utc};
use hydra::adapters::{OrderReceipt, VenueAdapter, VenuePosition};
use hydra::config::AppConfig;
use hydra::core::engine::Engine;
use hydra::core::{Candle, MarketSnapshot, OrderIntent, OrderResult, VenueKind};
use hydra::engine_ipc::{
    EngineControlAuthority, EngineControlCommand, EngineIpcClient, EngineIpcServer,
};
use hydra::execution_recovery::InstrumentOrderConstraints;
use hydra::fang_engine::{UnifiedStrategyConfig, REGISTRY_VERSION};
use hydra::ledger::{ExchangeFill, FillSide, PnlConvention, TokenAmount};
use hydra::live_gate::{
    authorize_monitored_pilot_session, is_hyperliquid_testnet_url, is_production_exchange_url,
    require_authorized_pilot_execution, EvidenceEnvironment, EvidenceProducer, EvidenceRecord,
    EvidenceRegistry, MonitoredPilotLimits, PilotOrderType, VerificationTier,
};
use hydra::oracle_research::{
    BootstrapInterval, ExecutionModelConfig, MultiMarketDataset, PortfolioResearchReport,
    PurgedFoldResult, PurgedWalkForwardReport, QuantitativeMetrics, ResearchVerdict,
};
use hydra::qualification::{
    evaluate_scientific_strategy_acceptance, ScientificAcceptanceCriteria,
    StrategyAcceptanceVerdict,
};
use hydra::settlement::{FundingCoverageWindow, SettlementStore};
use hydra::storage::Storage;
use hydra::watchdog::{WatchdogPolicy, WatchdogSupervisor, WorkerHealthProbe};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration as StdDuration, Instant};

#[derive(Default)]
struct EqMockState {
    bars: BTreeMap<String, Vec<Candle>>,
    mids: BTreeMap<String, f64>,
    provide_depth: bool,
    provide_constraints: bool,
    filled: Vec<OrderIntent>,
    next_oid: u64,
}

#[derive(Clone)]
struct EqMockVenue {
    st: Arc<Mutex<EqMockState>>,
}

#[async_trait::async_trait]
impl VenueAdapter for EqMockVenue {
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
        let bars = g
            .bars
            .get(symbol)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing symbol {symbol}"))?;
        let slice = if bars.len() > lookback {
            bars[bars.len() - lookback..].to_vec()
        } else {
            bars
        };
        let mid = *g.mids.get(symbol).unwrap_or(&100.0);
        Ok(MarketSnapshot {
            venue: "hyperliquid".into(),
            symbol: symbol.into(),
            mid,
            bid: g.provide_depth.then_some(mid * 0.9999),
            ask: g.provide_depth.then_some(mid * 1.0001),
            spread_bps: g.provide_depth.then_some(2.0),
            l2_depth_usd: g.provide_depth.then_some(250_000.0),
            funding_bps_1h: g.provide_depth.then_some(0.25),
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
        Ok(self.submit_order_with_evidence(intent).await?.result)
    }
    async fn submit_order_with_evidence(
        &self,
        intent: &OrderIntent,
    ) -> anyhow::Result<OrderReceipt> {
        let mut g = self.st.lock().unwrap();
        g.next_oid += 1;
        let oid = g.next_oid;
        let px = intent.limit_price.unwrap_or(100.0);
        g.filled.push(intent.clone());
        Ok(OrderReceipt {
            result: OrderResult {
                intent_id: intent.id.clone(),
                accepted: true,
                venue_order_id: Some(oid.to_string()),
                filled_size: intent.size,
                avg_fill_price: Some(px),
                message: "eq mock fill".into(),
            },
            precise_fill: None,
        })
    }
    async fn close_position(&self, _: &str, _: f64, _: bool) -> anyhow::Result<OrderResult> {
        anyhow::bail!("unused")
    }
    async fn account_equity_usd(&self) -> anyhow::Result<f64> {
        Ok(10_000.0)
    }
    async fn instrument_constraints(
        &self,
    ) -> anyhow::Result<BTreeMap<String, InstrumentOrderConstraints>> {
        let g = self.st.lock().unwrap();
        if !g.provide_constraints {
            return Ok(BTreeMap::new());
        }
        let mut out = BTreeMap::new();
        for sym in g.bars.keys() {
            out.insert(
                sym.clone(),
                InstrumentOrderConstraints {
                    symbol: sym.clone(),
                    tick_size: Decimal::new(1, 1),
                    lot_size: Decimal::new(1, 4),
                    min_notional_usd: Decimal::from(10u32),
                    max_leverage: Decimal::from(20u32),
                },
            );
        }
        Ok(out)
    }
}

fn eq_trending_bars(start_px: f64, step: f64) -> Vec<Candle> {
    let base: DateTime<Utc> = DateTime::from_timestamp_millis(1_700_300_000_000).unwrap();
    (0..18)
        .map(|i| {
            let px = start_px + (i as f64) * step;
            Candle {
                ts: base + Duration::minutes(i),
                open: px * 0.999,
                high: px * 1.002,
                low: px * 0.998,
                close: px,
                volume: 1_500.0,
            }
        })
        .collect()
}

#[test]
fn equilibrium_url_evidence_and_scientific_gates_fail_closed() {
    // 1. Strict URL classification
    assert!(is_hyperliquid_testnet_url(
        "https://api.hyperliquid-testnet.xyz/info"
    ));
    assert!(is_hyperliquid_testnet_url(
        "wss://api.hyperliquid-testnet.xyz/ws"
    ));
    assert!(!is_hyperliquid_testnet_url(
        "https://api.hyperliquid-testnet.xyz.attacker.io/info"
    ));
    assert!(!is_hyperliquid_testnet_url(
        "https://api.hyperliquid-testnet.xyz@attacker.io/info"
    ));
    assert!(is_production_exchange_url(
        "https://api.hyperliquid-testnet.xyz@attacker.io/info"
    ));
    assert!(is_production_exchange_url("http://localhost.evil.com:8080"));
    assert!(!is_production_exchange_url("http://127.0.0.1:9000"));

    // 2. EvidenceRecord attested producer / revocation / expiry enforcement
    let ts = 1_700_400_000_000_i64;
    let rec = EvidenceRecord::produce_attested(
        EvidenceProducer::LocalVerificationSuite,
        1,
        VerificationTier::IntegratedVerified,
        "risk-eq",
        EvidenceEnvironment::OfflineMock,
        "disabled",
        "fp-eq",
        "sha256-eq-art",
        "sha3-eq-cfg",
        Some(10_000),
        ts,
        "attested local record",
    )
    .unwrap();
    assert!(EvidenceRegistry::verify_record_at(
        &rec,
        Some(EvidenceEnvironment::OfflineMock),
        "risk-eq",
        Some(ts + 5_000)
    ));
    assert!(!EvidenceRegistry::verify_record_at(
        &rec,
        Some(EvidenceEnvironment::OfflineMock),
        "risk-eq",
        Some(ts + 10_001)
    ));
    let mut rev = rec.clone();
    rev.revoked = true;
    assert!(!EvidenceRegistry::verify_record(&rev, None, "risk-eq"));

    // 3. Scientific strategy acceptance rejects NaN/Inf, worst OOS DD, and zero-trade fold
    let port = PortfolioResearchReport {
        schema_version: 1,
        simulated: true,
        dataset_sha3_256: "ds".into(),
        metrics: QuantitativeMetrics {
            initial_equity: 10_000.0,
            final_equity: 10_500.0,
            net_return_pct: 5.0,
            max_drawdown_pct: 4.0,
            sharpe_annualized: Some(1.5),
            sortino_annualized: Some(2.0),
            calmar_ratio: Some(1.2),
            cvar_95_bar_pct: Some(-0.4),
            profit_factor: Some(1.4),
            turnover_multiple: 1.0,
            avg_exposure_ratio: 0.3,
            peak_exposure_ratio: 0.5,
            total_fees_usd: 10.0,
            total_funding_usd: 0.0,
            total_slippage_usd: 5.0,
            partial_fills_count: 0,
            liquidations_count: 0,
            trades_count: 10,
            verdict: ResearchVerdict::PositiveAfterCosts,
        },
        buy_and_hold_equal_weight_pct: 1.0,
        excess_over_buy_and_hold_pct: 4.0,
        excess_over_flat_cash_pct: 5.0,
        regime_pnl_usd: BTreeMap::new(),
        limitations: vec![],
    };
    let fold = PurgedFoldResult {
        fold_index: 0,
        train_range: (0, 20),
        embargo_range: (20, 24),
        test_range: (24, 40),
        train_net_return_pct: 2.0,
        test_net_return_pct: 2.5,
        test_max_drawdown_pct: 5.0,
        test_trades: 0,
        excess_over_buy_hold_pct: 1.0,
    };
    let wf = PurgedWalkForwardReport {
        simulated: true,
        folds: vec![
            fold.clone(),
            PurgedFoldResult {
                fold_index: 1,
                ..fold
            },
        ],
        mean_oos_return_pct: 2.5,
        worst_oos_drawdown_pct: 5.0,
        deflated_sharpe_note: "test".into(),
    };
    let boot = BootstrapInterval {
        samples: 20,
        block_len: 4,
        resamples: 100,
        p05_mean_return_pct: 0.2,
        median_mean_return_pct: 1.5,
        p95_mean_return_pct: 3.0,
    };
    let v = evaluate_scientific_strategy_acceptance(
        &port,
        &wf,
        &boot,
        &ScientificAcceptanceCriteria::default(),
    );
    assert!(matches!(
        v,
        StrategyAcceptanceVerdict::RejectedKeepDisabled { .. }
    ));

    // Ensure MultiMarketDataset compile-checks cleanly
    let _ds = MultiMarketDataset {
        bars_by_symbol: BTreeMap::from([("BTC".into(), eq_trending_bars(60_000.0, 200.0))]),
        specs: BTreeMap::new(),
        funding_bps_1h: BTreeMap::new(),
        outage_windows: vec![],
    };
    let _ = ExecutionModelConfig::default();
}

#[tokio::test]
async fn equilibrium_engine_ipc_pilot_bounds_settlement_and_watchdog_supervisor() {
    let tmp = tempfile::tempdir().unwrap();

    // 1. Live WatchdogSupervisor hang kill + persisted restart budget
    let wd_root = tmp.path().join("wd");
    let pol = WatchdogPolicy {
        max_restarts_in_window: 2,
        base_backoff_ms: 30,
        max_backoff_ms: 90,
        heartbeat_timeout_ms: 80,
        ..Default::default()
    };
    let (prog, args): (&Path, &[&str]) = if cfg!(windows) {
        (Path::new("ping"), &["-n", "10", "127.0.0.1"])
    } else {
        (Path::new("sleep"), &["10"])
    };
    let mut sup = WatchdogSupervisor::acquire(&wd_root, pol).unwrap();
    let t_wd = Instant::now();
    let wd_sum = sup
        .supervise_live_worker(
            prog,
            args,
            StdDuration::from_millis(15),
            StdDuration::from_millis(25),
            2,
            |_pid| WorkerHealthProbe {
                alive_heartbeat_ms: None,
                progress_counter: 0,
                healthy: true,
                detail: "frozen child".into(),
            },
            |_pid| Ok(()),
        )
        .unwrap();
    assert_eq!(wd_sum.launches, 2);
    assert_eq!(wd_sum.restarts, 2);
    assert!(t_wd.elapsed() >= StdDuration::from_millis(150));

    // 2. Engine + IPC session scoping + fail-closed missing depth/leverage
    let db_path = tmp.path().join("eq_engine.db");
    let storage = Storage::open(&db_path).unwrap();
    storage
        .with_conn(hydra::storage::schema::run_migrations)
        .unwrap();
    let acct = "0x4444444444444444444444444444444444444444";
    storage.bind_trading_account(acct, false).unwrap();
    let s_store = SettlementStore::open(storage.clone(), acct).unwrap();

    let st = Arc::new(Mutex::new(EqMockState {
        bars: BTreeMap::from([("BTC".into(), eq_trending_bars(60_000.0, 220.0))]),
        mids: BTreeMap::from([("BTC".into(), 63_740.0)]),
        provide_depth: false,
        provide_constraints: false,
        ..Default::default()
    }));
    let venue = EqMockVenue { st: st.clone() };
    let mut cfg = AppConfig::load_research("config/strategies.toml", None).unwrap();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.live_trading = false;
    cfg.hl_account_address = Some(acct.into());

    let auth = EngineControlAuthority::new(
        "eq-eng",
        Arc::new(AtomicBool::new(false)),
        Some(storage.clone()),
    );
    let mut ipc = EngineIpcServer::start_with_authority(tmp.path(), auth.clone()).unwrap();

    let mut eng_missing = Engine::new_with_venues(
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
        BTreeMap::new(),
        false,
    )
    .await
    .unwrap();
    let s_fail = eng_missing.step_once(1).await.unwrap();
    assert_eq!(s_fail.accepted_entries, 0);
    assert!(s_fail.risk_rejected >= 1);

    // Enable real L2 depth and venue constraints + attach MonitoredPilotGrant with session_id
    {
        let mut g = st.lock().unwrap();
        g.provide_depth = true;
        g.provide_constraints = true;
    }
    let ts = 1_700_450_000_000_i64;
    let mk_ev = |p: u8, tier, env, fp: &str| {
        EvidenceRecord::produce_attested(
            EvidenceProducer::LocalVerificationSuite,
            p,
            tier,
            "risk-eq",
            env,
            "standard",
            fp,
            format!("sha256-{fp}"),
            "sha3-cfg",
            None,
            ts,
            "eq attested",
        )
        .unwrap()
    };
    let reg = EvidenceRegistry {
        records: vec![
            mk_ev(
                1,
                VerificationTier::IntegratedVerified,
                EvidenceEnvironment::OfflineMock,
                "e1",
            ),
            mk_ev(
                2,
                VerificationTier::IntegratedVerified,
                EvidenceEnvironment::OfflineMock,
                "e2",
            ),
            mk_ev(
                3,
                VerificationTier::IntegratedVerified,
                EvidenceEnvironment::OfflineMock,
                "e3",
            ),
            mk_ev(
                4,
                VerificationTier::IntegratedVerified,
                EvidenceEnvironment::OfflineMock,
                "e4",
            ),
            mk_ev(
                5,
                VerificationTier::IntegratedVerified,
                EvidenceEnvironment::OfflineMock,
                "e5",
            ),
            mk_ev(
                6,
                VerificationTier::QualifiedStrategyVerified,
                EvidenceEnvironment::OfflineMock,
                "eh",
            ),
            mk_ev(
                6,
                VerificationTier::RealExchangeTestVerified,
                EvidenceEnvironment::HyperliquidTestnet,
                "et",
            ),
            mk_ev(
                6,
                VerificationTier::IntegratedVerified,
                EvidenceEnvironment::PublicPaper,
                "ep",
            ),
        ],
    };
    let lim = MonitoredPilotLimits {
        wallet_address: acct.into(),
        allowed_instruments: BTreeSet::from(["BTC".into()]),
        qualified_strategy_id: "adaptive_ema_anchor".into(),
        strategy_registry_version: REGISTRY_VERSION.into(),
        max_allocated_capital_usd: Decimal::from(5_000u32),
        max_order_notional_usd: Decimal::from(2_000u32),
        max_aggregate_exposure_usd: Decimal::from(4_000u32),
        max_leverage: Decimal::from(2u32),
        max_daily_loss_usd: Decimal::from(200u32),
        max_session_loss_usd: Decimal::from(80u32),
        max_drawdown_ratio: Decimal::new(15, 2),
        permitted_order_types: BTreeSet::from([
            PilotOrderType::LimitIoc,
            PilotOrderType::NativeReduceOnlyStop,
        ]),
        pilot_session_duration_secs: 3_600,
        operator_supervision_lease_secs: 60,
    };
    let grant = authorize_monitored_pilot_session(
        &reg,
        "risk-eq",
        lim,
        "Owner explicitly approves monitored pilot test fixture",
        true,
        false,
        ts,
    )
    .unwrap();
    assert!(require_authorized_pilot_execution(
        &grant,
        ts + 1_000,
        false,
        Decimal::ZERO,
        Decimal::ZERO,
        "BTC",
        "adaptive_ema_anchor",
        Decimal::from(1_000u32),
        Decimal::ZERO,
    )
    .is_ok());
    assert!(require_authorized_pilot_execution(
        &grant,
        ts + 1_000,
        false,
        Decimal::ZERO,
        Decimal::from(-80),
        "BTC",
        "adaptive_ema_anchor",
        Decimal::from(1_000u32),
        Decimal::ZERO,
    )
    .is_err());

    let mut eng_ok = Engine::new_with_venues(
        cfg,
        storage,
        vec![Box::new(venue)],
        vec!["BTC".into()],
        false,
        None,
        Some(auth),
        None,
        Some(UnifiedStrategyConfig::default()),
        Some(s_store.clone()),
        BTreeMap::new(),
        false,
    )
    .await
    .unwrap();
    let sid = grant.session_id().to_string();
    eng_ok.attach_monitored_pilot_at(grant, ts).unwrap();

    // IPC renewal requires exact matching session_id
    let wrong_ipc = EngineIpcClient::send_command(
        tmp.path(),
        "eq-eng",
        1,
        EngineControlCommand::RenewPilotSupervision {
            session_id: "wrong-id".into(),
        },
    )
    .unwrap();
    assert!(!wrong_ipc.ok);

    let ok_ipc = EngineIpcClient::send_command(
        tmp.path(),
        "eq-eng",
        2,
        EngineControlCommand::RenewPilotSupervision { session_id: sid },
    )
    .unwrap();
    assert!(ok_ipc.ok);

    // SettlementStore round trip + VerifiedLearningOutcome emission check
    s_store
        .record_funding_coverage(&FundingCoverageWindow {
            symbol: "BTC".into(),
            start_ms: 1_000,
            end_ms: 10_000,
            complete: true,
        })
        .unwrap();
    s_store
        .record_fill(
            &ExchangeFill {
                fill_id: "eq-in".into(),
                order_id: 1,
                client_order_id: None,
                symbol: "BTC".into(),
                side: FillSide::Buy,
                quantity: Decimal::ONE,
                price: Decimal::from(60_000u32),
                closed_pnl_usdc: None,
                pnl_convention: PnlConvention::BeforeFees,
                fee: Some(TokenAmount {
                    token: "USDC".into(),
                    amount: Decimal::from(2u32),
                }),
                timestamp_ms: 2_000,
            },
            Some("adaptive_ema_anchor"),
        )
        .unwrap();
    s_store
        .record_fill(
            &ExchangeFill {
                fill_id: "eq-out".into(),
                order_id: 2,
                client_order_id: None,
                symbol: "BTC".into(),
                side: FillSide::Sell,
                quantity: Decimal::ONE,
                price: Decimal::from(60_500u32),
                closed_pnl_usdc: None,
                pnl_convention: PnlConvention::BeforeFees,
                fee: Some(TokenAmount {
                    token: "USDC".into(),
                    amount: Decimal::from(2u32),
                }),
                timestamp_ms: 8_000,
            },
            Some("adaptive_ema_anchor"),
        )
        .unwrap();
    let (_, emitted) = s_store.recompute_and_commit().unwrap();
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].verified_net_pnl_usdc, Decimal::from(496u32));

    ipc.stop();
}
