//! Comprehensive Project Awakening (`v2.0.0`) Acceptance & Adversarial Verification Suite.
//!
//! Exercises A01–A08 without real-money keys, mainnet orders, or test-only bypasses:
//! - Authenticated `TrustStore` multi-producer population (`HistoricalOosQualification`,
//!   `PublicPaperLifecycle`, `TestnetExecutionProof`) -> `authorize_restricted_mainnet_pilot`.
//! - Controlled-transport full-assembly `Engine` loop: exact rounded post-fee AEGIS/Pilot admission,
//!   native `reduce_only` protective stop with read-back, genuine exchange fills/fees/funding ->
//!   `SettlementStore` net PnL -> original-R COGNITION learning, cash-flow HWM adjustment,
//!   crash/restart continuity without live auto-resume, IPC `EmergencySuspend` revocation.
//! - Exhaustive denial matrix: missing/wrong scope/account/signer/strategy-config/risk-policy/order-type,
//!   over-leverage/over-allocation/expired-lease, boundary equality (`<=` vs `+0.01`), missing marks
//!   (zero `entry_price` fallback), stale collateral, insufficient liquidation buffer, persisted
//!   drawdown breaker across restart + deposit, Testnet min-notional rejection & residual-exposure
//!   failure artifact, and empty `open_exchange_orders` retaining unresolved holds.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use hydra::adapters::account_evidence::{AccountExposureEvidence, PositionEvidence};
use hydra::adapters::{OrderReceipt, VenueAdapter, VenuePosition};
use hydra::aegis::AccountSnapshot;
use hydra::aegis_sovereign::SovereignRiskPolicy;
use hydra::config::AppConfig;
use hydra::core::engine::Engine;
use hydra::core::{Candle, MarketSnapshot, OrderIntent, OrderResult, Side, VenueKind};
use hydra::engine_ipc::{
    EngineControlAuthority, EngineControlCommand, EngineIpcClient, EngineIpcServer,
};
use hydra::execution_recovery::{ExchangeOrderSnapshot, InstrumentOrderConstraints};
use hydra::fang_engine::{StrategyFamily, UnifiedStrategyConfig};
use hydra::ledger::{ExchangeFill, FillSide, FundingPayment, PnlConvention, TokenAmount};
use hydra::live_gate::{
    authorize_monitored_pilot_session, authorize_restricted_mainnet_pilot,
    evaluate_monitored_pilot_eligibility, require_entry_mutation_authority,
    restricted_pilot_consent_text, EntryMutationContext, EvidenceEnvironment, EvidenceProducer,
    EvidenceRecord, EvidenceRegistry, MainnetPilotRequest, MonitoredPilotLimits,
    PilotAuthorityCell, PilotOrderType, VerificationTier,
};
use hydra::oracle_research::{InstrumentSpec, MultiMarketDataset};
use hydra::order_status::{Observation, Outcome};
use hydra::paper_pipeline::SynapsePipelineConfig;
use hydra::paper_service::attest_stopped_public_paper_session;
use hydra::paper_store::{PaperSessionKind, PaperSessionStatus, PaperStore};
use hydra::protective_orders::ProtectiveOrderSpec;
use hydra::qualification::evaluate_and_record_historical_oos;
use hydra::settlement::{ExternalCashFlow, ExternalCashFlowKind, SettlementStore};
use hydra::storage::Storage;
use hydra::synapse_bus::DataProvenance;
use hydra::testnet_qualify::{run_testnet_scenario, TestnetScenarioParams};
use hydra::treasury::{ProtectionBackend, SignerRelationship, TreasuryProfile};
use hydra::trusted_evidence::{DeploymentBinding, TrustStore};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

const TEST_ACCOUNT: &str = "0x4444444444444444444444444444444444444444";
const TEST_SIGNER: &str = "0x5555555555555555555555555555555555555555";

#[derive(Default)]
struct ControlledState {
    bars: BTreeMap<String, Vec<Candle>>,
    mids: BTreeMap<String, f64>,
    equity_usd: f64,
    withdrawable_usd: Option<f64>,
    stale_evidence: bool,
    liq_px_by_sym: BTreeMap<String, Decimal>,
    next_oid: u64,
    next_tid: u64,
    positions: Vec<VenuePosition>,
    stops: Vec<ProtectiveOrderSpec>,
    fills: Vec<ExchangeFill>,
    fundings: Vec<FundingPayment>,
    cash_flows: Vec<ExternalCashFlow>,
    open_orders: Vec<ExchangeOrderSnapshot>,
    fail_stop_cancel: bool,
    force_residual_pos: bool,
}

#[derive(Clone)]
struct ControlledVenue {
    st: Arc<Mutex<ControlledState>>,
    account: String,
    signer: String,
}

