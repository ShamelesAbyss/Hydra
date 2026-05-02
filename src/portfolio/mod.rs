//! Portfolio / position manager. Owns the open-position lifecycle:
//! - tracks live positions (DB-backed) plus trailing state
//! - manages stops / take-profits / time-stops per tick
//! - executes reconciliation ops from shutdown modes
//!
//! v1.3.2 adds chandelier trailing: once unrealized profit reaches
//! `breakeven_trigger_r` × R, the stop is pulled to entry (break-even) and
//! then trails at `peak − chandelier_trail_r × R` (mirrored for shorts).

use anyhow::Result;
use chrono::{Duration, Utc};
use std::collections::HashMap;

use crate::adapters::VenueAdapter;
use crate::config::AppConfig;
use crate::core::{MarketSnapshot, OpenPosition, OrderIntent};
use crate::execution::ExecutionEngine;
use crate::learning::{self, regime::RegimeView};
use crate::storage::{CompoundState, Storage};

/// v1.5.0 — context captured at entry time, replayed at close into the
/// `trade_context` table. Lives only in memory: positions opened in a
/// previous run will simply skip the learning record on close (the trade
/// itself is unaffected). Cheap and restart-safe.
#[derive(Debug, Clone)]
pub struct EntryContext {
    pub strategy: String,
    pub side: String,
    pub regime_view: RegimeView,
    pub atr_pct: f64,
    pub spread_bps: Option<f64>,
    pub edge_bps_predicted: f64,
}

pub struct PortfolioManager {
    storage: Storage,
    last_equity: f64,
    /// position_id → initial per-unit R (|entry − original stop|).
    /// Recorded on open; populated lazily for pre-v1.3.2 positions using
    /// their current stop distance as a best-effort approximation.
    initial_r: HashMap<i64, f64>,
    /// position_id → entry context, for v1.5.0 learning records.
    entry_ctx: HashMap<i64, EntryContext>,
}

impl PortfolioManager {
    pub fn new(storage: Storage) -> Self {
        Self {
            storage, last_equity: 0.0,
            initial_r: HashMap::new(),
            entry_ctx: HashMap::new(),
        }
    }

    pub fn last_known_equity(&self) -> f64 {
        if self.last_equity > 0.0 { return self.last_equity; }
        self.storage.last_equity().ok().flatten().map(|(_, e)| e).unwrap_or(1000.0)
    }

    /// Total & per-market open notional in USD (approximate: size * entry).
    pub fn exposure(&self, symbol: &str) -> (f64, f64) {
        let positions = self.storage.list_open_positions().unwrap_or_default();
        let mut total = 0.0;
        let mut per_mk = 0.0;
        for p in positions {
            let n = p.size.abs() * p.entry_price;
            total += n;
            if p.symbol == symbol { per_mk += n; }
        }
        (total, per_mk)
    }

    pub fn open_from_fill(
        &mut self,
        intent: &OrderIntent,
        res: &crate::core::OrderResult,
        strategy: Option<&str>,
        entry_ctx: Option<EntryContext>,
    ) -> Result<()> {
        if res.filled_size <= 0.0 { return Ok(()); }
        let price = res.avg_fill_price.or(intent.limit_price).unwrap_or(0.0);
        let p = OpenPosition {
            id: 0,
            venue: intent.venue.clone(),
            symbol: intent.symbol.clone(),
            side: intent.side.as_str().to_string(),
            size: res.filled_size,
            entry_price: price,
            stop_price: intent.stop_price,
            take_profit: intent.take_profit,
            opened_at: Utc::now(),
            strategy: strategy.map(|s| s.to_string()),
            trailing_high: Some(price),
        };
        let id = self.storage.insert_open_position(&p)?;
        // Record initial R so chandelier math is accurate for the lifetime
        // of this position, even after trailing moves the stored stop.
        if let Some(stop) = intent.stop_price {
            let r = (price - stop).abs();
            if r > 0.0 && r.is_finite() {
                self.initial_r.insert(id, r);
            }
        }
        if let Some(ctx) = entry_ctx {
            self.entry_ctx.insert(id, ctx);
        }
        Ok(())
    }

