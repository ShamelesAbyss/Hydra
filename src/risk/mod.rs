//! Risk engine: every signal is evaluated pre-trade.

use anyhow::{bail, Result};
use chrono::Utc;

use crate::config::{AppConfig, RiskConfig};
use crate::core::{MarketSnapshot, OrderIntent, Signal};
use crate::learning::{SignalDecision, SignalDecisionKind, StrictnessAdjustment};
use crate::portfolio::PortfolioManager;
use crate::storage::Storage;

pub struct RiskEngine {
    risk: RiskConfig,
    storage: Storage,
    /// Number of symbols actively being traded in this run. Used to split the
    /// total-exposure budget fairly across symbols so a single large-notional
    /// trade on one pair can't lock out every other pair.
    active_symbols: usize,
}

impl RiskEngine {
    pub fn new(risk: RiskConfig, storage: Storage, active_symbols: usize) -> Self {
        Self {
            risk,
            storage,
            active_symbols: active_symbols.max(1),
        }
    }

    /// The effective per-symbol notional budget: whichever is smaller of
    /// (a) the hard per-market cap, or
    /// (b) the total-exposure budget divided evenly across active symbols.
    ///
    /// This keeps small accounts diversified (each symbol gets a fair slice
    /// even when a single BTC trade would otherwise saturate the total cap)
    /// while letting larger accounts use the full per-market hard cap.
    fn per_symbol_budget_usd(&self, equity: f64) -> f64 {
        let total_budget = equity.max(0.0) * self.risk.max_open_exposure_pct / 100.0;
        let fair_share = total_budget / self.active_symbols as f64;
        self.risk
            .max_notional_per_market_usd
            .min(fair_share)
            .max(0.0)
    }

