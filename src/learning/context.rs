//! `AdaptiveContext` — the unified learning surface consumed by the risk
//! engine. Aggregates expectancy + regime + memory into one cohesive
//! per-signal decision.
//!
//! ```text
//!     signal in
//!         │
//!         ▼
//! ┌───────────────┐   for_symbol()
//! │  Expectancy   │   for_strategy()
//! │   queries     │   for_regime_combo()
//! └───────┬───────┘
//!         │
//!         ▼
//! ┌───────────────┐
//! │   k-NN        │   similar_trades()
//! │   memory      │
//! └───────┬───────┘
//!         │
//!         ▼
//! ┌───────────────┐
//! │ Aggregator    │   AdaptiveContext::for_signal()
//! └───────┬───────┘
//!         │
//!         ▼
//!     SignalDecision
//!         { Allow(size_mult, edge_threshold)
//!         | Block(reason) }
//! ```

use anyhow::Result;

use super::expectancy;
use super::memory::{self, FeatureVec};
use super::regime::RegimeView;
use crate::config::LearningCfg;
use crate::storage::Storage;

/// What the learning system says about a candidate signal.
#[derive(Debug, Clone)]
pub struct SignalDecision {
    pub kind: SignalDecisionKind,
    /// Position-size multiplier in [0.5, 1.5]. Applied AFTER the existing
    /// risk-engine sizing math; never violates the global exposure cap.
    pub size_mult: f64,
    /// Edge-threshold override (bps). When None, the risk engine uses the
    /// configured `min_net_edge_bps`.
    pub edge_threshold_override: Option<f64>,
    /// Human-readable summary for the log + cockpit feed.
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalDecisionKind {
    Allow,
    Block(String),
}

/// Heart of v1.5.0. Created once per signal evaluation.
pub struct AdaptiveContext<'a> {
    storage: &'a Storage,
    cfg: &'a LearningCfg,
}

impl<'a> AdaptiveContext<'a> {
    pub fn new(storage: &'a Storage, cfg: &'a LearningCfg) -> Self {
        Self { storage, cfg }
    }

    /// Compute the decision for a candidate signal.
    /// All math is bounded — even with zero history this returns a safe
    /// neutral result (`size_mult = 1.0`, no override, Allow).
    pub fn for_signal(
        &self,
        symbol: &str,
        strategy: &str,
        side: &str,
        regime_view: RegimeView,
        feat: &FeatureVec,
    ) -> Result<SignalDecision> {
        // Learning master switch — return identity decision and skip work.
        if !self.cfg.enabled {
            return Ok(SignalDecision {
                kind: SignalDecisionKind::Allow,
                size_mult: 1.0,
                edge_threshold_override: None,
                summary: "adaptive: disabled".into(),
            });
        }

        // ── 1. Expectancy buckets ──
        let exp_sym  = expectancy::for_symbol  (self.storage, symbol)?;
        let exp_str  = expectancy::for_strategy(self.storage, strategy)?;
        let regime_5m_str = format!("{:?}", regime_view.r_5m);
        let exp_combo = expectancy::for_regime_combo(
            self.storage, &regime_5m_str, symbol, strategy,
        )?;

        // ── 2. Memory similarity ──
        let memory_verdict = memory::similar_trades(self.storage, symbol, strategy, side, feat)?;

        // ── 3. Memory-based block (early refuse) ──
        if self.cfg.memory_block_enabled {
            let max_wr = (self.cfg.memory_block_winrate_pct / 100.0).clamp(0.0, 1.0);
            if memory_verdict.is_blocking_with(self.cfg.memory_min_matches, max_wr) {
                return Ok(SignalDecision {
                    kind: SignalDecisionKind::Block(memory_verdict.summary()),
                    size_mult: 0.0,
                    edge_threshold_override: None,
                    summary: memory_verdict.summary(),
                });
            }
        }

        // ── 4. Compose multipliers. Multiplicative, each clamped to [0.5, 1.5],
        //    then re-clamped after combining. Symbol × Strategy × RegimeCombo. ──
        let m_sym    = exp_sym.to_multiplier();
        let m_str    = exp_str.to_multiplier();
        // Regime-combo multiplier weighted by regime-confidence: when the 3
        // timeframes disagree the regime signal is unreliable, so we pull
        // its multiplier toward neutral.
        let regime_conf = regime_view.confidence() as f64 / 100.0;
        let m_combo_raw = exp_combo.to_multiplier();
        let m_combo    = 1.0 + (m_combo_raw - 1.0) * regime_conf;
        let mult_raw_unchecked = m_sym * m_str * m_combo;
        // v1.5.0 defense-in-depth: NaN/Inf can never reach the risk engine.
        let mult_raw = if mult_raw_unchecked.is_finite() {
            mult_raw_unchecked.clamp(0.5, 1.5)
        } else { 1.0 };
        let mult       = if self.cfg.size_mult_enabled { mult_raw } else { 1.0 };

        // ── 5. Adaptive edge threshold ──
        // If the strategy is on a winning streak (avg_r > 0), loosen the gate
        // by up to 4bps. If on a losing streak (avg_r < 0), tighten by up to 6bps.
        // Asymmetric on purpose — we want to fail safer than we succeed.
        let edge_override = if self.cfg.edge_override_enabled
            && exp_str.samples >= expectancy::MIN_SAMPLES
        {
            let r = exp_str.avg_r.clamp(-1.0, 1.0);
            let delta = if r >= 0.0 { -r * 4.0 * exp_str.confidence }
                        else        {  r.abs() * 6.0 * exp_str.confidence };
            Some(delta)
        } else { None };

        let summary = format!(
            "adaptive: sym×{:.2} str×{:.2} regime×{:.2} (conf {:.0}%) mult={:.2} | {}",
            m_sym, m_str, m_combo, regime_conf * 100.0, mult,
            memory_verdict.summary(),
        );
        Ok(SignalDecision {
            kind: SignalDecisionKind::Allow,
            size_mult: mult,
            edge_threshold_override: edge_override,
            summary,
        })
    }

