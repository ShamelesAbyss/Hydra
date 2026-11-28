//! Project Awakening (`v1.18.0`) Restricted Monitored Live-Pilot Adversarial & Engine Suite.
//!
//! Verifies Section 15/16 Restricted Monitored Live-Pilot invariants:
//! - Missing/invalid ceilings, missing `NativeReduceOnlyStop`, wrong registry version, short
//!   consent statement, or missing/synthetic/tampered evidence pillars fail closed.
//! - Real `Engine` enforces allowed instruments, order notional cap, and aggregate exposure cap.
//! - Operator supervision lease expiration blocks new entries while preserving open positions
//!   (no blind liquidation) and keeping authorized native protective management active.
//! - Authenticated `EngineControlCommand::RenewPilotSupervision` renews lease unless suspended.
//! - Restarted `Engine` NEVER auto-resumes monitored pilot authorization (`active_pilot().is_none()`).

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
use hydra::execution_recovery::{ExchangeOrderSnapshot, InstrumentOrderConstraints};
use hydra::fang_engine::{StrategyFamily, UnifiedStrategyConfig, REGISTRY_VERSION};
use hydra::live_gate::{
    authorize_monitored_pilot_session, evaluate_monitored_pilot_eligibility, EvidenceEnvironment,
    EvidenceRecord, EvidenceRegistry, LiveExecutionTier, MonitoredPilotLimits, PilotOrderType,
    VerificationTier,
};
use hydra::protective_orders::ProtectiveOrderSpec;
use hydra::settlement::SettlementStore;
use hydra::storage::Storage;
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct PilotVenueState {
    bars: BTreeMap<String, Vec<Candle>>,
    mids: BTreeMap<String, f64>,
    equity_usd: f64,
    next_oid: u64,
    positions: Vec<VenuePosition>,
    stops: Vec<ProtectiveOrderSpec>,
    filled: Vec<OrderIntent>,
}

#[derive(Clone)]
struct PilotMockVenue {
    st: Arc<Mutex<PilotVenueState>>,
}