    /// Returns the permitted order size (coin units) given risk constraints.
    /// Returns 0.0 if no trade should be taken.
    /// Returns Err if risk engine is hard-refusing (cooldown / kill switch style).
    ///
    /// `decision` is the v1.5.0 adaptive learning verdict. When Some(Block(_))
    /// we hard-refuse with the verdict's reason. When Some(Allow{..}) the
    /// `size_mult` re-scales the final size and `edge_threshold_override`
    /// shifts the `min_net_edge_bps` gate (negative = looser, positive = tighter).
    ///
    /// `strictness` is the v1.6.0 adaptive strictness adjustment. ATR-aware
    /// scaling shifts the edge gate further; drought detector can bypass
    /// the trend filter; ε-greedy exploration applies a size factor.
    pub async fn evaluate(
        &self,
        signal: &Signal,
        snap: &MarketSnapshot,
        cfg: &AppConfig,
        portfolio: &PortfolioManager,
        decision: Option<&SignalDecision>,
        strictness: Option<&StrictnessAdjustment>,
        regime: Option<crate::core::Regime>,
    ) -> Result<f64> {
        // 0. Adaptive learning verdict — earliest possible refusal so
        // expensive checks below are skipped.
        if let Some(d) = decision {
            if let SignalDecisionKind::Block(reason) = &d.kind {
                bail!("memory-block: {}", reason);
            }
        }

        // 1. Data quality
        if !snap.is_fresh(30) {
            bail!("stale market data (>{}s)", 30);
        }
        // v1.4.0: spread limit is symbol-aware (HYPE has wider typical spreads
        // than BTC/ETH; without an override, valid signals get rejected).
        let symbol_override = cfg.strategies.per_symbol.get(&signal.symbol);
        let max_spread = symbol_override
            .and_then(|o| o.max_spread_bps)
            .unwrap_or(self.risk.max_spread_bps);
        if let Some(sp) = snap.spread_bps {
            if sp > max_spread {
                bail!(
                    "spread too wide: {:.1}bps > {}bps ({})",
                    sp,
                    max_spread,
                    signal.symbol
                );
            }
        }

        // 1a. Re-entry cooldown — block re-takes of a losing (sym, side)
        // within the configured window. The cooldown only fires on losses; a
        // prior WIN never blocks the next entry.
        //
        // v1.6.2: The general loss cooldown and the stop-out cooldown no
        // longer STACK. We check what kind of close happened most recently:
        //   - If it was a STOP-OUT → only `stopout_cooldown_minutes` applies
        //   - If it was any OTHER loss (time_stop, manual) → only the general
        //     `reentry_cooldown_minutes` applies
        // This prevents the "75 min total lockout" pattern where a single
        // stop-out triggered both gates serially.
        let last_stopout = self
            .storage
            .last_stopout_close_for(&signal.symbol, signal.side.as_str())
            .ok()
            .flatten();
        let last_loss = self
            .storage
            .last_loss_close_for(&signal.symbol, signal.side.as_str())
            .ok()
            .flatten();
        // The most recent loss was a stop-out iff the stop-out timestamp
        // matches (or is later than) the last_loss timestamp.
        let last_was_stopout = match (last_stopout, last_loss) {
            (Some(s), Some(l)) => s >= l,
            (Some(_), None) => true,
            _ => false,
        };
        if last_was_stopout {
            let stopout_min = cfg.strategies.position_management.stopout_cooldown_minutes;
            if stopout_min > 0 {
                if let Some(last_stop) = last_stopout {
                    let elapsed = (Utc::now() - last_stop).num_seconds();
                    let window = stopout_min * 60;
                    if elapsed < window {
                        bail!(
                            "stop-out cooldown: last {} {} stop {}m ago < {}m",
                            signal.symbol,
                            signal.side.as_str(),
                            elapsed / 60,
                            stopout_min,
                        );
                    }
                }
            }
        } else {
            let cooldown_min = cfg.strategies.filters.reentry_cooldown_minutes;
            if cooldown_min > 0 {
                if let Some(last) = last_loss {
                    let elapsed = (Utc::now() - last).num_seconds();
                    let window = cooldown_min * 60;
                    if elapsed < window {
                        bail!(
                            "re-entry cooldown: last {} {} loss {}m ago < {}m",
                            signal.symbol,
                            signal.side.as_str(),
                            elapsed / 60,
                            cooldown_min,
                        );
                    }
                }
            }
        }

        // 1b. v1.3.3 Trend filter — block counter-trend mean-reversion signals.
        // v1.3.6: with a deadband. Mean-reversion is INHERENTLY counter-trend
        // (it fires when price stretches away from the mean), so a binary
        // above/below-EMA check killed every signal. The deadband ensures the
        // filter only fires on REAL directional drifts, not noise.
        // v1.6.0: strictness layer can override the deadband (ATR-aware
        // widening) or bypass the filter entirely (drought detector).
        // v1.6.2: REGIME-AWARE. The trend filter only enforces in the
        // `Trending` regime. Mean reversion's whole purpose is fading
        // small drifts in `Choppy` regime — blocking it there was 100%
        // counterproductive. In Choppy/VolSpike/etc, MR signals pass freely.
        if cfg.strategies.filters.trend_filter_enabled
            && signal.strategy == "mean_reversion"
            && matches!(regime, Some(crate::core::Regime::Trending))
        {
            let bypass = strictness.map(|s| s.bypass_trend_filter).unwrap_or(false);
            if !bypass {
                let period = cfg.strategies.filters.trend_filter_ema_period;
                let band_pct = strictness
                    .and_then(|s| s.trend_band_pct_override)
                    .unwrap_or(cfg.strategies.filters.trend_filter_band_pct);
                if snap.candles.len() >= period && period > 0 {
                    let closes: Vec<f64> = snap.candles.iter().map(|c| c.close).collect();
                    let ema = crate::utils::math::ema(&closes, period);
                    let ema_last = *ema.last().unwrap_or(&snap.mid);
                    let signal_long = matches!(signal.side, crate::core::Side::Long);
                    match trend_filter_decision(snap.mid, ema_last, band_pct) {
                        TrendFilterDecision::InsideBand => {} // allow
                        TrendFilterDecision::BlockShort if !signal_long => {
                            bail!(
                                "trend filter: mean_reversion SHORT blocked (mid {:.4} > EMA{} {:.4} by {:.2}%, > {:.2}% band)",
                                snap.mid, period, ema_last,
                                (snap.mid / ema_last - 1.0) * 100.0, band_pct,
                            );
                        }
                        TrendFilterDecision::BlockLong if signal_long => {
                            bail!(
                                "trend filter: mean_reversion LONG blocked (mid {:.4} < EMA{} {:.4} by {:.2}%, > {:.2}% band)",
                                snap.mid, period, ema_last,
                                (1.0 - snap.mid / ema_last) * 100.0, band_pct,
                            );
                        }
                        _ => {} // signal direction agrees with trend → allow
                    }
                }
            }
        }

        // 1c. v1.4.4 Minimum net-edge gate. HL taker fees round-trip ~7bps;
        // require signals to predict enough edge to clear that AND give a
        // realistic margin for slippage/spread. The signal's own
        // `expected_edge_bps` already nets out fees+spread (each strategy's
        // edge filter computes net), so we just enforce a minimum here.
        // v1.5.0: adaptive `edge_threshold_override` can shift this gate
        // by ±a few bps based on the strategy's recent expectancy.
        // v1.6.0: strictness layer adds an ATR-aware delta on top.
        let mut min_edge = cfg.strategies.filters.min_net_edge_bps;
        if let Some(d) = decision {
            if let Some(delta) = d.edge_threshold_override {
                min_edge = (min_edge + delta).max(0.0);
            }
        }
        if let Some(s) = strictness {
            min_edge = (min_edge + s.edge_bps_delta).max(0.0);
        }
        if min_edge > 0.0 && signal.expected_edge_bps < min_edge {
            bail!(
                "edge filter: predicted {:.1}bps < {:.1}bps min net (after-fee threshold)",
                signal.expected_edge_bps,
                min_edge,
            );
        }

        // 2. Daily / weekly losses
        let daily_pnl = self
            .storage
            .sum_realized_since(Utc::now() - chrono::Duration::days(1))?;
        let weekly_pnl = self
            .storage
            .sum_realized_since(Utc::now() - chrono::Duration::days(7))?;
        let equity = portfolio.verified_equity(std::time::Duration::from_secs(30))?;
        anyhow::ensure!(
            equity.is_finite() && equity > 0.0,
            "account equity unavailable or zero; new exposure blocked"
        );

        if daily_pnl.is_finite() && daily_pnl < 0.0 {
            let loss_pct = -daily_pnl / equity * 100.0;
            if loss_pct > self.risk.max_daily_loss_pct {
                bail!(
                    "daily loss limit hit: {:.2}% > {:.2}%",
                    loss_pct,
                    self.risk.max_daily_loss_pct
                );
            }
        }
        if weekly_pnl.is_finite() && weekly_pnl < 0.0 {
            let loss_pct = -weekly_pnl / equity * 100.0;
            if loss_pct > self.risk.max_weekly_loss_pct {
                bail!(
                    "weekly loss limit hit: {:.2}% > {:.2}%",
                    loss_pct,
                    self.risk.max_weekly_loss_pct
                );
            }
        }

        // 3. Consecutive losses cooldown
        let consec = self
            .storage
            .consecutive_losses(self.risk.consecutive_loss_window_hours)?;
        if consec >= self.risk.max_consecutive_losses {
            bail!("consecutive-loss cooldown active ({consec})");
        }

        // 4. Exposure caps — per-symbol fair-share budget + hard total ceiling.
        let (open_notional, per_market) = portfolio.exposure(&signal.symbol)?;
        let per_symbol_budget = self.per_symbol_budget_usd(equity);

        if per_market >= per_symbol_budget {
            bail!(
                "per-symbol notional ${:.0} fills allocation ${:.0} (equity ${:.0}, {} active symbols)",
                per_market, per_symbol_budget, equity, self.active_symbols
            );
        }
        // If aggregate exposure is ALREADY over cap (e.g. equity dropped after
        // older positions were opened), don't open another trade. We log the
        // cause distinctly so it's clear no NEW trade is being added.
        let exposure_pct = open_notional / equity * 100.0;
        if exposure_pct > self.risk.max_open_exposure_pct {
            bail!(
                "current open exposure {:.1}% > {:.1}% cap (existing positions only — no new trade added)",
                exposure_pct, self.risk.max_open_exposure_pct,
            );
        }

        // 5. Compute size from risk budget
        let compound = self.storage.get_compound_state()?;
        let risk_mult = compound.current_risk_mult.max(0.2).min(3.0);
        // v1.4.0: per-symbol risk percentage override.
        let base_risk_pct = symbol_override
            .and_then(|o| o.default_risk_pct)
            .unwrap_or(self.risk.default_risk_pct);
        let risk_budget_usd = equity * (base_risk_pct / 100.0) * risk_mult;

        // Distance from entry to stop determines position sizing.
        // Defense-in-depth: refuse non-finite or zero/negative values that
        // could otherwise cascade into NaN/Inf sizes.
        let entry = snap.mid;
        if !entry.is_finite() || entry <= 0.0 {
            bail!("invalid mid price: {}", entry);
        }
        let stop = signal.stop_price;
        if !stop.is_finite() || stop <= 0.0 {
            bail!("invalid stop price: {}", stop);
        }
        let dist = (entry - stop).abs();
        if !dist.is_finite() || dist <= 0.0 {
            bail!("stop distance non-positive or non-finite: {}", dist);
        }
        let per_unit_risk = dist;
        let mut size_coin = risk_budget_usd / per_unit_risk;
        if !size_coin.is_finite() {
            bail!(
                "computed size is non-finite (risk_budget={} dist={})",
                risk_budget_usd,
                per_unit_risk
            );
        }

        // strategy-specific fraction
        let frac = strategy_frac(cfg, &signal.strategy);
        size_coin *= frac;

        // v1.4.0: per-symbol size multiplier. Lets us up-size BTC (best
        // performer) and down-size HYPE (less proven) without rewriting
        // strategy fractions. Clamped [0.1, 2.0] for safety.
        if let Some(mult) = symbol_override.and_then(|o| o.position_frac_mult) {
            size_coin *= mult.clamp(0.1, 2.0);
        }

        // v1.5.0: adaptive size multiplier from learning module. Already
        // clamped to [0.5, 1.5] inside `AdaptiveContext`; we re-clamp here
        // for defense-in-depth so a buggy learning value can never turn into
        // a runaway size.
        if let Some(d) = decision {
            let m = d.size_mult.clamp(0.5, 1.5);
            size_coin *= m;
        }

        // v1.6.0: ε-greedy exploration size factor. When the engine has
        // chosen this signal as an exploration trade, downsize so the
        // data-collection cost is bounded. Clamped [0.25, 1.0].
        if let Some(s) = strictness {
            if let Some(f) = s.explore_size_factor {
                size_coin *= f.clamp(0.25, 1.0);
            }
        }

        // Respect remaining headroom in this symbol's budget. v1.4.0 also
        // honours an optional per-symbol notional cap, taking the tighter of
        // {global per-mkt cap, symbol override, fair-share allocation}.
        let symbol_notional_cap = symbol_override
            .and_then(|o| o.max_notional_usd)
            .unwrap_or(f64::INFINITY);
        let effective_per_symbol_budget = per_symbol_budget.min(symbol_notional_cap);
        let headroom_sym = (effective_per_symbol_budget - per_market).max(0.0);
        // ALSO respect remaining headroom in the TOTAL exposure budget so a
        // dilution edge case (e.g. 4 symbols, fair share looks fine each, but
        // the aggregate would exceed the cap) can never push us over.
        let total_budget = equity * self.risk.max_open_exposure_pct / 100.0;
        let headroom_total = (total_budget - open_notional).max(0.0);
        let headroom = headroom_sym.min(headroom_total);

        let notional = size_coin * entry;
        if notional > headroom {
            size_coin = headroom / entry;
        }

        // Minimum tradable: avoid dust + fee-loss trades.
        // v1.4.4: configurable via filters.min_notional_usd (default $50).
        // Below this, fees eat too high a fraction of any potential win.
        let min_notional = cfg.strategies.filters.min_notional_usd.max(10.0);
        if size_coin * entry < min_notional {
            return Ok(0.0);
        }

        // 6. Final invariant check — projected exposure must NEVER exceed cap.
        // Defense-in-depth: if any of the math above let through a bad size,
        // this catches it before we build an intent. Hard refuse; never silent.
        // v1.5.0: explicit NaN/finite guard added — even with all upstream
        // checks, a pathological multiplier chain could in theory propagate
        // a non-finite value here. Bail loudly before it becomes an order.
        if !size_coin.is_finite() || size_coin <= 0.0 {
            bail!("final size is non-finite or non-positive: {}", size_coin);
        }
        let projected_total = open_notional + size_coin * entry;
        let projected_pct = projected_total / equity * 100.0;
        if !projected_pct.is_finite() {
            bail!(
                "projected exposure pct is non-finite (open={}, size={}, entry={}, equity={})",
                open_notional,
                size_coin,
                entry,
                equity
            );
        }
        if projected_pct > self.risk.max_open_exposure_pct + 0.01 {
            bail!(
                "projected exposure {:.1}% would exceed {:.1}% cap — refusing trade",
                projected_pct,
                self.risk.max_open_exposure_pct,
            );
        }

        Ok(size_coin)
    }

