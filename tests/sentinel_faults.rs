//! Project Sentinel (`v1.17.0`) Intensive End-to-End Fault Validation Suite.
//!
//! Exercises the real converged `Engine`, `EngineIpcServer`/`EngineControlAuthority`,
//! `WatchdogSupervisor`, `ResourceBudgetScheduler`, `genesis_nsga2`, and version-bound
//! `EvidenceRegistry` under realistic live-process, network, exchange, persistence,
//! IPC-attack, disk-budget, contention, and evidence-tamper faults.

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use hydra::adapters::account_evidence::AccountExposureEvidence;
use hydra::adapters::{OrderReceipt, VenueAdapter, VenuePosition};
use hydra::aegis::AccountSnapshot;
use hydra::config::AppConfig;
use hydra::core::engine::Engine;
use hydra::core::{Candle, MarketSnapshot, OrderIntent, OrderResult, Side, VenueKind};
use hydra::engine_ipc::{
    AuthenticatedEngineRequest, EngineControlAuthority, EngineControlCommand, EngineIpcClient,
    EngineIpcServer,
};
use hydra::execution_recovery::{
    ExchangeOrderSnapshot, ExecutionJournalRecord, InstrumentOrderConstraints, OrderLifecycleState,
};
use hydra::fang_engine::UnifiedStrategyConfig;
use hydra::genesis_nsga2::{run_nsga2_campaign, Nsga2Options};
use hydra::live_gate::{
    evaluate_certification_readiness_with_registry, EvidenceEnvironment, EvidenceRecord,
    EvidenceRegistry, PillarStatus, VerificationTier,
};
use hydra::oracle_research::MultiMarketDataset;
use hydra::protective_orders::ProtectiveOrderSpec;
use hydra::qualification::{ResourceBudgetScheduler, WorkloadPriority};
use hydra::settlement::SettlementStore;
use hydra::storage::Storage;
use hydra::watchdog::{
    measure_host_resources, run_watchdog_health_check, WatchdogPolicy, WatchdogSupervisor,
    WorkerHealthProbe,
};
use rusqlite::Connection;
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct FaultVenueState {
    candles: Vec<Candle>,
    mid: f64,
    spread_bps: Option<f64>,
    equity_usd: f64,
    next_oid: u64,
    positions: Vec<VenuePosition>,
    open_orders: Vec<ExchangeOrderSnapshot>,
    stops: Vec<ProtectiveOrderSpec>,
    filled_intents: Vec<OrderIntent>,
    fail_snapshot: bool,
    fail_place_stop: bool,
}

#[derive(Clone)]
struct FaultMockVenue {
    st: Arc<Mutex<FaultVenueState>>,
}

#[async_trait]
impl VenueAdapter for FaultMockVenue {
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
        anyhow::ensure!(
            !g.fail_snapshot,
            "injected network/exchange snapshot outage"
        );
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
            last_update: Utc::now(),
            candles: slice,
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
                message: "fault mock fill".into(),
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
            account: "0x2222222222222222222222222222222222222222".into(),
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
        let mut g = self.st.lock().unwrap();
        anyhow::ensure!(
            !g.fail_place_stop,
            "injected protective stop placement failure"
        );
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
}

