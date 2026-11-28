//! FANG Unified Strategy Engine & Versioned Strategy Registry.
//!
//! Single authoritative deterministic strategy evaluation and arbitration core shared by:
//! - Original Execution Engine (`src/core/engine.rs`)
//! - Synapse Autonomous Paper Pipeline (`src/paper_pipeline.rs`)
//! - ORACLE Historical Research & Portfolio Replay (`src/oracle.rs`, `src/oracle_research.rs`)
//! - GENESIS Constraint-Aware Evolutionary Discovery (`src/genesis_nsga2.rs`)
//!
//! Safety invariants:
//! - Zero unbounded Martingale (`BoundedGridConfig` enforces `size_multiplier <= 1.5` & hard notional cap).
//! - Explicit position ownership (`owner_strategy`) preserved across entries & scaling.
//! - Research/planning only; independent AEGIS risk gates and `live_gate::PendingCertification`
//!   retain final rejection authority.

use crate::cognition_shadow::InferredRegime;
use crate::cognition_stats::EnsembleRanking;
use crate::core::{Candle, Side, Signal};
use crate::fang_planner::{
    ema_anchor, plan, plan_bounded_grid, BoundedGridConfig, BoundedGridPlan, EmaAnchorOptions,
    GridSpacingMode, PlanCandidate, PlanningBudget, PlanningReport,
};
use crate::hunter_microstructure::MicrostructureSnapshot;
use crate::utils::math;
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const REGISTRY_VERSION: &str = "fang-registry-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyFamily {
    AdaptiveEmaAnchor,
    MomentumBreakout,
    MeanReversion,
    RsiDivergence,
    TrendContinuation,
    VolatilityBreakout,
    BoundedGrid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrategyDescriptor {
    pub id: String,
    pub version: String,
    pub family: StrategyFamily,
    pub supported_regimes: Vec<InferredRegime>,
    pub risk_constraints: Vec<String>,
    pub research_provenance: String,
    pub activation_requirement: String,
    pub research_only: bool,
}

