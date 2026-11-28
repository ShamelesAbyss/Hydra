//! Main trading engine orchestrator: the forever loop.

use anyhow::{Context, Result};
use chrono::Utc;
use owo_colors::OwoColorize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

use crate::adapters::{make_venue, VenueAdapter};
use crate::aegis_sovereign::{
    evaluate_sovereign_admission, SovereignAdmissionProposal, SovereignPortfolioState,
    SovereignPositionInput, SovereignRiskDecision, SovereignRiskPolicy,
};
use crate::cli::Venue as CliVenue;
use crate::cognition_shadow::{
    infer_regime, InferredRegime, OutcomeSource, RegimeInferenceConfig, ShadowPolicy, ShadowSample,
};
use crate::cognition_stats::{rank_and_weight_ensemble, EnsembleRanking};
use crate::config::AppConfig;
use crate::core::{MarketSnapshot, OrderIntent, VenueKind};
use crate::engine_ipc::{EngineControlAuthority, EngineIpcServer};
use crate::execution::ExecutionEngine;
use crate::execution_recovery::{
    reconcile_execution_state, ExchangeOpenPositionSnapshot, ExchangeOrderSnapshot,
    ExecutionJournalRecord, InstrumentOrderConstraints, OrderLifecycleState, RecoveryAction,
    RecoveryReport,
};
use crate::fang_engine::{FangMarketInput, FangUnifiedPlanner, UnifiedStrategyConfig};
use crate::fang_planner::PlanningBudget;
use crate::hunter_microstructure::{
    compute_correlation_matrix, compute_microstructure_snapshot, MicrostructureSnapshot,
};
use crate::learning::{
    regime::{classify_view, RegimeView},
    strictness::DroughtState,
};
use crate::ledger::ExchangeFill;
use crate::monitoring::Heartbeat;
use crate::portfolio::{EntryContext, PortfolioManager};
use crate::protective_orders::{
    OpenProtectedPosition, ProtectiveOrderManager, ProtectiveReconcileAction,
    ProtectiveReconcileReport,
};
use crate::reporting::SessionReporter;
use crate::settlement::{SettlementStore, VerifiedLearningOutcome};
use crate::state::StateManager;
use crate::storage::Storage;
use crate::tui::SharedDash;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

fn utc_day_key(now_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(now_ms)
        .unwrap_or_else(Utc::now)
        .format("%Y-%m-%d")
        .to_string()
}

fn utc_week_key(now_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(now_ms)
        .unwrap_or_else(Utc::now)
        .format("%G-W%V")
        .to_string()
}

fn load_decimal_kv(storage: &Storage, key: &str) -> Option<Decimal> {
    storage
        .get_meta_kv(key)
        .ok()
        .flatten()
        .and_then(|s| Decimal::from_str_exact(s.trim()).ok())
}

fn shadow_sample_from_verified(
    lo: &VerifiedLearningOutcome,
    original_risk_usd: Option<Decimal>,
) -> Option<ShadowSample> {
    if lo.owner_strategy.eq_ignore_ascii_case("unattributed") {
        return None;
    }
    let risk_unit = original_risk_usd.filter(|r| *r > Decimal::ZERO)?;
    let r = (lo.verified_net_pnl_usdc / risk_unit).to_f64()?;
    if !r.is_finite() {
        return None;
    }
    Some(ShadowSample {
        id: format!("settled-{}", lo.round_trip_id),
        strategy: lo.owner_strategy.clone(),
        symbol: lo.symbol.clone(),
        regime: "*".into(),
        decision_at_ms: lo.opened_at_ms,
        outcome_at_ms: lo.closed_at_ms.max(lo.opened_at_ms + 1),
        normalized_r: r,
        source: OutcomeSource::CallerSuppliedNormalizedR,
    })
}

#[derive(Debug, Clone, Default)]
pub struct EngineStepSummary {
    pub tick: u64,
    pub planned_candidates: usize,
    pub accepted_entries: usize,
    pub risk_rejected: usize,
    pub protective_actions: usize,
    pub recovery_holds: usize,
    pub halts_blocked_entries: bool,
}

pub struct Engine {
    _account_ownership: Option<crate::ownership::AccountOwnership>,
    cfg: AppConfig,
    storage: Storage,
    venues: Vec<Box<dyn VenueAdapter>>,
    symbols: Vec<String>,
    portfolio: PortfolioManager,
    execution: ExecutionEngine,
    session_id: i64,
    stop: Arc<AtomicBool>,
    heartbeat: Heartbeat,
    live_override: bool,
    /// Dedup repeat signals: (strategy, symbol) -> last Instant we evaluated.
    signal_dedup: std::collections::HashMap<(String, String), std::time::Instant>,
    /// Optional shared dashboard state written to at the end of every tick.
    dash: Option<SharedDash>,
    last_pulse_at: std::time::Instant,
    drought: DroughtState,
    started_at: chrono::DateTime<chrono::Utc>,
    control_authority: EngineControlAuthority,
    sovereign_policy: SovereignRiskPolicy,
    unified_cfg: UnifiedStrategyConfig,
    settlement_store: Option<SettlementStore>,
    instrument_constraints: BTreeMap<String, InstrumentOrderConstraints>,
    order_journal: Vec<ExecutionJournalRecord>,
    halted_unresolved: bool,
    halted_unprotected: bool,
    daily_key: String,
    weekly_key: String,
    daily_pnl_usd: Decimal,
    weekly_pnl_usd: Decimal,
    session_pnl_usd: Decimal,
    high_water_equity_usd: Decimal,
    cash_flow_baseline_usd: Decimal,
    aegis_breaker_reason: Option<String>,
    last_ipc_supervision_ms: i64,
    ws_order_evidence: Vec<ExchangeOrderSnapshot>,
    learning_samples: Vec<ShadowSample>,
    ipc_server: Option<EngineIpcServer>,
    account_stream: Option<crate::account_stream::AccountStreamHandle>,
    last_ws_reconnect_seen: u64,
    settlement_cursor_ms: i64,
}

impl Drop for Engine {
    fn drop(&mut self) {
        if let Some(ref stream) = self.account_stream {
            stream.stop();
        }
        if let Some(mut srv) = self.ipc_server.take() {
            srv.stop();
        }
    }
}