    /// Read-only snapshot of all multipliers, for cockpit display.
    pub fn snapshot_for_symbols(&self, symbols: &[String], strategies: &[&str]) -> Result<AdaptiveSnapshot> {
        let mut sym_mults: Vec<(String, f64, i64)> = Vec::new();
        for s in symbols {
            let e = expectancy::for_symbol(self.storage, s)?;
            sym_mults.push((s.clone(), e.to_multiplier(), e.samples));
        }
        let mut str_mults: Vec<(String, f64, i64)> = Vec::new();
        for s in strategies {
            let e = expectancy::for_strategy(self.storage, s)?;
            str_mults.push((s.to_string(), e.to_multiplier(), e.samples));
        }
        Ok(AdaptiveSnapshot { sym_mults, str_mults })
    }
}

#[derive(Debug, Clone, Default)]
pub struct AdaptiveSnapshot {
    pub sym_mults: Vec<(String, f64, i64)>,   // (symbol, mult, samples)
    pub str_mults: Vec<(String, f64, i64)>,   // (strategy, mult, samples)
}

/// Persist a closed trade's full feature vector for future learning.
/// Called from `PortfolioManager::manage_open_positions` at close time.
#[allow(clippy::too_many_arguments)]
pub fn record_trade_context(
    storage: &Storage,
    symbol: &str,
    strategy: &str,
    side: &str,
    regime_view: RegimeView,
    atr_pct: f64,
    spread_bps: Option<f64>,
    edge_bps_predicted: f64,
    entry_price: f64,
    exit_price: f64,
    r_init: f64,
    realized_pnl: f64,
    duration_secs: i64,
    exit_reason: &str,
    opened_at: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    let realized_r = if r_init > 0.0 {
        let move_per_unit = (exit_price - entry_price).abs();
        let signed = if (side == "LONG" && exit_price > entry_price)
                       || (side == "SHORT" && exit_price < entry_price) {
            move_per_unit
        } else { -move_per_unit };
        signed / r_init
    } else { 0.0 };
    // v1.5.0 fix: hour_utc / day_of_week MUST reflect entry-time, not
    // close-time. The Memory Bot's k-NN keys on (hour, dow) at signal
    // emission, so a trade opened at 14:00 UTC and closed at 16:00 UTC
    // must be stored as hour=14. Otherwise we'd be matching new 14:00
    // signals against historical 16:00 close-time rows — useless.
    let hour_utc: i64 = opened_at.format("%H").to_string().parse().unwrap_or(0);
    let day_of_week: i64 = opened_at.format("%u").to_string().parse::<i64>()
        .map(|d| d - 1).unwrap_or(0);
    storage.with_conn(|conn| {
        conn.execute(
            "INSERT INTO trade_context (
                symbol, strategy, side,
                regime_1m, regime_5m, regime_15m,
                atr_pct, spread_bps, edge_bps_predicted,
                hour_utc, day_of_week,
                entry_price, exit_price, r_init,
                realized_pnl, realized_r, duration_secs, exit_reason,
                opened_at, closed_at
            ) VALUES (
                ?1, ?2, ?3,
                ?4, ?5, ?6,
                ?7, ?8, ?9,
                ?10, ?11,
                ?12, ?13, ?14,
                ?15, ?16, ?17, ?18,
                ?19, ?20
            )",
            rusqlite::params![
                symbol, strategy, side,
                format!("{:?}", regime_view.r_1m),
                format!("{:?}", regime_view.r_5m),
                format!("{:?}", regime_view.r_15m),
                atr_pct, spread_bps, edge_bps_predicted,
                hour_utc, day_of_week,
                entry_price, exit_price, r_init,
                realized_pnl, realized_r, duration_secs, exit_reason,
                opened_at.to_rfc3339(), chrono::Utc::now().to_rfc3339(),
            ],
        ).map(|_| ()).map_err(|e| anyhow::anyhow!(e))
    })
}
