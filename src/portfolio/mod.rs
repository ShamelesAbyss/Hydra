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
use crate::learning::regime::RegimeView;
use crate::storage::{CompoundState, Storage};

/// v1.5.0 — context captured at entry time, replayed at close into the
/// `trade_context` table. Persisted atomically with each new position so
/// restarts retain the original learning and risk context.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    last_equity_at: Option<std::time::Instant>,
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
            storage,
            last_equity: 0.0,
            last_equity_at: None,
            initial_r: HashMap::new(),
            entry_ctx: HashMap::new(),
        }
    }

    pub fn last_known_equity(&self) -> f64 {
        if self.last_equity > 0.0 {
            return self.last_equity;
        }
        self.storage
            .last_equity()
            .ok()
            .flatten()
            .map(|(_, e)| e)
            .unwrap_or(0.0)
    }

    pub fn verified_equity(&self, max_age: std::time::Duration) -> Result<f64> {
        anyhow::ensure!(
            self.last_equity_at
                .is_some_and(|at| at.elapsed() <= max_age),
            "account equity stale or unavailable"
        );
        anyhow::ensure!(
            self.last_equity.is_finite() && self.last_equity > 0.0,
            "account equity unavailable or zero"
        );
        Ok(self.last_equity)
    }

    pub fn invalidate_equity(&mut self) {
        self.last_equity_at = None;
    }

    /// Total & per-market open notional in USD (approximate: size * entry).
    pub fn exposure(&self, symbol: &str) -> Result<(f64, f64)> {
        let positions = self.storage.list_open_positions()?;
        let mut total = 0.0;
        let mut per_mk = 0.0;
        for p in positions {
            anyhow::ensure!(
                p.size.is_finite()
                    && p.size > 0.0
                    && p.entry_price.is_finite()
                    && p.entry_price > 0.0,
                "invalid persisted position; exposure unavailable"
            );
            let n = p.size.abs() * p.entry_price;
            anyhow::ensure!(n.is_finite(), "position notional overflow");
            total += n;
            if p.symbol == symbol {
                per_mk += n;
            }
        }
        anyhow::ensure!(total.is_finite() && per_mk.is_finite(), "exposure overflow");
        Ok((total, per_mk))
    }

    pub fn open_from_fill(
        &mut self,
        intent: &OrderIntent,
        res: &crate::core::OrderResult,
        strategy: Option<&str>,
        entry_ctx: Option<EntryContext>,
    ) -> Result<()> {
        anyhow::ensure!(
            res.accepted
                && res.intent_id == intent.id
                && res.filled_size.is_finite()
                && res.filled_size > 0.0
                && res.filled_size <= intent.size,
            "invalid entry fill"
        );
        let price = res
            .avg_fill_price
            .ok_or_else(|| anyhow::anyhow!("entry fill price unavailable"))?;
        anyhow::ensure!(price.is_finite() && price > 0.0, "invalid entry fill price");
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
        let initial_r = intent.stop_price.and_then(|stop| {
            let r = if intent.side.is_buy() {
                price - stop
            } else {
                stop - price
            };
            (r.is_finite() && r > 0.0).then_some(r)
        });
        let context_json = entry_ctx.as_ref().map(serde_json::to_string).transpose()?;
        let id = crate::position_context::insert_position_with_context(
            &self.storage,
            &p,
            &intent.id,
            initial_r,
            context_json.as_deref(),
        )?;
        // Record initial R so chandelier math is accurate for the lifetime
        // of this position, even after trailing moves the stored stop.
        self.initial_r.insert(id, initial_r.unwrap_or(0.0));
        if let Some(ctx) = entry_ctx {
            self.entry_ctx.insert(id, ctx);
        }
        Ok(())
    }

    /// Recover original entry facts, preserving unknown legacy risk explicitly.
    fn initial_r_for(&mut self, p: &OpenPosition) -> Result<f64> {
        if let Some(r) = self.initial_r.get(&p.id) {
            return Ok(*r);
        }
        let entry = crate::position_context::load_entry_state(&self.storage, p.id)?;
        if let Some(json) = entry.context_json {
            match serde_json::from_str(&json) {
                Ok(context) => {
                    self.entry_ctx.insert(p.id, context);
                }
                Err(error) => {
                    tracing::warn!(position_id=p.id,%error,"learning context unavailable; original risk metadata retained")
                }
            }
        }
        let r = entry.initial_r.unwrap_or(0.0);
        self.initial_r.insert(p.id, r);
        if r == 0.0 {
            tracing::warn!(
                position_id = p.id,
                "original risk distance unavailable; retained stop still applies"
            );
        }
        Ok(r)
    }

    pub async fn manage_open_positions(
        &mut self,
        snapshots: &[MarketSnapshot],
        cfg: &AppConfig,
        execution: &mut ExecutionEngine,
        venues: &[Box<dyn VenueAdapter>],
    ) -> Result<()> {
        let positions = self.storage.list_open_positions()?;
        let mut close_failures = Vec::new();
        let pm = &cfg.strategies.position_management;
        for p in positions {
            let snap = match snapshots
                .iter()
                .find(|s| s.venue == p.venue && s.symbol == p.symbol)
            {
                Some(s) => s,
                None => continue,
            };
            let mid = snap.mid;
            let is_long = p.side == "LONG";
            let mut new_trail = p.trailing_high.unwrap_or(p.entry_price);
            let mut new_stop = p.stop_price;
            let mut stop_changed = false;

            let r_init = match self.initial_r_for(&p) {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(position_id=p.id, %error, "entry context unavailable; maintaining existing protective exits");
                    0.0
                }
            };

            // ── (1) Existing ATR-based trailing: ratchet only on new peaks ──
            if pm.trailing_enabled {
                if is_long && mid > new_trail {
                    new_trail = mid;
                    let atr_stop = mid
                        - (mid - p.entry_price).abs() * 0.25
                        - pm.trailing_atr_mult * (mid * 0.001);
                    if let Some(s) = new_stop {
                        new_stop = Some(s.max(atr_stop));
                    }
                    stop_changed = true;
                } else if !is_long && mid < new_trail {
                    new_trail = mid;
                    let atr_stop = mid + pm.trailing_atr_mult * (mid * 0.001);
                    if let Some(s) = new_stop {
                        new_stop = Some(s.min(atr_stop));
                    }
                    stop_changed = true;
                }
            }

            // ── (2) v1.3.2 chandelier: break-even + peak-based trail ──
            //
            // Only runs when the user enables it via config (defaults ON with
            // 0.5R/0.5R). Evaluated every tick — not just on new peaks —
            // because the whole point is to defend profit once earned.
            if pm.breakeven_trigger_r > 0.0 {
                if r_init > 0.0 {
                    let profit_r = if is_long {
                        (mid - p.entry_price) / r_init
                    } else {
                        (p.entry_price - mid) / r_init
                    };
                    if profit_r >= pm.breakeven_trigger_r {
                        let target = chandelier_target(
                            is_long,
                            p.entry_price,
                            new_trail,
                            r_init,
                            pm.chandelier_trail_r,
                        );
                        // Monotonic move: only raise (long) / lower (short) the stop.
                        let new_candidate = match new_stop {
                            Some(cur) => {
                                if is_long {
                                    if target > cur {
                                        Some(target)
                                    } else {
                                        None
                                    }
                                } else {
                                    if target < cur {
                                        Some(target)
                                    } else {
                                        None
                                    }
                                }
                            }
                            None => Some(target),
                        };
                        if let Some(nv) = new_candidate {
                            new_stop = Some(nv);
                            stop_changed = true;
                            tracing::info!(
                                position_id = p.id,
                                side = if is_long { "LONG" } else { "SHORT" },
                                profit_r = profit_r,
                                new_stop = nv,
                                "chandelier: stop defended",
                            );
                        }
                    }
                }
            }

            if stop_changed {
                self.storage
                    .update_position_trailing(p.id, new_trail, new_stop)?;
            }

            // ── Exit conditions (UNCHANGED) ──
            let hit_stop = match (is_long, new_stop) {
                (true, Some(s)) => mid <= s,
                (false, Some(s)) => mid >= s,
                _ => false,
            };
            let hit_tp = match (is_long, p.take_profit) {
                (true, Some(t)) => mid >= t,
                (false, Some(t)) => mid <= t,
                _ => false,
            };
            let age = Utc::now() - p.opened_at;
            let time_stop = age > Duration::minutes(pm.time_stop_minutes);

            if hit_stop || hit_tp || time_stop {
                let reason = if hit_stop {
                    "stop"
                } else if hit_tp {
                    "tp"
                } else {
                    "time_stop"
                };
                tracing::info!(position_id = p.id, reason, mid, "closing position");
                if let Err(error) = self
                    .close_confirmed(&p, p.size, reason, execution, venues)
                    .await
                {
                    close_failures.push(format!("{}/{}: {error}", p.venue, p.symbol));
                }
            }
        }
        anyhow::ensure!(
            close_failures.is_empty(),
            "position exits incomplete: {}",
            close_failures.join("; ")
        );
        Ok(())
    }

    /// Close only from a validated fill plus a matching post-order account position.
    async fn close_confirmed(
        &mut self,
        p: &OpenPosition,
        requested: f64,
        reason: &str,
        execution: &mut ExecutionEngine,
        venues: &[Box<dyn VenueAdapter>],
    ) -> Result<()> {
        anyhow::ensure!(
            self.storage
                .list_open_positions()?
                .iter()
                .filter(|other| other.venue == p.venue && other.symbol == p.symbol)
                .count()
                == 1,
            "ambiguous local position ownership; reconcile before closing"
        );
        let venue = venues
            .iter()
            .find(|v| v.kind().as_str() == p.venue)
            .ok_or_else(|| anyhow::anyhow!("close venue unavailable"))?;
        let res = execution
            .close(venue.as_ref(), &p.symbol, requested, p.side == "SHORT")
            .await?;
        if !res.accepted || res.filled_size == 0.0 {
            anyhow::bail!("close not filled: {}", res.message);
        }
        anyhow::ensure!(
            res.filled_size.is_finite() && res.filled_size > 0.0 && res.filled_size <= requested,
            "invalid close fill quantity; reconciliation required"
        );
        let px = res
            .avg_fill_price
            .ok_or_else(|| anyhow::anyhow!("missing close fill price"))?;
        anyhow::ensure!(px.is_finite() && px > 0.0, "invalid close fill price");
        let remote = venue.open_positions().await?;
        let same: Vec<_> = remote.iter().filter(|r| r.symbol == p.symbol).collect();
        anyhow::ensure!(same.len() <= 1, "duplicate remote positions");
        let signed = same.first().map(|r| r.size).unwrap_or(0.0);
        anyhow::ensure!(
            signed.is_finite() && (signed == 0.0 || (signed > 0.0) == (p.side == "LONG")),
            "remote position side changed; reconciliation required"
        );
        let remaining = signed.abs();
        let expected = p.size - res.filled_size;
        anyhow::ensure!(
            (remaining - expected).abs() <= 1e-10 * p.size.max(1.0),
            "remote residual disagrees with confirmed fill; reconciliation required"
        );
        self.storage
            .apply_confirmed_reduction(p, &res, remaining, reason)?;
        if remaining == 0.0 {
            self.initial_r.remove(&p.id);
            self.entry_ctx.remove(&p.id);
        }
        tracing::info!(
            position_id = p.id,
            remaining,
            "confirmed reduction; net PnL pending fee/funding reconciliation"
        );
        Ok(())
    }

    pub async fn reduce_all(
        &mut self,
        _cfg: &AppConfig,
        execution: &mut ExecutionEngine,
        venues: &[Box<dyn VenueAdapter>],
    ) -> Result<()> {
        self.close_all(0.5, "shutdown_reduce", execution, venues)
            .await
    }

    pub async fn flatten_all(
        &mut self,
        _cfg: &AppConfig,
        execution: &mut ExecutionEngine,
        venues: &[Box<dyn VenueAdapter>],
    ) -> Result<()> {
        self.close_all(1.0, "manual_flatten", execution, venues)
            .await
    }

    async fn close_all(
        &mut self,
        fraction: f64,
        reason: &str,
        execution: &mut ExecutionEngine,
        venues: &[Box<dyn VenueAdapter>],
    ) -> Result<()> {
        let mut failures = Vec::new();
        for p in self.storage.list_open_positions()? {
            if let Err(e) = self
                .close_confirmed(&p, p.size * fraction, reason, execution, venues)
                .await
            {
                failures.push(format!("{}/{}: {e}", p.venue, p.symbol));
            }
        }
        anyhow::ensure!(
            failures.is_empty(),
            "close incomplete: {}",
            failures.join("; ")
        );
        Ok(())
    }

    pub fn sync_equity(&mut self, venue: &str, equity: f64) -> Result<()> {
        anyhow::ensure!(
            equity.is_finite() && equity >= 0.0,
            "invalid account equity"
        );
        self.storage.record_equity(venue, equity)?;
        self.last_equity = equity;
        self.last_equity_at = Some(std::time::Instant::now());
        Ok(())
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
    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

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
        assert!(
            t > 77534.0,
            "must be above break-even after that much profit"
        );
    }
}