impl Engine {
    pub async fn new(
        cfg: AppConfig,
        storage: Storage,
        venue: CliVenue,
        symbols: Vec<String>,
        live_override: bool,
        dash: Option<SharedDash>,
    ) -> Result<Self> {
        crate::execution::authorized_live(cfg.live_trading, live_override)?;
        if cfg.live_trading {
            let req = crate::live_gate::LiveAuthorizationRequest::from_treasury_store(
                crate::live_gate::ExecutionChannel::ProductionEngine,
                cfg.live_trading,
                live_override,
            );
            crate::live_gate::require_live_runtime_construction(&req)?;
        }

        let kinds: Vec<VenueKind> = match venue {
            CliVenue::Hyperliquid => vec![VenueKind::Hyperliquid],
        };

        let mut venues: Vec<Box<dyn VenueAdapter>> = Vec::new();
        for k in kinds {
            match make_venue(k, &cfg).await {
                Ok(v) => venues.push(v),
                Err(e) => tracing::warn!(venue=?k, error=%e, "venue adapter unavailable"),
            }
        }
        if venues.is_empty() {
            anyhow::bail!("no venue adapters could be initialised");
        }
        let account_scope =
            crate::ownership::configured_account(&cfg)?.unwrap_or_else(|| "local-engine".into());
        let settlement_store = SettlementStore::open(storage.clone(), account_scope)?;
        let mut instrument_constraints = BTreeMap::new();
        for v in &venues {
            if let Ok(map) = v.instrument_constraints().await {
                instrument_constraints.extend(map);
            }
        }
        let authority = EngineControlAuthority::new(
            "main-engine",
            Arc::new(AtomicBool::new(false)),
            Some(storage.clone()),
        );
        let account_stream = venues.iter().find_map(|v| {
            let (acct, _) = v.account_identity()?;
            let ws_url = if v.is_production_venue() {
                crate::hunter_stream::HYPERLIQUID_PUBLIC_WS_URL
            } else {
                return None;
            };
            crate::account_stream::spawn_account_stream(
                crate::account_stream::AccountStreamConfig {
                    ws_url: ws_url.into(),
                    account: acct,
                },
            )
            .ok()
        });
        let mut engine = Self::new_with_venues(
            cfg.clone(),
            storage,
            venues,
            symbols,
            live_override,
            dash,
            Some(authority.clone()),
            None,
            None,
            Some(settlement_store),
            instrument_constraints,
            true,
        )
        .await?;
        engine.account_stream = account_stream;
        match EngineIpcServer::start_with_authority(&cfg.data_dir, authority) {
            Ok(srv) => engine.ipc_server = Some(srv),
            Err(e) => {
                anyhow::ensure!(!cfg.live_trading, "live Engine IPC startup failed: {e}");
                tracing::warn!(error=%e, "engine IPC server unavailable in non-live mode");
            }
        }
        Ok(engine)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn new_with_venues(
        cfg: AppConfig,
        storage: Storage,
        venues: Vec<Box<dyn VenueAdapter>>,
        symbols: Vec<String>,
        live_override: bool,
        dash: Option<SharedDash>,
        control_authority: Option<EngineControlAuthority>,
        sovereign_policy: Option<SovereignRiskPolicy>,
        unified_cfg: Option<UnifiedStrategyConfig>,
        settlement_store: Option<SettlementStore>,
        instrument_constraints: BTreeMap<String, InstrumentOrderConstraints>,
        install_ctrlc: bool,
    ) -> Result<Self> {
        crate::execution::authorized_live(cfg.live_trading, live_override)?;
        let any_prod = venues.iter().any(|v| v.is_production_venue());
        if cfg.live_trading && any_prod {
            let req = crate::live_gate::LiveAuthorizationRequest::from_treasury_store(
                crate::live_gate::ExecutionChannel::ProductionEngine,
                cfg.live_trading,
                live_override,
            );
            crate::live_gate::require_live_runtime_construction(&req)?;
        }
        let ownership = if cfg.live_trading && any_prod {
            let account = crate::ownership::configured_account(&cfg)?
                .context("live trading requires an account identity")?;
            storage.bind_trading_account(&account, false)?;
            Some(crate::ownership::AccountOwnership::acquire(
                &crate::ownership::AccountOwnership::default_root()?,
                &account,
            )?)
        } else {
            None
        };
        SessionReporter::print_header(&cfg, &storage)?;

        // Validate requested symbols against each venue so invalid ones surface loudly.
        let mut validated_symbols: Vec<String> = Vec::new();
        for sym in &symbols {
            let mut found_on_any = false;
            for v in &venues {
                match v.snapshot(sym, 2).await {
                    Ok(_) => {
                        found_on_any = true;
                        break;
                    }
                    Err(e) => {
                        tracing::debug!(venue=v.kind().as_str(), symbol=%sym, error=%e, "symbol probe failed")
                    }
                }
            }
            if found_on_any {
                validated_symbols.push(sym.clone());
            } else {
                println!(
                    "  {}  symbol {} is not tradeable as a perp on any configured venue — dropping.",
                    "⚠".bright_yellow().bold(),
                    sym.bright_red().bold()
                );
                println!("     Common perps: BTC, ETH, HYPE, SOL, AVAX, ARB, OP, DOGE, kPEPE. Browse the Trade page on the venue UI for the full list.");
            }
        }
        if validated_symbols.is_empty() {
            anyhow::bail!("no valid symbols remain after validation");
        }
        let symbols = validated_symbols;

        let mut portfolio = PortfolioManager::new(storage.clone());
        let mut execution = ExecutionEngine::new(
            storage.clone(),
            crate::execution::authorized_live(cfg.live_trading, live_override)?,
        );
        if cfg.live_trading && any_prod {
            use std::str::FromStr;
            let account =
                crate::ownership::configured_account(&cfg)?.context("trading account missing")?;
            let limits = crate::aegis::RiskLimits {
                max_total_exposure_ratio: Decimal::from_str(
                    &(cfg.risk.max_open_exposure_pct / 100.0).to_string(),
                )?,
                max_market_notional: Decimal::from_str(
                    &cfg.risk.max_notional_per_market_usd.to_string(),
                )?,
                max_leverage: Decimal::ONE,
                max_drawdown_ratio: Decimal::from_str(
                    &(cfg.risk.max_drawdown_pct / 100.0).to_string(),
                )?,
                max_snapshot_age_ms: 30_000,
            };
            execution = execution.with_authority(
                crate::aegis::Aegis::new(storage.clone(), &account, limits)?,
                cfg.risk.max_slippage_bps,
            )?;
            execution = execution.with_ledger(crate::ledger::FinancialLedger::new(
                storage.clone(),
                crate::ledger::AccountScope::hyperliquid(
                    crate::ledger::LedgerEnvironment::Mainnet,
                    &account,
                )?,
            )?);
        }

        let session_id = storage.begin_session(
            &cfg,
            &venues
                .iter()
                .map(|v| v.kind().as_str().to_string())
                .collect::<Vec<_>>(),
            &symbols,
        )?;

        // Restart reconciliation
        let sm = StateManager::new(storage.clone());
        sm.reconcile_with_venues(&cfg, &venues).await?;

        // Preflight equity check
        let mut init_equity = Decimal::ZERO;
        for v in &venues {
            match v.account_equity_usd().await {
                Ok(eq) => {
                    if let Ok(d) = Decimal::try_from(eq) {
                        init_equity = init_equity.max(d);
                    }
                    let _ = portfolio.sync_equity(v.kind().as_str(), eq);
                }
                Err(e) => tracing::warn!(venue=v.kind().as_str(), error=%e, "equity probe failed"),
            }
        }

        let authority = match control_authority {
            Some(a) => a,
            None => EngineControlAuthority::new(
                "main-engine",
                Arc::new(AtomicBool::new(false)),
                Some(storage.clone()),
            ),
        };
        execution = execution.with_pilot_authority(authority.pilot.clone());
        for v in &venues {
            v.attach_pilot_authority(authority.pilot.clone());
        }
        let stop = authority.stop_flag.clone();
        if install_ctrlc {
            let stop_signal = stop.clone();
            let _ = ctrlc::set_handler(move || {
                tracing::warn!("SIGINT received - scheduling graceful shutdown");
                stop_signal.store(true, Ordering::SeqCst);
            });
        }

        let mut u_cfg = unified_cfg.unwrap_or_default();
        u_cfg.generate_grid_proposals = false;
        u_cfg
            .enabled_families
            .retain(|f| !matches!(f, crate::fang_engine::StrategyFamily::BoundedGrid));
        if let Ok(mv) = Decimal::try_from(cfg.risk.max_notional_per_market_usd) {
            u_cfg.max_market_notional = u_cfg.max_market_notional.min(mv);
        }

        let policy = sovereign_policy.unwrap_or_else(|| SovereignRiskPolicy {
            max_market_notional_usd: Decimal::try_from(cfg.risk.max_notional_per_market_usd)
                .unwrap_or(Decimal::from(5_000u32)),
            max_total_exposure_ratio: Decimal::try_from(cfg.risk.max_open_exposure_pct / 100.0)
                .unwrap_or(Decimal::new(50, 2)),
            max_daily_loss_ratio: Decimal::try_from(cfg.risk.max_daily_loss_pct / 100.0)
                .unwrap_or(Decimal::new(3, 2)),
            max_weekly_loss_ratio: Decimal::try_from(cfg.risk.max_weekly_loss_pct / 100.0)
                .unwrap_or(Decimal::new(8, 2)),
            max_drawdown_ratio: Decimal::try_from(cfg.risk.max_drawdown_pct / 100.0)
                .unwrap_or(Decimal::new(15, 2)),
            ..Default::default()
        });
        if let (Ok(cs), Ok(rs)) = (u_cfg.config_sha3(), policy.policy_sha3()) {
            authority.pilot.set_deployed_digests(cs, rs);
        }

        // Reconstruct durable execution journal from open positions; unknown ownership stays
        // `"unattributed"` so protective stops keep working while entry gating stays honest.
        let mut restored_journal = Vec::new();
        if let Ok(open) = storage.list_open_positions() {
            for p in open {
                if let Ok(qty) = Decimal::try_from(p.size.abs()) {
                    if qty > Decimal::ZERO {
                        restored_journal.push(ExecutionJournalRecord {
                            cloid: format!("restored-pos-{}", p.id),
                            exchange_oid: None,
                            symbol: p.symbol,
                            side: if p.side.eq_ignore_ascii_case("LONG") {
                                crate::core::Side::Long
                            } else {
                                crate::core::Side::Short
                            },
                            owner_strategy: p.strategy.unwrap_or_else(|| "unattributed".into()),
                            requested_qty: qty,
                            limit_price: Decimal::try_from(p.entry_price).ok(),
                            filled_qty: qty,
                            state: OrderLifecycleState::Filled,
                            submitted_at_ms: p.opened_at.timestamp_millis(),
                        });
                    }
                }
            }
        }

        let now_init_ms = Utc::now().timestamp_millis();
        let today_key = utc_day_key(now_init_ms);
        let cur_week_key = utc_week_key(now_init_ms);
        let saved_daily_key = storage.get_meta_kv("aegis_daily_key").ok().flatten();
        let saved_weekly_key = storage.get_meta_kv("aegis_weekly_key").ok().flatten();
        let mut persisted_daily = if saved_daily_key.as_deref() == Some(today_key.as_str()) {
            load_decimal_kv(&storage, "aegis_daily_pnl_usd").unwrap_or(Decimal::ZERO)
        } else {
            Decimal::ZERO
        };
        let mut persisted_weekly = if saved_weekly_key.as_deref() == Some(cur_week_key.as_str()) {
            load_decimal_kv(&storage, "aegis_weekly_pnl_usd").unwrap_or(Decimal::ZERO)
        } else {
            Decimal::ZERO
        };
        if !any_prod && persisted_daily == Decimal::ZERO && saved_daily_key.is_none() {
            if let Ok(d) = storage.sum_realized_since(Utc::now() - chrono::Duration::hours(24)) {
                if let Ok(dec) = Decimal::try_from(d) {
                    persisted_daily = dec;
                }
            }
        }
        if !any_prod && persisted_weekly == Decimal::ZERO && saved_weekly_key.is_none() {
            if let Ok(w) = storage.sum_realized_since(Utc::now() - chrono::Duration::days(7)) {
                if let Ok(dec) = Decimal::try_from(w) {
                    persisted_weekly = dec;
                }
            }
        }
        let persisted_hwm = load_decimal_kv(&storage, "aegis_high_water_equity_usd")
            .unwrap_or(Decimal::ZERO)
            .max(init_equity);
        let cash_flow_baseline_usd =
            load_decimal_kv(&storage, "aegis_external_cashflow_baseline_usd")
                .unwrap_or(Decimal::ZERO);
        let persisted_breaker = storage
            .get_meta_kv("aegis_breaker_latched_reason")
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty());
        let settlement_cursor_ms = storage
            .get_meta_kv("nexus_settlement_cursor_ms")
            .ok()
            .flatten()
            .and_then(|s| s.trim().parse::<i64>().ok())
            .unwrap_or_else(|| now_init_ms.saturating_sub(300_000));
        if persisted_breaker.is_some() {
            authority.update_runtime_status(0, 0, true, true);
        }

        let mut resolved_constraints = instrument_constraints;
        if resolved_constraints.is_empty() {
            for v in &venues {
                if let Ok(map) = v.instrument_constraints().await {
                    resolved_constraints.extend(map);
                }
            }
        }

        let mut learning_samples = Vec::new();
        if let Some(ref store) = settlement_store {
            if let Ok(outcomes) = store.emitted_learning_outcomes() {
                for lo in &outcomes {
                    let risk_usd = load_decimal_kv(
                        &storage,
                        &format!("settlement_risk_rt:{}", lo.round_trip_id),
                    )
                    .or_else(|| {
                        load_decimal_kv(&storage, &format!("settlement_risk_sym:{}", lo.symbol))
                    });
                    if let Some(s) = shadow_sample_from_verified(lo, risk_usd) {
                        learning_samples.push(s);
                    }
                }
            }
        }

        let engine = Self {
            _account_ownership: ownership,
            cfg,
            storage,
            venues,
            symbols,
            portfolio,
            execution,
            session_id,
            stop,
            heartbeat: Heartbeat::new(),
            live_override,
            signal_dedup: std::collections::HashMap::new(),
            dash,
            last_pulse_at: std::time::Instant::now(),
            drought: DroughtState::default(),
            started_at: Utc::now(),
            control_authority: authority,
            sovereign_policy: policy,
            unified_cfg: u_cfg,
            settlement_store,
            instrument_constraints: resolved_constraints,
            order_journal: restored_journal,
            halted_unresolved: false,
            halted_unprotected: false,
            daily_key: today_key,
            weekly_key: cur_week_key,
            daily_pnl_usd: persisted_daily,
            weekly_pnl_usd: persisted_weekly,
            session_pnl_usd: Decimal::ZERO,
            high_water_equity_usd: persisted_hwm,
            cash_flow_baseline_usd,
            aegis_breaker_reason: persisted_breaker,
            last_ipc_supervision_ms: 0,
            ws_order_evidence: Vec::new(),
            learning_samples,
            ipc_server: None,
            account_stream: None,
            last_ws_reconnect_seen: 0,
            settlement_cursor_ms,
        };
        engine.persist_risk_state();
        Ok(engine)
    }

    pub fn set_account_stream(&mut self, stream: crate::account_stream::AccountStreamHandle) {
        self.account_stream = Some(stream);
    }

    pub fn set_unified_strategy_config(&mut self, mut u_cfg: UnifiedStrategyConfig) -> Result<()> {
        u_cfg.generate_grid_proposals = false;
        u_cfg
            .enabled_families
            .retain(|f| !matches!(f, crate::fang_engine::StrategyFamily::BoundedGrid));
        if let Ok(mv) = Decimal::try_from(self.cfg.risk.max_notional_per_market_usd) {
            u_cfg.max_market_notional = u_cfg.max_market_notional.min(mv);
        }
        let cs = u_cfg.config_sha3()?;
        let rs = self.sovereign_policy.policy_sha3()?;
        self.unified_cfg = u_cfg;
        self.control_authority.pilot.set_deployed_digests(cs, rs);
        Ok(())
    }

    fn persist_risk_state(&self) {
        let _ = self.storage.set_meta_kv("aegis_daily_key", &self.daily_key);
        let _ = self
            .storage
            .set_meta_kv("aegis_daily_pnl_usd", &self.daily_pnl_usd.to_string());
        let _ = self
            .storage
            .set_meta_kv("aegis_weekly_key", &self.weekly_key);
        let _ = self
            .storage
            .set_meta_kv("aegis_weekly_pnl_usd", &self.weekly_pnl_usd.to_string());
        let _ = self.storage.set_meta_kv(
            "aegis_high_water_equity_usd",
            &self.high_water_equity_usd.to_string(),
        );
        let _ = self.storage.set_meta_kv(
            "aegis_external_cashflow_baseline_usd",
            &self.cash_flow_baseline_usd.to_string(),
        );
        let _ = self.storage.set_meta_kv(
            "nexus_settlement_cursor_ms",
            &self.settlement_cursor_ms.to_string(),
        );
        if let Some(ref reason) = self.aegis_breaker_reason {
            let _ = self
                .storage
                .set_meta_kv("aegis_breaker_latched_reason", reason);
        }
    }

    fn latch_breaker(&mut self, reason: &str) {
        if self.aegis_breaker_reason.is_none() {
            self.aegis_breaker_reason = Some(reason.to_string());
        }
        let _ = self.storage.set_meta_kv(
            "aegis_breaker_latched_reason",
            self.aegis_breaker_reason.as_deref().unwrap_or(reason),
        );
        let open_cnt = self
            .storage
            .list_open_positions()
            .map(|v| v.len())
            .unwrap_or(0);
        let unres = self.storage.count_unresolved_intents().unwrap_or(0);
        self.control_authority.update_runtime_status(
            open_cnt,
            unres,
            !self.halted_unprotected,
            true,
        );
    }