#[async_trait]
impl VenueAdapter for ControlledVenue {
    fn kind(&self) -> VenueKind {
        VenueKind::Hyperliquid
    }
    fn is_production_venue(&self) -> bool {
        false
    }
    fn account_identity(&self) -> Option<(String, String)> {
        Some((self.account.clone(), self.signer.clone()))
    }
    async fn ping(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn snapshot(&self, symbol: &str, lookback: usize) -> anyhow::Result<MarketSnapshot> {
        let g = self.st.lock().unwrap();
        let candles = g.bars.get(symbol).cloned().unwrap_or_default();
        let slice = if candles.len() > lookback {
            candles[candles.len() - lookback..].to_vec()
        } else {
            candles
        };
        let mid = *g
            .mids
            .get(symbol)
            .ok_or_else(|| anyhow::anyhow!("no mid for {symbol}"))?;
        Ok(MarketSnapshot {
            venue: "hyperliquid".into(),
            symbol: symbol.into(),
            mid,
            bid: Some(mid * 0.9999),
            ask: Some(mid * 1.0001),
            spread_bps: Some(2.0),
            l2_depth_usd: Some(500_000.0),
            funding_bps_1h: Some(0.5),
            last_update: Utc::now(),
            observed_exchange_ts_ms: Some(Utc::now().timestamp_millis()),
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
        g.next_tid += 1;
        let oid = g.next_oid;
        let tid = g.next_tid;
        let mid = g.mids.get(&intent.symbol).copied().unwrap_or(50_000.0);
        let fill_px = intent.limit_price.unwrap_or(mid);
        let qty_dec = Decimal::try_from(intent.size)
            .unwrap_or(Decimal::ZERO)
            .round_dp(4);
        let px_dec = Decimal::try_from(fill_px)
            .unwrap_or(Decimal::ONE)
            .round_dp(2);
        let fee_usd = Decimal::new(60, 2); // exact $0.60 exchange fill fee
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
            g.liq_px_by_sym.insert(
                intent.symbol.clone(),
                (px_dec * Decimal::new(70, 2)).round_dp(2),
            );
        } else if !g.force_residual_pos {
            g.positions
                .retain(|p| !p.symbol.eq_ignore_ascii_case(&intent.symbol));
        }
        g.fills.push(ExchangeFill {
            fill_id: format!("hl-tid-{tid}"),
            order_id: oid,
            client_order_id: Some(intent.id.clone()),
            symbol: intent.symbol.clone(),
            side: if intent.side.is_buy() {
                FillSide::Buy
            } else {
                FillSide::Sell
            },
            quantity: qty_dec,
            price: px_dec,
            closed_pnl_usdc: None,
            pnl_convention: PnlConvention::BeforeFees,
            fee: Some(TokenAmount {
                token: "USDC".into(),
                amount: fee_usd,
            }),
            timestamp_ms: Utc::now().timestamp_millis().max(1),
        });
        Ok(OrderReceipt {
            result: OrderResult {
                intent_id: intent.id.clone(),
                accepted: true,
                venue_order_id: Some(oid.to_string()),
                filled_size: intent.size,
                avg_fill_price: Some(fill_px),
                message: "controlled fill".into(),
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
        anyhow::bail!("use journaled ExecutionEngine::submit")
    }
    async fn account_equity_usd(&self) -> anyhow::Result<f64> {
        Ok(self.st.lock().unwrap().equity_usd)
    }
    async fn account_exposure_evidence(&self) -> anyhow::Result<AccountExposureEvidence> {
        let g = self.st.lock().unwrap();
        let now_ms = if g.stale_evidence {
            Utc::now().timestamp_millis() - 120_000
        } else {
            Utc::now().timestamp_millis()
        };
        let mut ex_by_mkt = BTreeMap::new();
        let mut pos_ev = Vec::new();
        for p in &g.positions {
            let mark = g.mids.get(&p.symbol).copied().unwrap_or(p.entry_price);
            let ntl = Decimal::try_from((p.size.abs() * mark).max(0.0)).unwrap_or(Decimal::ZERO);
            *ex_by_mkt.entry(p.symbol.clone()).or_insert(Decimal::ZERO) += ntl;
            pos_ev.push(PositionEvidence {
                symbol: p.symbol.clone(),
                signed_size: Decimal::try_from(p.size).unwrap_or(Decimal::ONE),
                entry_price: Decimal::try_from(p.entry_price).unwrap_or(Decimal::ONE),
                notional: ntl,
                liquidation_px: g.liq_px_by_sym.get(&p.symbol).copied(),
                leverage: Some(Decimal::try_from(p.leverage).unwrap_or(Decimal::ONE)),
                margin_used: Some(ntl),
            });
        }
        let eq = Decimal::try_from(g.equity_usd).unwrap_or(Decimal::from(10_000u32));
        Ok(AccountExposureEvidence {
            account: self.account.clone(),
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
            positions: pos_ev,
            open_orders: Vec::new(),
            withdrawable_collateral: g.withdrawable_usd.and_then(|w| Decimal::try_from(w).ok()),
            total_margin_used: None,
        })
    }
    async fn order_status_observation(&self, intent: &OrderIntent) -> anyhow::Result<Observation> {
        let g = self.st.lock().unwrap();
        let now_ms = Utc::now().timestamp_millis();
        let matched = g
            .fills
            .iter()
            .find(|f| f.client_order_id.as_deref() == Some(intent.id.as_str()));
        let outcome = if let Some(f) = matched {
            Outcome::Order(Box::new(hydra::order_status::OrderState {
                order_id: f.order_id.to_string(),
                cloid: intent.id.clone(),
                symbol: f.symbol.clone(),
                buy: f.side == FillSide::Buy,
                reduce_only: intent.reduce_only,
                original_size: f.quantity,
                remaining_size: Decimal::ZERO,
                limit_price: f.price,
                order_type: "Limit".into(),
                tif: Some("Ioc".into()),
                status: "filled".into(),
                order_time_ms: f.timestamp_ms,
                status_time_ms: f.timestamp_ms,
            }))
        } else {
            Outcome::Missing
        };
        Ok(Observation {
            id: uuid::Uuid::new_v4().to_string(),
            intent_id: intent.id.clone(),
            account: self.account.clone(),
            venue: "hyperliquid".into(),
            environment: "mainnet".into(),
            requested_cloid: intent.id.clone(),
            acquired_started_ms: now_ms,
            acquired_finished_ms: now_ms,
            outcome,
        })
    }
    async fn open_exchange_orders(&self) -> anyhow::Result<Vec<ExchangeOrderSnapshot>> {
        Ok(self.st.lock().unwrap().open_orders.clone())
    }
    async fn fetch_account_fills(
        &self,
        _start_ms: i64,
        _end_ms: i64,
    ) -> anyhow::Result<Vec<ExchangeFill>> {
        Ok(self.st.lock().unwrap().fills.clone())
    }
    async fn fetch_account_funding(
        &self,
        _start_ms: i64,
        _end_ms: i64,
    ) -> anyhow::Result<Vec<FundingPayment>> {
        Ok(self.st.lock().unwrap().fundings.clone())
    }
    async fn fetch_account_cash_flows(
        &self,
        _start_ms: i64,
        _end_ms: i64,
    ) -> anyhow::Result<Vec<ExternalCashFlow>> {
        Ok(self.st.lock().unwrap().cash_flows.clone())
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
        let mut p = spec.clone();
        p.exchange_oid = Some(g.next_oid);
        g.stops.push(p.clone());
        Ok(p)
    }
    async fn cancel_protective_stop(
        &self,
        symbol: &str,
        exchange_oid: Option<u64>,
        cloid: &str,
    ) -> anyhow::Result<()> {
        let mut g = self.st.lock().unwrap();
        anyhow::ensure!(
            !g.fail_stop_cancel,
            "injected protective stop cancel failure"
        );
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

fn trending_candles(start_px: f64, step: f64, n: usize, start_ms: i64) -> Vec<Candle> {
    let base = DateTime::from_timestamp_millis(start_ms).unwrap();
    (0..n)
        .map(|i| {
            let px = start_px + (i as f64) * step;
            Candle {
                ts: base + Duration::minutes(i as i64),
                open: px * 0.999,
                high: px * 1.002,
                low: px * 0.998,
                close: px,
                volume: 2_500.0,
            }
        })
        .collect()
}

fn build_qualifying_dataset() -> MultiMarketDataset {
    // Alternating sharp bull legs (trend captures gain) and bear pullbacks (flat/stopped while B&H drops)
    // across 160 bars so multi-fold walk-forward + bootstrap OOS passes genuinely.
    let base = DateTime::from_timestamp_millis(1_700_000_000_000).unwrap();
    let mut bars = Vec::with_capacity(160);
    let mut px = 100.0f64;
    for i in 0..160usize {
        let phase = i % 32;
        let next = if phase < 18 { px * 1.015 } else { px * 0.972 };
        bars.push(Candle {
            ts: base + Duration::hours(i as i64),
            open: px,
            high: px.max(next) * 1.002,
            low: px.min(next) * 0.998,
            close: next,
            volume: 100_000.0,
        });
        px = next;
    }
    let specs = BTreeMap::from([(
        "BTC".into(),
        InstrumentSpec {
            symbol: "BTC".into(),
            max_leverage: 10.0,
            maintenance_margin_rate: 0.05,
            min_notional_usd: 10.0,
        },
    )]);
    MultiMarketDataset {
        bars_by_symbol: BTreeMap::from([("BTC".into(), bars)]),
        specs,
        funding_bps_1h: BTreeMap::new(),
        outage_windows: Vec::new(),
    }
}

fn test_limits() -> MonitoredPilotLimits {
    MonitoredPilotLimits {
        wallet_address: TEST_ACCOUNT.into(),
        allowed_instruments: BTreeSet::from(["BTC".into()]),
        qualified_strategy_id: "adaptive_ema_anchor".into(),
        strategy_registry_version: hydra::fang_engine::REGISTRY_VERSION.into(),
        max_allocated_capital_usd: Decimal::from(2_000u32),
        max_order_notional_usd: Decimal::from(1_600u32),
        max_aggregate_exposure_usd: Decimal::from(2_500u32),
        max_leverage: Decimal::from(2u32),
        max_daily_loss_usd: Decimal::from(100u32),
        max_session_loss_usd: Decimal::from(50u32),
        max_drawdown_ratio: Decimal::new(10, 2),
        permitted_order_types: BTreeSet::from([
            PilotOrderType::LimitIoc,
            PilotOrderType::NativeReduceOnlyStop,
        ]),
        pilot_session_duration_secs: 3_600,
        operator_supervision_lease_secs: 60,
    }
}

fn test_treasury_profile(backend: ProtectionBackend) -> TreasuryProfile {
    TreasuryProfile {
        schema_version: 1,
        dedicated_wallet_address: TEST_ACCOUNT.into(),
        derived_signer_address: TEST_SIGNER.into(),
        signer_relationship: SignerRelationship::ApprovedAgent,
        protection_backend: backend,
        risk_acknowledged: true,
        risk_acknowledged_at: Some(Utc::now().to_rfc3339()),
        risk_acknowledgment_hash: Some("hash".into()),
        wallet_authorized: true,
        emergency_suspended: false,
        emergency_suspended_at: None,
        emergency_suspension_reason: None,
        updated_at: Utc::now().to_rfc3339(),
    }
}

async fn populate_full_trust_store(
    trust_store: &TrustStore,
    dir: &std::path::Path,
    u_cfg: &UnifiedStrategyConfig,
) {
    // 1. Historical OOS Qualification (genuine simulation + CV + bootstrap producer)
    let ds = build_qualifying_dataset();
    let (verdict, _payload) =
        evaluate_and_record_historical_oos(&ds, u_cfg, 3, 2, Some(trust_store)).unwrap();
    assert!(
        matches!(
            verdict,
            hydra::qualification::StrategyAcceptanceVerdict::EligibleForMonitoredPilot { .. }
        ),
        "synthetic qualification dataset unexpectedly failed: {verdict:?}"
    );

    // 2. PublicPaperLifecycle (genuine >=3-cycle SQLite paper store attested on clean Stop)
    let pdb = PaperStore::open(dir.join("paper_prod.db")).unwrap();
    let sess = pdb
        .create_session(
            PaperSessionKind::PublicPaper,
            "Acceptance Paper",
            SynapsePipelineConfig::default(),
        )
        .unwrap();
    let mut pipe = pdb.load_pipeline(&sess.session_id).unwrap();
    for _ in 0..3 {
        let before_ev = pipe.bus.total_published();
        let prev_closed = pipe.ledger.closed_trades.len();
        let sum = pipe.run_deterministic_demo_step().unwrap();
        let evs: Vec<_> = pipe
            .bus
            .all_chronological()
            .filter(|e| e.event_id > before_ev)
            .cloned()
            .collect();
        let marks = BTreeMap::from([("BTC".into(), Decimal::from(65_000u32))]);
        pdb.commit_cycle(
            &sess.session_id,
            &pipe,
            &sum,
            prev_closed,
            &evs,
            &marks,
            DataProvenance::AuthoritativeExchange,
            Some("{}"),
        )
        .unwrap();
    }
    pdb.set_status(&sess.session_id, PaperSessionStatus::Stopped, None)
        .unwrap();
    attest_stopped_public_paper_session(&pdb, &sess.session_id, 0, trust_store).unwrap();

    // 3. TestnetExecutionProof (genuine 7-step Testnet qualification scenario)
    let tn_state = Arc::new(Mutex::new(ControlledState {
        bars: BTreeMap::from([(
            "BTC".into(),
            trending_candles(60_000.0, 200.0, 18, 1_700_200_000_000),
        )]),
        mids: BTreeMap::from([("BTC".into(), 63_400.0)]),
        equity_usd: 1_000.0,
        withdrawable_usd: Some(1_000.0),
        next_oid: 500,
        next_tid: 500,
        ..Default::default()
    }));
    let tn_venue: Arc<dyn VenueAdapter> = Arc::new(ControlledVenue {
        st: tn_state,
        account: TEST_ACCOUNT.into(),
        signer: TEST_SIGNER.into(),
    });
    let rep = run_testnet_scenario(
        &TestnetScenarioParams {
            account: TEST_ACCOUNT.into(),
            signer: TEST_SIGNER.into(),
            symbol: "BTC".into(),
            requested_max_notional_usd: 25.0,
            trust_fingerprint: "testnet-acceptance-fp".into(),
        },
        tn_venue,
        &dir.join("tn_work"),
        trust_store,
        None,
    )
    .await
    .unwrap();
    assert!(rep.scenario_passed);
}

#[tokio::test]
async fn controlled_transport_pilot_full_assembly_settlement_learning_recovery_and_suspend() {
    let tmp = tempfile::tempdir().unwrap();
    let trust = TrustStore::at_root(tmp.path().join("evidence"));
    let u_cfg = UnifiedStrategyConfig {
        enabled_families: vec![StrategyFamily::AdaptiveEmaAnchor],
        ..Default::default()
    };
    populate_full_trust_store(&trust, tmp.path(), &u_cfg).await;

    let cfg_sha = u_cfg.config_sha3().unwrap();
    let policy = SovereignRiskPolicy::default();
    let policy_sha = policy.policy_sha3().unwrap();
    let limits = test_limits();
    let consent = restricted_pilot_consent_text(&limits);
    let profile = test_treasury_profile(ProtectionBackend::WindowsDpapiCurrentUser);

    let now_ms = Utc::now().timestamp_millis();
    // Mint real ProductionMainnet grant through the ONLY constructor.
    let grant = authorize_restricted_mainnet_pilot(MainnetPilotRequest {
        trust_store: &trust,
        strategy_config_sha3: &cfg_sha,
        risk_policy_sha3: &policy_sha,
        limits: limits.clone(),
        owner_consent_statement: &consent,
        adapter_account: TEST_ACCOUNT,
        engine_signer: TEST_SIGNER,
        treasury_profile: Some(&profile),
        treasury_key_present: true,
        emergency_suspended: false,
        now_ms,
    })
    .unwrap();
    assert_eq!(
        grant.scope(),
        hydra::live_gate::PilotGrantScope::ProductionMainnet
    );

    // Prepare controlled-transport Engine with Storage + SettlementStore + IPC.
    let mut cfg = AppConfig::from_embedded_defaults(tmp.path()).unwrap();
    cfg.live_trading = true;
    cfg.hl_account_address = Some(TEST_ACCOUNT.into());
    cfg.risk.max_notional_per_market_usd = 5_000.0;
    cfg.risk.max_open_exposure_pct = 80.0;

    let db_path = tmp.path().join("engine.db");
    let storage = Storage::open(&db_path).unwrap();
    storage.migrate().unwrap();
    storage.bind_trading_account(TEST_ACCOUNT, false).unwrap();
    let s_store = SettlementStore::open(storage.clone(), TEST_ACCOUNT).unwrap();

    let st = Arc::new(Mutex::new(ControlledState {
        bars: BTreeMap::from([(
            "BTC".into(),
            trending_candles(60_000.0, 220.0, 18, now_ms - 2_000_000),
        )]),
        mids: BTreeMap::from([("BTC".into(), 63_740.0)]),
        equity_usd: 10_000.0,
        withdrawable_usd: Some(10_000.0),
        next_oid: 1_000,
        next_tid: 2_000,
        ..Default::default()
    }));
    let venue = ControlledVenue {
        st: st.clone(),
        account: TEST_ACCOUNT.into(),
        signer: TEST_SIGNER.into(),
    };
    let constraints = venue.instrument_constraints().await.unwrap();
    let auth = EngineControlAuthority::new(
        "awakening-eng",
        Arc::new(AtomicBool::new(false)),
        Some(storage.clone()),
    );
    let mut ipc_srv = EngineIpcServer::start_with_authority(tmp.path(), auth.clone()).unwrap();

    let mut engine = Engine::new_with_venues(
        cfg.clone(),
        storage.clone(),
        vec![Box::new(venue.clone())],
        vec!["BTC".into()],
        false,
        None,
        Some(auth.clone()),
        Some(policy.clone()),
        Some(u_cfg.clone()),
        Some(s_store.clone()),
        constraints.clone(),
        false,
    )
    .await
    .unwrap();
    engine
        .attach_monitored_pilot_at(grant.clone(), now_ms)
        .unwrap();

    // Tick 1: entry admitted on rounded post-fee notional -> filled -> native stop placed & read-back.
    let s1 = engine.step_once_at(1, now_ms + 500).await.unwrap();
    assert_eq!(s1.accepted_entries, 1);
    assert_eq!(st.lock().unwrap().fills.len(), 1);
    assert!(st
        .lock()
        .unwrap()
        .stops
        .iter()
        .any(|s| s.symbol == "BTC" && s.reduce_only));

    // Advance mid past take-profit & inject genuine USDC funding payment (-$0.20).
    let open = storage.list_open_positions().unwrap();
    assert_eq!(open.len(), 1);
    let tp_px = open[0].take_profit.unwrap() + 10.0;
    {
        let mut g = st.lock().unwrap();
        g.mids.insert("BTC".into(), tp_px);
        let mut bars = g.bars.get("BTC").cloned().unwrap();
        let last_ts = bars.last().unwrap().ts;
        bars.push(Candle {
            ts: last_ts + Duration::minutes(1),
            open: tp_px * 0.999,
            high: tp_px * 1.001,
            low: tp_px * 0.998,
            close: tp_px,
            volume: 2_500.0,
        });
        g.bars.insert("BTC".into(), bars);
        g.fundings.push(FundingPayment {
            funding_id: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:fund:BTC".into(),
            symbol: "BTC".into(),
            payment: Some(TokenAmount {
                token: "USDC".into(),
                amount: Decimal::new(-20, 2),
            }),
            timestamp_ms: now_ms + 1_000,
        });
    }

    // Tick 2: exit triggers -> close fill ($0.60 fee) + entry fill ($0.60 fee) + funding (-$0.20)
    // settle into verified net PnL & emit COGNITION ShadowSample scaled by original stop risk!
    let _s2 = engine.step_once_at(2, now_ms + 2_000).await.unwrap();
    let samples = engine.learning_samples();
    assert_eq!(
        samples.len(),
        1,
        "expected exactly one verified-settled learning sample"
    );
    assert_ne!(samples[0].strategy, "unattributed");
    assert!(samples[0].normalized_r > 0.0 && samples[0].normalized_r.is_finite());

    // Simulate external deposit (+$500 USDC) -> HWM rises by deposit without counting as profit.
    {
        let mut g = st.lock().unwrap();
        g.equity_usd += 500.0;
        g.withdrawable_usd = Some(10_500.0);
        g.cash_flows.push(ExternalCashFlow {
            cash_flow_id: "dep-1".into(),
            kind: ExternalCashFlowKind::Deposit,
            token: "USDC".into(),
            signed_amount: Decimal::from(500u32),
            timestamp_ms: now_ms + 2_500,
        });
    }
    let _ = engine
        .reconcile_settlement_from_venues(now_ms + 3_000)
        .await
        .unwrap();

    // IPC EmergencySuspend immediately revokes PilotAuthorityCell while stop management stays active.
    let susp = EngineIpcClient::send_command(
        tmp.path(),
        "awakening-eng",
        EngineIpcClient::next_nonce(),
        EngineControlCommand::EmergencySuspend {
            reason: "acceptance drill".into(),
        },
    )
    .unwrap();
    assert!(susp.ok && susp.emergency_suspended);
    assert!(engine.active_pilot().is_none());
    let s3 = engine.step_once_at(3, now_ms + 4_000).await.unwrap();
    assert!(s3.halts_blocked_entries);
    ipc_srv.stop();

    // Simulated restart: reopened Engine NEVER auto-resumes live pilot.
    let restarted = Engine::new_with_venues(
        cfg,
        storage,
        vec![Box::new(venue)],
        vec!["BTC".into()],
        false,
        None,
        None,
        Some(policy),
        Some(u_cfg),
        Some(s_store),
        constraints,
        false,
    )
    .await
    .unwrap();
    assert!(restarted.active_pilot().is_none());
}

#[tokio::test]
async fn adversarial_gate_boundaries_stale_marks_drawdown_and_testnet_failures() {
    let tmp = tempfile::tempdir().unwrap();
    let trust = TrustStore::at_root(tmp.path().join("evidence"));
    let u_cfg = UnifiedStrategyConfig::default();
    populate_full_trust_store(&trust, tmp.path(), &u_cfg).await;

    let cfg_sha = u_cfg.config_sha3().unwrap();
    let policy_sha = SovereignRiskPolicy::default().policy_sha3().unwrap();
    let limits = test_limits();
    let consent = restricted_pilot_consent_text(&limits);
    let profile = test_treasury_profile(ProtectionBackend::WindowsDpapiCurrentUser);
    let now_ms = Utc::now().timestamp_millis();

    // 1. authorize_restricted_mainnet_pilot denials:
    // Wrong consent
    assert!(authorize_restricted_mainnet_pilot(MainnetPilotRequest {
        trust_store: &trust,
        strategy_config_sha3: &cfg_sha,
        risk_policy_sha3: &policy_sha,
        limits: limits.clone(),
        owner_consent_statement: "short consent",
        adapter_account: TEST_ACCOUNT,
        engine_signer: TEST_SIGNER,
        treasury_profile: Some(&profile),
        treasury_key_present: true,
        emergency_suspended: false,
        now_ms,
    })
    .is_err());

    // Wrong strategy config digest
    assert!(authorize_restricted_mainnet_pilot(MainnetPilotRequest {
        trust_store: &trust,
        strategy_config_sha3: &"f".repeat(64),
        risk_policy_sha3: &policy_sha,
        limits: limits.clone(),
        owner_consent_statement: &consent,
        adapter_account: TEST_ACCOUNT,
        engine_signer: TEST_SIGNER,
        treasury_profile: Some(&profile),
        treasury_key_present: true,
        emergency_suspended: false,
        now_ms,
    })
    .is_err());

    // Unencrypted Unix permissions backend refused for ProductionMainnet
    let unenc_profile = test_treasury_profile(ProtectionBackend::UnixOwnerOnly0600);
    assert!(authorize_restricted_mainnet_pilot(MainnetPilotRequest {
        trust_store: &trust,
        strategy_config_sha3: &cfg_sha,
        risk_policy_sha3: &policy_sha,
        limits: limits.clone(),
        owner_consent_statement: &consent,
        adapter_account: TEST_ACCOUNT,
        engine_signer: TEST_SIGNER,
        treasury_profile: Some(&unenc_profile),
        treasury_key_present: true,
        emergency_suspended: false,
        now_ms,
    })
    .is_err());

    // Valid grant + PilotAuthorityCell boundary enforcement
    let grant = authorize_restricted_mainnet_pilot(MainnetPilotRequest {
        trust_store: &trust,
        strategy_config_sha3: &cfg_sha,
        risk_policy_sha3: &policy_sha,
        limits: limits.clone(),
        owner_consent_statement: &consent,
        adapter_account: TEST_ACCOUNT,
        engine_signer: TEST_SIGNER,
        treasury_profile: Some(&profile),
        treasury_key_present: true,
        emergency_suspended: false,
        now_ms,
    })
    .unwrap();
    let cell = PilotAuthorityCell::new();
    cell.attach(grant.clone());
    cell.set_deployed_digests(&cfg_sha, &policy_sha);

    let ctx_at =
        |notional: Decimal, otype: PilotOrderType, acct: &'static str| EntryMutationContext {
            production: true,
            account: acct,
            signer: TEST_SIGNER,
            symbol: "BTC",
            strategy_id: "adaptive_ema_anchor",
            order_type: otype,
            requested_leverage: Decimal::ONE,
            requested_notional_usd: notional,
            current_exposure_usd: Decimal::ZERO,
            emergency_suspended: false,
        };
    // Exact boundary allowed
    require_entry_mutation_authority(
        Some(&cell),
        &ctx_at(
            Decimal::from(1_600u32),
            PilotOrderType::LimitIoc,
            TEST_ACCOUNT,
        ),
        now_ms + 1,
    )
    .unwrap();
    // +$0.01 over max_order_notional_usd rejected
    assert!(require_entry_mutation_authority(
        Some(&cell),
        &ctx_at(
            Decimal::new(160_001, 2),
            PilotOrderType::LimitIoc,
            TEST_ACCOUNT
        ),
        now_ms + 1,
    )
    .is_err());
    // Unpermitted order type rejected
    assert!(require_entry_mutation_authority(
        Some(&cell),
        &ctx_at(
            Decimal::from(100u32),
            PilotOrderType::LimitPostOnly,
            TEST_ACCOUNT
        ),
        now_ms + 1,
    )
    .is_err());
    // Wrong account rejected
    assert!(require_entry_mutation_authority(
        Some(&cell),
        &ctx_at(
            Decimal::from(100u32),
            PilotOrderType::LimitIoc,
            "0x9999999999999999999999999999999999999999"
        ),
        now_ms + 1,
    )
    .is_err());
    // Expired supervision lease rejected right before transmission
    assert!(require_entry_mutation_authority(
        Some(&cell),
        &ctx_at(
            Decimal::from(100u32),
            PilotOrderType::LimitIoc,
            TEST_ACCOUNT
        ),
        now_ms + 65_000,
    )
    .is_err());
    // OfflineFixture grant CANNOT authorize production
    let art_hex = "b".repeat(64);
    let legacy_reg = EvidenceRegistry {
        records: vec![
            EvidenceRecord::produce_attested(
                EvidenceProducer::LocalVerificationSuite,
                1,
                VerificationTier::IntegratedVerified,
                &policy_sha,
                EvidenceEnvironment::OfflineMock,
                "disabled",
                "f",
                &art_hex,
                &cfg_sha,
                None,
                now_ms,
                "1",
            )
            .unwrap(),
            EvidenceRecord::produce_attested(
                EvidenceProducer::LocalVerificationSuite,
                2,
                VerificationTier::IntegratedVerified,
                &policy_sha,
                EvidenceEnvironment::OfflineMock,
                "disabled",
                "f",
                &art_hex,
                &cfg_sha,
                None,
                now_ms,
                "2",
            )
            .unwrap(),
            EvidenceRecord::produce_attested(
                EvidenceProducer::LocalVerificationSuite,
                3,
                VerificationTier::IntegratedVerified,
                &policy_sha,
                EvidenceEnvironment::OfflineMock,
                "disabled",
                "f",
                &art_hex,
                &cfg_sha,
                None,
                now_ms,
                "3",
            )
            .unwrap(),
            EvidenceRecord::produce_attested(
                EvidenceProducer::LocalVerificationSuite,
                4,
                VerificationTier::IntegratedVerified,
                &policy_sha,
                EvidenceEnvironment::OfflineMock,
                "disabled",
                "f",
                &art_hex,
                &cfg_sha,
                None,
                now_ms,
                "4",
            )
            .unwrap(),
            EvidenceRecord::produce_attested(
                EvidenceProducer::LocalVerificationSuite,
                5,
                VerificationTier::IntegratedVerified,
                &policy_sha,
                EvidenceEnvironment::OfflineMock,
                "disabled",
                "f",
                &art_hex,
                &cfg_sha,
                None,
                now_ms,
                "5",
            )
            .unwrap(),
            EvidenceRecord::produce_attested(
                EvidenceProducer::HistoricalLaboratory,
                6,
                VerificationTier::QualifiedStrategyVerified,
                &policy_sha,
                EvidenceEnvironment::OfflineMock,
                "disabled",
                "f",
                &art_hex,
                &cfg_sha,
                None,
                now_ms,
                "6a",
            )
            .unwrap(),
            EvidenceRecord::produce_attested(
                EvidenceProducer::PublicPaperService,
                6,
                VerificationTier::IntegratedVerified,
                &policy_sha,
                EvidenceEnvironment::PublicPaper,
                "paper",
                "f",
                &art_hex,
                &cfg_sha,
                None,
                now_ms,
                "6b",
            )
            .unwrap(),
            EvidenceRecord::produce_attested(
                EvidenceProducer::HyperliquidTestnetProbe,
                6,
                VerificationTier::RealExchangeTestVerified,
                &policy_sha,
                EvidenceEnvironment::HyperliquidTestnet,
                "tn",
                "f",
                &art_hex,
                &cfg_sha,
                None,
                now_ms,
                "6c",
            )
            .unwrap(),
        ],
    };
    assert!(
        evaluate_monitored_pilot_eligibility(&legacy_reg, &policy_sha, Some(&limits), true, false)
            .eligible
    );
    let offline_grant = authorize_monitored_pilot_session(
        &legacy_reg,
        &policy_sha,
        limits.clone(),
        "Legacy offline fixture consent",
        true,
        false,
        now_ms,
    )
    .unwrap();
    let offline_cell = PilotAuthorityCell::new();
    offline_cell.attach(offline_grant);
    assert!(require_entry_mutation_authority(
        Some(&offline_cell),
        &ctx_at(
            Decimal::from(100u32),
            PilotOrderType::LimitIoc,
            TEST_ACCOUNT
        ),
        now_ms + 1,
    )
    .is_err());

    // 2. Testnet min-notional rejection (NO upward clamping) + residual failure artifact (passed=false)
    let tn_state = Arc::new(Mutex::new(ControlledState {
        bars: BTreeMap::from([(
            "BTC".into(),
            trending_candles(60_000.0, 200.0, 18, now_ms - 2_000_000),
        )]),
        mids: BTreeMap::from([("BTC".into(), 63_400.0)]),
        equity_usd: 1_000.0,
        withdrawable_usd: Some(1_000.0),
        fail_stop_cancel: true,
        force_residual_pos: true,
        next_oid: 800,
        next_tid: 800,
        ..Default::default()
    }));
    let tn_venue: Arc<dyn VenueAdapter> = Arc::new(ControlledVenue {
        st: tn_state,
        account: TEST_ACCOUNT.into(),
        signer: TEST_SIGNER.into(),
    });
    let small_err = run_testnet_scenario(
        &TestnetScenarioParams {
            account: TEST_ACCOUNT.into(),
            signer: TEST_SIGNER.into(),
            symbol: "BTC".into(),
            requested_max_notional_usd: 5.0,
            trust_fingerprint: "too-small".into(),
        },
        tn_venue.clone(),
        &tmp.path().join("tn_small"),
        &trust,
        None,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(
        small_err.contains("never clamped upward"),
        "got: {small_err}"
    );

    let fail_trust = TrustStore::at_root(tmp.path().join("fail_evidence"));
    let out_art = tmp.path().join("tn_fail_artifact.json");
    assert!(run_testnet_scenario(
        &TestnetScenarioParams {
            account: TEST_ACCOUNT.into(),
            signer: TEST_SIGNER.into(),
            symbol: "BTC".into(),
            requested_max_notional_usd: 25.0,
            trust_fingerprint: "tn-residual-fail".into(),
        },
        tn_venue,
        &tmp.path().join("tn_fail_work"),
        &fail_trust,
        Some(&out_art),
    )
    .await
    .is_err());
    let art_json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out_art).unwrap()).unwrap();
    assert_eq!(art_json["scenario_passed"].as_bool(), Some(false));
    assert!(
        !fail_trust
            .readiness(&DeploymentBinding::current(&cfg_sha), now_ms + 1_000)
            .all_satisfied
    );

    // 3. Recovery: empty `open_exchange_orders` NEVER clears unresolved `'submitted'` intent without fill/status
    let rec_db = Storage::open(tmp.path().join("recovery.db")).unwrap();
    rec_db.migrate().unwrap();
    rec_db.bind_trading_account(TEST_ACCOUNT, false).unwrap();
    let ambig_intent = OrderIntent {
        id: uuid::Uuid::new_v4().to_string(),
        venue: "hyperliquid".into(),
        symbol: "BTC".into(),
        side: Side::Long,
        size: 0.01,
        limit_price: Some(60_000.0),
        reduce_only: false,
        stop_price: Some(58_000.0),
        take_profit: Some(64_000.0),
        source_signal: Some("adaptive_ema_anchor".into()),
        created_at: Utc::now(),
    };
    rec_db.reserve_intent(&ambig_intent).unwrap();
    let empty_venue = ControlledVenue {
        st: Arc::new(Mutex::new(ControlledState {
            bars: BTreeMap::from([(
                "BTC".into(),
                trending_candles(60_000.0, 200.0, 18, now_ms - 2_000_000),
            )]),
            mids: BTreeMap::from([("BTC".into(), 63_400.0)]),
            equity_usd: 10_000.0,
            withdrawable_usd: Some(10_000.0),
            ..Default::default()
        })),
        account: TEST_ACCOUNT.into(),
        signer: TEST_SIGNER.into(),
    };
    let mut cfg_rec = AppConfig::from_embedded_defaults(tmp.path()).unwrap();
    cfg_rec.hl_account_address = Some(TEST_ACCOUNT.into());
    let mut rec_eng = Engine::new_with_venues(
        cfg_rec,
        rec_db,
        vec![Box::new(empty_venue)],
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
    let rec_rep = rec_eng.reconcile_startup_and_recovery().await.unwrap();
    assert!(rec_rep.unresolved_holds >= 1);
    assert!(rec_eng.halted_unresolved());
}