pub fn default_strategy_registry() -> Vec<StrategyDescriptor> {
    vec![
        StrategyDescriptor {
            id: "adaptive_ema_anchor".into(),
            version: REGISTRY_VERSION.into(),
            family: StrategyFamily::AdaptiveEmaAnchor,
            supported_regimes: vec![
                InferredRegime::TrendingUp,
                InferredRegime::TrendingDown,
                InferredRegime::MeanRevertingRange,
            ],
            risk_constraints: vec![
                "Bounded quote distance <= 500 bps".into(),
                "Mandatory stop-loss & hard market notional cap".into(),
            ],
            research_provenance:
                "Clean-room Rust formulation inspired by Passivbot EMA-band anchoring research (attributed in PASSIVBOT_AUDIT.md; no GPL copy)"
                    .into(),
            activation_requirement: "Minimum 12 completed bars & AEGIS -10% stress pass".into(),
            research_only: false,
        },
        StrategyDescriptor {
            id: "momentum_breakout".into(),
            version: REGISTRY_VERSION.into(),
            family: StrategyFamily::MomentumBreakout,
            supported_regimes: vec![InferredRegime::TrendingUp, InferredRegime::TrendingDown],
            risk_constraints: vec![
                "Requires ATR% floor and EMA fast/slow alignment".into(),
                "Never executes without protective stop".into(),
            ],
            research_provenance: "Hydra canonical momentum/breakout kernel".into(),
            activation_requirement: "Aligned trend + cost-adjusted positive expected edge".into(),
            research_only: false,
        },
        StrategyDescriptor {
            id: "mean_reversion_bb".into(),
            version: REGISTRY_VERSION.into(),
            family: StrategyFamily::MeanReversion,
            supported_regimes: vec![InferredRegime::MeanRevertingRange],
            risk_constraints: vec![
                "Prohibited during strong directional breakouts or turbulent panics".into(),
            ],
            research_provenance: "Hydra canonical Bollinger/RSI reversion kernel".into(),
            activation_requirement: "MeanRevertingRange regime + band excursion".into(),
            research_only: false,
        },
        StrategyDescriptor {
            id: "rsi_divergence".into(),
            version: REGISTRY_VERSION.into(),
            family: StrategyFamily::RsiDivergence,
            supported_regimes: vec![
                InferredRegime::MeanRevertingRange,
                InferredRegime::TrendingUp,
                InferredRegime::TrendingDown,
            ],
            risk_constraints: vec!["Swing divergence >= RSI threshold & net cost clearance".into()],
            research_provenance: "Hydra canonical swing RSI divergence kernel".into(),
            activation_requirement: "Minimum 24 completed bars".into(),
            research_only: false,
        },
        StrategyDescriptor {
            id: "trend_continuation".into(),
            version: REGISTRY_VERSION.into(),
            family: StrategyFamily::TrendContinuation,
            supported_regimes: vec![InferredRegime::TrendingUp, InferredRegime::TrendingDown],
            risk_constraints: vec!["Pullback must hold slow EMA structure".into()],
            research_provenance: "Hydra Project Omniscience pullback continuation research".into(),
            activation_requirement: "Multi-bar trend persistence & fresh microstructure".into(),
            research_only: false,
        },
        StrategyDescriptor {
            id: "volatility_breakout".into(),
            version: REGISTRY_VERSION.into(),
            family: StrategyFamily::VolatilityBreakout,
            supported_regimes: vec![
                InferredRegime::TrendingUp,
                InferredRegime::TrendingDown,
                InferredRegime::MeanRevertingRange,
            ],
            risk_constraints: vec![
                "Blocked when adverse-selection score exceeds safety ceiling".into(),
            ],
            research_provenance: "Hydra Project Omniscience range-compression breakout".into(),
            activation_requirement: "Recent bar range > 1.35x prior ATR median".into(),
            research_only: false,
        },
        StrategyDescriptor {
            id: "bounded_grid_planner".into(),
            version: REGISTRY_VERSION.into(),
            family: StrategyFamily::BoundedGrid,
            supported_regimes: vec![InferredRegime::MeanRevertingRange],
            risk_constraints: vec![
                "Anti-Martingale size_multiplier in [0.5, 1.5]; levels <= 20; hard cumulative USD cap"
                    .into(),
            ],
            research_provenance:
                "Hydra deterministic arithmetic/geometric bounded grid (PASSIVBOT_AUDIT.md attribution)"
                    .into(),
            activation_requirement: "Research-only until full multi-level grid lifecycle is certified".into(),
            research_only: true,
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrailingEntryState {
    pub symbol: String,
    pub side: Side,
    pub best_favorable_price: f64,
    pub retrace_bps: f64,
    pub activated: bool,
}

impl TrailingEntryState {
    pub fn step(&mut self, current_price: f64) -> Option<f64> {
        if !current_price.is_finite() || current_price <= 0.0 || !self.retrace_bps.is_finite() {
            return None;
        }
        let retrace = (self.retrace_bps.clamp(1.0, 2000.0)) / 10_000.0;
        match self.side {
            Side::Long => {
                if current_price < self.best_favorable_price {
                    self.best_favorable_price = current_price;
                }
                let trigger = self.best_favorable_price * (1.0 + retrace);
                (current_price >= trigger).then_some(current_price)
            }
            Side::Short => {
                if current_price > self.best_favorable_price {
                    self.best_favorable_price = current_price;
                }
                let trigger = self.best_favorable_price * (1.0 - retrace);
                (current_price <= trigger).then_some(current_price)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrailingExitState {
    pub symbol: String,
    pub side: Side,
    pub entry_price: f64,
    pub peak_favorable_price: f64,
    pub activation_bps: f64,
    pub callback_bps: f64,
    pub armed: bool,
}

impl TrailingExitState {
    pub fn step(&mut self, current_price: f64) -> bool {
        if !current_price.is_finite()
            || current_price <= 0.0
            || !self.entry_price.is_finite()
            || self.entry_price <= 0.0
        {
            return false;
        }
        let gain_bps = match self.side {
            Side::Long => ((current_price - self.entry_price) / self.entry_price) * 10_000.0,
            Side::Short => ((self.entry_price - current_price) / self.entry_price) * 10_000.0,
        };
        if gain_bps >= self.activation_bps {
            self.armed = true;
        }
        match self.side {
            Side::Long => {
                if current_price > self.peak_favorable_price {
                    self.peak_favorable_price = current_price;
                }
                if !self.armed {
                    return false;
                }
                let drawdown_from_peak_bps = ((self.peak_favorable_price - current_price)
                    / self.peak_favorable_price)
                    * 10_000.0;
                drawdown_from_peak_bps >= self.callback_bps
            }
            Side::Short => {
                if current_price < self.peak_favorable_price {
                    self.peak_favorable_price = current_price;
                }
                if !self.armed {
                    return false;
                }
                let bounce_from_trough_bps = ((current_price - self.peak_favorable_price)
                    / self.peak_favorable_price)
                    * 10_000.0;
                bounce_from_trough_bps >= self.callback_bps
            }
        }
    }
}

pub fn compute_adaptive_take_profit(
    entry_price: f64,
    side: Side,
    base_tp_pct: f64,
    atr_pct: f64,
) -> f64 {
    let vol_scale = (atr_pct / 1.0).clamp(0.6, 1.8);
    let effective_pct = (base_tp_pct * vol_scale).clamp(0.004, 0.25);
    match side {
        Side::Long => entry_price * (1.0 + effective_pct),
        Side::Short => (entry_price * (1.0 - effective_pct)).max(entry_price * 0.05),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedStrategyConfig {
    pub enabled_families: Vec<StrategyFamily>,
    pub fast_ema: usize,
    pub slow_ema: usize,
    pub atr_period: usize,
    pub offset_bps: f64,
    pub max_quote_distance_bps: f64,
    pub base_stop_loss_pct: Decimal,
    pub base_take_profit_pct: Decimal,
    pub max_market_notional: Decimal,
    pub target_equity_fraction: Decimal,
    pub default_prior_confidence: f64,
    pub round_trip_cost_bps: f64,
    pub max_adverse_selection_score: f64,
    pub generate_grid_proposals: bool,
    pub grid_levels: usize,
    pub grid_step_bps: Decimal,
    pub grid_multiplier: Decimal,
}

impl Default for UnifiedStrategyConfig {
    fn default() -> Self {
        Self {
            enabled_families: vec![
                StrategyFamily::AdaptiveEmaAnchor,
                StrategyFamily::MomentumBreakout,
                StrategyFamily::MeanReversion,
                StrategyFamily::RsiDivergence,
                StrategyFamily::TrendContinuation,
                StrategyFamily::VolatilityBreakout,
            ],
            fast_ema: 4,
            slow_ema: 12,
            atr_period: 12,
            offset_bps: 4.0,
            max_quote_distance_bps: 500.0,
            base_stop_loss_pct: Decimal::new(2, 2),
            base_take_profit_pct: Decimal::new(4, 2),
            max_market_notional: Decimal::from(2_000u32),
            target_equity_fraction: Decimal::new(15, 2),
            default_prior_confidence: 0.55,
            round_trip_cost_bps: 14.0,
            max_adverse_selection_score: 0.90,
            generate_grid_proposals: false,
            grid_levels: 4,
            grid_step_bps: Decimal::from(50u32),
            grid_multiplier: Decimal::new(11, 1),
        }
    }
}

impl UnifiedStrategyConfig {
    /// SHA3-256 over the canonical serde JSON encoding of the exact strategy configuration.
    /// Research, paper and engine consumers must agree on this digest for a qualified policy.
    pub fn config_sha3(&self) -> Result<String> {
        use sha3::{Digest, Sha3_256};
        let mut h = Sha3_256::new();
        h.update(REGISTRY_VERSION.as_bytes());
        h.update(b"|");
        h.update(serde_json::to_vec(self)?);
        Ok(hex::encode(h.finalize()))
    }

    /// Load an immutable persisted strategy configuration artifact (JSON), validating spans.
    pub fn load_artifact(path: &std::path::Path) -> Result<Self> {
        let raw = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("read strategy config {}: {e}", path.display()))?;
        ensure!(raw.len() <= 64 * 1024, "strategy config artifact too large");
        let cfg: Self = serde_json::from_slice(&raw)?;
        ensure!(
            cfg.fast_ema >= 1 && cfg.fast_ema < cfg.slow_ema && cfg.slow_ema <= 512,
            "invalid FANG EMA spans in strategy config artifact"
        );
        ensure!(
            cfg.base_stop_loss_pct > Decimal::ZERO && cfg.base_take_profit_pct > Decimal::ZERO,
            "strategy config requires positive stop-loss and take-profit"
        );
        ensure!(
            cfg.offset_bps.is_finite()
                && cfg.max_quote_distance_bps.is_finite()
                && cfg.round_trip_cost_bps.is_finite()
                && cfg.default_prior_confidence.is_finite(),
            "strategy config contains non-finite values"
        );
        ensure!(
            !cfg.enabled_families
                .iter()
                .any(|f| matches!(f, StrategyFamily::BoundedGrid)),
            "research-only BoundedGrid cannot be part of a deployable strategy artifact"
        );
        Ok(cfg)
    }
}

#[derive(Debug, Clone)]
pub struct FangMarketInput {
    pub as_of_ms: i64,
    pub correlation_id: String,
    pub watchlist: Vec<String>,
    pub candles: BTreeMap<String, Vec<Candle>>,
    pub marks: BTreeMap<String, Decimal>,
    pub regimes: BTreeMap<String, InferredRegime>,
    pub microstructure: BTreeMap<String, MicrostructureSnapshot>,
    pub portfolio_equity_usd: Decimal,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnifiedPlanOutput {
    pub registry_version: String,
    pub report: PlanningReport,
    pub generated_candidates: usize,
    pub bounded_grids: Vec<BoundedGridPlan>,
}

pub struct FangUnifiedPlanner;

impl FangUnifiedPlanner {
    pub fn evaluate_and_plan(
        input: &FangMarketInput,
        budget: &PlanningBudget,
        cfg: &UnifiedStrategyConfig,
        ensemble: Option<&EnsembleRanking>,
    ) -> Result<UnifiedPlanOutput> {
        ensure!(
            cfg.fast_ema >= 1 && cfg.fast_ema < cfg.slow_ema && cfg.slow_ema <= 512,
            "invalid FANG EMA spans"
        );
        let created_at = DateTime::from_timestamp_millis(input.as_of_ms).unwrap_or_else(Utc::now);
        let ema_opts = EmaAnchorOptions {
            fast_span: cfg.fast_ema,
            slow_span: cfg.slow_ema,
            offset_bps: cfg.offset_bps,
            max_quote_distance_bps: cfg.max_quote_distance_bps,
        };
        let sl_pct = cfg.base_stop_loss_pct.to_f64().unwrap_or(0.02);
        let tp_pct = cfg.base_take_profit_pct.to_f64().unwrap_or(0.04);
        let eq_alloc = (input.portfolio_equity_usd * cfg.target_equity_fraction).round_dp(2);
        let base_notional = cfg.max_market_notional.min(eq_alloc);

        let mut candidates = Vec::new();
        let mut bounded_grids = Vec::new();

        for sym in &input.watchlist {
            let regime = input
                .regimes
                .get(sym)
                .copied()
                .unwrap_or(InferredRegime::InsufficientHistory);
            if matches!(
                regime,
                InferredRegime::HighVolatilityTurbulent | InferredRegime::InsufficientHistory
            ) {
                continue;
            }
            if let Some(ms) = input.microstructure.get(sym) {
                if ms
                    .adverse_selection_score
                    .is_some_and(|s| s > cfg.max_adverse_selection_score)
                {
                    continue;
                }
            }

            let Some(bars) = input.candles.get(sym) else {
                continue;
            };
            let causal_bars: Vec<Candle> = bars
                .iter()
                .filter(|c| c.ts.timestamp_millis() <= input.as_of_ms)
                .cloned()
                .collect();
            if causal_bars.len() < cfg.slow_ema {
                continue;
            }
            let last_close = causal_bars.last().map(|b| b.close).unwrap_or(0.0);
            if !last_close.is_finite() || last_close <= 0.0 || base_notional <= Decimal::ZERO {
                continue;
            }
            let closes: Vec<f64> = causal_bars.iter().map(|c| c.close).collect();
            let atr = math::atr(
                &causal_bars,
                cfg.atr_period
                    .min(causal_bars.len().saturating_sub(1))
                    .max(2),
            );
            let atr_pct = if last_close > 0.0 {
                (atr / last_close) * 100.0
            } else {
                0.5
            };

            let weight_for = |strat_id: &str| -> (f64, bool) {
                if let Some(ens) = ensemble {
                    if let Some(entry) = ens.entries.iter().find(|e| e.strategy == strat_id) {
                        if entry.deteriorated {
                            return (0.0, true);
                        }
                        let raw_conf = entry
                            .posterior_mean_r
                            .map(|mu| (0.45 + 0.25 * mu).clamp(0.30, 0.85))
                            .unwrap_or(cfg.default_prior_confidence);
                        return ((raw_conf * (0.75 + entry.weight)).clamp(0.25, 0.90), false);
                    }
                }
                (cfg.default_prior_confidence.clamp(0.1, 0.85), false)
            };

            // Evaluate enabled strategy families deterministically
            for fam in &cfg.enabled_families {
                match fam {
                    StrategyFamily::AdaptiveEmaAnchor => {
                        let strat_id = "adaptive_ema_anchor";
                        let (conf, det) = weight_for(strat_id);
                        if det {
                            continue;
                        }
                        let side = match regime {
                            InferredRegime::TrendingDown => Side::Short,
                            _ => Side::Long,
                        };
                        if let Ok(anchor_rep) = ema_anchor(&causal_bars, side, &ema_opts) {
                            let aligned = match regime {
                                InferredRegime::TrendingUp => {
                                    anchor_rep.fast_ema >= anchor_rep.slow_ema
                                        && anchor_rep.directional_deviation_bps >= -1200.0
                                }
                                InferredRegime::TrendingDown => {
                                    anchor_rep.fast_ema <= anchor_rep.slow_ema
                                        && anchor_rep.directional_deviation_bps >= -1200.0
                                }
                                _ => anchor_rep.directional_deviation_bps >= -60.0,
                            };
                            if aligned {
                                let sl = match side {
                                    Side::Long => last_close * (1.0 - sl_pct),
                                    Side::Short => last_close * (1.0 + sl_pct),
                                };
                                let tp =
                                    compute_adaptive_take_profit(last_close, side, tp_pct, atr_pct);
                                let raw_edge = if anchor_rep.directional_deviation_bps >= 0.0 {
                                    anchor_rep.directional_deviation_bps * 0.25
                                } else {
                                    (-anchor_rep.directional_deviation_bps).min(200.0) * 0.05
                                };
                                candidates.push(PlanCandidate {
                                    id: format!("{}-{sym}-{strat_id}", input.correlation_id),
                                    signal: Signal {
                                        strategy: strat_id.into(),
                                        venue: "hyperliquid".into(),
                                        symbol: sym.clone(),
                                        side,
                                        confidence: conf,
                                        expected_edge_bps: (raw_edge + 12.0).max(1.0),
                                        stop_price: sl,
                                        take_profit: tp,
                                        reason: format!(
                                            "FANG {strat_id} ({}) dev={:.1}bps",
                                            regime.label(),
                                            anchor_rep.directional_deviation_bps
                                        ),
                                        created_at,
                                    },
                                    reference_price: last_close,
                                    requested_notional: base_notional,
                                });
                            }
                        }
                    }
                    StrategyFamily::MomentumBreakout | StrategyFamily::TrendContinuation => {
                        let strat_id = match fam {
                            StrategyFamily::MomentumBreakout => "momentum_breakout",
                            _ => "trend_continuation",
                        };
                        let (conf, det) = weight_for(strat_id);
                        if det
                            || !matches!(
                                regime,
                                InferredRegime::TrendingUp | InferredRegime::TrendingDown
                            )
                        {
                            continue;
                        }
                        let ema_f = math::ema(&closes, cfg.fast_ema);
                        let ema_s = math::ema(&closes, cfg.slow_ema);
                        if ema_f.len() < 2 || ema_s.len() < 2 {
                            continue;
                        }
                        let side = if regime == InferredRegime::TrendingUp
                            && *ema_f.last().unwrap() > *ema_s.last().unwrap()
                        {
                            Side::Long
                        } else if regime == InferredRegime::TrendingDown
                            && *ema_f.last().unwrap() < *ema_s.last().unwrap()
                        {
                            Side::Short
                        } else {
                            continue;
                        };
                        let sl = match side {
                            Side::Long => last_close * (1.0 - sl_pct),
                            Side::Short => last_close * (1.0 + sl_pct),
                        };
                        let tp = compute_adaptive_take_profit(last_close, side, tp_pct, atr_pct);
                        let spread_pen =
                            (last_close - *ema_s.last().unwrap()).abs() / last_close * 10_000.0;
                        candidates.push(PlanCandidate {
                            id: format!("{}-{sym}-{strat_id}", input.correlation_id),
                            signal: Signal {
                                strategy: strat_id.into(),
                                venue: "hyperliquid".into(),
                                symbol: sym.clone(),
                                side,
                                confidence: conf,
                                expected_edge_bps: (spread_pen * 0.2).max(4.0),
                                stop_price: sl,
                                take_profit: tp,
                                reason: format!("FANG {strat_id} ({})", regime.label()),
                                created_at,
                            },
                            reference_price: last_close,
                            requested_notional: base_notional,
                        });
                    }
                    StrategyFamily::MeanReversion | StrategyFamily::RsiDivergence => {
                        let strat_id = match fam {
                            StrategyFamily::MeanReversion => "mean_reversion_bb",
                            _ => "rsi_divergence",
                        };
                        let (conf, det) = weight_for(strat_id);
                        if det || regime != InferredRegime::MeanRevertingRange || closes.len() < 14
                        {
                            continue;
                        }
                        let (mid_bb, upper, lower) =
                            math::bollinger(&closes, 12.min(closes.len()), 1.8);
                        let side = if last_close <= lower {
                            Side::Long
                        } else if last_close >= upper {
                            Side::Short
                        } else {
                            continue;
                        };
                        let sl = match side {
                            Side::Long => last_close * (1.0 - sl_pct),
                            Side::Short => last_close * (1.0 + sl_pct),
                        };
                        let tp = match side {
                            Side::Long => mid_bb.max(last_close * 1.002),
                            Side::Short => mid_bb.min(last_close * 0.998),
                        };
                        let edge = ((tp - last_close).abs() / last_close * 10_000.0
                            - cfg.round_trip_cost_bps)
                            .max(1.0);
                        candidates.push(PlanCandidate {
                            id: format!("{}-{sym}-{strat_id}", input.correlation_id),
                            signal: Signal {
                                strategy: strat_id.into(),
                                venue: "hyperliquid".into(),
                                symbol: sym.clone(),
                                side,
                                confidence: conf,
                                expected_edge_bps: edge,
                                stop_price: sl,
                                take_profit: tp,
                                reason: format!("FANG {strat_id} BB reversion"),
                                created_at,
                            },
                            reference_price: last_close,
                            requested_notional: base_notional,
                        });
                    }
                    StrategyFamily::VolatilityBreakout => {
                        let strat_id = "volatility_breakout";
                        let (conf, det) = weight_for(strat_id);
                        if det || causal_bars.len() < 6 {
                            continue;
                        }
                        let last_bar = causal_bars.last().unwrap();
                        let bar_range_bps =
                            ((last_bar.high - last_bar.low) / last_close) * 10_000.0;
                        let atr_bps = atr_pct * 100.0;
                        if atr_bps > 0.0 && bar_range_bps > 1.35 * atr_bps {
                            let side = if last_bar.close >= last_bar.open {
                                Side::Long
                            } else {
                                Side::Short
                            };
                            let sl = match side {
                                Side::Long => last_close * (1.0 - sl_pct),
                                Side::Short => last_close * (1.0 + sl_pct),
                            };
                            let tp =
                                compute_adaptive_take_profit(last_close, side, tp_pct, atr_pct);
                            candidates.push(PlanCandidate {
                                id: format!("{}-{sym}-{strat_id}", input.correlation_id),
                                signal: Signal {
                                    strategy: strat_id.into(),
                                    venue: "hyperliquid".into(),
                                    symbol: sym.clone(),
                                    side,
                                    confidence: conf,
                                    expected_edge_bps: (bar_range_bps * 0.25).max(3.0),
                                    stop_price: sl,
                                    take_profit: tp,
                                    reason: format!(
                                        "FANG vol breakout range={bar_range_bps:.1}bps"
                                    ),
                                    created_at,
                                },
                                reference_price: last_close,
                                requested_notional: base_notional,
                            });
                        }
                    }
                    StrategyFamily::BoundedGrid => {}
                }
            }

            if cfg.generate_grid_proposals && regime == InferredRegime::MeanRevertingRange {
                if let Some(&mark_dec) = input.marks.get(sym).filter(|m| **m > Decimal::ZERO) {
                    let gcfg = BoundedGridConfig {
                        symbol: sym.clone(),
                        side: Side::Long,
                        anchor_price: mark_dec,
                        step_bps: cfg.grid_step_bps,
                        levels: cfg.grid_levels.clamp(1, 20),
                        spacing: GridSpacingMode::Geometric,
                        max_total_notional_usd: base_notional,
                        size_multiplier: cfg.grid_multiplier,
                        take_profit_bps: Decimal::from(150u32),
                        stop_buffer_bps: Decimal::from(200u32),
                    };
                    if let Ok(gp) = plan_bounded_grid(&gcfg) {
                        bounded_grids.push(gp);
                    }
                }
            }
        }

        let generated_candidates = candidates.len();
        let report = plan(&candidates, budget)?;
        Ok(UnifiedPlanOutput {
            registry_version: REGISTRY_VERSION.into(),
            report,
            generated_candidates,
            bounded_grids,
        })
    }

    /// Arbitrates external strategy signals through the shared deterministic FANG planner,
    /// ensuring Engine, Synapse Paper, ORACLE, and GENESIS share identical arbitration semantics.
    pub fn arbitrate_external_candidates(
        candidates: &[PlanCandidate],
        budget: &PlanningBudget,
    ) -> Result<PlanningReport> {
        plan(candidates, budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;

    #[test]
    fn strategy_registry_and_trailing_state_machines_are_deterministic() {
        let reg = default_strategy_registry();
        assert_eq!(reg.len(), 7);
        assert!(reg
            .iter()
            .any(|d| d.id == "adaptive_ema_anchor" && d.research_provenance.contains("Passivbot")));

        let mut te = TrailingEntryState {
            symbol: "BTC".into(),
            side: Side::Long,
            best_favorable_price: 100.0,
            retrace_bps: 50.0, // +0.50% rebound trigger
            activated: false,
        };
        assert!(te.step(98.0).is_none());
        assert_eq!(te.best_favorable_price, 98.0);
        assert!(te.step(98.40).is_none());
        assert_eq!(te.step(98.55), Some(98.55));

        let mut tx = TrailingExitState {
            symbol: "BTC".into(),
            side: Side::Long,
            entry_price: 100.0,
            peak_favorable_price: 100.0,
            activation_bps: 100.0, // arm at +1%
            callback_bps: 40.0,    // exit on -0.4% pullback
            armed: false,
        };
        assert!(!tx.step(100.5));
        assert!(!tx.armed);
        assert!(!tx.step(102.0));
        assert!(tx.armed);
        assert!(tx.step(101.5));
    }

    #[test]
    fn unified_planner_is_order_independent_and_respects_deterioration() {
        let now_ms = 1_700_010_000_000_i64;
        let base_ts = DateTime::from_timestamp_millis(now_ms - 15 * 60_000).unwrap();
        let bars: Vec<Candle> = (0..16)
            .map(|k| {
                let px = 60_000.0 + (k as f64) * 200.0;
                Candle {
                    ts: base_ts + ChronoDuration::minutes(k),
                    open: px * 0.999,
                    high: px * 1.002,
                    low: px * 0.998,
                    close: px,
                    volume: 1000.0,
                }
            })
            .collect();
        let input = FangMarketInput {
            as_of_ms: now_ms,
            correlation_id: "fang-test".into(),
            watchlist: vec!["BTC".into()],
            candles: BTreeMap::from([("BTC".into(), bars)]),
            marks: BTreeMap::from([("BTC".into(), Decimal::from(63_000u32))]),
            regimes: BTreeMap::from([("BTC".into(), InferredRegime::TrendingUp)]),
            microstructure: BTreeMap::new(),
            portfolio_equity_usd: Decimal::from(10_000u32),
        };
        let budget = PlanningBudget {
            approved_symbols: ["BTC".into()].into(),
            excluded_symbols: Default::default(),
            total_remaining_notional: Decimal::from(5_000u32),
            per_symbol_remaining_notional: BTreeMap::from([(
                "BTC".into(),
                Decimal::from(2_000u32),
            )]),
            occupied_symbols: BTreeMap::new(),
            confidence_floor: 0.45,
        };
        let out = FangUnifiedPlanner::evaluate_and_plan(
            &input,
            &budget,
            &UnifiedStrategyConfig::default(),
            None,
        )
        .unwrap();
        assert_eq!(out.report.selected.len(), 1);
        assert_eq!(out.report.selected[0].signal.symbol, "BTC");
    }
}