    fn rollover_utc_windows(&mut self, now_ms: i64) {
        let cur_day = utc_day_key(now_ms);
        let cur_week = utc_week_key(now_ms);
        let mut changed = false;
        if cur_day != self.daily_key {
            self.daily_key = cur_day;
            self.daily_pnl_usd = Decimal::ZERO;
            changed = true;
        }
        if cur_week != self.weekly_key {
            self.weekly_key = cur_week;
            self.weekly_pnl_usd = Decimal::ZERO;
            changed = true;
        }
        if changed {
            self.persist_risk_state();
        }
    }

    pub fn active_pilot(&self) -> Option<crate::live_gate::MonitoredPilotGrant> {
        self.control_authority.pilot.snapshot()
    }

    pub fn attach_monitored_pilot(
        &mut self,
        grant: crate::live_gate::MonitoredPilotGrant,
    ) -> Result<()> {
        self.attach_monitored_pilot_at(grant, Utc::now().timestamp_millis())
    }

    pub fn attach_monitored_pilot_at(
        &mut self,
        grant: crate::live_gate::MonitoredPilotGrant,
        now_ms: i64,
    ) -> Result<()> {
        anyhow::ensure!(
            !self.control_authority.is_emergency_suspended(),
            "cannot attach monitored pilot while emergency suspension is active"
        );
        anyhow::ensure!(
            grant.is_active_at(now_ms),
            "monitored pilot grant is expired or inactive at timestamp {now_ms}"
        );
        grant.limits().validate()?;
        if grant.scope() == crate::live_gate::PilotGrantScope::ProductionMainnet {
            anyhow::ensure!(
                grant.build_identity() == crate::trusted_evidence::build_identity(),
                "monitored pilot grant build identity does not match running executable"
            );
            anyhow::ensure!(
                grant.strategy_config_sha3() == self.unified_cfg.config_sha3()?,
                "monitored pilot grant strategy configuration digest mismatch"
            );
            anyhow::ensure!(
                grant.risk_policy_sha3() == self.sovereign_policy.policy_sha3()?,
                "monitored pilot grant risk policy digest mismatch"
            );
        }
        self.control_authority
            .set_active_pilot_session_id(Some(grant.session_id().to_string()));
        self.control_authority.pilot.attach(grant);
        Ok(())
    }

    pub fn renew_pilot_supervision(&mut self, now_ms: i64) -> Result<()> {
        anyhow::ensure!(
            !self.control_authority.is_emergency_suspended(),
            "cannot renew operator supervision while emergency suspension is active"
        );
        let sid = self
            .control_authority
            .pilot
            .active_session_id()
            .ok_or_else(|| anyhow::anyhow!("no active monitored pilot attached"))?;
        self.control_authority.pilot.renew(&sid, now_ms)?;
        Ok(())
    }

    pub fn revoke_monitored_pilot(&mut self) {
        self.control_authority.pilot.revoke("revoked by operator");
        self.control_authority.set_active_pilot_session_id(None);
    }

    pub fn control_authority(&self) -> &EngineControlAuthority {
        &self.control_authority
    }

    pub fn settlement_store(&self) -> Option<&SettlementStore> {
        self.settlement_store.as_ref()
    }