fn make_bars(count: usize, slope: f64) -> Vec<Candle> {
    let base: DateTime<Utc> = DateTime::from_timestamp_millis(1_700_100_000_000).unwrap();
    (0..count)
        .map(|i| {
            let px = 60_000.0 + (i as f64) * slope;
            Candle {
                ts: base + ChronoDuration::hours(i as i64),
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
fn sentinel_live_watchdog_hang_kill_backoff_persisted_budget_and_disk_db_faults() {
    let dir = tempfile::tempdir().unwrap();
    let pol = WatchdogPolicy {
        max_restarts_in_window: 2,
        base_backoff_ms: 40,
        max_backoff_ms: 120,
        heartbeat_timeout_ms: 100,
        ..Default::default()
    };

    let (prog, args): (&Path, &[&str]) = if cfg!(windows) {
        (Path::new("ping"), &["-n", "10", "127.0.0.1"])
    } else {
        (Path::new("sleep"), &["10"])
    };

    let mut sup = WatchdogSupervisor::acquire(dir.path(), pol.clone()).unwrap();
    let grace_called = Arc::new(AtomicBool::new(false));
    let grace_ref = grace_called.clone();
    let t0 = Instant::now();
    let sum = sup
        .supervise_live_worker(
            prog,
            args,
            Duration::from_millis(15),
            Duration::from_millis(25),
            2,
            |_pid| WorkerHealthProbe {
                alive_heartbeat_ms: None,
                progress_counter: 0,
                healthy: true,
                detail: "simulated frozen worker".into(),
            },
            move |_pid| {
                grace_ref.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .unwrap();
    assert!(grace_called.load(Ordering::SeqCst));
    assert_eq!(sum.launches, 2);
    assert_eq!(sum.restarts, 2);
    assert!(!sum.escalated);
    assert!(
        t0.elapsed() >= Duration::from_millis(200),
        "expected real timeout + backoff elapsed, got {:?}",
        t0.elapsed()
    );
    drop(sup);

    // Restart budget survives WatchdogSupervisor re-instantiation and escalates cleanly on next stall
    let mut sup2 = WatchdogSupervisor::acquire(dir.path(), pol).unwrap();
    assert_eq!(sup2.tracker().total_restarts, 2);
    let sum2 = sup2
        .supervise_live_worker(
            prog,
            args,
            Duration::from_millis(15),
            Duration::from_millis(20),
            2,
            |_pid| WorkerHealthProbe {
                alive_heartbeat_ms: None,
                progress_counter: 0,
                healthy: false,
                detail: "stalled heartbeat".into(),
            },
            |_pid| Ok(()),
        )
        .unwrap();
    assert!(sum2.escalated);
    let alerts = std::fs::read_to_string(sup2.alerts_path()).unwrap();
    assert!(alerts.contains("worker_hang_detected"));
    assert!(alerts.contains("worker_hang_terminated"));
    assert!(alerts.contains("restart_budget_exhausted"));
    drop(sup2);

    // Interrupted persistence (corrupt SQLite DB) -> quick_check fails & simulation resumption blocked
    let corrupt_db = dir.path().join("corrupt.db");
    std::fs::write(&corrupt_db, b"corrupted-sentinel-wal-bytes").unwrap();
    let (bad_db, _, cl_bad) = run_watchdog_health_check(
        &dir.path().join("wd-bad-db"),
        &corrupt_db,
        &WatchdogPolicy::default(),
    )
    .unwrap();
    assert!(!bad_db.quick_check_ok);
    assert!(!cl_bad.can_resume_simulation);
    assert!(cl_bad.live_auto_resume_prohibited);

    // Simulated disk exhaustion (WITHOUT filling real disk) blocks resumption
    let good_db = dir.path().join("good.db");
    let conn = Connection::open(&good_db).unwrap();
    conn.execute_batch("CREATE TABLE meta_kv(key TEXT PRIMARY KEY, value TEXT NOT NULL);")
        .unwrap();
    drop(conn);
    let low_disk_pol = WatchdogPolicy {
        min_free_disk_bytes: u64::MAX / 2,
        ..Default::default()
    };
    let (_, disk_rep, cl_disk) =
        run_watchdog_health_check(&dir.path().join("wd-disk"), &good_db, &low_disk_pol).unwrap();
    assert!(!disk_rep.sufficient);
    assert!(!cl_disk.can_resume_simulation);
    assert!(cl_disk.live_auto_resume_prohibited);

    let host = measure_host_resources(dir.path(), &good_db);
    assert!(host.logical_cores >= 1);
    assert!(host.available_disk_bytes.is_some());
}

#[tokio::test]
async fn sentinel_converged_engine_faults_network_stop_failure_ipc_attack_and_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("sentinel_engine.db");
    let bars = make_bars(18, 220.0);
    let mid = bars.last().unwrap().close;

    let st = Arc::new(Mutex::new(FaultVenueState {
        candles: bars,
        mid,
        spread_bps: Some(2.0),
        equity_usd: 10_000.0,
        next_oid: 500,
        fail_snapshot: false,
        fail_place_stop: true, // Inject protective-stop placement failure on first tick!
        ..Default::default()
    }));
    let venue = FaultMockVenue { st: st.clone() };

    let mut cfg = AppConfig::load_research("config/strategies.toml", None).unwrap();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.live_trading = true;
    cfg.hl_account_address = Some("0x2222222222222222222222222222222222222222".into());
    cfg.risk.max_open_exposure_pct = 50.0;
    cfg.risk.max_notional_per_market_usd = 2_000.0;
    cfg.risk.max_slippage_bps = 30.0;

    let storage = Storage::open(&db_path).unwrap();
    storage
        .with_conn(hydra::storage::schema::run_migrations)
        .unwrap();
    storage
        .bind_trading_account("0x2222222222222222222222222222222222222222", false)
        .unwrap();
    let s_store = SettlementStore::open(
        storage.clone(),
        "0x2222222222222222222222222222222222222222",
    )
    .unwrap();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let auth = EngineControlAuthority::new("sentinel-eng", stop_flag, Some(storage.clone()));
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

    // Fault A: fill succeeds while protective stop placement fails -> engine marks protective_stops_verified=false and refuses new entries on subsequent ticks
    let s1 = engine.step_once(1).await.unwrap();
    assert_eq!(s1.accepted_entries, 1);
    assert!(engine.halted_unprotected());
    assert!(!auth.snapshot().protective_stops_verified);

    // Fault B: network / exchange outage during snapshot -> step degrades safely (0 snapshots/entries while still reconciling protection)
    st.lock().unwrap().fail_snapshot = true;
    let s2 = engine.step_once(2).await.unwrap();
    assert_eq!(s2.planned_candidates, 0);
    assert_eq!(s2.accepted_entries, 0);
    assert!(engine.halted_unprotected());

    // Recover snapshot & stop placement -> orphan stops cleaned, protection verified again
    {
        let mut g = st.lock().unwrap();
        g.fail_snapshot = false;
        g.fail_place_stop = false;
        g.stops.push(ProtectiveOrderSpec {
            cloid: "orphan-sol".into(),
            exchange_oid: Some(8888),
            symbol: "SOL".into(),
            owner_strategy: "old".into(),
            position_side: Side::Long,
            protective_side: Side::Short,
            qty: Decimal::ONE,
            trigger_price: Decimal::from(90u32),
            reduce_only: true,
        });
    }

    // Fault C: IPC attack (bad token + replay nonce rejected fail-closed), followed by valid EmergencySuspend
    let bad = EngineIpcClient::send_raw(
        ipc_srv.descriptor.port,
        &AuthenticatedEngineRequest {
            token_hex: "forged-token".into(),
            nonce: 1,
            issued_at_ms: Utc::now().timestamp_millis(),
            command: EngineControlCommand::Heartbeat,
        },
    )
    .unwrap();
    assert!(!bad.ok);
    assert!(bad.emergency_suspended);
    assert!(!bad.protective_stops_verified);

    let ok_hb = EngineIpcClient::send_command(
        tmp.path(),
        "sentinel-eng",
        10,
        EngineControlCommand::Heartbeat,
    )
    .unwrap();
    assert!(ok_hb.ok);
    let replay = EngineIpcClient::send_command(
        tmp.path(),
        "sentinel-eng",
        10,
        EngineControlCommand::GetState,
    )
    .unwrap();
    assert!(!replay.ok);

    let susp = EngineIpcClient::send_command(
        tmp.path(),
        "sentinel-eng",
        11,
        EngineControlCommand::EmergencySuspend {
            reason: "sentinel fault drill".into(),
        },
    )
    .unwrap();
    assert!(susp.ok && susp.emergency_suspended);

    // While suspended: entries remain blocked, YET authorized protective management still cleans orphan SOL & places BTC stop!
    let s3 = engine.step_once(3).await.unwrap();
    assert_eq!(s3.accepted_entries, 0);
    {
        let g = st.lock().unwrap();
        assert!(!g.stops.iter().any(|s| s.symbol == "SOL"));
        assert!(g.stops.iter().any(|s| s.symbol == "BTC" && s.reduce_only));
    }

    // Fault D: Ambiguous Submitted intent injected across restart holds entries conservatively
    let count_before = st.lock().unwrap().filled_intents.len();
    let mut engine_restarted = Engine::new_with_venues(
        cfg,
        storage,
        vec![Box::new(venue)],
        vec!["BTC".into()],
        false,
        None,
        Some(auth),
        None,
        Some(UnifiedStrategyConfig::default()),
        Some(s_store),
        constraints,
        false,
    )
    .await
    .unwrap();
    engine_restarted.inject_journal_record_for_recovery(ExecutionJournalRecord {
        cloid: "ambig-sentinel".into(),
        exchange_oid: None,
        symbol: "ETH".into(),
        side: Side::Long,
        owner_strategy: "adaptive_ema_anchor".into(),
        requested_qty: Decimal::ONE,
        limit_price: None,
        filled_qty: Decimal::ZERO,
        state: OrderLifecycleState::Submitted,
        submitted_at_ms: Utc::now().timestamp_millis(),
    });
    let rec = engine_restarted
        .reconcile_startup_and_recovery()
        .await
        .unwrap();
    assert_eq!(rec.unresolved_holds, 1);
    let s4 = engine_restarted.step_once(4).await.unwrap();
    assert_eq!(s4.accepted_entries, 0);
    assert_eq!(st.lock().unwrap().filled_intents.len(), count_before);

    ipc_srv.stop();
}

#[test]
fn sentinel_research_contention_and_evidence_registry_fail_closed() {
    // Research scheduler reserves >= 1 ProtectiveCritical slot even under max_workers saturation
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .max(2);
    let mut sched = ResourceBudgetScheduler::new(cores, 1).unwrap();
    while sched.try_acquire(WorkloadPriority::ResearchBackground) {}
    assert!(
        sched.try_acquire(WorkloadPriority::ProtectiveCritical),
        "ProtectiveCritical must never be starved by ResearchBackground"
    );

    let bars = make_bars(64, 50.0);
    let ds = MultiMarketDataset {
        bars_by_symbol: BTreeMap::from([("BTC".into(), bars)]),
        specs: BTreeMap::new(),
        funding_bps_1h: BTreeMap::new(),
        outage_windows: vec![],
    };
    let rep = run_nsga2_campaign(
        &ds,
        40,
        2,
        &Nsga2Options {
            population: 4,
            generations: 1,
            max_workers: 64,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    assert!(!rep.pareto_front.is_empty());

    // Version-bound EvidenceRegistry fail-closed behavior
    let empty_check = evaluate_certification_readiness_with_registry(None, "risk-v1");
    assert!(!empty_check.live_trading_authorized);
    assert!(empty_check
        .pillars
        .iter()
        .all(|p| p.status == PillarStatus::PendingExternalValidation));

    let valid_rec = EvidenceRecord::signed_new(
        1,
        VerificationTier::IntegratedVerified,
        "risk-v1",
        EvidenceEnvironment::OfflineMock,
        "disabled",
        "fp-123",
        1_700_000_000_000,
        "settlement store verified",
    );
    assert!(EvidenceRegistry::verify_record(
        &valid_rec,
        Some(EvidenceEnvironment::OfflineMock),
        "risk-v1"
    ));

    // Tampered version, risk hash, environment, or SHA-3 fails closed immediately
    let mut bad_ver = valid_rec.clone();
    bad_ver.software_version = "0.0.1".into();
    assert!(!EvidenceRegistry::verify_record(&bad_ver, None, "risk-v1"));

    let mut bad_digest = valid_rec.clone();
    bad_digest.summary = "tampered summary".into();
    assert!(!EvidenceRegistry::verify_record(
        &bad_digest,
        None,
        "risk-v1"
    ));

    assert!(!EvidenceRegistry::verify_record(
        &valid_rec,
        Some(EvidenceEnvironment::HyperliquidTestnet),
        "risk-v1"
    ));
    assert!(!EvidenceRegistry::verify_record(
        &valid_rec,
        None,
        "wrong-risk"
    ));

    let reg = EvidenceRegistry {
        records: vec![valid_rec],
    };
    let check = evaluate_certification_readiness_with_registry(Some(&reg), "risk-v1");
    assert_eq!(check.pillars[0].status, PillarStatus::EvidenceVerified);
    assert_eq!(check.pillars[0].tier, VerificationTier::IntegratedVerified);
    assert_eq!(
        check.pillars[5].status,
        PillarStatus::PendingExternalValidation
    );
    assert!(!check.live_trading_authorized);
}