    /// Best-effort fetch of the initial R for a position. Exact for positions
    /// opened under v1.3.2+. For pre-v1.3.2 positions (no cache entry), falls
    /// back to the current stop distance — worst case the chandelier triggers
    /// a tad early, which is a conservative failure mode.
    fn initial_r_for(&mut self, p: &OpenPosition) -> f64 {
        if let Some(r) = self.initial_r.get(&p.id) {
            return *r;
        }
        let stop = p.stop_price.unwrap_or(p.entry_price);
        let r = (p.entry_price - stop).abs();
        if r > 0.0 && r.is_finite() {
            self.initial_r.insert(p.id, r);
        }
        r
    }

    pub async fn manage_open_positions(
        &mut self,
        snapshots: &[MarketSnapshot],
        cfg: &AppConfig,
        execution: &mut ExecutionEngine,
        venues: &[Box<dyn VenueAdapter>],
    ) -> Result<()> {
        let positions = self.storage.list_open_positions()?;
        let pm = &cfg.strategies.position_management;
        for p in positions {
            let snap = match snapshots.iter().find(|s| s.venue == p.venue && s.symbol == p.symbol) {
                Some(s) => s,
                None => continue,
            };
            let mid = snap.mid;
            let is_long = p.side == "LONG";
            let mut new_trail = p.trailing_high.unwrap_or(p.entry_price);
            let mut new_stop = p.stop_price;
            let mut stop_changed = false;

            // ── (1) Existing ATR-based trailing: ratchet only on new peaks ──
            if pm.trailing_enabled {
                if is_long && mid > new_trail {
                    new_trail = mid;
                    let atr_stop = mid - (mid - p.entry_price).abs() * 0.25 - pm.trailing_atr_mult * (mid * 0.001);
                    if let Some(s) = new_stop { new_stop = Some(s.max(atr_stop)); }
                    stop_changed = true;
                } else if !is_long && mid < new_trail {
                    new_trail = mid;
                    let atr_stop = mid + pm.trailing_atr_mult * (mid * 0.001);
                    if let Some(s) = new_stop { new_stop = Some(s.min(atr_stop)); }
                    stop_changed = true;
                }
            }

            // ── (2) v1.3.2 chandelier: break-even + peak-based trail ──
            //
            // Only runs when the user enables it via config (defaults ON with
            // 0.5R/0.5R). Evaluated every tick — not just on new peaks —
            // because the whole point is to defend profit once earned.
            if pm.breakeven_trigger_r > 0.0 {
                let r_init = self.initial_r_for(&p);
                if r_init > 0.0 {
                    let profit_r = if is_long {
                        (mid - p.entry_price) / r_init
                    } else {
                        (p.entry_price - mid) / r_init
                    };
                    if profit_r >= pm.breakeven_trigger_r {
                        let target = chandelier_target(
                            is_long, p.entry_price, new_trail, r_init, pm.chandelier_trail_r,
                        );
                        // Monotonic move: only raise (long) / lower (short) the stop.
                        let new_candidate = match new_stop {
                            Some(cur) => {
                                if is_long {
                                    if target > cur { Some(target) } else { None }
                                } else {
                                    if target < cur { Some(target) } else { None }
                                }
                            }
                            None => Some(target),
                        };
                        if let Some(nv) = new_candidate {
                            new_stop = Some(nv);
                            stop_changed = true;
                            tracing::info!(
                                position_id = p.id,
                                side = if is_long {"LONG"} else {"SHORT"},
                                profit_r = profit_r,
                                new_stop = nv,
                                "chandelier: stop defended",
                            );
                        }
                    }
                }
            }

            if stop_changed {
                self.storage.update_position_trailing(p.id, new_trail, new_stop)?;
            }

            // ── Exit conditions (UNCHANGED) ──
            let hit_stop = match (is_long, new_stop) {
                (true, Some(s)) => mid <= s,
                (false, Some(s)) => mid >= s,
                _ => false,
            };
            let hit_tp = match (is_long, p.take_profit) {
                (true, Some(t))  => mid >= t,
                (false, Some(t)) => mid <= t,
                _ => false,
            };
            let age = Utc::now() - p.opened_at;
            let time_stop = age > Duration::minutes(pm.time_stop_minutes);

            if hit_stop || hit_tp || time_stop {
                let reason = if hit_stop {"stop"} else if hit_tp {"tp"} else {"time_stop"};
                tracing::info!(position_id=p.id, reason, mid, "closing position");
                if let Some(v) = venues.iter().find(|v| v.kind().as_str() == p.venue) {
                    let _res = execution.close(v.as_ref(), &p.symbol, p.size, !is_long).await.ok();
                }
                let realized = if is_long { (mid - p.entry_price) * p.size } else { (p.entry_price - mid) * p.size };
                let session = self.storage.list_sessions(1)?.first().map(|s| s.id).unwrap_or(0);
                self.storage.close_position(p.id, mid, realized, session, reason)?;

                // v1.5.0: persist the trade context for the learning module.
                // Best-effort: if we don't have entry context (position was
                // opened before this process started, or restart), skip.
                if let Some(ctx) = self.entry_ctx.remove(&p.id) {
                    let r_init = self.initial_r.get(&p.id).copied().unwrap_or_else(|| {
                        let stop = p.stop_price.unwrap_or(p.entry_price);
                        (p.entry_price - stop).abs()
                    });
                    let dur = (Utc::now() - p.opened_at).num_seconds().max(0);
                    if let Err(e) = learning::context::record_trade_context(
                        &self.storage,
                        &p.symbol,
                        &ctx.strategy,
                        &ctx.side,
                        ctx.regime_view,
                        ctx.atr_pct,
                        ctx.spread_bps,
                        ctx.edge_bps_predicted,
                        p.entry_price,
                        mid,
                        r_init,
                        realized,
                        dur,
                        reason,
                        p.opened_at,
                    ) {
                        tracing::warn!(error=%e, position_id=p.id, "failed to record trade_context for learning");
                    }
                }

                self.initial_r.remove(&p.id);
                self.apply_compounding(realized)?;
            }
        }
        Ok(())
    }