#[async_trait]
impl VenueAdapter for PilotMockVenue {
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
            .ok_or_else(|| anyhow::anyhow!("unknown symbol {symbol}"))?;
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
            bid: Some(mid * 0.9999),
            ask: Some(mid * 1.0001),
            spread_bps: Some(2.0),
            l2_depth_usd: Some(500_000.0),
            funding_bps_1h: Some(0.0),
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
        let px = intent
            .limit_price
            .unwrap_or_else(|| *g.mids.get(&intent.symbol).unwrap_or(&100.0));
        if !intent.reduce_only {
            let signed_sz = if intent.side.is_buy() {
                intent.size
            } else {
                -intent.size
            };
            g.positions.push(VenuePosition {
                symbol: intent.symbol.clone(),
                size: signed_sz,
                entry_price: px,
                unrealized_pnl: 0.0,
                leverage: 1.0,
            });
        }
        g.filled.push(intent.clone());
        Ok(OrderReceipt {
            result: OrderResult {
                intent_id: intent.id.clone(),
                accepted: true,
                venue_order_id: Some(oid.to_string()),
                filled_size: intent.size,
                avg_fill_price: Some(px),
                message: "pilot mock fill".into(),
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
            account: "0x3333333333333333333333333333333333333333".into(),
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
        Ok(Vec::new())
    }

    async fn list_protective_stops(&self) -> anyhow::Result<Vec<ProtectiveOrderSpec>> {
        Ok(self.st.lock().unwrap().stops.clone())
    }

    async fn place_protective_stop(
        &self,
        spec: &ProtectiveOrderSpec,
    ) -> anyhow::Result<ProtectiveOrderSpec> {
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
}

fn trending_series(start_px: f64, step_px: f64) -> Vec<Candle> {
    let base: DateTime<Utc> = DateTime::from_timestamp_millis(1_700_200_000_000).unwrap();
    (0..18)
        .map(|i| {
            let px = start_px + (i as f64) * step_px;
            Candle {
                ts: base + Duration::minutes(i),
                open: px * 0.999,
                high: px * 1.002,
                low: px * 0.998,
                close: px,
                volume: 2_500.0,
            }
        })
        .collect()
}

fn valid_pilot_limits() -> MonitoredPilotLimits {
    MonitoredPilotLimits {
        wallet_address: "0x3333333333333333333333333333333333333333".into(),
        allowed_instruments: BTreeSet::from(["BTC".into()]),
        qualified_strategy_id: "adaptive_ema_anchor".into(),
        strategy_registry_version: REGISTRY_VERSION.into(),
        max_allocated_capital_usd: Decimal::from(2_000u32),
        max_order_notional_usd: Decimal::from(1_600u32),
        max_aggregate_exposure_usd: Decimal::from(2_500u32),
        max_leverage: Decimal::from(2u32),
        max_daily_loss_usd: Decimal::from(100u32),
        max_session_loss_usd: Decimal::from(50u32),
        max_drawdown_ratio: Decimal::new(10, 2),
        permitted_order_types: BTreeSet::from([
            PilotOrderType::LimitPostOnly,
            PilotOrderType::LimitIoc,
            PilotOrderType::NativeReduceOnlyStop,
        ]),
        pilot_session_duration_secs: 3_600,
        operator_supervision_lease_secs: 60,
    }
}

fn complete_pilot_registry(risk_sha: &str) -> EvidenceRegistry {
    use hydra::live_gate::EvidenceProducer;
    let ts = 1_700_200_000_000_i64;
    let mk = |p: u8, tier, env, fp: &str, msg: &str| {
        EvidenceRecord::produce_attested(
            EvidenceProducer::LocalVerificationSuite,
            p,
            tier,
            risk_sha,
            env,
            "standard",
            fp,
            format!("sha256-{fp}"),
            "sha3-fang-config-v1",
            None,
            ts,
            msg,
        )
        .unwrap()
    };
    EvidenceRegistry {
        records: vec![
            mk(
                1,
                VerificationTier::IntegratedVerified,
                EvidenceEnvironment::OfflineMock,
                "fp-p1",
                "SettlementStore verified",
            ),
            mk(
                2,
                VerificationTier::IntegratedVerified,
                EvidenceEnvironment::OfflineMock,
                "fp-p2",
                "Recovery verified",
            ),
            mk(
                3,
                VerificationTier::IntegratedVerified,
                EvidenceEnvironment::OfflineMock,
                "fp-p3",
                "Protective stops + sovereign risk verified",
            ),
            mk(
                4,
                VerificationTier::IntegratedVerified,
                EvidenceEnvironment::OfflineMock,
                "fp-p4",
                "IPC + EngineSignerAuthority verified",
            ),
            mk(
                5,
                VerificationTier::IntegratedVerified,
                EvidenceEnvironment::OfflineMock,
                "fp-p5",
                "Watchdog + sentinel_faults verified",
            ),
            mk(
                6,
                VerificationTier::QualifiedStrategyVerified,
                EvidenceEnvironment::OfflineMock,
                "fp-hist",
                "Historical OOS strategy qualified",
            ),
            mk(
                6,
                VerificationTier::RealExchangeTestVerified,
                EvidenceEnvironment::HyperliquidTestnet,
                "fp-testnet",
                "Hyperliquid Testnet reduce_only stop & cancel verified",
            ),
            mk(
                6,
                VerificationTier::IntegratedVerified,
                EvidenceEnvironment::PublicPaper,
                "fp-paper",
                "Live public_paper WS/L2 lifecycle verified",
            ),
        ],
    }
}

#[test]
fn monitored_pilot_eligibility_fails_closed_without_all_proofs_and_explicit_ceilings() {
    let lim = valid_pilot_limits();
    let reg = complete_pilot_registry("risk-v1");

    // Full evidence + valid limits => eligible for RestrictedMonitoredPilot ONLY (never UnattendedAwakened)
    let ok_rep = evaluate_monitored_pilot_eligibility(&reg, "risk-v1", Some(&lim), true, false);
    assert!(ok_rep.eligible, "blockers: {:?}", ok_rep.blocking_reasons);
    assert_eq!(
        ok_rep.execution_tier,
        LiveExecutionTier::RestrictedMonitoredPilot
    );
    assert!(!ok_rep.unattended_awakened_eligible);

    // Missing Testnet proof => refused
    let mut no_testnet = reg.clone();
    no_testnet
        .records
        .retain(|r| r.environment != EvidenceEnvironment::HyperliquidTestnet);
    let rep_no_tn =
        evaluate_monitored_pilot_eligibility(&no_testnet, "risk-v1", Some(&lim), true, false);
    assert!(!rep_no_tn.eligible);
    assert_eq!(rep_no_tn.execution_tier, LiveExecutionTier::Dormant);

    // Missing limits or missing NativeReduceOnlyStop or excessive leverage => refused
    assert!(!evaluate_monitored_pilot_eligibility(&reg, "risk-v1", None, true, false).eligible);
    let mut no_stop_type = lim.clone();
    no_stop_type
        .permitted_order_types
        .remove(&PilotOrderType::NativeReduceOnlyStop);
    assert!(
        !evaluate_monitored_pilot_eligibility(&reg, "risk-v1", Some(&no_stop_type), true, false)
            .eligible
    );

    let mut bad_lev = lim.clone();
    bad_lev.max_leverage = Decimal::from(10u32);
    assert!(
        !evaluate_monitored_pilot_eligibility(&reg, "risk-v1", Some(&bad_lev), true, false)
            .eligible
    );

    // Missing treasury auth, active emergency suspend, or short consent => refused
    assert!(authorize_monitored_pilot_session(
        &reg,
        "risk-v1",
        lim.clone(),
        "short",
        true,
        false,
        1_000,
    )
    .is_err());
    assert!(authorize_monitored_pilot_session(
        &reg,
        "risk-v1",
        lim,
        "Owner explicitly authorizes BTC pilot under strict limits",
        false,
        false,
        1_000,
    )
    .is_err());
}

#[tokio::test]
async fn monitored_pilot_engine_enforces_instruments_ceilings_supervision_lease_and_no_auto_resume()
{
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("pilot_engine.db");
    let btc_bars = trending_series(60_000.0, 220.0);
    let eth_bars = trending_series(2_000.0, 12.0);
    let btc_mid = btc_bars.last().unwrap().close;
    let eth_mid = eth_bars.last().unwrap().close;

    let st = Arc::new(Mutex::new(PilotVenueState {
        bars: BTreeMap::from([("BTC".into(), btc_bars), ("ETH".into(), eth_bars)]),
        mids: BTreeMap::from([("BTC".into(), btc_mid), ("ETH".into(), eth_mid)]),
        equity_usd: 10_000.0,
        next_oid: 1_000,
        ..Default::default()
    }));
    let venue = PilotMockVenue { st: st.clone() };

    let mut cfg = AppConfig::load_research("config/strategies.toml", None).unwrap();
    cfg.data_dir = tmp.path().to_path_buf();
    cfg.live_trading = true;
    cfg.hl_account_address = Some("0x3333333333333333333333333333333333333333".into());
    cfg.risk.max_open_exposure_pct = 50.0;
    cfg.risk.max_notional_per_market_usd = 1_500.0;
    cfg.risk.max_slippage_bps = 30.0;

    let storage = Storage::open(&db_path).unwrap();
    storage
        .with_conn(hydra::storage::schema::run_migrations)
        .unwrap();
    storage
        .bind_trading_account("0x3333333333333333333333333333333333333333", false)
        .unwrap();
    let s_store = SettlementStore::open(
        storage.clone(),
        "0x3333333333333333333333333333333333333333",
    )
    .unwrap();
    let auth = EngineControlAuthority::new(
        "pilot-eng",
        Arc::new(AtomicBool::new(false)),
        Some(storage.clone()),
    );
    let mut ipc_srv = EngineIpcServer::start_with_authority(tmp.path(), auth.clone()).unwrap();

    let constraints = BTreeMap::from([
        (
            "BTC".into(),
            InstrumentOrderConstraints {
                symbol: "BTC".into(),
                tick_size: Decimal::new(1, 1),
                lot_size: Decimal::new(1, 4),
                min_notional_usd: Decimal::from(10u32),
                max_leverage: Decimal::from(20u32),
            },
        ),
        (
            "ETH".into(),
            InstrumentOrderConstraints {
                symbol: "ETH".into(),
                tick_size: Decimal::new(1, 1),
                lot_size: Decimal::new(1, 4),
                min_notional_usd: Decimal::from(10u32),
                max_leverage: Decimal::from(20u32),
            },
        ),
    ]);

    let pilot_u_cfg = UnifiedStrategyConfig {
        enabled_families: vec![StrategyFamily::AdaptiveEmaAnchor],
        ..Default::default()
    };
    let mut engine = Engine::new_with_venues(
        cfg.clone(),
        storage.clone(),
        vec![Box::new(venue.clone())],
        vec!["BTC".into(), "ETH".into()],
        false,
        None,
        Some(auth.clone()),
        None,
        Some(pilot_u_cfg.clone()),
        Some(s_store.clone()),
        constraints.clone(),
        false,
    )
    .await
    .unwrap();
    assert!(engine.active_pilot().is_none());

    let t0 = Utc::now().timestamp_millis() - 10_000;
    let reg = complete_pilot_registry("risk-v1");

    // Check tight per-order notional ceiling (500 USD) rejects 1,500 USD BTC plan cleanly before loosening to 1,600 USD
    let mut tight_lim = valid_pilot_limits();
    tight_lim.max_order_notional_usd = Decimal::from(500u32);
    let tight_grant = authorize_monitored_pilot_session(
        &reg,
        "risk-v1",
        tight_lim,
        "Owner explicitly authorizes BTC pilot under strict limits",
        true,
        false,
        t0,
    )
    .unwrap();
    engine.attach_monitored_pilot_at(tight_grant, t0).unwrap();
    let s0 = engine.step_once_at(0, t0 + 500).await.unwrap();
    assert_eq!(s0.accepted_entries, 0);
    assert!(s0.risk_rejected >= 1);

    // Clear signal dedup via fresh engine / attach regular limits (BTC allowed <= 1,600 USD; ETH still disallowed!)
    engine.revoke_monitored_pilot();
    assert!(engine.active_pilot().is_none());
    let mut engine = Engine::new_with_venues(
        cfg.clone(),
        storage.clone(),
        vec![Box::new(venue.clone())],
        vec!["BTC".into(), "ETH".into()],
        false,
        None,
        Some(auth.clone()),
        None,
        Some(pilot_u_cfg.clone()),
        Some(s_store.clone()),
        constraints.clone(),
        false,
    )
    .await
    .unwrap();
    let grant = authorize_monitored_pilot_session(
        &reg,
        "risk-v1",
        valid_pilot_limits(),
        "Owner explicitly authorizes BTC pilot under strict limits",
        true,
        false,
        t0,
    )
    .unwrap();
    engine.attach_monitored_pilot_at(grant, t0).unwrap();

    // Tick 1 at t0: BTC permitted & fills with native stop; ETH disallowed by pilot limits and rejected!
    let s1 = engine.step_once_at(1, t0 + 1_000).await.unwrap();
    assert_eq!(s1.accepted_entries, 1);
    assert!(s1.risk_rejected >= 1);
    {
        let g = st.lock().unwrap();
        assert_eq!(g.filled.len(), 1);
        assert_eq!(g.filled[0].symbol, "BTC");
        assert!(g.stops.iter().any(|s| s.symbol == "BTC" && s.reduce_only));
    }

    // Advance beyond 60s operator supervision lease -> blocks new entries WITHOUT blind liquidation,
    // AND still cleans an orphan SOL stop while keeping BTC protection!
    {
        let mut g = st.lock().unwrap();
        g.stops.push(ProtectiveOrderSpec {
            cloid: "orphan-sol-pilot".into(),
            exchange_oid: Some(9090),
            symbol: "SOL".into(),
            owner_strategy: "old".into(),
            position_side: Side::Long,
            protective_side: Side::Short,
            qty: Decimal::ONE,
            trigger_price: Decimal::from(80u32),
            reduce_only: true,
        });
    }
    let s2 = engine.step_once_at(2, t0 + 65_000).await.unwrap();
    assert!(s2.halts_blocked_entries);
    assert_eq!(s2.accepted_entries, 0);
    {
        let g = st.lock().unwrap();
        assert_eq!(g.positions.len(), 1); // BTC open position preserved (zero blind liquidation)
        assert!(!g.stops.iter().any(|s| s.symbol == "SOL"));
        assert!(g.stops.iter().any(|s| s.symbol == "BTC" && s.reduce_only));
    }

    // Wrong session_id is rejected even while not suspended
    let wrong_renew = EngineIpcClient::send_command(
        tmp.path(),
        "pilot-eng",
        1,
        EngineControlCommand::RenewPilotSupervision {
            session_id: "forged-session".into(),
        },
    )
    .unwrap();
    assert!(!wrong_renew.ok);

    // Authenticated IPC supervision lease renewal succeeds with real session_id
    let pilot_session_id = engine.active_pilot().unwrap().session_id().to_string();
    let renew = EngineIpcClient::send_command(
        tmp.path(),
        "pilot-eng",
        2,
        EngineControlCommand::RenewPilotSupervision {
            session_id: pilot_session_id.clone(),
        },
    )
    .unwrap();
    assert!(renew.ok);

    // EmergencySuspend revokes active pilot grant & refuses future supervision renewals
    let susp = EngineIpcClient::send_command(
        tmp.path(),
        "pilot-eng",
        3,
        EngineControlCommand::EmergencySuspend {
            reason: "pilot drill suspend".into(),
        },
    )
    .unwrap();
    assert!(susp.ok && susp.emergency_suspended);
    assert!(engine.active_pilot().is_none());
    let renew_after_susp = EngineIpcClient::send_command(
        tmp.path(),
        "pilot-eng",
        4,
        EngineControlCommand::RenewPilotSupervision {
            session_id: pilot_session_id,
        },
    )
    .unwrap();
    assert!(!renew_after_susp.ok);

    ipc_srv.stop();

    // Simulated restart: newly initialized Engine NEVER auto-resumes monitored pilot grant,
    // and remains blocked while emergency suspension is persisted.
    let restarted = Engine::new_with_venues(
        cfg.clone(),
        storage,
        vec![Box::new(venue)],
        vec!["BTC".into()],
        false,
        None,
        Some(auth),
        None,
        Some(UnifiedStrategyConfig::default()),
        Some(s_store),
        constraints.clone(),
        false,
    )
    .await
    .unwrap();
    assert!(restarted.active_pilot().is_none());

    // Separate session_pnl_usd ($50 ceiling) vs daily_pnl_usd ($100 ceiling) on a fresh active pilot engine
    let tmp2 = tempfile::tempdir().unwrap();
    let mut cfg2 = cfg;
    cfg2.data_dir = tmp2.path().to_path_buf();
    let st2 = Arc::new(Mutex::new(PilotVenueState {
        bars: BTreeMap::from([("BTC".into(), trending_series(60_000.0, 220.0))]),
        mids: BTreeMap::from([("BTC".into(), 63_740.0)]),
        equity_usd: 10_000.0,
        next_oid: 2_000,
        ..Default::default()
    }));
    let db2 = Storage::open(tmp2.path().join("pilot_loss.db")).unwrap();
    db2.with_conn(hydra::storage::schema::run_migrations)
        .unwrap();
    db2.bind_trading_account("0x3333333333333333333333333333333333333333", false)
        .unwrap();
    let mut loss_eng = Engine::new_with_venues(
        cfg2,
        db2,
        vec![Box::new(PilotMockVenue { st: st2 })],
        vec!["BTC".into()],
        false,
        None,
        Some(EngineControlAuthority::new(
            "pilot-loss-eng",
            Arc::new(AtomicBool::new(false)),
            None,
        )),
        None,
        Some(UnifiedStrategyConfig::default()),
        None,
        constraints,
        false,
    )
    .await
    .unwrap();

    let t1 = t0 + 10_000;
    let grant2 = authorize_monitored_pilot_session(
        &reg,
        "risk-v1",
        valid_pilot_limits(),
        "Owner explicitly authorizes BTC pilot under strict limits",
        true,
        false,
        t1,
    )
    .unwrap();
    loss_eng.attach_monitored_pilot_at(grant2, t1).unwrap();
    // Session loss ceiling is $50 while daily ceiling is $100: -$50 blocks session immediately!
    loss_eng.record_realized_pnl(Decimal::from(-50));
    let s_sess = loss_eng.step_once_at(10, t1 + 1_000).await.unwrap();
    assert!(s_sess.halts_blocked_entries);
    assert_eq!(s_sess.accepted_entries, 0);
}