    pub fn learning_samples(&self) -> &[ShadowSample] {
        &self.learning_samples
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn halted_unresolved(&self) -> bool {
        self.halted_unresolved
    }

    pub fn halted_unprotected(&self) -> bool {
        self.halted_unprotected
    }

    pub fn inject_journal_record_for_recovery(&mut self, rec: ExecutionJournalRecord) {
        self.order_journal.push(rec);
    }

    pub fn record_realized_pnl(&mut self, pnl_usd: Decimal) {
        self.daily_pnl_usd += pnl_usd;
        self.weekly_pnl_usd += pnl_usd;
        self.session_pnl_usd += pnl_usd;
    }

    pub fn push_ws_order_evidence(&mut self, snap: ExchangeOrderSnapshot) {
        self.ws_order_evidence.push(snap);
        if self.ws_order_evidence.len() > 256 {
            let drain = self.ws_order_evidence.len() - 256;
            self.ws_order_evidence.drain(0..drain);
        }
    }

    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    fn stop_flag_path(&self) -> std::path::PathBuf {
        self.cfg.data_dir.join("STOP")
    }

    /// Shared stop flag. External tasks (e.g. the TUI) may clone this to signal
    /// graceful shutdown; it is the same atomic triggered by the SIGINT handler.
    /// Authoritatively reconciles local order journal & unresolved intents against exchange REST
    /// snapshots, consuming `RecoveryAction`s and blocking new entries whenever state is uncertain.
    pub async fn reconcile_startup_and_recovery(&mut self) -> Result<RecoveryReport> {
        let mut ws_needs_backfill = false;
        let mut ws_reconnects = self.last_ws_reconnect_seen;
        if let Some(ref stream) = self.account_stream {
            let batch = stream.drain_updates();
            ws_needs_backfill =
                batch.needs_rest_backfill || batch.reconnect_count > self.last_ws_reconnect_seen;
            ws_reconnects = batch.reconnect_count;
            for ord in batch.orders {
                let dup = self.ws_order_evidence.iter().any(|existing| {
                    existing.exchange_oid == ord.exchange_oid
                        && existing.state == ord.state
                        && existing.filled_qty == ord.filled_qty
                        && existing.observed_at_ms == ord.observed_at_ms
                });
                if !dup {
                    self.push_ws_order_evidence(ord);
                }
            }
            if let Some(ref store) = self.settlement_store {
                for f in &batch.fills {
                    let strat = self.resolve_fill_strategy(f);
                    if let Err(e) = store.record_fill(f, strat.as_deref()) {
                        tracing::error!(error=%e, fill_id=%f.fill_id, "WS fill persistence failed");
                        self.halted_unresolved = true;
                    }
                }
                for fp in &batch.fundings {
                    if let Err(e) = store.record_funding(fp) {
                        tracing::error!(error=%e, funding_id=%fp.funding_id, "WS funding persistence failed");
                        self.halted_unresolved = true;
                    }
                }
            }
        }

        let mut ex_orders = Vec::new();
        let mut ex_positions = Vec::new();
        let mut venue_query_failed = false;
        for v in &self.venues {
            match v.open_exchange_orders().await {
                Ok(orders) => ex_orders.extend(orders),
                Err(e) => {
                    let msg = e.to_string();
                    if v.is_production_venue()
                        || !msg.contains("does not supply authoritative open order snapshots")
                    {
                        venue_query_failed = true;
                        tracing::warn!(venue=v.kind().as_str(), error=%e, "open_exchange_orders failed; blocking new entries");
                    } else {
                        tracing::debug!(venue=v.kind().as_str(), error=%e, "open_exchange_orders unavailable");
                    }
                }
            }
            match v.open_positions().await {
                Ok(pos) => {
                    for p in pos {
                        if let Ok(qty) = Decimal::try_from(p.size.abs()) {
                            if qty > Decimal::ZERO {
                                ex_positions.push(ExchangeOpenPositionSnapshot {
                                    symbol: p.symbol,
                                    side: if p.size >= 0.0 {
                                        crate::core::Side::Long
                                    } else {
                                        crate::core::Side::Short
                                    },
                                    qty,
                                });
                            }
                        }
                    }
                }
                Err(e) => {
                    venue_query_failed = true;
                    tracing::warn!(venue=v.kind().as_str(), error=%e, "open_positions recovery query failed; blocking new entries");
                }
            }
        }

        // Empty `open_exchange_orders` NEVER clears unresolved SQLite intents without explicit
        // exchange terminal/order-status evidence or fill evidence.
        if let Ok(unresolved_list) = self.storage.list_unresolved_intents() {
            for (_id, intent) in unresolved_list {
                let in_journal = self.order_journal.iter().any(|r| r.cloid == intent.id);
                if !in_journal {
                    self.order_journal.push(ExecutionJournalRecord {
                        cloid: intent.id.clone(),
                        exchange_oid: None,
                        symbol: intent.symbol.clone(),
                        side: intent.side,
                        owner_strategy: intent
                            .source_signal
                            .clone()
                            .unwrap_or_else(|| "unattributed".into()),
                        requested_qty: Decimal::try_from(intent.size).unwrap_or(Decimal::ZERO),
                        limit_price: intent.limit_price.and_then(|p| Decimal::try_from(p).ok()),
                        filled_qty: Decimal::ZERO,
                        state: OrderLifecycleState::Submitted,
                        submitted_at_ms: intent.created_at.timestamp_millis(),
                    });
                }
                let known_open = ex_orders
                    .iter()
                    .any(|o| o.cloid.as_deref() == Some(intent.id.as_str()));
                let known_ws = self
                    .ws_order_evidence
                    .iter()
                    .any(|o| o.cloid.as_deref() == Some(intent.id.as_str()));
                if !known_open && !known_ws {
                    if let Some(v) = self
                        .venues
                        .iter()
                        .find(|v| v.kind().as_str() == intent.venue)
                    {
                        if let Ok(obs) = v.order_status_observation(&intent).await {
                            if let crate::order_status::Outcome::Order(ord) = obs.outcome {
                                let state = match ord.status.as_str() {
                                    "filled" => OrderLifecycleState::Filled,
                                    "canceled" | "cancelled" | "marginCanceled" => {
                                        OrderLifecycleState::Cancelled
                                    }
                                    s if s.contains("reject") => OrderLifecycleState::Rejected,
                                    "open" => OrderLifecycleState::Acknowledged,
                                    _ => OrderLifecycleState::Unresolved,
                                };
                                if matches!(
                                    state,
                                    OrderLifecycleState::Filled
                                        | OrderLifecycleState::Cancelled
                                        | OrderLifecycleState::Rejected
                                        | OrderLifecycleState::Acknowledged
                                ) {
                                    let oid_num = ord.order_id.parse::<u64>().unwrap_or(0);
                                    if oid_num > 0 {
                                        let filled = (ord.original_size - ord.remaining_size)
                                            .max(Decimal::ZERO);
                                        self.push_ws_order_evidence(ExchangeOrderSnapshot {
                                            source: crate::ledger::EvidenceSource::RestSnapshot,
                                            cloid: Some(intent.id.clone()),
                                            exchange_oid: oid_num,
                                            symbol: intent.symbol.clone(),
                                            side: intent.side,
                                            state,
                                            filled_qty: filled,
                                            avg_fill_px: Some(ord.limit_price),
                                            observed_at_ms: obs.acquired_finished_ms,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let rep = reconcile_execution_state(
            &self.order_journal,
            &ex_orders,
            &self.ws_order_evidence,
            &ex_positions,
        )?;
        for act in &rep.actions {
            match act {
                RecoveryAction::MarkAcknowledged {
                    cloid,
                    exchange_oid,
                } => {
                    if let Some(rec) = self.order_journal.iter_mut().find(|r| r.cloid == *cloid) {
                        rec.state = OrderLifecycleState::Acknowledged;
                        rec.exchange_oid = Some(*exchange_oid);
                    }
                }
                RecoveryAction::ApplyPartialFill {
                    cloid,
                    exchange_oid,
                    total_filled_qty,
                    ..
                } => {
                    if let Some(rec) = self.order_journal.iter_mut().find(|r| r.cloid == *cloid) {
                        rec.state = OrderLifecycleState::PartiallyFilled;
                        rec.exchange_oid = Some(*exchange_oid);
                        rec.filled_qty = *total_filled_qty;
                    }
                }
                RecoveryAction::MarkFilled {
                    cloid,
                    exchange_oid,
                    total_filled_qty,
                } => {
                    if let Some(rec) = self.order_journal.iter_mut().find(|r| r.cloid == *cloid) {
                        rec.state = OrderLifecycleState::Filled;
                        rec.exchange_oid = Some(*exchange_oid);
                        rec.filled_qty = *total_filled_qty;
                    }
                }
                RecoveryAction::MarkCancelled {
                    cloid,
                    exchange_oid,
                } => {
                    if let Some(rec) = self.order_journal.iter_mut().find(|r| r.cloid == *cloid) {
                        rec.state = OrderLifecycleState::Cancelled;
                        if exchange_oid.is_some() {
                            rec.exchange_oid = *exchange_oid;
                        }
                    }
                }
                RecoveryAction::MarkRejected { cloid, .. } => {
                    if let Some(rec) = self.order_journal.iter_mut().find(|r| r.cloid == *cloid) {
                        rec.state = OrderLifecycleState::Rejected;
                    }
                }
                RecoveryAction::HoldUnresolved { cloid, reason } => {
                    tracing::warn!(cloid=%cloid, reason=%reason, "execution recovery hold: blocking new entries");
                }
                RecoveryAction::RefuseBlindRetry { cloid, state } => {
                    tracing::warn!(cloid=%cloid, ?state, "refusing blind retry on ambiguous order");
                }
                RecoveryAction::ReconcileOwnership { .. } => {}
            }
        }

        let settlement_ok = if ws_needs_backfill
            || !ex_orders.is_empty()
            || !self.ws_order_evidence.is_empty()
        {
            match self
                .reconcile_settlement_from_venues(Utc::now().timestamp_millis())
                .await
            {
                Ok(_) => true,
                Err(e) => {
                    tracing::error!(error=%e, "settlement REST backfill failed; retaining recovery hold");
                    false
                }
            }
        } else {
            true
        };
        if ws_needs_backfill && !venue_query_failed && settlement_ok {
            if let Some(ref stream) = self.account_stream {
                stream.mark_rest_backfill_complete(ws_reconnects);
                self.last_ws_reconnect_seen = ws_reconnects;
            }
        }

        let db_unresolved = self.storage.count_unresolved_intents()?;
        let holds = rep.unresolved_holds.max(db_unresolved);
        self.halted_unresolved =
            holds > 0 || venue_query_failed || !rep.consistency.consistent || !settlement_ok;
        let open_cnt = self
            .storage
            .list_open_positions()
            .map(|v| v.len())
            .unwrap_or(0);
        self.control_authority.update_runtime_status(
            open_cnt,
            holds,
            !self.halted_unprotected,
            self.aegis_breaker_reason.is_some(),
        );
        Ok(rep)
    }

    fn resolve_fill_strategy(&self, fill: &ExchangeFill) -> Option<String> {
        if let Some(ref cloid) = fill.client_order_id {
            if let Some(rec) = self.order_journal.iter().find(|r| r.cloid == *cloid) {
                if rec.owner_strategy != "unattributed" {
                    return Some(rec.owner_strategy.clone());
                }
            }
            if let Ok(Some(s)) = self
                .storage
                .get_meta_kv(&format!("settlement_strategy_intent:{cloid}"))
            {
                if !s.trim().is_empty() {
                    return Some(s);
                }
            }
        }
        if fill.order_id > 0 {
            if let Ok(Some(cloid)) = self
                .storage
                .get_meta_kv(&format!("settlement_oid_intent:{}", fill.order_id))
            {
                if let Ok(Some(s)) = self
                    .storage
                    .get_meta_kv(&format!("settlement_strategy_intent:{cloid}"))
                {
                    if !s.trim().is_empty() {
                        return Some(s);
                    }
                }
            }
            if let Some(rec) = self
                .order_journal
                .iter()
                .find(|r| r.exchange_oid == Some(fill.order_id))
            {
                if rec.owner_strategy != "unattributed" {
                    return Some(rec.owner_strategy.clone());
                }
            }
        }
        if let Ok(open) = self.storage.list_open_positions() {
            let matching: Vec<_> = open
                .iter()
                .filter(|p| p.symbol.eq_ignore_ascii_case(&fill.symbol))
                .collect();
            if matching.len() == 1 {
                if let Some(ref s) = matching[0].strategy {
                    if !s.trim().is_empty() && s != "unattributed" {
                        return Some(s.clone());
                    }
                }
            }
        }
        None
    }

    /// Reconciles genuine exchange fills, fees, funding payments (with verified complete coverage),
    /// and non-funding external cash flows into durable `SettlementStore`, emitting verified
    /// net-of-cost COGNITION learning samples scaled by each trade's original entry risk.
    pub async fn reconcile_settlement_from_venues(&mut self, now_ms: i64) -> Result<usize> {
        let Some(store) = self.settlement_store.clone() else {
            return Ok(0);
        };
        let end_ms = now_ms.max(self.settlement_cursor_ms + 1);
        let since_ms = self.settlement_cursor_ms.saturating_sub(60_000).max(0);
        let mut ingested = 0usize;
        let mut any_supported = false;
        let mut covered_symbols: std::collections::BTreeSet<String> = self
            .symbols
            .iter()
            .map(|s| s.trim().to_ascii_uppercase())
            .collect();
        if let Ok(open) = self.storage.list_open_positions() {
            for p in open {
                covered_symbols.insert(p.symbol.trim().to_ascii_uppercase());
            }
        }

        let mut cash_flows_total = self.cash_flow_baseline_usd;
        let mut cf_updated = false;
        for v in &self.venues {
            match v.fetch_account_fills(since_ms, end_ms).await {
                Ok(fills) => {
                    any_supported = true;
                    for f in &fills {
                        covered_symbols.insert(f.symbol.trim().to_ascii_uppercase());
                        let mapped_cloid = f.client_order_id.clone().or_else(|| {
                            (f.order_id > 0)
                                .then(|| {
                                    self.storage
                                        .get_meta_kv(&format!(
                                            "settlement_oid_intent:{}",
                                            f.order_id
                                        ))
                                        .ok()
                                        .flatten()
                                })
                                .flatten()
                        });
                        if let Some(ref c) = mapped_cloid {
                            let _ = self
                                .storage
                                .set_meta_kv(&format!("settlement_fill_cloid:{}", f.fill_id), c);
                        }
                        let strat = self.resolve_fill_strategy(f);
                        if store.record_fill(f, strat.as_deref())? {
                            ingested += 1;
                        }
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    if v.is_production_venue()
                        || !msg.contains("does not supply authoritative account fills")
                    {
                        return Err(e.context("fetch_account_fills failed"));
                    }
                }
            }
            match v.fetch_account_funding(since_ms, end_ms).await {
                Ok(fundings) => {
                    any_supported = true;
                    for fp in &fundings {
                        covered_symbols.insert(fp.symbol.trim().to_ascii_uppercase());
                        if store.record_funding(fp)? {
                            ingested += 1;
                        }
                    }
                    // Clean pagination termination guarantees full funding coverage for [since_ms, end_ms].
                    for sym in &covered_symbols {
                        store.record_funding_coverage(
                            &crate::settlement::FundingCoverageWindow {
                                symbol: sym.clone(),
                                start_ms: since_ms,
                                end_ms,
                                complete: true,
                            },
                        )?;
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    if v.is_production_venue()
                        || !msg.contains("does not supply authoritative account funding")
                    {
                        return Err(e.context("fetch_account_funding failed"));
                    }
                }
            }
            match v.fetch_account_cash_flows(since_ms, end_ms).await {
                Ok(flows) => {
                    any_supported = true;
                    for cf in &flows {
                        if store.record_external_cash_flow(cf)? {
                            ingested += 1;
                            if cf.token == "USDC" {
                                cash_flows_total += cf.signed_amount;
                                cf_updated = true;
                            }
                        }
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    if v.is_production_venue()
                        || !msg.contains("does not supply authoritative account cash flows")
                    {
                        return Err(e.context("fetch_account_cash_flows failed"));
                    }
                }
            }
        }

        if cf_updated {
            let delta_cf = cash_flows_total - self.cash_flow_baseline_usd;
            self.cash_flow_baseline_usd = cash_flows_total;
            self.high_water_equity_usd = (self.high_water_equity_usd + delta_cf).max(Decimal::ZERO);
            self.persist_risk_state();
        }

        if !any_supported && ingested == 0 {
            return Ok(0);
        }

        let (rts, newly_emitted) = store.recompute_and_commit()?;
        for lo in &newly_emitted {
            let rt_opt = rts.iter().find(|r| r.round_trip_id == lo.round_trip_id);
            let mut risk_usd: Option<Decimal> = None;
            if let Some(rt) = rt_opt {
                for eid in &rt.entry_fill_ids {
                    if let Ok(Some(cloid)) = self
                        .storage
                        .get_meta_kv(&format!("settlement_fill_cloid:{eid}"))
                    {
                        if let Some(r) = load_decimal_kv(
                            &self.storage,
                            &format!("settlement_risk_intent:{cloid}"),
                        ) {
                            risk_usd = Some(r);
                            break;
                        }
                    }
                }
            }
            if risk_usd.is_none() {
                risk_usd =
                    load_decimal_kv(&self.storage, &format!("settlement_risk_sym:{}", lo.symbol));
            }
            if let Some(r) = risk_usd {
                let _ = self.storage.set_meta_kv(
                    &format!("settlement_risk_rt:{}", lo.round_trip_id),
                    &r.to_string(),
                );
            }
            if let Some(sample) = shadow_sample_from_verified(lo, risk_usd) {
                self.learning_samples.push(sample);
            }
            if any_supported {
                self.record_realized_pnl(lo.verified_net_pnl_usdc);
            }
        }
        self.settlement_cursor_ms = end_ms;
        self.persist_risk_state();
        Ok(newly_emitted.len())
    }

    /// Reconciles native `reduce_only: true` exchange protective stops against all open durable
    /// positions. Remains authorized EVEN WHEN new entries are suspended, and enforces place-new-
    /// first before canceling superseded stops so positions are never left unprotected.
    pub async fn reconcile_protective_orders(&mut self) -> Result<ProtectiveReconcileReport> {
        let open = self.storage.list_open_positions()?;
        let mut protected_positions = Vec::new();
        for p in &open {
            let side = if p.side.eq_ignore_ascii_case("LONG") {
                crate::core::Side::Long
            } else {
                crate::core::Side::Short
            };
            let qty = Decimal::try_from(p.size.abs()).unwrap_or(Decimal::ZERO);
            let entry = Decimal::try_from(p.entry_price).unwrap_or(Decimal::ZERO);
            if qty <= Decimal::ZERO || entry <= Decimal::ZERO {
                continue;
            }
            let stop = p
                .stop_price
                .and_then(|s| Decimal::try_from(s).ok())
                .filter(|s| {
                    *s > Decimal::ZERO
                        && match side {
                            crate::core::Side::Long => *s < entry,
                            crate::core::Side::Short => *s > entry,
                        }
                })
                .unwrap_or_else(|| match side {
                    crate::core::Side::Long => (entry * Decimal::new(98, 2)).round_dp(4),
                    crate::core::Side::Short => (entry * Decimal::new(102, 2)).round_dp(4),
                });
            protected_positions.push(OpenProtectedPosition {
                symbol: p.symbol.clone(),
                owner_strategy: p.strategy.clone().unwrap_or_else(|| "unattributed".into()),
                side,
                qty,
                entry_price: entry,
                stop_price: stop,
            });
        }

        let mut exchange_stops = Vec::new();
        let mut supports_protective = false;
        for v in &self.venues {
            if let Ok(stops) = v.list_protective_stops().await {
                supports_protective = true;
                exchange_stops.extend(stops);
            }
        }
        let blocked = self.control_authority.is_emergency_suspended() || self.halted_unresolved;
        if !supports_protective {
            // Venue does not implement native protective inspection: remain fail-safe on protection state.
            let verified = protected_positions.is_empty();
            self.halted_unprotected = !verified;
            self.control_authority.update_runtime_status(
                open.len(),
                self.storage.count_unresolved_intents().unwrap_or(0),
                verified,
                false,
            );
            return Ok(ProtectiveReconcileReport {
                entries_blocked_by_caller: blocked,
                protection_management_permitted: true,
                protection_verified: verified,
                halt_new_entries: !verified,
                actions: Vec::new(),
            });
        }

        let initial_rep = ProtectiveOrderManager::reconcile_positions_and_stops(
            &protected_positions,
            &exchange_stops,
            &std::collections::BTreeSet::new(),
            blocked,
        )?;
        let mut mutated = false;
        let mut failed_syms = std::collections::BTreeSet::new();
        let venue_idx_for = |symbol: &str| -> Option<usize> {
            if let Some(pos_venue) = open
                .iter()
                .find(|p| p.symbol == symbol)
                .map(|p| p.venue.as_str())
            {
                if let Some(idx) = self
                    .venues
                    .iter()
                    .position(|v| v.kind().as_str() == pos_venue)
                {
                    return Some(idx);
                }
                return None;
            }
            if self.venues.len() == 1 {
                Some(0)
            } else {
                None
            }
        };
        for act in &initial_rep.actions {
            match act {
                ProtectiveReconcileAction::PlaceReduceOnlyStop(spec) => {
                    mutated = true;
                    if let Some(idx) = venue_idx_for(&spec.symbol) {
                        if self.venues[idx].place_protective_stop(spec).await.is_err() {
                            failed_syms.insert(spec.symbol.clone());
                        }
                    } else {
                        failed_syms.insert(spec.symbol.clone());
                    }
                }
                ProtectiveReconcileAction::ReplaceReduceOnlyStop {
                    replace_exchange_oid,
                    old_cloid,
                    new_spec,
                } => {
                    mutated = true;
                    if let Some(idx) = venue_idx_for(&new_spec.symbol) {
                        // Safe ordering: place & verify new stop BEFORE canceling old stop.
                        match self.venues[idx].place_protective_stop(new_spec).await {
                            Ok(_) => {
                                if self.venues[idx]
                                    .cancel_protective_stop(
                                        &new_spec.symbol,
                                        *replace_exchange_oid,
                                        old_cloid,
                                    )
                                    .await
                                    .is_err()
                                {
                                    failed_syms.insert(new_spec.symbol.clone());
                                }
                            }
                            Err(_) => {
                                failed_syms.insert(new_spec.symbol.clone());
                            }
                        }
                    } else {
                        failed_syms.insert(new_spec.symbol.clone());
                    }
                }
                ProtectiveReconcileAction::CancelOrphanStop {
                    symbol,
                    exchange_oid,
                    cloid,
                } => {
                    mutated = true;
                    if let Some(idx) = venue_idx_for(symbol) {
                        if self.venues[idx]
                            .cancel_protective_stop(symbol, *exchange_oid, cloid)
                            .await
                            .is_err()
                        {
                            failed_syms.insert(symbol.clone());
                        }
                    } else {
                        failed_syms.insert(symbol.clone());
                    }
                }
                _ => {}
            }
        }

        let mut post_read_failed = false;
        let final_rep = if mutated {
            let mut latest_stops = Vec::new();
            for v in &self.venues {
                match v.list_protective_stops().await {
                    Ok(s) => latest_stops.extend(s),
                    Err(e) => {
                        let msg = e.to_string();
                        if v.is_production_venue()
                            || !msg.contains("does not supply native protective stop inspection")
                        {
                            post_read_failed = true;
                        }
                    }
                }
            }
            ProtectiveOrderManager::reconcile_positions_and_stops(
                &protected_positions,
                &latest_stops,
                &failed_syms,
                blocked,
            )?
        } else {
            initial_rep
        };

        self.halted_unprotected =
            !final_rep.protection_verified || final_rep.halt_new_entries || post_read_failed;
        self.control_authority.update_runtime_status(
            open.len(),
            self.storage.count_unresolved_intents().unwrap_or(0),
            final_rep.protection_verified,
            false,
        );
        Ok(final_rep)
    }

    /// Evaluates shared deterministic FANG planning directly inside `Engine` using causal
    /// `cognition_shadow` regimes and current `PortfolioManager` budgets.
    pub fn unified_planning_step(
        &mut self,
        tick: u64,
        snapshots: &[MarketSnapshot],
    ) -> Result<crate::fang_engine::UnifiedPlanOutput> {
        let eq_dec =
            Decimal::try_from(self.portfolio.last_known_equity().max(0.0)).unwrap_or(Decimal::ZERO);
        let max_tot = eq_dec
            * Decimal::try_from((self.cfg.risk.max_open_exposure_pct / 100.0).max(0.0))
                .unwrap_or(Decimal::new(50, 2));
        let open = self.storage.list_open_positions()?;
        let mut occupied = BTreeMap::new();
        let mut sym_exposure: BTreeMap<String, Decimal> = BTreeMap::new();
        let mut total_exposure = Decimal::ZERO;
        for p in &open {
            let mark_px = snapshots
                .iter()
                .find(|s| s.symbol == p.symbol && s.mid.is_finite() && s.mid > 0.0)
                .map(|s| s.mid);
            if let Some(m) = mark_px {
                let ntl = Decimal::try_from(p.size.abs() * m).unwrap_or(Decimal::ZERO);
                total_exposure += ntl;
                *sym_exposure
                    .entry(p.symbol.clone())
                    .or_insert(Decimal::ZERO) += ntl;
            }
            occupied.insert(
                p.symbol.clone(),
                p.strategy.clone().unwrap_or_else(|| p.side.clone()),
            );
        }

        let mut watchlist = Vec::new();
        let mut candles_map = BTreeMap::new();
        let mut marks = BTreeMap::new();
        let mut regimes = BTreeMap::new();
        let mut microstructure_map: BTreeMap<String, MicrostructureSnapshot> = BTreeMap::new();
        let mut per_sym_rem = BTreeMap::new();
        let mut max_as_of_ms = 0i64;
        for snap in snapshots {
            if !snap.mid.is_finite() || snap.mid <= 0.0 || snap.candles.is_empty() {
                continue;
            }
            let sym_ts = snap.candles.last().unwrap().ts.timestamp_millis();
            max_as_of_ms = max_as_of_ms.max(sym_ts);
            let reg = infer_regime(
                &snap.candles,
                &RegimeInferenceConfig {
                    lookback_bars: 12,
                    ..Default::default()
                },
            )
            .map(|r| r.regime)
            .unwrap_or(InferredRegime::InsufficientHistory);
            let mark = Decimal::try_from(snap.mid)
                .unwrap_or(Decimal::ZERO)
                .round_dp(6);
            let micro = snap.microstructure.clone().unwrap_or_else(|| {
                compute_microstructure_snapshot(
                    &snap.symbol,
                    sym_ts,
                    None,
                    None,
                    &[],
                    &snap.candles,
                    snap.funding_bps_1h,
                    None,
                    1_000.0,
                )
            });
            watchlist.push(snap.symbol.clone());
            candles_map.insert(snap.symbol.clone(), snap.candles.clone());
            marks.insert(snap.symbol.clone(), mark);
            regimes.insert(snap.symbol.clone(), reg);
            microstructure_map.insert(snap.symbol.clone(), micro);
            let used = sym_exposure
                .get(&snap.symbol)
                .copied()
                .unwrap_or(Decimal::ZERO);
            per_sym_rem.insert(
                snap.symbol.clone(),
                (self.unified_cfg.max_market_notional - used).max(Decimal::ZERO),
            );
        }

        let as_of_ms = if max_as_of_ms > 0 {
            max_as_of_ms
        } else {
            Utc::now().timestamp_millis()
        };
        let ensemble: Option<EnsembleRanking> = if self.learning_samples.is_empty() {
            None
        } else {
            let candidate_strategies = vec![
                "adaptive_ema_anchor".to_string(),
                "momentum_breakout".to_string(),
                "mean_reversion_bb".to_string(),
                "rsi_divergence".to_string(),
                "trend_continuation".to_string(),
                "volatility_breakout".to_string(),
            ];
            rank_and_weight_ensemble(
                as_of_ms,
                "*",
                OutcomeSource::CallerSuppliedNormalizedR,
                &candidate_strategies,
                &self.learning_samples,
                &ShadowPolicy::default(),
                150.0,
                self.unified_cfg.round_trip_cost_bps,
            )
            .ok()
        };

        let input = FangMarketInput {
            as_of_ms,
            correlation_id: format!("engine-t{tick}"),
            watchlist: watchlist.clone(),
            candles: candles_map,
            marks,
            regimes,
            microstructure: microstructure_map,
            portfolio_equity_usd: eq_dec,
        };
        let budget = PlanningBudget {
            approved_symbols: watchlist.into_iter().collect(),
            excluded_symbols: std::collections::BTreeSet::new(),
            total_remaining_notional: (max_tot - total_exposure).max(Decimal::ZERO),
            per_symbol_remaining_notional: per_sym_rem,
            occupied_symbols: occupied,
            confidence_floor: 0.45,
        };
        FangUnifiedPlanner::evaluate_and_plan(&input, &budget, &self.unified_cfg, ensemble.as_ref())
    }

    /// Executes one authoritative converged engine tick (snapshots -> recovery -> native stops
    /// -> shared FANG plan -> AEGIS sovereign risk -> precision rounding -> durable execution).
    pub async fn step_once(&mut self, tick: u64) -> Result<EngineStepSummary> {
        self.step_once_at(tick, Utc::now().timestamp_millis()).await
    }

    /// Executes one authoritative converged engine tick at an explicit clock timestamp `now_ms`,
    /// enabling deterministic verification of monitored-pilot supervision leases & session limits.
    pub async fn step_once_at(&mut self, tick: u64, now_ms: i64) -> Result<EngineStepSummary> {
        let mut summary = EngineStepSummary {
            tick,
            ..Default::default()
        };
        let poll_ms = self.cfg.poll_interval_ms.max(250);
        self.heartbeat.beat("engine_tick");
        self.rollover_utc_windows(now_ms);
        let any_prod = self.venues.iter().any(|v| v.is_production_venue());

        let ipc_snap = self.control_authority.snapshot();
        if ipc_snap.last_supervision_renewed_ms > self.last_ipc_supervision_ms
            && !ipc_snap.emergency_suspended
        {
            self.last_ipc_supervision_ms = ipc_snap.last_supervision_renewed_ms;
            if let Some(sid) = self.control_authority.pilot.active_session_id() {
                let _ = self
                    .control_authority
                    .pilot
                    .renew(&sid, now_ms.max(ipc_snap.last_supervision_renewed_ms));
            }
        }
        if !ipc_snap.breaker_latched && self.aegis_breaker_reason.is_some() {
            // Audited IPC rearm cleared the breaker latch in storage; sync local state cleanly.
            if self
                .storage
                .get_meta_kv("aegis_breaker_latched_reason")
                .ok()
                .flatten()
                .is_none()
            {
                self.aegis_breaker_reason = None;
                self.high_water_equity_usd =
                    load_decimal_kv(&self.storage, "aegis_high_water_equity_usd")
                        .unwrap_or(Decimal::ZERO);
            }
        }

        if tick == 1 || tick.is_multiple_of((10_000 / poll_ms).max(1)) {
            for venue in &self.venues {
                match venue.account_equity_usd().await {
                    Ok(equity) => {
                        if let Ok(d) = Decimal::try_from(equity) {
                            if d > self.high_water_equity_usd {
                                self.high_water_equity_usd = d;
                                self.persist_risk_state();
                            }
                        }
                        if let Err(error) =
                            self.portfolio.sync_equity(venue.kind().as_str(), equity)
                        {
                            self.portfolio.invalidate_equity();
                            tracing::error!(%error, "equity persistence failed; new exposure blocked");
                        }
                    }
                    Err(error) => {
                        self.portfolio.invalidate_equity();
                        tracing::warn!(%error, "equity unavailable; new exposure blocked");
                    }
                }
            }
        }

        // === 1. Market snapshots ===
        let mut snapshots: Vec<MarketSnapshot> = Vec::new();
        for v in &self.venues {
            for sym in &self.symbols {
                match v
                    .snapshot(sym, self.cfg.strategies.global.lookback_candles)
                    .await
                {
                    Ok(snap) => snapshots.push(snap),
                    Err(e) => {
                        tracing::warn!(venue=v.kind().as_str(), symbol=sym, error=%e, "snapshot failed");
                        self.storage
                            .record_error(self.session_id, "snapshot", &e.to_string())?;
                    }
                }
            }
        }

        // === 2. Execution recovery & Native Protective Orders (always active) ===
        match self.reconcile_startup_and_recovery().await {
            Ok(rec) => summary.recovery_holds = rec.unresolved_holds,
            Err(e) => {
                self.halted_unresolved = true;
                tracing::warn!(error=%e, "execution recovery failed; blocking new entries");
            }
        }
        match self.reconcile_protective_orders().await {
            Ok(prot) => summary.protective_actions = prot.actions.len(),
            Err(e) => {
                self.halted_unprotected = true;
                tracing::warn!(error=%e, "protective stop reconciliation failed; blocking new entries");
            }
        }

        // === 3. Position trailing / TP / time-stop management & Authoritative Settlement ===
        let realized_before = self
            .storage
            .sum_realized_in_session(self.session_id)
            .unwrap_or(0.0);
        {
            let Engine {
                portfolio,
                execution,
                venues,
                cfg,
                ..
            } = &mut *self;
            if let Err(e) = portfolio
                .manage_open_positions(&snapshots, cfg, execution, venues)
                .await
            {
                self.halted_unresolved = true;
                tracing::warn!(error=%e, "position management error; halting new entries until resolved");
                let _ = self
                    .storage
                    .record_error(self.session_id, "exit", &e.to_string());
            }
        }
        if let Err(e) = self.reconcile_settlement_from_venues(now_ms).await {
            self.halted_unresolved = true;
            let msg = format!("settlement reconciliation failed: {e}");
            let _ = self
                .storage
                .record_error(self.session_id, "settlement", &msg);
        } else if !any_prod {
            // Non-production offline simulation venues without `fetch_account_fills` fall back
            // to local simulation `closed_trades` PnL delta.
            let has_fills_api = self.venues.iter().any(|v| v.account_identity().is_some());
            if !has_fills_api {
                let realized_after = self
                    .storage
                    .sum_realized_in_session(self.session_id)
                    .unwrap_or(realized_before);
                let realized_delta = realized_after - realized_before;
                if realized_delta.abs() > 1e-9 {
                    if let Ok(d) = Decimal::try_from(realized_delta) {
                        self.record_realized_pnl(d);
                    }
                }
            }
        }
        self.control_authority
            .pilot
            .update_pnl(self.daily_pnl_usd, self.session_pnl_usd);

        // === 4. Fail-closed admission gate on suspension, unresolved intents, missing protection, or pilot expiry/loss ===
        let active_grant = self.control_authority.pilot.snapshot();
        let entry_block_reason = if self.control_authority.is_emergency_suspended() {
            "emergency_suspended".to_string()
        } else if self.halted_unresolved {
            "unresolved_recovery_or_settlement_hold".to_string()
        } else if self.halted_unprotected {
            "unprotected_position_hold".to_string()
        } else if self.aegis_breaker_reason.is_some()
            || self.control_authority.snapshot().breaker_latched
        {
            self.aegis_breaker_reason
                .clone()
                .unwrap_or_else(|| "aegis_breaker_latched".into())
        } else if let Some(ref grant) = active_grant {
            if !grant.is_active_at(now_ms) {
                "pilot_lease_or_session_expired".to_string()
            } else if -self.daily_pnl_usd >= grant.limits().max_daily_loss_usd {
                self.latch_breaker("pilot max_daily_loss_usd breached");
                "pilot_daily_loss_limit".to_string()
            } else if -self.session_pnl_usd >= grant.limits().max_session_loss_usd {
                self.latch_breaker("pilot max_session_loss_usd breached");
                "pilot_session_loss_limit".to_string()
            } else {
                String::new()
            }
        } else if any_prod && self.cfg.live_trading {
            "no_active_restricted_pilot_grant".to_string()
        } else if !self.cfg.live_trading {
            format!(
                "mode_{}_entries_disabled",
                self.control_authority.snapshot().mode
            )
        } else {
            String::new()
        };

        let pilot_blocked = if let Some(ref grant) = active_grant {
            !grant.is_active_at(now_ms)
                || (-self.daily_pnl_usd >= grant.limits().max_daily_loss_usd)
                || (-self.session_pnl_usd >= grant.limits().max_session_loss_usd)
        } else {
            any_prod && self.cfg.live_trading
        };
        if self.control_authority.is_emergency_suspended()
            || self.halted_unresolved
            || self.halted_unprotected
            || self.aegis_breaker_reason.is_some()
            || pilot_blocked
        {
            summary.halts_blocked_entries = true;
            if tick.is_multiple_of((10_000 / poll_ms.max(1)).max(1)) {
                self.heartbeat.persist(&self.storage, self.session_id).ok();
            }
            self.control_authority
                .record_tick(tick, now_ms, &entry_block_reason);
            if self.dash.is_some() {
                self.update_dashboard(tick, &snapshots);
            }
            return Ok(summary);
        }

        // === 5. Shared FANG unified plan + AEGIS sovereign risk + precision rounding -> durable execution ===
        let plan_out = self.unified_planning_step(tick, &snapshots)?;
        summary.planned_candidates = plan_out.report.selected.len();
        let candles_by_sym: BTreeMap<String, Vec<crate::core::Candle>> = snapshots
            .iter()
            .filter(|s| !s.candles.is_empty())
            .map(|s| (s.symbol.to_ascii_uppercase(), s.candles.clone()))
            .collect();
        let corr_matrix = compute_correlation_matrix(&candles_by_sym, 24).ok();

        for cand in plan_out.report.selected {
            // Revalidate suspension, halts, and monitored pilot state immediately before every candidate!
            let active_now = self.control_authority.pilot.snapshot();
            let mid_batch_pilot_blocked = if let Some(ref grant) = active_now {
                !grant.is_active_at(now_ms)
                    || (-self.daily_pnl_usd >= grant.limits().max_daily_loss_usd)
                    || (-self.session_pnl_usd >= grant.limits().max_session_loss_usd)
            } else {
                any_prod && self.cfg.live_trading
            };
            if self.control_authority.is_emergency_suspended()
                || self.halted_unresolved
                || self.halted_unprotected
                || self.aegis_breaker_reason.is_some()
                || mid_batch_pilot_blocked
            {
                summary.halts_blocked_entries = true;
                break;
            }

            let sig = &cand.signal;
            let key = (sig.strategy.clone(), sig.symbol.clone());
            let now = std::time::Instant::now();
            if let Some(last) = self.signal_dedup.get(&key) {
                if now.duration_since(*last) < std::time::Duration::from_secs(60) {
                    continue;
                }
            }
            self.signal_dedup.insert(key, now);
            self.storage.record_signal(self.session_id, sig)?;
            self.push_signal_row(sig, "emitted");

            let Some(snap) = snapshots.iter().find(|s| s.symbol == sig.symbol) else {
                continue;
            };
            let regime_view: RegimeView = classify_view(&snap.candles, &self.cfg.strategies.regime);
            let atr_pct_now = if snap.candles.len() >= 15 && snap.mid > 0.0 {
                crate::utils::math::atr(&snap.candles, 14) / snap.mid * 100.0
            } else {
                0.0
            };

            // Acquire authoritative scoped account exposure evidence before any admission calculation.
            let mut latest_evidence: Option<
                crate::adapters::account_evidence::AccountExposureEvidence,
            > = None;
            let mut evidence_failed = false;
            if let Some(venue) = self.venues.iter().find(|v| v.kind().as_str() == snap.venue) {
                match venue.account_exposure_evidence().await {
                    Ok(ev) => {
                        if let Err(e) = ev.validate(Utc::now().timestamp_millis(), 30_000) {
                            tracing::warn!(error=%e, "stale/invalid account exposure evidence");
                            evidence_failed = true;
                        } else {
                            latest_evidence = Some(ev);
                        }
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if venue.is_production_venue()
                            || !msg.contains("does not supply scoped account exposure evidence")
                        {
                            evidence_failed = true;
                            tracing::warn!(error=%e, "account_exposure_evidence failed; blocking new entry");
                        }
                    }
                }
            }
            if evidence_failed {
                summary.risk_rejected += 1;
                let _ = self.storage.record_error(
                    self.session_id,
                    "risk",
                    "authoritative account evidence unavailable or stale",
                );
                self.push_signal_row(sig, "blocked: stale evidence");
                continue;
            }

            let eq_dec = if let Some(ref ev) = latest_evidence {
                Some(ev.snapshot.equity)
            } else if !any_prod {
                self.portfolio
                    .verified_equity(std::time::Duration::from_secs(60))
                    .ok()
                    .and_then(|e| Decimal::try_from(e).ok())
            } else {
                None
            };
            if let Some(e) = eq_dec {
                if e > self.high_water_equity_usd {
                    self.high_water_equity_usd = e;
                    self.persist_risk_state();
                }
            }

            let open_pos = self.storage.list_open_positions()?;
            let mut missing_mark = false;
            let mut sov_positions: Vec<SovereignPositionInput> = Vec::with_capacity(open_pos.len());
            for p in &open_pos {
                let pos_snap = snapshots.iter().find(|s| s.symbol == p.symbol);
                let Some(mark_px) = pos_snap
                    .map(|s| s.mid)
                    .filter(|m| m.is_finite() && *m > 0.0)
                else {
                    missing_mark = true;
                    break;
                };
                let Some(ntl) = Decimal::try_from(p.size.abs() * mark_px).ok() else {
                    missing_mark = true;
                    break;
                };
                let liq_dist_pct = latest_evidence
                    .as_ref()
                    .and_then(|ev| ev.positions.iter().find(|ep| ep.symbol == p.symbol))
                    .and_then(|ep| ep.liquidation_px)
                    .and_then(|lp| {
                        let m_dec = Decimal::try_from(mark_px).ok()?;
                        if m_dec > Decimal::ZERO {
                            Some((((m_dec - lp).abs() / m_dec) * Decimal::from(100u32)).round_dp(4))
                        } else {
                            None
                        }
                    });
                if let Some(ld) = liq_dist_pct {
                    if ld < self.sovereign_policy.min_liquidation_distance_pct {
                        missing_mark = true;
                        let _ = self.storage.record_error(
                            self.session_id,
                            "risk",
                            &format!(
                                "existing position {} liquidation buffer {ld}% below floor {}%",
                                p.symbol, self.sovereign_policy.min_liquidation_distance_pct
                            ),
                        );
                        break;
                    }
                }
                let fund = pos_snap
                    .and_then(|s| s.funding_bps_1h)
                    .filter(|f| f.is_finite())
                    .and_then(|f| Decimal::try_from(f).ok());
                sov_positions.push(SovereignPositionInput {
                    symbol: p.symbol.clone(),
                    owner_strategy: p.strategy.clone().unwrap_or_else(|| "unattributed".into()),
                    side: if p.side.eq_ignore_ascii_case("LONG") {
                        crate::core::Side::Long
                    } else {
                        crate::core::Side::Short
                    },
                    notional_usd: ntl,
                    liquidation_distance_pct: liq_dist_pct,
                    funding_bps_1h: fund,
                });
            }
            if missing_mark {
                summary.risk_rejected += 1;
                let _ = self.storage.record_error(
                    self.session_id,
                    "risk",
                    "open position lacks fresh valid mark price or liquidation buffer; entry denied",
                );
                self.push_signal_row(sig, "blocked: missing mark");
                continue;
            }

            let used_ntl: Decimal = sov_positions.iter().map(|p| p.notional_usd).sum();
            let journal_reserved: Decimal = self
                .order_journal
                .iter()
                .filter(|r| {
                    matches!(
                        r.state,
                        OrderLifecycleState::Submitted
                            | OrderLifecycleState::Acknowledged
                            | OrderLifecycleState::PartiallyFilled
                    )
                })
                .map(|r| {
                    (r.requested_qty - r.filled_qty).max(Decimal::ZERO)
                        * r.limit_price.unwrap_or(Decimal::ZERO)
                })
                .sum();
            let db_reserved = self
                .storage
                .active_aegis_reserved_notional()
                .unwrap_or(Decimal::ZERO);
            let exchange_order_reserved: Decimal = latest_evidence
                .as_ref()
                .map(|ev| {
                    ev.open_orders
                        .iter()
                        .filter(|o| !o.reduce_only)
                        .map(|o| o.size * o.limit_price.max(o.mark_price))
                        .sum()
                })
                .unwrap_or(Decimal::ZERO);
            let reserved_ntl = journal_reserved
                .max(db_reserved)
                .max(exchange_order_reserved);

            let mut pairwise_correlations = BTreeMap::new();
            let cand_upper = sig.symbol.trim().to_ascii_uppercase();
            if let Some(ref cm) = corr_matrix {
                for pos in &sov_positions {
                    let ps = pos.symbol.trim().to_ascii_uppercase();
                    if ps == cand_upper {
                        continue;
                    }
                    let idx_a = cm.symbols.iter().position(|s| *s == cand_upper);
                    let idx_b = cm.symbols.iter().position(|s| *s == ps);
                    let rho = match (idx_a, idx_b) {
                        (Some(ia), Some(ib)) => cm.correlations[ia][ib].unwrap_or(1.0),
                        _ => 1.0,
                    };
                    pairwise_correlations.insert((cand_upper.clone(), ps), rho);
                }
            } else {
                for pos in &sov_positions {
                    let ps = pos.symbol.trim().to_ascii_uppercase();
                    if ps != cand_upper {
                        pairwise_correlations.insert((cand_upper.clone(), ps), 1.0);
                    }
                }
            }

            // Round order size/price to exact exchange precision BEFORE risk & pilot limit checks.
            let inst_c = self.instrument_constraints.get(&sig.symbol);
            let req_lev = Decimal::ONE;
            let px_dec = Decimal::try_from(cand.reference_price)
                .unwrap_or(Decimal::ONE)
                .max(Decimal::new(1, 4));
            let raw_qty = (cand.requested_notional / px_dec).round_dp(8);
            let (final_px_dec, final_qty_dec) = match inst_c {
                Some(c) => match c.validate_and_round(px_dec, raw_qty, req_lev) {
                    Ok(pair) => pair,
                    Err(e) => {
                        summary.risk_rejected += 1;
                        let _ =
                            self.storage
                                .record_error(self.session_id, "precision", &e.to_string());
                        continue;
                    }
                },
                None => (px_dec, raw_qty),
            };
            let rounded_notional = (final_px_dec * final_qty_dec).round_dp(6);
            let est_fee = (rounded_notional * Decimal::new(5, 4)).round_dp(6);
            let post_fee_notional = rounded_notional + est_fee;

            let avail_collateral = latest_evidence
                .as_ref()
                .and_then(|ev| ev.withdrawable_collateral)
                .map(|w| (w - reserved_ntl - est_fee).max(Decimal::ZERO))
                .or_else(|| {
                    if !any_prod {
                        eq_dec.map(|e| (e - used_ntl - reserved_ntl).max(Decimal::ZERO))
                    } else {
                        None
                    }
                });

            self.control_authority.pilot.update_runtime_context(
                self.daily_pnl_usd,
                self.session_pnl_usd,
                used_ntl + reserved_ntl,
                eq_dec,
                (self.high_water_equity_usd > Decimal::ZERO).then_some(self.high_water_equity_usd),
            );

            if active_now.is_some() || (any_prod && self.cfg.live_trading) {
                let (acct_id, signer_id) = self
                    .venues
                    .iter()
                    .find(|v| v.kind().as_str() == snap.venue)
                    .and_then(|v| v.account_identity())
                    .unwrap_or_default();
                let acct_for_ctx = latest_evidence
                    .as_ref()
                    .map(|e| e.account.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(acct_id.as_str());
                if let Err(err) = crate::live_gate::require_entry_mutation_authority(
                    Some(&self.control_authority.pilot),
                    &crate::live_gate::EntryMutationContext {
                        production: any_prod,
                        account: acct_for_ctx,
                        signer: &signer_id,
                        symbol: &sig.symbol,
                        strategy_id: &sig.strategy,
                        order_type: crate::live_gate::PilotOrderType::LimitIoc,
                        requested_leverage: req_lev,
                        requested_notional_usd: post_fee_notional,
                        current_exposure_usd: used_ntl + reserved_ntl,
                        emergency_suspended: self.control_authority.is_emergency_suspended(),
                    },
                    now_ms,
                ) {
                    summary.risk_rejected += 1;
                    let msg = err.to_string();
                    let _ = self.storage.record_error(self.session_id, "pilot", &msg);
                    self.push_signal_row(sig, "blocked: pilot ceiling");
                    continue;
                }
            }

            let sov_state = SovereignPortfolioState {
                account_equity_usd: eq_dec,
                available_collateral_usd: avail_collateral,
                reserved_notional_usd: reserved_ntl,
                open_positions: sov_positions,
                daily_pnl_usd: Some(self.daily_pnl_usd),
                weekly_pnl_usd: Some(self.weekly_pnl_usd),
                high_water_equity_usd: (self.high_water_equity_usd > Decimal::ZERO)
                    .then_some(self.high_water_equity_usd),
                pairwise_correlations,
                emergency_suspended: self.control_authority.is_emergency_suspended(),
                breaker_reason: self.aegis_breaker_reason.clone().or_else(|| {
                    self.control_authority
                        .snapshot()
                        .breaker_latched
                        .then(|| "breaker_latched".into())
                }),
            };
            // Conservative prospective liquidation distance derived from actual collateral,
            // post-order portfolio exposure, and instrument maintenance margin tier (`>= 2.5%`).
            let est_liq_dist_pct = match (eq_dec, avail_collateral, inst_c.map(|c| c.max_leverage))
            {
                (Some(eq), Some(avail), Some(max_lev))
                    if eq > Decimal::ZERO
                        && max_lev >= Decimal::ONE
                        && rounded_notional > Decimal::ZERO =>
                {
                    let mmr = (Decimal::ONE / (Decimal::TWO * max_lev)).max(Decimal::new(25, 3));
                    let post_exp = used_ntl + reserved_ntl + rounded_notional;
                    let maint_req = (post_exp * mmr).round_dp(6);
                    let free_after_entry =
                        (avail - (rounded_notional / req_lev) - est_fee).max(Decimal::ZERO);
                    let buf = ((free_after_entry + (eq - maint_req).max(Decimal::ZERO) * mmr)
                        / post_exp.max(Decimal::ONE))
                    .min((Decimal::ONE / req_lev - mmr).max(Decimal::ZERO));
                    Some((buf * Decimal::from(100u32)).round_dp(4))
                }
                _ => None,
            };
            let market_depth_usd = snap
                .l2_depth_usd
                .filter(|d| d.is_finite() && *d > 0.0)
                .and_then(|d| Decimal::try_from(d).ok());
            let funding_bps_1h = snap
                .funding_bps_1h
                .filter(|f| f.is_finite())
                .and_then(|f| Decimal::try_from(f).ok());
            let sov_prop = SovereignAdmissionProposal {
                symbol: sig.symbol.clone(),
                owner_strategy: sig.strategy.clone(),
                side: sig.side,
                requested_notional_usd: rounded_notional,
                estimated_fee_usd: est_fee,
                estimated_liquidation_distance_pct: est_liq_dist_pct,
                market_depth_usd,
                market_volatility_pct: (atr_pct_now > 0.0)
                    .then(|| Decimal::try_from(atr_pct_now).unwrap_or(Decimal::ONE)),
                funding_bps_1h,
                instrument_max_leverage: inst_c.map(|c| c.max_leverage),
                requested_leverage: req_lev,
                rearm_authorization: None,
            };

            if let SovereignRiskDecision::Rejected {
                reasons,
                missing_inputs,
                breaker_triggered,
            } = evaluate_sovereign_admission(&self.sovereign_policy, &sov_state, &sov_prop)
            {
                summary.risk_rejected += 1;
                let msg = format!(
                    "AEGIS sovereign block (breaker={breaker_triggered}): reasons={reasons:?} missing={missing_inputs:?}"
                );
                if breaker_triggered {
                    self.latch_breaker(&reasons.join("; "));
                }
                let _ = self.storage.record_error(self.session_id, "risk", &msg);
                self.push_signal_row(sig, &format!("blocked: {}", truncate_reason(&msg)));
                continue;
            }

            let final_px = final_px_dec.to_f64().unwrap_or(cand.reference_price);
            let final_qty = final_qty_dec.to_f64().unwrap_or(0.0);
            if final_qty <= 0.0 || !final_px.is_finite() || final_px <= 0.0 {
                continue;
            }

            let cloid = uuid::Uuid::new_v4().to_string();
            let intent = OrderIntent {
                id: cloid.clone(),
                venue: snap.venue.clone(),
                symbol: sig.symbol.clone(),
                side: sig.side,
                size: final_qty,
                limit_price: Some(final_px),
                reduce_only: false,
                stop_price: Some(sig.stop_price),
                take_profit: Some(sig.take_profit),
                source_signal: Some(sig.strategy.clone()),
                created_at: Utc::now(),
            };
            self.order_journal.push(ExecutionJournalRecord {
                cloid: cloid.clone(),
                exchange_oid: None,
                symbol: sig.symbol.clone(),
                side: sig.side,
                owner_strategy: sig.strategy.clone(),
                requested_qty: final_qty_dec,
                limit_price: Some(final_px_dec),
                filled_qty: Decimal::ZERO,
                state: OrderLifecycleState::Submitted,
                submitted_at_ms: Utc::now().timestamp_millis(),
            });
            let _ = self.storage.set_meta_kv(
                &format!("settlement_strategy_intent:{cloid}"),
                &sig.strategy,
            );

            if let Some(venue) = self.venues.iter().find(|v| v.kind().as_str() == snap.venue) {
                match self.execution.submit(&intent, venue.as_ref()).await {
                    Ok(res) => {
                        self.storage.record_order(self.session_id, &intent, &res)?;
                        if let Some(ref oid_str) = res.venue_order_id {
                            let _ = self
                                .storage
                                .set_meta_kv(&format!("settlement_oid_intent:{oid_str}"), &cloid);
                        }
                        if let Some(rec) = self.order_journal.iter_mut().find(|r| r.cloid == cloid)
                        {
                            rec.exchange_oid = res
                                .venue_order_id
                                .as_deref()
                                .and_then(|s| s.parse::<u64>().ok());
                            rec.filled_qty =
                                Decimal::try_from(res.filled_size).unwrap_or(Decimal::ZERO);
                            rec.state = if !res.accepted {
                                OrderLifecycleState::Rejected
                            } else if res.filled_size >= final_qty - 1e-9 {
                                OrderLifecycleState::Filled
                            } else if res.filled_size > 0.0 {
                                OrderLifecycleState::PartiallyFilled
                            } else {
                                OrderLifecycleState::Acknowledged
                            };
                        }
                        if res.accepted && res.filled_size > 0.0 {
                            summary.accepted_entries += 1;
                            let entry_ctx = EntryContext {
                                strategy: sig.strategy.clone(),
                                side: sig.side.as_str().to_string(),
                                regime_view,
                                atr_pct: atr_pct_now,
                                spread_bps: snap.spread_bps,
                                edge_bps_predicted: sig.expected_edge_bps,
                            };
                            self.portfolio.open_from_fill(
                                &intent,
                                &res,
                                Some(&sig.strategy),
                                Some(entry_ctx),
                            )?;
                            self.storage
                                .finish_intent(&intent.id, "applied", Some(&res))?;
                            self.push_fill_row(&intent, &res, "open", None);
                            self.push_signal_row(sig, "filled");
                            self.drought.clear_for(&sig.symbol);

                            // Record the trade's exact original entry risk (|entry - stop| * filled_qty)
                            // for COGNITION normalized R calculation once authoritative exchange
                            // fills and funding settle the closed round-trip.
                            if let Some(entry_fill_px) = res.avg_fill_price {
                                if let (Ok(ep_dec), Ok(sp_dec), Ok(fq_dec)) = (
                                    Decimal::try_from(entry_fill_px),
                                    Decimal::try_from(sig.stop_price),
                                    Decimal::try_from(res.filled_size),
                                ) {
                                    let risk_usd = ((ep_dec - sp_dec).abs() * fq_dec).round_dp(6);
                                    if risk_usd > Decimal::ZERO {
                                        let _ = self.storage.set_meta_kv(
                                            &format!("settlement_risk_intent:{}", intent.id),
                                            &risk_usd.to_string(),
                                        );
                                        let _ = self.storage.set_meta_kv(
                                            &format!("settlement_risk_sym:{}", sig.symbol),
                                            &risk_usd.to_string(),
                                        );
                                    }
                                }
                            }

                            // Immediately reconcile exchange settlement and native reduce-only protective stop.
                            if let Err(e) = self.reconcile_settlement_from_venues(now_ms).await {
                                self.halted_unresolved = true;
                                let _ = self.storage.record_error(
                                    self.session_id,
                                    "settlement",
                                    &format!("post-entry settlement error: {e}"),
                                );
                            }
                            if let Ok(prot) = self.reconcile_protective_orders().await {
                                summary.protective_actions += prot.actions.len();
                            }
                        }
                    }
                    Err(e) => {
                        self.halted_unresolved = true;
                        tracing::error!(error=%e, "execution error (halting new entries until reconciled)");
                        self.storage
                            .record_error(self.session_id, "execution", &e.to_string())?;
                    }
                }
            }
        }

        if tick.is_multiple_of((10_000 / poll_ms.max(1)).max(1)) {
            self.heartbeat.persist(&self.storage, self.session_id).ok();
        }
        self.control_authority
            .record_tick(tick, now_ms, &entry_block_reason);
        if self.dash.is_some() {
            self.update_dashboard(tick, &snapshots);
        }
        if self.last_pulse_at.elapsed() >= std::time::Duration::from_secs(30) {
            self.emit_scan_pulse(&snapshots);
            self.last_pulse_at = std::time::Instant::now();
        }
        Ok(summary)
    }

    pub async fn run(&mut self) -> Result<()> {
        tracing::info!(
            session_id = self.session_id,
            live = self.execution.is_live(),
            "engine started"
        );
        let poll_ms = self.cfg.poll_interval_ms.max(250);
        let mut tick: u64 = 0;
        let start = Utc::now();

        loop {
            if self.stop.load(Ordering::SeqCst) || self.stop_flag_path().exists() {
                break;
            }
            tick = tick.wrapping_add(1);
            if let Err(e) = self.step_once(tick).await {
                tracing::error!(tick, error=%e, "engine step error");
            }
            sleep(Duration::from_millis(poll_ms)).await;
        }

        // === Shutdown ===
        let reason = if self.stop_flag_path().exists() {
            "stop_flag"
        } else {
            "sigint"
        };
        tracing::warn!(reason, "shutting down");
        // remove stop flag so next start isn't blocked
        let _ = std::fs::remove_file(self.stop_flag_path());

        match self.cfg.shutdown_mode {
            crate::config::ShutdownMode::KeepOpen => {
                tracing::info!("keeping open positions; persisted state remains authoritative");
            }
            crate::config::ShutdownMode::Reduce => {
                if let Err(e) = self
                    .portfolio
                    .reduce_all(&self.cfg, &mut self.execution, &self.venues)
                    .await
                {
                    tracing::error!(error=%e, "reduce-on-shutdown failed");
                }
            }
            crate::config::ShutdownMode::Flatten => {
                if let Err(e) = self
                    .portfolio
                    .flatten_all(&self.cfg, &mut self.execution, &self.venues)
                    .await
                {
                    tracing::error!(error=%e, "flatten-on-shutdown failed");
                }
            }
        }

        let end = Utc::now();
        self.storage.end_session(self.session_id, reason, end)?;
        SessionReporter::print_footer(
            &self.cfg,
            &self.storage,
            self.session_id,
            start,
            end,
            reason,
        )?;
        println!("{}", "session complete".green().bold());
        let _ = self.live_override; // keep field used
        Ok(())
    }
}

impl Engine {
    fn push_signal_row(&self, sig: &crate::core::Signal, outcome: &str) {
        let dash = match self.dash.as_ref() {
            Some(d) => d,
            None => return,
        };
        if let Ok(mut d) = dash.try_write() {
            d.push_signal(crate::tui::state::SignalRow {
                ts: Utc::now(),
                strategy: sig.strategy.clone(),
                symbol: sig.symbol.clone(),
                side: sig.side.as_str().to_string(),
                edge_bps: sig.expected_edge_bps,
                outcome: outcome.to_string(),
            });
        }
    }

    fn push_fill_row(
        &self,
        intent: &crate::core::OrderIntent,
        res: &crate::core::OrderResult,
        kind: &str,
        realized: Option<f64>,
    ) {
        let dash = match self.dash.as_ref() {
            Some(d) => d,
            None => return,
        };
        if let Ok(mut d) = dash.try_write() {
            d.push_fill(crate::tui::state::FillRow {
                ts: Utc::now(),
                symbol: intent.symbol.clone(),
                side: intent.side.as_str().to_string(),
                size: res.filled_size,
                price: res.avg_fill_price.unwrap_or(0.0),
                kind: kind.to_string(),
                realized_usd: realized,
            });
        }
    }

    /// Write a best-effort snapshot of the engine's world into the shared
    /// dashboard. Uses `try_write` so it can never block the trading loop.
    fn update_dashboard(&mut self, tick: u64, snapshots: &[crate::core::MarketSnapshot]) {
        let dash = match self.dash.as_ref() {
            Some(d) => d,
            None => return,
        };
        let mut d = match dash.try_write() {
            Ok(g) => g,
            Err(_) => return, // contention — skip this frame
        };

        d.session_id = self.session_id;
        d.started_at = Some(self.started_at);
        d.live_trading = self.execution.is_live();
        d.venues = self
            .venues
            .iter()
            .map(|v| v.kind().as_str().to_string())
            .collect();
        d.symbols = self.symbols.clone();
        d.tick = tick;
        d.last_heartbeat = Some(Utc::now());

        for snap in snapshots {
            let candles = &snap.candles;
            let atr_pct = if candles.len() >= 15 && snap.mid > 0.0 {
                let atr = crate::utils::math::atr(candles, 14);
                atr / snap.mid * 100.0
            } else {
                0.0
            };
            let regime =
                crate::strategies::regime::classify_regime(candles, &self.cfg.strategies.regime);
            // v1.5.0: full multi-TF regime view + per-symbol adaptive mult.
            let view = classify_view(candles, &self.cfg.strategies.regime);
            let view_str = format!(
                "{}/{}/{}",
                short_regime(view.r_1m),
                short_regime(view.r_5m),
                short_regime(view.r_15m),
            );
            let adaptive_mult = if self.cfg.strategies.learning.enabled {
                let exp = crate::learning::expectancy::for_symbol(&self.storage, &snap.symbol).ok();
                exp.map(|e| (e.to_multiplier(), e.samples))
            } else {
                None
            };
            d.upsert_market(crate::tui::state::MarketRow {
                symbol: snap.symbol.clone(),
                mid: snap.mid,
                spread_bps: snap.spread_bps,
                atr_pct,
                regime: format!("{:?}", regime),
                regime_view: Some(view_str),
                adaptive_mult,
                price_history: std::collections::VecDeque::new(),
                last_update: snap.last_update,
            });
        }

        let open = self.storage.list_open_positions().unwrap_or_default();
        let positions: Vec<crate::tui::state::PositionRow> = open
            .iter()
            .map(|p| {
                let mark = snapshots
                    .iter()
                    .find(|s| s.venue == p.venue && s.symbol == p.symbol)
                    .map(|s| s.mid)
                    .unwrap_or(p.entry_price);
                let upnl = if p.side == "LONG" {
                    (mark - p.entry_price) * p.size
                } else {
                    (p.entry_price - mark) * p.size
                };
                crate::tui::state::PositionRow {
                    venue: p.venue.clone(),
                    symbol: p.symbol.clone(),
                    side: p.side.clone(),
                    size: p.size,
                    entry_price: p.entry_price,
                    mark_price: mark,
                    unrealized_pnl_usd: upnl,
                    stop_price: p.stop_price,
                    take_profit: p.take_profit,
                    opened_at: p.opened_at,
                    strategy: p.strategy.clone(),
                }
            })
            .collect();
        let open_exposure_usd: f64 = positions.iter().map(|p| p.size.abs() * p.entry_price).sum();
        d.set_positions(positions);

        let equity = self.portfolio.last_known_equity();
        d.push_equity(equity);

        // Live trade stats — read from closed_trades so the cockpit updates
        // during the session, not just at shutdown.
        d.session_realized_usd = self
            .storage
            .sum_realized_in_session(self.session_id)
            .unwrap_or(0.0);
        d.session_trades = self
            .storage
            .count_closed_trades_in_session(self.session_id)
            .unwrap_or(0);

        let compound = self.storage.get_compound_state().ok();
        let consec = self
            .storage
            .consecutive_losses(self.cfg.risk.consecutive_loss_window_hours)
            .unwrap_or(0);
        let daily = self
            .storage
            .sum_realized_since(Utc::now() - chrono::Duration::days(1))
            .unwrap_or(0.0);
        let weekly = self
            .storage
            .sum_realized_since(Utc::now() - chrono::Duration::days(7))
            .unwrap_or(0.0);
        d.risk = crate::tui::state::RiskView {
            default_risk_pct: self.cfg.risk.default_risk_pct,
            max_daily_loss_pct: self.cfg.risk.max_daily_loss_pct,
            max_weekly_loss_pct: self.cfg.risk.max_weekly_loss_pct,
            max_consecutive_losses: self.cfg.risk.max_consecutive_losses,
            max_open_exposure_pct: self.cfg.risk.max_open_exposure_pct,
            max_notional_per_market_usd: self.cfg.risk.max_notional_per_market_usd,
            risk_mult: compound
                .as_ref()
                .map(|c| c.current_risk_mult)
                .unwrap_or(1.0),
            consec_losses: consec,
            open_exposure_usd,
            daily_realized_usd: daily,
            weekly_realized_usd: weekly,
        };
    }
}

fn truncate_reason(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= 32 {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(31).collect();
        out.push('…');
        out
    }
}

/// Short 2-char tag for a regime, used in the cockpit's compact "1m/5m/15m"
/// regime display so the markets table never wraps. Mapping is stable so
/// the user can match log entries against TUI output.
fn short_regime(r: crate::core::Regime) -> &'static str {
    match r {
        crate::core::Regime::Trending => "Tr",
        crate::core::Regime::Choppy => "Ch",
        crate::core::Regime::Panic => "Pn",
        crate::core::Regime::LowLiquidity => "Lq",
        crate::core::Regime::VolSpike => "Vs",
        crate::core::Regime::NoTrade => "No",
    }
}

impl Engine {
    /// v1.5.0 QoL: emit a one-line "the bot is alive" feed entry summarising
    /// every active market in compact form. Fired at most every 30s; the
    /// engine's normal signal/fill log lines take precedence — when something
    /// happens, those will visibly punctuate the pulses.
    fn emit_scan_pulse(&self, snapshots: &[crate::core::MarketSnapshot]) {
        if snapshots.is_empty() {
            tracing::info!(target: "hydra::pulse",
                "scan pulse · no live snapshots yet — waiting on venue data");
            return;
        }
        // Per-market mini-summary: SYM regime ATR%
        let parts: Vec<String> = snapshots
            .iter()
            .map(|s| {
                let regime = crate::strategies::regime::classify_regime(
                    &s.candles,
                    &self.cfg.strategies.regime,
                );
                let atr_pct = if s.candles.len() >= 15 && s.mid > 0.0 {
                    crate::utils::math::atr(&s.candles, 14) / s.mid * 100.0
                } else {
                    0.0
                };
                format!("{}={}({:.2}%)", s.symbol, short_regime(regime), atr_pct)
            })
            .collect();
        let open_count = self
            .storage
            .list_open_positions()
            .map(|v| v.len())
            .unwrap_or(0);
        let trades = self
            .storage
            .count_closed_trades_in_session(self.session_id)
            .unwrap_or(0);
        tracing::info!(target: "hydra::pulse",
            "scan pulse · {} open · {} trades this session · {}",
            open_count, trades, parts.join(" "),
        );
    }
}

/// Arbitrates candidate execution signals through the authoritative FANG planner (`fang_planner::plan`)
/// so the live/dry-run engine consumes the exact same deterministic priority & budget semantics
/// as Synapse Paper, ORACLE, and GENESIS.
pub fn plan_engine_candidates(
    signals: &[crate::core::Signal],
    marks: &std::collections::BTreeMap<String, rust_decimal::Decimal>,
    budget: &crate::fang_planner::PlanningBudget,
    requested_notional: rust_decimal::Decimal,
) -> anyhow::Result<crate::fang_planner::PlanningReport> {
    use rust_decimal::prelude::ToPrimitive;
    let cands: Vec<crate::fang_planner::PlanCandidate> = signals
        .iter()
        .enumerate()
        .map(|(i, s)| crate::fang_planner::PlanCandidate {
            id: format!("eng-{i}-{}-{}", s.strategy, s.symbol),
            signal: s.clone(),
            reference_price: marks
                .get(&s.symbol)
                .and_then(|d| d.to_f64())
                .unwrap_or(0.5 * (s.stop_price + s.take_profit)),
            requested_notional,
        })
        .collect();
    crate::fang_engine::FangUnifiedPlanner::arbitrate_external_candidates(&cands, budget)
}