    fn apply_compounding(&self, realized: f64) -> Result<()> {
        let mut state = self.storage.get_compound_state()?;
        if state.starting_capital == 0.0 {
            state.starting_capital = self.last_known_equity();
        }
        state.realized_profit += realized;
        let excess = (state.realized_profit - 0.0).max(0.0);
        let scale = 1.0 + (excess / state.starting_capital.max(1.0)).min(2.0);
        state.current_risk_mult = scale.max(0.2);
        if state.realized_profit < 0.0 { state.current_risk_mult *= 0.6; }
        state.updated_at = Utc::now().to_rfc3339();
        self.storage.upsert_compound_state(&state)?;
        Ok(())
    }

    pub async fn reduce_all(
        &mut self,
        cfg: &AppConfig,
        execution: &mut ExecutionEngine,
        venues: &[Box<dyn VenueAdapter>],
    ) -> Result<()> {
        let _ = cfg;
        for p in self.storage.list_open_positions()? {
            if let Some(v) = venues.iter().find(|v| v.kind().as_str() == p.venue) {
                let reduce_size = p.size * 0.5;
                let _ = execution.close(v.as_ref(), &p.symbol, reduce_size, p.side == "SHORT").await;
                let new_size = p.size - reduce_size;
                let conn = self.storage.clone();
                let _ = conn;
                tracing::info!(position_id=p.id, remaining=new_size, "reduced on shutdown");
            }
        }
        Ok(())
    }

    pub async fn flatten_all(
        &mut self,
        cfg: &AppConfig,
        execution: &mut ExecutionEngine,
        venues: &[Box<dyn VenueAdapter>],
    ) -> Result<()> {
        let _ = cfg;
        for p in self.storage.list_open_positions()? {
            if let Some(v) = venues.iter().find(|v| v.kind().as_str() == p.venue) {
                let _ = execution.close(v.as_ref(), &p.symbol, p.size, p.side == "SHORT").await;
                let session = self.storage.list_sessions(1)?.first().map(|s| s.id).unwrap_or(0);
                self.storage.close_position(p.id, p.entry_price, 0.0, session, "manual_flatten")?;
            }
        }
        Ok(())
    }

    pub fn sync_equity(&mut self, venue: &str, equity: f64) -> Result<()> {
        self.last_equity = equity;
        self.storage.record_equity(venue, equity)
    }
}

// Keep CompoundState importable from modules that only import storage:
pub type CompoundStateAlias = CompoundState;