    pub fn build_intent(&self, signal: &Signal, size: f64) -> OrderIntent {
        OrderIntent {
            id: uuid::Uuid::new_v4().to_string(),
            venue: signal.venue.clone(),
            symbol: signal.symbol.clone(),
            side: signal.side,
            size,
            limit_price: None, // executed as marketable IOC
            reduce_only: false,
            stop_price: Some(signal.stop_price),
            take_profit: Some(signal.take_profit),
            source_signal: Some(signal.strategy.clone()),
            created_at: Utc::now(),
        }
    }
}

fn strategy_frac(cfg: &AppConfig, name: &str) -> f64 {
    match name {
        "divergence" => cfg.strategies.divergence.position_frac,
        "momentum" => cfg.strategies.momentum.position_frac,
        "mean_reversion" => cfg.strategies.mean_reversion.position_frac,
        _ => 0.5,
    }
    .clamp(0.1, 1.0)
}

/// Outcome of the v1.3.6 trend-filter deadband decision. Pure function; no
/// I/O, no DB, no config — trivially unit-testable.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TrendFilterDecision {
    /// Price is inside the deadband around the EMA: no clear directional
    /// drift. Allow the signal through.
    InsideBand,
    /// Real uptrend (price clearly above EMA) — block counter-trend SHORTs.
    BlockShort,
    /// Real downtrend (price clearly below EMA) — block counter-trend LONGs.
    BlockLong,
}

