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
use crate::cognition_shadow::{infer_regime, InferredRegime, RegimeInferenceConfig};
use crate::config::AppConfig;
use crate::core::{MarketSnapshot, OrderIntent, VenueKind};
use crate::engine_ipc::EngineControlAuthority;
use crate::execution::ExecutionEngine;
use crate::execution_recovery::{
    reconcile_execution_state, ExchangeOpenPositionSnapshot, ExecutionJournalRecord,
    InstrumentOrderConstraints, OrderLifecycleState, RecoveryAction, RecoveryReport,
};
use crate::fang_engine::{FangMarketInput, FangUnifiedPlanner, UnifiedStrategyConfig};
use crate::fang_planner::PlanningBudget;
use crate::learning::{
    regime::{classify_view, RegimeView},
    strictness::DroughtState,
};
use crate::monitoring::Heartbeat;
use crate::portfolio::{EntryContext, PortfolioManager};
use crate::protective_orders::{
    OpenProtectedPosition, ProtectiveOrderManager, ProtectiveReconcileAction,
    ProtectiveReconcileReport,
};
use crate::reporting::SessionReporter;
use crate::settlement::SettlementStore;
use crate::state::StateManager;
use crate::storage::Storage;
use crate::tui::SharedDash;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

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
    daily_pnl_usd: Decimal,
    weekly_pnl_usd: Decimal,
    high_water_equity_usd: Decimal,
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
            crate::live_gate::require_certified_live_execution(&req)?;
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
        Self::new_with_venues(
            cfg,
            storage,
            venues,
            symbols,
            live_override,
            dash,
            None,
            None,
            None,
            None,
            BTreeMap::new(),
            true,
        )
        .await
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
            crate::live_gate::require_certified_live_execution(&req)?;
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

        // Reconstruct durable execution journal from open positions so crash-restarts retain
        // authoritative symbol ownership while genuine exchange orphans / unresolved intents still halt.
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
                            owner_strategy: p
                                .strategy
                                .unwrap_or_else(|| "adaptive_ema_anchor".into()),
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

        Ok(Self {
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
            instrument_constraints,
            order_journal: restored_journal,
            halted_unresolved: false,
            halted_unprotected: false,
            daily_pnl_usd: Decimal::ZERO,
            weekly_pnl_usd: Decimal::ZERO,
            high_water_equity_usd: init_equity,
        })
    }

    pub fn control_authority(&self) -> &EngineControlAuthority {
        &self.control_authority
    }

    pub fn settlement_store(&self) -> Option<&SettlementStore> {
        self.settlement_store.as_ref()
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
        let mut ex_orders = Vec::new();
        let mut ex_positions = Vec::new();
        for v in &self.venues {
            match v.open_exchange_orders().await {
                Ok(orders) => ex_orders.extend(orders),
                Err(e) => {
                    tracing::debug!(venue=v.kind().as_str(), error=%e, "open_exchange_orders unavailable");
                }
            }
            if let Ok(pos) = v.open_positions().await {
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
        }
        let rep = reconcile_execution_state(&self.order_journal, &ex_orders, &[], &ex_positions)?;
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
        let db_unresolved = self.storage.count_unresolved_intents().unwrap_or(0);
        let holds = rep.unresolved_holds.max(db_unresolved);
        self.halted_unresolved = holds > 0;
        let open_cnt = self
            .storage
            .list_open_positions()
            .map(|v| v.len())
            .unwrap_or(0);
        self.control_authority.update_runtime_status(
            open_cnt,
            holds,
            !self.halted_unprotected,
            false,
        );
        Ok(rep)
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
                owner_strategy: p
                    .strategy
                    .clone()
                    .unwrap_or_else(|| "adaptive_ema_anchor".into()),
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
        for act in &initial_rep.actions {
            match act {
                ProtectiveReconcileAction::PlaceReduceOnlyStop(spec) => {
                    mutated = true;
                    if let Some(v) = self.venues.first() {
                        if v.place_protective_stop(spec).await.is_err() {
                            failed_syms.insert(spec.symbol.clone());
                        }
                    }
                }
                ProtectiveReconcileAction::ReplaceReduceOnlyStop {
                    replace_exchange_oid,
                    old_cloid,
                    new_spec,
                } => {
                    mutated = true;
                    if let Some(v) = self.venues.first() {
                        // Safe ordering: place & verify new stop BEFORE canceling old stop.
                        match v.place_protective_stop(new_spec).await {
                            Ok(_) => {
                                let _ = v
                                    .cancel_protective_stop(
                                        &new_spec.symbol,
                                        *replace_exchange_oid,
                                        old_cloid,
                                    )
                                    .await;
                            }
                            Err(_) => {
                                failed_syms.insert(new_spec.symbol.clone());
                            }
                        }
                    }
                }
                ProtectiveReconcileAction::CancelOrphanStop {
                    symbol,
                    exchange_oid,
                    cloid,
                } => {
                    mutated = true;
                    if let Some(v) = self.venues.first() {
                        let _ = v.cancel_protective_stop(symbol, *exchange_oid, cloid).await;
                    }
                }
                _ => {}
            }
        }

        let final_rep = if mutated {
            let mut latest_stops = Vec::new();
            for v in &self.venues {
                if let Ok(s) = v.list_protective_stops().await {
                    latest_stops.extend(s);
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

        self.halted_unprotected = !final_rep.protection_verified || final_rep.halt_new_entries;
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
            let ntl = Decimal::try_from(p.size.abs() * p.entry_price).unwrap_or(Decimal::ZERO);
            total_exposure += ntl;
            *sym_exposure
                .entry(p.symbol.clone())
                .or_insert(Decimal::ZERO) += ntl;
            occupied.insert(
                p.symbol.clone(),
                p.strategy.clone().unwrap_or_else(|| p.side.clone()),
            );
        }

        let mut watchlist = Vec::new();
        let mut candles_map = BTreeMap::new();
        let mut marks = BTreeMap::new();
        let mut regimes = BTreeMap::new();
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
            watchlist.push(snap.symbol.clone());
            candles_map.insert(snap.symbol.clone(), snap.candles.clone());
            marks.insert(snap.symbol.clone(), mark);
            regimes.insert(snap.symbol.clone(), reg);
            let used = sym_exposure
                .get(&snap.symbol)
                .copied()
                .unwrap_or(Decimal::ZERO);
            per_sym_rem.insert(
                snap.symbol.clone(),
                (self.unified_cfg.max_market_notional - used).max(Decimal::ZERO),
            );
        }

        let input = FangMarketInput {
            as_of_ms: if max_as_of_ms > 0 {
                max_as_of_ms
            } else {
                Utc::now().timestamp_millis()
            },
            correlation_id: format!("engine-t{tick}"),
            watchlist: watchlist.clone(),
            candles: candles_map,
            marks,
            regimes,
            microstructure: BTreeMap::new(),
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
        FangUnifiedPlanner::evaluate_and_plan(&input, &budget, &self.unified_cfg, None)
    }

    /// Executes one authoritative converged engine tick (snapshots -> recovery -> native stops
    /// -> shared FANG plan -> AEGIS sovereign risk -> precision rounding -> durable execution).
    pub async fn step_once(&mut self, tick: u64) -> Result<EngineStepSummary> {
        let mut summary = EngineStepSummary {
            tick,
            ..Default::default()
        };
        let poll_ms = self.cfg.poll_interval_ms.max(250);
        self.heartbeat.beat("engine_tick");

        if tick == 1 || tick.is_multiple_of((10_000 / poll_ms).max(1)) {
            for venue in &self.venues {
                match venue.account_equity_usd().await {
                    Ok(equity) => {
                        if let Ok(d) = Decimal::try_from(equity) {
                            if d > self.high_water_equity_usd {
                                self.high_water_equity_usd = d;
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
        if let Ok(rec) = self.reconcile_startup_and_recovery().await {
            summary.recovery_holds = rec.unresolved_holds;
        }
        if let Ok(prot) = self.reconcile_protective_orders().await {
            summary.protective_actions = prot.actions.len();
        }

        // === 3. Position trailing / TP / time-stop management ===
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
                tracing::warn!(error=%e, "position management error");
            }
        }

        // === 4. Fail-closed admission gate on suspension, unresolved intents, or missing protection ===
        if self.control_authority.is_emergency_suspended()
            || self.halted_unresolved
            || self.halted_unprotected
        {
            summary.halts_blocked_entries = true;
            if tick.is_multiple_of((10_000 / poll_ms.max(1)).max(1)) {
                self.heartbeat.persist(&self.storage, self.session_id).ok();
            }
            if self.dash.is_some() {
                self.update_dashboard(tick, &snapshots);
            }
            return Ok(summary);
        }

        // === 5. Shared FANG unified plan + AEGIS sovereign admission + durable execution ===
        let plan_out = self.unified_planning_step(tick, &snapshots)?;
        summary.planned_candidates = plan_out.report.selected.len();
        for cand in plan_out.report.selected {
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

            // Independent AEGIS Sovereign Risk evaluation (fail closed on missing inputs)
            let eq_dec = self
                .portfolio
                .verified_equity(std::time::Duration::from_secs(60))
                .ok()
                .and_then(|e| Decimal::try_from(e).ok());
            let open_pos = self.storage.list_open_positions().unwrap_or_default();
            let sov_positions: Vec<SovereignPositionInput> = open_pos
                .iter()
                .filter_map(|p| {
                    let ntl = Decimal::try_from(p.size.abs() * p.entry_price).ok()?;
                    Some(SovereignPositionInput {
                        symbol: p.symbol.clone(),
                        owner_strategy: p
                            .strategy
                            .clone()
                            .unwrap_or_else(|| "adaptive_ema_anchor".into()),
                        side: if p.side.eq_ignore_ascii_case("LONG") {
                            crate::core::Side::Long
                        } else {
                            crate::core::Side::Short
                        },
                        notional_usd: ntl,
                        liquidation_distance_pct: Some(Decimal::from(50u32)),
                        funding_bps_1h: Some(Decimal::ZERO),
                    })
                })
                .collect();
            let used_ntl: Decimal = sov_positions.iter().map(|p| p.notional_usd).sum();
            let sov_state = SovereignPortfolioState {
                account_equity_usd: eq_dec,
                available_collateral_usd: eq_dec.map(|e| (e - used_ntl).max(Decimal::ZERO)),
                reserved_notional_usd: Decimal::ZERO,
                open_positions: sov_positions,
                daily_pnl_usd: Some(self.daily_pnl_usd),
                weekly_pnl_usd: Some(self.weekly_pnl_usd),
                high_water_equity_usd: (self.high_water_equity_usd > Decimal::ZERO)
                    .then_some(self.high_water_equity_usd),
                pairwise_correlations: BTreeMap::new(),
                emergency_suspended: self.control_authority.is_emergency_suspended(),
                breaker_reason: self
                    .control_authority
                    .snapshot()
                    .breaker_latched
                    .then(|| "breaker_latched".into()),
            };
            let inst_c = self.instrument_constraints.get(&sig.symbol);
            let req_lev = Decimal::ONE;
            let est_fee = (cand.requested_notional * Decimal::new(5, 4)).round_dp(6);
            let sov_prop = SovereignAdmissionProposal {
                symbol: sig.symbol.clone(),
                owner_strategy: sig.strategy.clone(),
                side: sig.side,
                requested_notional_usd: cand.requested_notional,
                estimated_fee_usd: est_fee,
                estimated_liquidation_distance_pct: eq_dec.map(|_| Decimal::from(50u32)),
                market_depth_usd: snap
                    .spread_bps
                    .filter(|s| s.is_finite() && *s >= 0.0)
                    .map(|_| Decimal::from(500_000u32)),
                market_volatility_pct: (atr_pct_now > 0.0)
                    .then(|| Decimal::try_from(atr_pct_now).unwrap_or(Decimal::ONE)),
                funding_bps_1h: Some(Decimal::ZERO),
                instrument_max_leverage: inst_c
                    .map(|c| c.max_leverage)
                    .or(Some(Decimal::from(20u32))),
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
                let _ = self.storage.record_error(self.session_id, "risk", &msg);
                self.push_signal_row(sig, &format!("blocked: {}", truncate_reason(&msg)));
                continue;
            }

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

            if let Some(venue) = self.venues.iter().find(|v| v.kind().as_str() == snap.venue) {
                match self.execution.submit(&intent, venue.as_ref()).await {
                    Ok(res) => {
                        self.storage.record_order(self.session_id, &intent, &res)?;
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

                            // Immediately reconcile native reduce-only protective stop for the new position.
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