/// Pure chandelier math, extracted so it's trivially unit-testable.
///
/// * `is_long` — direction of the position.
/// * `entry` — fill price.
/// * `peak` — best price seen since open (`trailing_high` for longs, lowest
///   mid for shorts; already tracked by existing trailing code).
/// * `r_init` — initial per-unit risk (|entry − original stop|). MUST be > 0.
/// * `trail_r_mult` — how many R below the peak the stop trails at.
///
/// Returns the target stop price: `max(entry, peak − trail_r × R)` for longs,
/// mirrored for shorts. Never returns a value worse than break-even.
pub(crate) fn chandelier_target(
    is_long: bool,
    entry: f64,
    peak: f64,
    r_init: f64,
    trail_r_mult: f64,
) -> f64 {
    let trail = trail_r_mult.max(0.0) * r_init;
    if is_long {
        entry.max(peak - trail)
    } else {
        entry.min(peak + trail)
    }
}

#[cfg(test)]
mod tests {
    use super::chandelier_target;

    const EPS: f64 = 1e-9;
    fn near(a: f64, b: f64) -> bool { (a - b).abs() < EPS }

    #[test]
    fn long_at_1r_profit_locks_breakeven() {
        // Entry 100, initial stop 98 → R = 2. Peak at 102 (1R profit).
        // Chandelier target = max(100, 102 − 0.5×2) = max(100, 101) = 101.
        let t = chandelier_target(true, 100.0, 102.0, 2.0, 0.5);
        assert!(near(t, 101.0), "got {}", t);
        assert!(t >= 100.0, "must never go below break-even");
    }

    #[test]
    fn long_at_0_5r_profit_exactly_at_break_even() {
        // Peak = entry + 0.5R = 101. Target = max(100, 101 − 1) = max(100, 100) = 100.
        let t = chandelier_target(true, 100.0, 101.0, 2.0, 0.5);
        assert!(near(t, 100.0), "got {}", t);
    }

    #[test]
    fn long_at_3r_profit_trails_peak_minus_half_r() {
        // Peak = 106 (3R). Target = max(100, 106 − 1) = 105.
        let t = chandelier_target(true, 100.0, 106.0, 2.0, 0.5);
        assert!(near(t, 105.0), "got {}", t);
    }

    #[test]
    fn short_mirror_symmetry() {
        // Short entry 100, stop 102 → R = 2. Peak (lowest) = 94 → 3R profit.
        // Target = min(100, 94 + 1) = min(100, 95) = 95.
        let t = chandelier_target(false, 100.0, 94.0, 2.0, 0.5);
        assert!(near(t, 95.0), "got {}", t);
    }

    #[test]
    fn short_never_worse_than_break_even() {
        // At exactly 0.5R profit on short: peak = 99, r = 2.
        // Target = min(100, 99 + 1) = min(100, 100) = 100.
        let t = chandelier_target(false, 100.0, 99.0, 2.0, 0.5);
        assert!(near(t, 100.0));
    }

    #[test]
    fn tighter_trail_r_gives_tighter_stop() {
        // trail_r_mult = 0.25 (aggressive) vs 1.0 (loose), same peak.
        let tight = chandelier_target(true, 100.0, 106.0, 2.0, 0.25);
        let loose = chandelier_target(true, 100.0, 106.0, 2.0, 1.0);
        assert!(tight > loose, "tight={} loose={}", tight, loose);
    }

    #[test]
    fn zero_trail_is_hard_peak_lock() {
        // trail_r_mult = 0.0 means the stop sits at the peak itself (max with entry).
        let t = chandelier_target(true, 100.0, 106.0, 2.0, 0.0);
        assert!(near(t, 106.0));
    }

    #[test]
    fn btc_scenario_reproduces_user_case() {
        // User's actual trade shape: entry 77534, stop ≈ 77280 (ATR-based),
        // peak at 77784 (+$1.60 unrealized on 0.0064 BTC).
        // R = 254. Profit_R at peak = (77784 − 77534) / 254 ≈ 0.984 — above 0.5 trigger.
        // Chandelier target = max(77534, 77784 − 127) = max(77534, 77657) = 77657.
        // So on retrace, stop hits at 77657 instead of letting price fall back to entry.
        let t = chandelier_target(true, 77534.0, 77784.0, 254.0, 0.5);
        assert!((t - 77657.0).abs() < 0.5, "got {}", t);
        assert!(t > 77534.0, "must be above break-even after that much profit");
    }
}