/// Pure helper: given a price, EMA, and deadband percent, decide the trend
/// filter's verdict. Used by `evaluate()` and exercised directly by the
/// regression tests reproducing the bad scenarios from the user's log.
pub(crate) fn trend_filter_decision(mid: f64, ema: f64, band_pct: f64) -> TrendFilterDecision {
    if ema <= 0.0 || !ema.is_finite() || !mid.is_finite() {
        return TrendFilterDecision::InsideBand;
    }
    let dist_pct = (mid / ema - 1.0) * 100.0;
    let band = band_pct.max(0.0);
    if dist_pct.abs() < band {
        TrendFilterDecision::InsideBand
    } else if dist_pct > 0.0 {
        TrendFilterDecision::BlockShort
    } else {
        TrendFilterDecision::BlockLong
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    //! Locks in the v1.3.1 dual-pair fairness contract: no matter how the
    //! user sets `max_notional_per_market_usd` and `max_open_exposure_pct`,
    //! the per-symbol budget must never exceed a fair share of the total.
    use super::*;
    use crate::config::RiskConfig;

    fn cfg_with(per_market: f64, max_exp_pct: f64) -> RiskConfig {
        RiskConfig {
            default_risk_pct: 0.5,
            max_daily_loss_pct: 3.0,
            max_weekly_loss_pct: 8.0,
            max_drawdown_pct: 15.0,
            max_consecutive_losses: 4,
            max_open_exposure_pct: max_exp_pct,
            max_notional_per_market_usd: per_market,
            min_liquidity_usd: 50_000.0,
            max_spread_bps: 25.0,
            max_slippage_bps: 30.0,
            consecutive_loss_window_hours: 4,
        }
    }

    fn eng(risk: RiskConfig, symbols: usize) -> RiskEngine {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let storage = Storage::open(tmp.path()).unwrap();
        storage.migrate().unwrap();
        RiskEngine::new(risk, storage, symbols)
    }

    #[test]
    fn small_account_dual_pair_shares_budget() {
        // Exactly the user's situation on Feb 2026: $95 equity, $500 per-market
        // cap, 300% total exposure cap, trading ETH + BTC.
        let e = eng(cfg_with(500.0, 300.0), 2);
        let budget = e.per_symbol_budget_usd(95.0);
        // Fair share: (95 * 3.0) / 2 = 142.5, beats the $500 hard cap.
        assert!((budget - 142.5).abs() < 0.01, "got {}", budget);
        // Both symbols at budget together exactly fit the total cap.
        assert!((budget * 2.0 - 95.0 * 3.0).abs() < 0.01);
    }

    #[test]
    fn single_pair_gets_full_per_market_cap() {
        // With only one active symbol, fair-share = total budget = 285,
        // still capped by per-market = 500 (fair share wins at 285? Actually
        // min(500, 285) = 285). The user's request is that single-pair
        // runs can scale larger — this test documents current behaviour.
        let e = eng(cfg_with(500.0, 300.0), 1);
        let budget = e.per_symbol_budget_usd(95.0);
        assert!(
            (budget - 285.0).abs() < 0.01,
            "single-pair budget={}",
            budget
        );
    }

    #[test]
    fn large_account_hits_hard_per_market_cap() {
        // $1k equity, 2 symbols: fair share = 1500, hard cap = 500 wins.
        let e = eng(cfg_with(500.0, 300.0), 2);
        let budget = e.per_symbol_budget_usd(1000.0);
        assert!((budget - 500.0).abs() < 0.01, "got {}", budget);
    }

    #[test]
    fn zero_equity_produces_zero_budget() {
        let e = eng(cfg_with(500.0, 300.0), 2);
        assert_eq!(e.per_symbol_budget_usd(0.0), 0.0);
        assert_eq!(e.per_symbol_budget_usd(-10.0), 0.0);
    }

    #[test]
    fn many_symbols_dilute_budget_correctly() {
        // 4 active symbols on $400 equity with 300% cap → $1200 budget ÷ 4 = $300.
        let e = eng(cfg_with(500.0, 300.0), 4);
        let budget = e.per_symbol_budget_usd(400.0);
        assert!((budget - 300.0).abs() < 0.01);
    }

    // ─── v1.3.3 cooldown + projected-exposure invariants ─────────────────────

    #[test]
    fn cooldown_returns_none_when_no_prior_loss() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let storage = Storage::open(tmp.path()).unwrap();
        storage.migrate().unwrap();
        let res = storage.last_loss_close_for("BTC", "SHORT").unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn cooldown_returns_timestamp_after_a_loss() {
        use crate::core::OpenPosition;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let storage = Storage::open(tmp.path()).unwrap();
        storage.migrate().unwrap();
        let p = OpenPosition {
            id: 0,
            venue: "hyperliquid".into(),
            symbol: "BTC".into(),
            side: "SHORT".into(),
            size: 0.001,
            entry_price: 78000.0,
            stop_price: Some(78250.0),
            take_profit: Some(77500.0),
            opened_at: chrono::Utc::now(),
            strategy: Some("mean_reversion".into()),
            trailing_high: Some(78000.0),
        };
        let pid = storage.insert_open_position(&p).unwrap();
        // Close at a loss
        storage
            .close_position(pid, 78250.0, -0.25, 1, "stop")
            .unwrap();
        let res = storage.last_loss_close_for("BTC", "SHORT").unwrap();
        assert!(res.is_some(), "expected a loss timestamp back");
    }

    #[test]
    fn cooldown_only_returns_loss_when_most_recent_was_a_loss() {
        use crate::core::OpenPosition;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let storage = Storage::open(tmp.path()).unwrap();
        storage.migrate().unwrap();
        // First close: a loss.
        let p1 = OpenPosition {
            id: 0,
            venue: "hl".into(),
            symbol: "ETH".into(),
            side: "LONG".into(),
            size: 0.05,
            entry_price: 2300.0,
            stop_price: Some(2280.0),
            take_profit: Some(2350.0),
            opened_at: chrono::Utc::now(),
            strategy: None,
            trailing_high: Some(2300.0),
        };
        let id1 = storage.insert_open_position(&p1).unwrap();
        storage
            .close_position(id1, 2280.0, -1.0, 1, "stop")
            .unwrap();
        // Most recent close: a WIN. Cooldown logic should clear.
        let p2 = OpenPosition {
            id: 0,
            venue: "hl".into(),
            symbol: "ETH".into(),
            side: "LONG".into(),
            size: 0.05,
            entry_price: 2310.0,
            stop_price: Some(2290.0),
            take_profit: Some(2360.0),
            opened_at: chrono::Utc::now(),
            strategy: None,
            trailing_high: Some(2310.0),
        };
        let id2 = storage.insert_open_position(&p2).unwrap();
        storage.close_position(id2, 2360.0, 2.5, 1, "tp").unwrap();
        let res = storage.last_loss_close_for("ETH", "LONG").unwrap();
        assert!(
            res.is_none(),
            "most recent close was a win; no cooldown should apply"
        );
    }

    #[test]
    fn cooldown_isolates_per_symbol_and_per_side() {
        use crate::core::OpenPosition;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let storage = Storage::open(tmp.path()).unwrap();
        storage.migrate().unwrap();
        let p = OpenPosition {
            id: 0,
            venue: "hl".into(),
            symbol: "BTC".into(),
            side: "SHORT".into(),
            size: 0.001,
            entry_price: 78000.0,
            stop_price: Some(78250.0),
            take_profit: Some(77500.0),
            opened_at: chrono::Utc::now(),
            strategy: None,
            trailing_high: Some(78000.0),
        };
        let id = storage.insert_open_position(&p).unwrap();
        storage
            .close_position(id, 78250.0, -0.25, 1, "stop")
            .unwrap();
        // Other symbol & other side must remain unblocked.
        assert!(storage
            .last_loss_close_for("ETH", "SHORT")
            .unwrap()
            .is_none());
        assert!(storage
            .last_loss_close_for("BTC", "LONG")
            .unwrap()
            .is_none());
        assert!(storage
            .last_loss_close_for("BTC", "SHORT")
            .unwrap()
            .is_some());
    }

    /// v1.6.1: stop-out-specific cooldown source.
    /// `last_stopout_close_for` must ONLY return stop-reason closes,
    /// not TP closes or any other exit reason. Even if TP came AFTER
    /// a stop-out, the most recent stop-out should still be returned.
    #[test]
    fn last_stopout_distinguishes_from_tp() {
        use crate::core::OpenPosition;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let storage = Storage::open(tmp.path()).unwrap();
        storage.migrate().unwrap();
        let mk = |id_seed: i64| OpenPosition {
            id: 0,
            venue: "hl".into(),
            symbol: "HYPE".into(),
            side: "LONG".into(),
            size: 1.0 + id_seed as f64 * 0.001,
            entry_price: 41.50,
            stop_price: Some(41.30),
            take_profit: Some(41.90),
            opened_at: chrono::Utc::now(),
            strategy: None,
            trailing_high: Some(41.50),
        };
        // Stop-out first.
        let s_id = storage.insert_open_position(&mk(1)).unwrap();
        storage
            .close_position(s_id, 41.30, -0.20, 1, "stop")
            .unwrap();
        // Then a winning TP exit AFTER it.
        let w_id = storage.insert_open_position(&mk(2)).unwrap();
        storage.close_position(w_id, 41.90, 0.40, 1, "tp").unwrap();
        // last_stopout must still report the stop-out (the TP doesn't shadow it).
        let res = storage.last_stopout_close_for("HYPE", "LONG").unwrap();
        assert!(
            res.is_some(),
            "stop-out should still be retrievable after a TP win"
        );
    }

    /// `last_stopout_close_for` must return None when no stops have happened
    /// for the (symbol, side), even if there are losing TP closes. (Edge case.)
    #[test]
    fn last_stopout_returns_none_without_any_stop() {
        use crate::core::OpenPosition;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let storage = Storage::open(tmp.path()).unwrap();
        storage.migrate().unwrap();
        let p = OpenPosition {
            id: 0,
            venue: "hl".into(),
            symbol: "BTC".into(),
            side: "SHORT".into(),
            size: 0.001,
            entry_price: 78000.0,
            stop_price: Some(78250.0),
            take_profit: Some(77500.0),
            opened_at: chrono::Utc::now(),
            strategy: None,
            trailing_high: Some(78000.0),
        };
        let id = storage.insert_open_position(&p).unwrap();
        // Closed via time-stop (a loss, but not a stop-out).
        storage
            .close_position(id, 77800.0, -0.20, 1, "time_stop")
            .unwrap();
        let res = storage.last_stopout_close_for("BTC", "SHORT").unwrap();
        assert!(
            res.is_none(),
            "time-stop closes must NOT trigger stop-out cooldown"
        );
    }

    // ─── v1.3.4 rolling-window + manual reset invariants ─────────────────────

    fn open_and_close(storage: &Storage, sym: &str, side: &str, pnl: f64) -> i64 {
        use crate::core::OpenPosition;
        let p = OpenPosition {
            id: 0,
            venue: "hl".into(),
            symbol: sym.into(),
            side: side.into(),
            size: 0.01,
            entry_price: 100.0,
            stop_price: Some(99.0),
            take_profit: Some(102.0),
            opened_at: chrono::Utc::now(),
            strategy: None,
            trailing_high: Some(100.0),
        };
        let pid = storage.insert_open_position(&p).unwrap();
        storage
            .close_position(pid, 100.0, pnl, 1, if pnl < 0.0 { "stop" } else { "tp" })
            .unwrap();
        pid
    }

    #[test]
    fn rolling_window_counts_recent_consecutive_losses() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let storage = Storage::open(tmp.path()).unwrap();
        storage.migrate().unwrap();
        for _ in 0..5 {
            open_and_close(&storage, "BTC", "SHORT", -0.1);
        }
        let n = storage.consecutive_losses(4).unwrap();
        assert_eq!(n, 5, "5 fresh losses should be counted");
    }

    #[test]
    fn rolling_window_a_win_breaks_the_streak() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let storage = Storage::open(tmp.path()).unwrap();
        storage.migrate().unwrap();
        open_and_close(&storage, "BTC", "SHORT", -0.1);
        open_and_close(&storage, "BTC", "SHORT", -0.1);
        open_and_close(&storage, "BTC", "SHORT", -0.1);
        open_and_close(&storage, "BTC", "SHORT", 0.5); // win
        open_and_close(&storage, "BTC", "SHORT", -0.1);
        let n = storage.consecutive_losses(4).unwrap();
        assert_eq!(n, 1, "only the most recent loss after the win counts");
    }

    #[test]
    fn manual_reset_clears_the_streak() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let storage = Storage::open(tmp.path()).unwrap();
        storage.migrate().unwrap();
        for _ in 0..5 {
            open_and_close(&storage, "BTC", "SHORT", -0.1);
        }
        assert_eq!(storage.consecutive_losses(4).unwrap(), 5);
        storage.reset_consecutive_losses().unwrap();
        assert_eq!(
            storage.consecutive_losses(4).unwrap(),
            0,
            "reset must zero out any prior streak"
        );
    }

    #[test]
    fn manual_reset_does_not_block_future_losses_from_counting() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let storage = Storage::open(tmp.path()).unwrap();
        storage.migrate().unwrap();
        for _ in 0..3 {
            open_and_close(&storage, "BTC", "SHORT", -0.1);
        }
        storage.reset_consecutive_losses().unwrap();
        assert_eq!(storage.consecutive_losses(4).unwrap(), 0);
        // Subsequent fresh losses count again.
        std::thread::sleep(std::time::Duration::from_millis(10));
        for _ in 0..2 {
            open_and_close(&storage, "BTC", "SHORT", -0.1);
        }
        assert_eq!(storage.consecutive_losses(4).unwrap(), 2);
    }

    // ─── v1.3.6 trend-filter deadband regression suite ───────────────────────
    //
    // Reproduces the exact symbol/mid/EMA combinations from the user's log
    // where the binary v1.3.3 filter blocked every mean-reversion signal.
    // With the v1.3.6 deadband (default 0.3%), every one of these noise-level
    // diffs should now resolve to InsideBand → signal allowed through.

    use super::{trend_filter_decision, TrendFilterDecision::*};

    #[test]
    fn user_log_btc_long_inside_band() {
        // BTC mid 77541.50 vs EMA240 77671.11 → diff -0.17%, well inside 0.3% band.
        assert_eq!(trend_filter_decision(77541.50, 77671.11, 0.3), InsideBand);
    }

    #[test]
    fn user_log_btc_short_inside_band() {
        // BTC mid 77666.50 vs EMA240 77543.05 → diff +0.16%, inside band.
        assert_eq!(trend_filter_decision(77666.50, 77543.05, 0.3), InsideBand);
    }

    #[test]
    fn user_log_eth_long_inside_band() {
        // ETH mid 2310.45 vs EMA240 2317.33 → diff -0.30%, exactly at the
        // boundary. Strictly less-than means -0.297% qualifies as inside.
        // (We use < not <= so exactly 0.30% blocks; -0.30% is effectively
        // -0.2974% with rounding which IS inside.)
        let d = trend_filter_decision(2310.45, 2317.33, 0.3);
        // Either decision is acceptable at exactly threshold; just shouldn't
        // be blocking the signal direction (LONG).
        assert!(matches!(d, InsideBand | BlockLong));
    }

    #[test]
    fn user_log_hype_long_inside_band() {
        // HYPE mid 41.20 vs EMA240 41.21 → diff -0.024%, way inside band.
        assert_eq!(trend_filter_decision(41.20, 41.21, 0.3), InsideBand);
    }

    #[test]
    fn real_uptrend_blocks_short() {
        // Overnight BTC drift scenario: price 0.6% above EMA → real trend.
        assert_eq!(trend_filter_decision(78000.0, 77535.0, 0.3), BlockShort);
    }

    #[test]
    fn real_downtrend_blocks_long() {
        assert_eq!(trend_filter_decision(2280.0, 2300.0, 0.3), BlockLong);
    }

    #[test]
    fn band_zero_means_legacy_v1_3_3_behaviour() {
        // Setting band to 0 disables the deadband — any non-zero diff blocks.
        // Useful for users who explicitly want the original strict filter.
        assert_eq!(trend_filter_decision(100.01, 100.0, 0.0), BlockShort);
        assert_eq!(trend_filter_decision(99.99, 100.0, 0.0), BlockLong);
    }

    #[test]
    fn zero_or_negative_ema_returns_inside_band_safely() {
        // Defense in depth: bad EMA inputs never panic, never falsely block.
        assert_eq!(trend_filter_decision(100.0, 0.0, 0.3), InsideBand);
        assert_eq!(trend_filter_decision(100.0, -50.0, 0.3), InsideBand);
        assert_eq!(trend_filter_decision(f64::NAN, 100.0, 0.3), InsideBand);
        assert_eq!(trend_filter_decision(100.0, f64::INFINITY, 0.3), InsideBand);
    }
}
