//! Main trading engine orchestrator: the forever loop.

use anyhow::{Context, Result};
use chrono::Utc;
use owo_colors::OwoColorize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

use crate::adapters::{make_venue, VenueAdapter};
use crate::cli::Venue as CliVenue;
use crate::config::AppConfig;
use crate::core::{MarketSnapshot, VenueKind};
use crate::execution::ExecutionEngine;
use crate::learning::{
    self,
    memory::{regime_to_id, FeatureVec},
    regime::{classify_view, RegimeView},
    strictness::{compose as compose_strictness, DroughtState},
    AdaptiveContext, SignalDecisionKind,
};
use crate::monitoring::Heartbeat;
use crate::portfolio::{EntryContext, PortfolioManager};
use crate::reporting::SessionReporter;
use crate::risk::RiskEngine;
use crate::state::StateManager;
use crate::storage::Storage;
use crate::strategies::{make_strategies, regime::classify_regime, Strategy};
use crate::tui::SharedDash;

pub struct Engine {
    _account_ownership: Option<crate::ownership::AccountOwnership>,
    cfg: AppConfig,
    storage: Storage,
    venues: Vec<Box<dyn VenueAdapter>>,
    symbols: Vec<String>,
    strategies: Vec<Box<dyn Strategy>>,
    risk: RiskEngine,
    portfolio: PortfolioManager,
    execution: ExecutionEngine,
    session_id: i64,
    stop: Arc<AtomicBool>,
    heartbeat: Heartbeat,
    live_override: bool,
    /// Dedup repeat signals: (strategy, symbol) -> last Instant we evaluated.
    signal_dedup: std::collections::HashMap<(String, String), std::time::Instant>,
    /// Optional shared dashboard state written to at the end of every tick.
    /// When `None`, the TUI is disabled and this field has zero effect.
    dash: Option<SharedDash>,
    /// v1.5.0 QoL: last time we emitted a "scan pulse" feed line. Periodic
    /// pulse summarises what the bot is seeing across the symbol set so the
    /// feed stays alive even during long no-signal windows.
    last_pulse_at: std::time::Instant,
    /// v1.6.0 Adaptive Strictness — per-symbol drought state. Tracks
    /// trend-filter blocks and bypass windows; reset per-symbol on any fill.
    drought: DroughtState,
    started_at: chrono::DateTime<chrono::Utc>,
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
        let ownership = if cfg.live_trading {
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
                use owo_colors::OwoColorize;
                println!("  {}  symbol {} is not tradeable as a perp on any configured venue — dropping.",
                    "⚠".bright_yellow().bold(),
                    sym.bright_red().bold());
                println!("     Common perps: BTC, ETH, HYPE, SOL, AVAX, ARB, OP, DOGE, kPEPE. Browse the Trade page on the venue UI for the full list.");
            }
        }
        if validated_symbols.is_empty() {
            anyhow::bail!("no valid symbols remain after validation");
        }
        let symbols = validated_symbols;

        let strategies = make_strategies(&cfg.strategies)?;
        let risk = RiskEngine::new(cfg.risk.clone(), storage.clone(), symbols.len());
        let mut portfolio = PortfolioManager::new(storage.clone());
        let mut execution = ExecutionEngine::new(
            storage.clone(),
            crate::execution::authorized_live(cfg.live_trading, live_override)?,
        );
        if cfg.live_trading {
            use rust_decimal::Decimal;
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

        // Preflight equity check — warn loudly if the account isn't funded yet.
        for v in &venues {
            match v.account_equity_usd().await {
                Ok(eq) => {
                    if eq < 10.0 {
                        tracing::warn!(venue=v.kind().as_str(), equity_usd=eq,
                            "account equity below $10 — fund USDC before trades can clear the $10 minimum");
                        use owo_colors::OwoColorize;
                        println!(
                            "\n  {}  {}  equity ≈ ${:.2}\n  {}\n",
                            "⚠".bright_yellow().bold(),
                            format!("{} account is UNFUNDED", v.kind().as_str().to_uppercase()).bright_yellow().bold(),
                            eq,
                            "     Fund USDC (Arbitrum → Hyperliquid bridge) and HYDRA will start trading on the next tick.".dimmed()
                        );
                    } else {
                        tracing::info!(
                            venue = v.kind().as_str(),
                            equity_usd = eq,
                            "account equity ok"
                        );
                    }
                    let _ = portfolio.sync_equity(v.kind().as_str(), eq);
                }
                Err(e) => tracing::warn!(venue=v.kind().as_str(), error=%e, "equity probe failed"),
            }
        }

        let stop = Arc::new(AtomicBool::new(false));
        let stop_signal = stop.clone();
        ctrlc::set_handler(move || {
            tracing::warn!("SIGINT received - scheduling graceful shutdown");
            stop_signal.store(true, Ordering::SeqCst);
        })
        .context("failed to install SIGINT handler")?;

        Ok(Self {
            _account_ownership: ownership,
            cfg,
            storage,
            venues,
            symbols,
            strategies,
            risk,
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
        })
    }

    /// Shared stop flag. External tasks (e.g. the TUI) may clone this to signal
    /// graceful shutdown; it is the same atomic triggered by the SIGINT handler.
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    fn stop_flag_path(&self) -> std::path::PathBuf {
        self.cfg.data_dir.join("STOP")
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
            self.heartbeat.beat("engine_tick");
            // Refresh account risk inputs independently of market signal production.
            if tick == 1 || tick % (10_000 / poll_ms).max(1) == 0 {
                for venue in &self.venues {
                    match venue.account_equity_usd().await {
                        Ok(equity) => {
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
                            self.storage.record_error(
                                self.session_id,
                                "snapshot",
                                &e.to_string(),
                            )?;
                        }
                    }
                }
            }

            // === 2. Position management (stops / trailing / time-stop) ===
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

            // === 3. Regime + signals + risk + execute ===
            for snap in &snapshots {
                let regime = classify_regime(&snap.candles, &self.cfg.strategies.regime);
                if self.cfg.strategies.global.regime_required && !regime.allows_trading() {
                    if tick % 20 == 0 {
                        tracing::debug!(venue=%snap.venue, symbol=%snap.symbol, regime=?regime, "no-trade regime");
                    }
                    continue;
                }
                // v1.5.0: multi-timeframe regime view for adaptive context.
                let regime_view: RegimeView =
                    classify_view(&snap.candles, &self.cfg.strategies.regime);
                // Cheap snapshot features that don't depend on the strategy.
                let atr_pct_now: f64 = if snap.candles.len() >= 15 && snap.mid > 0.0 {
                    crate::utils::math::atr(&snap.candles, 14) / snap.mid * 100.0
                } else {
                    0.0
                };
                for strat in &self.strategies {
                    let maybe = strat.evaluate(snap, regime, &self.cfg);
                    if let Some(sig) = maybe {
                        // Dedup: skip if we already evaluated this (strategy, symbol) pair within 60s.
                        let key = (sig.strategy.clone(), sig.symbol.clone());
                        let now = std::time::Instant::now();
                        if let Some(last) = self.signal_dedup.get(&key) {
                            if now.duration_since(*last) < std::time::Duration::from_secs(60) {
                                continue;
                            }
                        }
                        self.signal_dedup.insert(key, now);

                        self.storage.record_signal(self.session_id, &sig)?;
                        tracing::info!(strategy=%sig.strategy, symbol=%sig.symbol, side=sig.side.as_str(),
                            edge_bps=sig.expected_edge_bps, "signal");
                        self.push_signal_row(&sig, "emitted");

                        // v1.5.0: build AdaptiveContext, get a SignalDecision.
                        let feat = FeatureVec {
                            atr_pct: atr_pct_now,
                            spread_bps: snap.spread_bps.unwrap_or(2.0),
                            edge_bps: sig.expected_edge_bps,
                            regime_5m_id: regime_to_id(&format!("{:?}", regime_view.r_5m)),
                            hour_utc: chrono::Utc::now()
                                .format("%H")
                                .to_string()
                                .parse::<u8>()
                                .unwrap_or(0),
                            day_of_week: chrono::Utc::now()
                                .format("%u")
                                .to_string()
                                .parse::<u8>()
                                .map(|d| d.saturating_sub(1))
                                .unwrap_or(0),
                        };
                        let adaptive =
                            AdaptiveContext::new(&self.storage, &self.cfg.strategies.learning);
                        let decision = adaptive
                            .for_signal(
                                &sig.symbol,
                                &sig.strategy,
                                sig.side.as_str(),
                                regime_view,
                                &feat,
                            )
                            .unwrap_or_else(|e| {
                                tracing::warn!(error=%e, "adaptive context failed; using neutral");
                                learning::SignalDecision {
                                    kind: SignalDecisionKind::Allow,
                                    size_mult: 1.0,
                                    edge_threshold_override: None,
                                    summary: format!("adaptive: error: {}", e),
                                }
                            });
                        // Always log the adaptive verdict so user can review
                        // what the brain saw. Same line whether allow or block.
                        match &decision.kind {
                            SignalDecisionKind::Allow => {
                                tracing::debug!(symbol=%sig.symbol, strategy=%sig.strategy,
                                    "{}", decision.summary);
                            }
                            SignalDecisionKind::Block(_) => {
                                tracing::info!(symbol=%sig.symbol, strategy=%sig.strategy,
                                    "adaptive block: {}", decision.summary);
                            }
                        }

                        // v1.6.0: build strictness adjustment (ATR-aware
                        // gate scaling + drought-induced trend-filter bypass).
                        let mut strictness = compose_strictness(
                            &mut self.drought,
                            &sig.symbol,
                            atr_pct_now,
                            &self.cfg.strategies.learning,
                        );
                        if self.cfg.strategies.learning.strictness_enabled
                            && (strictness.edge_bps_delta != 0.0
                                || strictness.bypass_trend_filter
                                || strictness.trend_band_pct_override.is_some())
                        {
                            tracing::debug!(symbol=%sig.symbol, "{}", strictness.summary);
                        }

                        // Risk — first attempt with current strictness.
                        let mut attempt_label = "primary";
                        let mut risk_outcome = self
                            .risk
                            .evaluate(
                                &sig,
                                snap,
                                &self.cfg,
                                &self.portfolio,
                                Some(&decision),
                                Some(&strictness),
                                Some(regime),
                            )
                            .await;

                        // ε-greedy retry: if the ONLY thing that blocked us
                        // was the trend filter, AND exploration is enabled,
                        // AND this is the Nth eligible signal, retry with the
                        // trend filter bypassed at half size.
                        if let Err(ref e) = risk_outcome {
                            let msg = e.to_string();
                            let is_trend_block = msg.contains("trend filter");
                            if is_trend_block {
                                self.drought.record_trend_block(
                                    &sig.symbol,
                                    self.cfg.strategies.learning.drought_window_hours,
                                );
                            }
                            if is_trend_block
                                && self.cfg.strategies.learning.strictness_enabled
                                && self.cfg.strategies.learning.explore_enabled
                                && !strictness.bypass_trend_filter
                                && self
                                    .drought
                                    .should_explore(self.cfg.strategies.learning.explore_one_in_n)
                            {
                                strictness.bypass_trend_filter = true;
                                strictness.explore_size_factor =
                                    Some(self.cfg.strategies.learning.explore_size_factor);
                                strictness.summary = format!("explored: {}", strictness.summary);
                                attempt_label = "explored";
                                tracing::info!(
                                    symbol=%sig.symbol, strategy=%sig.strategy,
                                    "ε-greedy explore: retrying signal with trend bypass at {}× size",
                                    self.cfg.strategies.learning.explore_size_factor,
                                );
                                risk_outcome = self
                                    .risk
                                    .evaluate(
                                        &sig,
                                        snap,
                                        &self.cfg,
                                        &self.portfolio,
                                        Some(&decision),
                                        Some(&strictness),
                                        Some(regime),
                                    )
                                    .await;
                            }
                        }

                        match risk_outcome {
                            Ok(size) if size > 0.0 => {
                                let intent = self.risk.build_intent(&sig, size);
                                if let Some(venue) =
                                    self.venues.iter().find(|v| v.kind().as_str() == snap.venue)
                                {
                                    match self.execution.submit(&intent, venue.as_ref()).await {
                                        Ok(res) => {
                                            self.storage.record_order(
                                                self.session_id,
                                                &intent,
                                                &res,
                                            )?;
                                            if res.accepted && res.filled_size > 0.0 {
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
                                                self.storage.finish_intent(
                                                    &intent.id,
                                                    "applied",
                                                    Some(&res),
                                                )?;
                                                let fill_kind = if attempt_label == "explored" {
                                                    "open·explored"
                                                } else {
                                                    "open"
                                                };
                                                self.push_fill_row(&intent, &res, fill_kind, None);
                                                self.push_signal_row(
                                                    &sig,
                                                    if attempt_label == "explored" {
                                                        "filled (explored)"
                                                    } else {
                                                        "filled"
                                                    },
                                                );
                                                // Fill clears drought state for this symbol — we earned
                                                // back our way into respecting the trend filter again.
                                                self.drought.clear_for(&sig.symbol);
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!(error=%e, "execution error");
                                            self.storage.record_error(
                                                self.session_id,
                                                "execution",
                                                &e.to_string(),
                                            )?;
                                        }
                                    }
                                }
                            }
                            Ok(_) => {
                                tracing::debug!("risk declined signal (zero size)");
                                self.push_signal_row(&sig, "sized-out");
                            }
                            Err(e) => {
                                tracing::info!(reason=%e, "risk blocked signal");
                                self.storage.record_error(
                                    self.session_id,
                                    "risk",
                                    &e.to_string(),
                                )?;
                                self.push_signal_row(
                                    &sig,
                                    &format!("blocked: {}", truncate_reason(&e.to_string())),
                                );
                            }
                        }
                    }
                }
            }

            // === 4. Heartbeat to DB every ~10s ===
            if tick % (10_000 / poll_ms.max(1)).max(1) == 0 {
                self.heartbeat.persist(&self.storage, self.session_id).ok();
            }

            // === 5. Dashboard write (best-effort, non-blocking) ===
            if self.dash.is_some() {
                self.update_dashboard(tick, &snapshots);
            }

            // === 6. Scan pulse — QoL keep-alive in the cockpit feed ===
            // Every 30s emit a one-line summary of what the bot is seeing
            // even when no signals fire. Keeps the feed pane visibly alive
            // during long quiet windows ("scanning... 4 markets calm").
            if self.last_pulse_at.elapsed() >= std::time::Duration::from_secs(30) {
                self.emit_scan_pulse(&snapshots);
                self.last_pulse_at = std::time::Instant::now();
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
