//! Interconnected multi-head autonomous **PAPER / SIMULATION** pipeline
//! (**HUNTER -> COGNITION -> FANG -> AEGIS -> NEXUS**).
//!
//! Safety & truthfulness:
//! - Permanently `SIMULATION_ONLY`: zero access to signing keys, OS vaults, or live
//!   exchange submission endpoints.
//! - Uses exact `rust_decimal::Decimal` accounting for entry/exit notionals, fees,
//!   drawdown high-water marks, and realized net PnL.
//! - Protective exits (stop-loss / take-profit) remain permitted even when the AEGIS
//!   drawdown circuit breaker is latched against new exposure.

use crate::cognition_shadow::{
    evaluate as evaluate_shadow, infer_regime, InferredRegime, OutcomeSource,
    RegimeInferenceConfig, ShadowPolicy, ShadowRequest, ShadowSample,
};
use crate::core::{Candle, Side, Signal};
use crate::fang_planner::{
    ema_anchor, plan as plan_candidates, EmaAnchorOptions, PlanCandidate, PlanningBudget,
};
use crate::hunter::{evaluate as evaluate_hunter, HunterPolicy, HunterRequest, MarketObservation};
use crate::hunter_stream::ObservedMarketSnapshot;
use crate::portfolio_research::{evaluate_margin_stress, MarginPositionInput};
use crate::synapse_bus::{DataProvenance, HeadId, SynapseBus, SynapsePayload};
use anyhow::{ensure, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SIMULATION_ONLY: bool = true;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperPosition {
    pub symbol: String,
    pub owner_strategy: String,
    pub side: Side,
    pub entry_price: Decimal,
    pub qty: Decimal,
    pub notional: Decimal,
    pub stop_price: Decimal,
    pub take_profit_price: Decimal,
    pub entry_fee_usd: Decimal,
    pub opened_at_ms: i64,
    pub correlation_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperClosedTrade {
    pub symbol: String,
    pub owner_strategy: String,
    pub side: Side,
    pub entry_price: Decimal,
    pub exit_price: Decimal,
    pub qty: Decimal,
    pub gross_pnl_usd: Decimal,
    pub fees_usd: Decimal,
    pub realized_net_pnl_usd: Decimal,
    pub exit_reason: String,
    pub closed_at_ms: i64,
    pub normalized_r: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperLedger {
    pub simulation_only: bool,
    pub starting_equity: Decimal,
    pub cash_equity: Decimal,
    pub open_positions: BTreeMap<String, PaperPosition>,
    pub closed_trades: Vec<PaperClosedTrade>,
    pub total_fees_usd: Decimal,
    #[serde(default)]
    pub total_funding_usd: Decimal,
    pub realized_net_pnl_usd: Decimal,
    pub high_water_equity: Decimal,
    pub max_drawdown_ratio: Decimal,
    pub breaker_reason: Option<String>,
}

impl PaperLedger {
    pub fn new(starting_equity: Decimal) -> Result<Self> {
        ensure!(
            starting_equity > Decimal::ZERO,
            "paper starting_equity must be positive"
        );
        Ok(Self {
            simulation_only: SIMULATION_ONLY,
            starting_equity,
            cash_equity: starting_equity,
            open_positions: BTreeMap::new(),
            closed_trades: Vec::new(),
            total_fees_usd: Decimal::ZERO,
            total_funding_usd: Decimal::ZERO,
            realized_net_pnl_usd: Decimal::ZERO,
            high_water_equity: starting_equity,
            max_drawdown_ratio: Decimal::ZERO,
            breaker_reason: None,
        })
    }

    pub fn allocated_exposure(&self) -> Decimal {
        self.open_positions
            .values()
            .fold(Decimal::ZERO, |acc, p| acc + p.notional)
    }

    /// Computes exact unrealized PnL, returning an error if any open position lacks a valid positive mark.
    pub fn checked_unrealized_pnl(&self, marks: &BTreeMap<String, Decimal>) -> Result<Decimal> {
        let mut total = Decimal::ZERO;
        for (sym, pos) in &self.open_positions {
            let mark = marks
                .get(sym)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("missing mark price for open position {sym}"))?;
            ensure!(
                mark > Decimal::ZERO,
                "non-positive mark price for open position {sym}"
            );
            let diff = match pos.side {
                Side::Long => mark - pos.entry_price,
                Side::Short => pos.entry_price - mark,
            };
            total += diff * pos.qty;
        }
        Ok(total)
    }

    /// Computes exact mark-to-market equity; fails closed when any open position mark is missing/invalid.
    pub fn checked_mark_to_market_equity(
        &self,
        marks: &BTreeMap<String, Decimal>,
    ) -> Result<Decimal> {
        Ok(self.cash_equity + self.checked_unrealized_pnl(marks)?)
    }

    pub fn unrealized_pnl(&self, marks: &BTreeMap<String, Decimal>) -> Decimal {
        let mut total = Decimal::ZERO;
        for (sym, pos) in &self.open_positions {
            let Some(&mark) = marks.get(sym).filter(|m| **m > Decimal::ZERO) else {
                continue;
            };
            let diff = match pos.side {
                Side::Long => mark - pos.entry_price,
                Side::Short => pos.entry_price - mark,
            };
            total += diff * pos.qty;
        }
        total
    }

    pub fn mark_to_market_equity(&self, marks: &BTreeMap<String, Decimal>) -> Decimal {
        self.cash_equity + self.unrealized_pnl(marks)
    }

    /// Applies an hourly simulated funding settlement (`funding_bps_1h`) to an open position
    /// proportional to `elapsed_hours`. Positive funding costs longs / credits shorts.
    pub fn apply_simulated_funding(
        &mut self,
        symbol: &str,
        funding_bps_1h: f64,
        elapsed_hours: f64,
        mark_px: Decimal,
    ) -> Option<Decimal> {
        if !funding_bps_1h.is_finite() || !elapsed_hours.is_finite() || elapsed_hours <= 0.0 {
            return None;
        }
        let pos = self.open_positions.get(symbol)?;
        if mark_px <= Decimal::ZERO {
            return None;
        }
        let rate_dec = Decimal::try_from((funding_bps_1h / 10_000.0) * elapsed_hours)
            .ok()?
            .round_dp(8);
        let base_ntl = (pos.qty * mark_px).round_dp(4);
        let cash_delta = match pos.side {
            Side::Long => -(base_ntl * rate_dec).round_dp(6),
            Side::Short => (base_ntl * rate_dec).round_dp(6),
        };
        self.cash_equity += cash_delta;
        self.total_funding_usd += cash_delta;
        self.realized_net_pnl_usd += cash_delta;
        Some(cash_delta)
    }

    pub fn current_drawdown_ratio(&self, marks: &BTreeMap<String, Decimal>) -> Decimal {
        let eq = self.mark_to_market_equity(marks);
        if self.high_water_equity <= Decimal::ZERO || eq >= self.high_water_equity {
            Decimal::ZERO
        } else {
            ((self.high_water_equity - eq) / self.high_water_equity)
                .round_dp(6)
                .max(Decimal::ZERO)
        }
    }

    fn update_drawdown(&mut self, marks: &BTreeMap<String, Decimal>, max_drawdown_ratio: Decimal) {
        let eq = self.mark_to_market_equity(marks);
        if eq > self.high_water_equity {
            self.high_water_equity = eq;
        }
        let dd = self.current_drawdown_ratio(marks);
        if dd > self.max_drawdown_ratio {
            self.max_drawdown_ratio = dd;
        }
        if dd >= max_drawdown_ratio && self.breaker_reason.is_none() {
            self.breaker_reason = Some(format!(
                "Simulated drawdown {dd} reached AEGIS ceiling {max_drawdown_ratio}"
            ));
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynapsePipelineConfig {
    pub starting_equity: Decimal,
    pub max_total_exposure_ratio: Decimal,
    pub max_market_notional: Decimal,
    pub max_drawdown_ratio: Decimal,
    pub fee_bps: Decimal,
    pub slippage_bps: Decimal,
    pub stop_loss_pct: Decimal,
    pub take_profit_pct: Decimal,
    pub stale_market_max_age_ms: i64,
    pub confidence_floor: f64,
}

impl Default for SynapsePipelineConfig {
    fn default() -> Self {
        Self {
            starting_equity: Decimal::from(10_000u32),
            max_total_exposure_ratio: Decimal::new(50, 2),
            max_market_notional: Decimal::from(2_000u32),
            max_drawdown_ratio: Decimal::new(15, 2),
            fee_bps: Decimal::from(4u32),
            slippage_bps: Decimal::from(3u32),
            stop_loss_pct: Decimal::new(2, 2),
            take_profit_pct: Decimal::new(4, 2),
            stale_market_max_age_ms: 60_000,
            confidence_floor: 0.45,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleSummary {
    pub cycle: u64,
    pub correlation_id: String,
    pub watchlist: Vec<String>,
    pub selected_candidates: usize,
    pub approved_entries: usize,
    pub closed_trades: usize,
    pub events_emitted: usize,
    pub mtm_equity_usd: Decimal,
    pub breaker_latched: bool,
}

#[derive(Debug, Clone)]
pub struct SynapsePipeline {
    pub config: SynapsePipelineConfig,
    pub ledger: PaperLedger,
    pub bus: SynapseBus,
    pub watchlist: Vec<String>,
    pub simulated_samples: Vec<ShadowSample>,
    pub cycle_count: u64,
    demo_candles: BTreeMap<String, Vec<Candle>>,
}

impl SynapsePipeline {
    pub fn new(config: SynapsePipelineConfig) -> Result<Self> {
        let ledger = PaperLedger::new(config.starting_equity)?;
        Ok(Self {
            config,
            ledger,
            bus: SynapseBus::default(),
            watchlist: Vec::new(),
            simulated_samples: Vec::new(),
            cycle_count: 0,
            demo_candles: BTreeMap::new(),
        })
    }

    pub fn step_cycle(
        &mut self,
        now_ms: i64,
        markets: &[MarketObservation],
        candles: &BTreeMap<String, Vec<Candle>>,
        marks: &BTreeMap<String, Decimal>,
    ) -> Result<CycleSummary> {
        self.step_cycle_with_provenance(
            now_ms,
            DataProvenance::AuthoritativeExchange,
            markets,
            candles,
            marks,
        )
    }

    pub fn step_cycle_with_provenance(
        &mut self,
        now_ms: i64,
        market_provenance: DataProvenance,
        markets: &[MarketObservation],
        candles: &BTreeMap<String, Vec<Candle>>,
        marks: &BTreeMap<String, Decimal>,
    ) -> Result<CycleSummary> {
        self.cycle_count = self.cycle_count.saturating_add(1);
        let correlation_id = format!("synapse-{:06}", self.cycle_count);
        let before_events = self.bus.total_published();

        // 1. Settle protective exits first (permits exits even if AEGIS entry breaker is latched).
        let closed_this_cycle = self.evaluate_exits(now_ms, &correlation_id, marks)?;
        let mut missing_marks_halt = false;
        let _mtm_eq = match self.ledger.checked_mark_to_market_equity(marks) {
            Ok(eq) => {
                self.ledger
                    .update_drawdown(marks, self.config.max_drawdown_ratio);
                eq
            }
            Err(err) => {
                missing_marks_halt = true;
                self.bus.publish(
                    &correlation_id,
                    now_ms,
                    HeadId::Aegis,
                    DataProvenance::OperationalFault,
                    SynapsePayload::PipelineFault {
                        stage: "aegis_mark_validation".into(),
                        message: format!(
                            "New entries halted; open position mark unavailable: {err}"
                        ),
                    },
                );
                self.ledger.cash_equity
            }
        };

        // 2. HUNTER — market surveillance, stale-data rejection, and watchlist rotation.
        let mut stale_rejected = 0usize;
        for m in markets {
            let age = now_ms.saturating_sub(m.observed_at_ms);
            if age < 0 || age > self.config.stale_market_max_age_ms {
                stale_rejected += 1;
            }
        }
        let approved_set: BTreeSet<String> = markets.iter().map(|m| m.symbol.clone()).collect();
        let hunter_req = HunterRequest {
            as_of_ms: now_ms,
            source: match market_provenance {
                DataProvenance::SyntheticTestFixture => "synapse_synthetic_fixture".into(),
                DataProvenance::HistoricalValidation => "synapse_historical_replay".into(),
                _ => "synapse_observed_market_feed".into(),
            },
            approved: approved_set.clone(),
            excluded: BTreeSet::new(),
            current_watchlist: self.watchlist.clone(),
            policy: HunterPolicy {
                max_age_ms: self.config.stale_market_max_age_ms,
                min_volume: 100_000.0,
                min_depth: 10_000.0,
                max_spread: 30.0,
                max_abs_funding: 25.0,
                min_coverage: 0.80,
                min_age: 1.0,
                slots: 5,
                switch_margin: 3.0,
                min_open_interest: 50_000.0,
            },
            markets: markets.to_vec(),
        };
        let hunter_rep = evaluate_hunter(&hunter_req).context("HUNTER evaluation failed")?;
        self.watchlist = hunter_rep.watchlist.clone();
        let eligible_count = hunter_rep.ranked.iter().filter(|r| r.eligible).count();
        let top_assessment = hunter_rep.ranked.iter().find(|r| r.eligible);
        self.bus.publish(
            &correlation_id,
            now_ms,
            HeadId::Hunter,
            market_provenance,
            SynapsePayload::HunterObservation {
                watchlist: self.watchlist.clone(),
                eligible_count,
                top_symbol: top_assessment.map(|a| a.observation.symbol.clone()),
                top_score: top_assessment.and_then(|a| a.score),
                stale_rejected,
                stream_healthy: eligible_count > 0 && !missing_marks_halt,
            },
        );

        // 3. COGNITION — zero-lookahead causal regime classification + simulated outcome stats.
        let mut regimes: BTreeMap<String, InferredRegime> = BTreeMap::new();
        let shadow_rep = if !self.simulated_samples.is_empty() {
            evaluate_shadow(&ShadowRequest {
                as_of_ms: now_ms,
                policy: ShadowPolicy {
                    min_samples: 3,
                    min_effective_samples: 2.0,
                    ..Default::default()
                },
                samples: self.simulated_samples.clone(),
            })
            .ok()
        } else {
            None
        };
        let conf_lb = shadow_rep.as_ref().and_then(|r| {
            r.groups
                .first()
                .and_then(|a| a.estimate.approximate_lower_bound_r)
        });
        let reg_cfg = RegimeInferenceConfig {
            lookback_bars: 12,
            ..Default::default()
        };
        for sym in &self.watchlist {
            let inferred = if let Some(bars) = candles.get(sym) {
                match infer_regime(bars, &reg_cfg) {
                    Ok(a) => a.regime,
                    Err(e) => {
                        self.bus.publish(
                            &correlation_id,
                            now_ms,
                            HeadId::Cognition,
                            DataProvenance::OperationalFault,
                            SynapsePayload::PipelineFault {
                                stage: "cognition_infer_regime".into(),
                                message: format!("{sym}: {e}"),
                            },
                        );
                        InferredRegime::InsufficientHistory
                    }
                }
            } else {
                InferredRegime::InsufficientHistory
            };
            regimes.insert(sym.clone(), inferred);
            self.bus.publish(
                &correlation_id,
                now_ms,
                HeadId::Cognition,
                DataProvenance::CausalInference,
                SynapsePayload::CognitionAssessment {
                    symbol: sym.clone(),
                    regime: inferred.label().into(),
                    confidence_lb: conf_lb,
                    sample_count: self.simulated_samples.len(),
                    simulated_only: true,
                },
            );
        }

        // 4. FANG — shared deterministic strategy planner honoring budgets & ownership.
        let mtm_eq = self.ledger.mark_to_market_equity(marks);
        let allocated = self.ledger.allocated_exposure();
        let total_cap = (mtm_eq * self.config.max_total_exposure_ratio).round_dp(4);
        let total_remaining = (total_cap - allocated).max(Decimal::ZERO);

        let mut per_symbol_remaining = BTreeMap::new();
        let mut occupied_symbols = BTreeMap::new();
        for (sym, pos) in &self.ledger.open_positions {
            occupied_symbols.insert(sym.clone(), pos.owner_strategy.clone());
        }
        let mut candidates = Vec::new();
        let ema_opts = EmaAnchorOptions {
            fast_span: 4,
            slow_span: 12,
            offset_bps: 4.0,
            max_quote_distance_bps: 500.0,
        };
        let created_at = DateTime::from_timestamp_millis(now_ms).unwrap_or_else(Utc::now);

        for sym in &self.watchlist {
            let held_notional = self
                .ledger
                .open_positions
                .get(sym)
                .map(|p| p.notional)
                .unwrap_or(Decimal::ZERO);
            per_symbol_remaining.insert(
                sym.clone(),
                (self.config.max_market_notional - held_notional).max(Decimal::ZERO),
            );
            let regime = regimes
                .get(sym)
                .copied()
                .unwrap_or(InferredRegime::InsufficientHistory);
            let Some(bars) = candles.get(sym) else {
                continue;
            };
            if bars.len() < ema_opts.slow_span {
                continue;
            }
            if matches!(
                regime,
                InferredRegime::HighVolatilityTurbulent | InferredRegime::InsufficientHistory
            ) {
                continue;
            }
            let side = match regime {
                InferredRegime::TrendingDown => Side::Short,
                _ => Side::Long,
            };
            if let Ok(anchor_rep) = ema_anchor(bars, side, &ema_opts) {
                let last_close = bars.last().map(|b| b.close).unwrap_or(0.0);
                if last_close <= 0.0 || !last_close.is_finite() {
                    continue;
                }
                let confidence = match regime {
                    InferredRegime::TrendingUp | InferredRegime::TrendingDown => 0.72,
                    InferredRegime::MeanRevertingRange => 0.58,
                    _ => 0.40,
                };
                let aligned_trend = match regime {
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
                if aligned_trend && confidence >= self.config.confidence_floor {
                    let sl = match side {
                        Side::Long => {
                            last_close * (1.0 - f64_from_decimal(self.config.stop_loss_pct))
                        }
                        Side::Short => {
                            last_close * (1.0 + f64_from_decimal(self.config.stop_loss_pct))
                        }
                    };
                    let tp = match side {
                        Side::Long => {
                            last_close * (1.0 + f64_from_decimal(self.config.take_profit_pct))
                        }
                        Side::Short => {
                            last_close * (1.0 - f64_from_decimal(self.config.take_profit_pct))
                        }
                    };
                    let desired_notional = self
                        .config
                        .max_market_notional
                        .min((mtm_eq * Decimal::new(15, 2)).round_dp(2));
                    let edge_bonus = if anchor_rep.directional_deviation_bps >= 0.0 {
                        anchor_rep.directional_deviation_bps * 0.25
                    } else {
                        (-anchor_rep.directional_deviation_bps).min(200.0) * 0.05
                    };
                    if desired_notional > Decimal::ZERO {
                        candidates.push(PlanCandidate {
                            id: format!("{correlation_id}-{sym}"),
                            signal: Signal {
                                strategy: format!("synapse_ema_{}", regime.label().to_lowercase()),
                                venue: "hyperliquid".into(),
                                symbol: sym.clone(),
                                side,
                                confidence,
                                expected_edge_bps: 18.0 + edge_bonus,
                                stop_price: sl,
                                take_profit: tp,
                                reason: format!("Causal {} EMA anchor", regime.label()),
                                created_at,
                            },
                            reference_price: last_close,
                            requested_notional: desired_notional,
                        });
                    }
                }
            }
        }

        let budget = PlanningBudget {
            approved_symbols: approved_set,
            excluded_symbols: BTreeSet::new(),
            total_remaining_notional: total_remaining,
            per_symbol_remaining_notional: per_symbol_remaining,
            occupied_symbols,
            confidence_floor: self.config.confidence_floor,
        };
        let plan_rep = plan_candidates(&candidates, &budget).context("FANG planner failed")?;
        let rejection_reasons: Vec<String> = plan_rep
            .rejected
            .iter()
            .map(|r| format!("{}:{:?}", r.id, r.reason))
            .collect();
        self.bus.publish(
            &correlation_id,
            now_ms,
            HeadId::Fang,
            DataProvenance::DeterministicPlan,
            SynapsePayload::FangPlanEvaluated {
                selected_count: plan_rep.selected.len(),
                rejected_count: plan_rep.rejected.len(),
                top_candidate_id: plan_rep.selected.first().map(|c| c.id.clone()),
                top_symbol: plan_rep.selected.first().map(|c| c.signal.symbol.clone()),
                rejection_reasons,
            },
        );

        // 5. AEGIS & 6. NEXUS Paper Execution
        let mut approved_entries = 0usize;
        for cand in &plan_rep.selected {
            let sym = &cand.signal.symbol;
            let cur_eq = self
                .ledger
                .checked_mark_to_market_equity(marks)
                .unwrap_or(Decimal::ZERO);
            let cur_alloc = self.ledger.allocated_exposure();
            let slip_frac = self.config.slippage_bps / Decimal::from(10_000u32);
            let fee_frac = self.config.fee_bps / Decimal::from(10_000u32);
            let post_fill_metrics = marks
                .get(sym)
                .copied()
                .filter(|p| *p > Decimal::ZERO)
                .or_else(|| decimal_positive(cand.reference_price))
                .and_then(|ref_px| {
                    let fill_px = match cand.signal.side {
                        Side::Long => (ref_px * (Decimal::ONE + slip_frac)).round_dp(6),
                        Side::Short => (ref_px * (Decimal::ONE - slip_frac)).round_dp(6),
                    };
                    if fill_px <= Decimal::ZERO {
                        return None;
                    }
                    let qty = (cand.requested_notional / fill_px).round_dp(6);
                    if qty <= Decimal::ZERO {
                        return None;
                    }
                    let actual_notional = (fill_px * qty).round_dp(4);
                    let fee_usd = (actual_notional * fee_frac).round_dp(4);
                    Some((fill_px, qty, actual_notional, fee_usd))
                });

            let projected_alloc = cur_alloc
                + post_fill_metrics
                    .map(|(_, _, ntl, fee)| ntl + fee)
                    .unwrap_or(cand.requested_notional);
            let projected_ratio = if cur_eq > Decimal::ZERO {
                (projected_alloc / cur_eq).round_dp(4)
            } else {
                Decimal::from(99u32)
            };

            let stale_ok = markets.iter().find(|m| &m.symbol == sym).is_some_and(|m| {
                let age = now_ms.saturating_sub(m.observed_at_ms);
                age >= 0 && age <= self.config.stale_market_max_age_ms
            });

            let mut stress_ok = true;
            if let (Some((fill_px, qty, _, _)), true) = (post_fill_metrics, !missing_marks_halt) {
                if cur_eq > Decimal::ZERO {
                    let mut stress_inputs = Vec::new();
                    for p in self.ledger.open_positions.values() {
                        if let Some(&mp) = marks.get(&p.symbol).filter(|m| **m > Decimal::ZERO) {
                            stress_inputs.push(MarginPositionInput {
                                symbol: p.symbol.clone(),
                                is_long: p.side == Side::Long,
                                quantity: p.qty,
                                entry_price: p.entry_price,
                                mark_price: mp,
                                maintenance_margin_rate: Decimal::new(5, 2),
                            });
                        } else {
                            stress_ok = false;
                            break;
                        }
                    }
                    if stress_ok {
                        stress_inputs.push(MarginPositionInput {
                            symbol: sym.clone(),
                            is_long: cand.signal.side == Side::Long,
                            quantity: qty,
                            entry_price: fill_px,
                            mark_price: fill_px,
                            maintenance_margin_rate: Decimal::new(5, 2),
                        });
                        if let Ok(rep) =
                            evaluate_margin_stress(cur_eq, &stress_inputs, &[Decimal::from(10u32)])
                        {
                            if rep.scenarios.iter().any(|s| s.maintenance_breached) {
                                stress_ok = false;
                            }
                        } else {
                            stress_ok = false;
                        }
                    }
                } else {
                    stress_ok = false;
                }
            } else {
                stress_ok = false;
            }

            let (approved, reason) = if missing_marks_halt {
                (
                    false,
                    "Held open position lacks valid mark price; new entries blocked".into(),
                )
            } else if let Some(ref b) = self.ledger.breaker_reason {
                (false, format!("Breaker latched: {b}"))
            } else if !stale_ok {
                (false, "Market observation stale or missing".into())
            } else if post_fill_metrics.is_none() {
                (
                    false,
                    "Post-fill quantity rounded to zero or mark invalid".into(),
                )
            } else if self.ledger.open_positions.contains_key(sym) {
                (false, "Position already held for symbol".into())
            } else if projected_ratio > self.config.max_total_exposure_ratio {
                (false, "Post-fill exposure ceiling exceeded".into())
            } else if post_fill_metrics
                .is_some_and(|(_, _, actual_ntl, _)| actual_ntl > self.config.max_market_notional)
            {
                (false, "Post-fill per-market notional cap exceeded".into())
            } else if !stress_ok {
                (false, "AEGIS -10% adverse shock maintenance breach".into())
            } else {
                (
                    true,
                    "Within AEGIS paper policy & -10% stress floor (SimulationAssumption)".into(),
                )
            };

            self.bus.publish(
                &correlation_id,
                now_ms,
                HeadId::Aegis,
                DataProvenance::IndependentRisk,
                SynapsePayload::AegisDecision {
                    candidate_id: cand.id.clone(),
                    symbol: sym.clone(),
                    approved,
                    reason,
                    exposure_ratio: projected_ratio.to_string(),
                    breaker_latched: self.ledger.breaker_reason.is_some() || missing_marks_halt,
                },
            );

            if approved {
                let (fill_px, qty, notional, fee_usd) = post_fill_metrics
                    .ok_or_else(|| anyhow::anyhow!("approved post-fill metrics missing"))?;
                let stop_px = match cand.signal.side {
                    Side::Long => {
                        (fill_px * (Decimal::ONE - self.config.stop_loss_pct)).round_dp(6)
                    }
                    Side::Short => {
                        (fill_px * (Decimal::ONE + self.config.stop_loss_pct)).round_dp(6)
                    }
                };
                let tp_px = match cand.signal.side {
                    Side::Long => {
                        (fill_px * (Decimal::ONE + self.config.take_profit_pct)).round_dp(6)
                    }
                    Side::Short => {
                        (fill_px * (Decimal::ONE - self.config.take_profit_pct)).round_dp(6)
                    }
                };
                if stop_px <= Decimal::ZERO || tp_px <= Decimal::ZERO {
                    continue;
                }
                self.ledger.cash_equity -= fee_usd;
                self.ledger.total_fees_usd += fee_usd;
                self.ledger.realized_net_pnl_usd -= fee_usd;
                self.ledger.open_positions.insert(
                    sym.clone(),
                    PaperPosition {
                        symbol: sym.clone(),
                        owner_strategy: cand.signal.strategy.clone(),
                        side: cand.signal.side,
                        entry_price: fill_px,
                        qty,
                        notional,
                        stop_price: stop_px,
                        take_profit_price: tp_px,
                        entry_fee_usd: fee_usd,
                        opened_at_ms: now_ms,
                        correlation_id: correlation_id.clone(),
                    },
                );
                approved_entries += 1;
                self.bus.publish(
                    &correlation_id,
                    now_ms,
                    HeadId::Nexus,
                    DataProvenance::SimulatedPaperExecution,
                    SynapsePayload::NexusPaperExecution {
                        intent_id: cand.id.clone(),
                        symbol: sym.clone(),
                        action: format!("OPEN_{}", cand.signal.side.as_str()),
                        price: fill_px.to_string(),
                        qty: qty.to_string(),
                        notional: notional.to_string(),
                        fee_usd: fee_usd.to_string(),
                        realized_pnl_usd: None,
                        open_positions: self.ledger.open_positions.len(),
                    },
                );
            }
        }

        self.ledger
            .update_drawdown(marks, self.config.max_drawdown_ratio);

        Ok(CycleSummary {
            cycle: self.cycle_count,
            correlation_id,
            watchlist: self.watchlist.clone(),
            selected_candidates: plan_rep.selected.len(),
            approved_entries,
            closed_trades: closed_this_cycle,
            events_emitted: self.bus.total_published().saturating_sub(before_events) as usize,
            mtm_equity_usd: self.ledger.mark_to_market_equity(marks).round_dp(2),
            breaker_latched: self.ledger.breaker_reason.is_some(),
        })
    }

    fn evaluate_exits(
        &mut self,
        now_ms: i64,
        correlation_id: &str,
        marks: &BTreeMap<String, Decimal>,
    ) -> Result<usize> {
        let mut to_close: Vec<(String, Decimal, &'static str)> = Vec::new();
        for (sym, pos) in &self.ledger.open_positions {
            let Some(&mark) = marks.get(sym) else {
                continue;
            };
            let hit = match pos.side {
                Side::Long => {
                    if mark <= pos.stop_price {
                        Some("PROTECTIVE_STOP_LOSS")
                    } else if mark >= pos.take_profit_price {
                        Some("TAKE_PROFIT_TARGET")
                    } else {
                        None
                    }
                }
                Side::Short => {
                    if mark >= pos.stop_price {
                        Some("PROTECTIVE_STOP_LOSS")
                    } else if mark <= pos.take_profit_price {
                        Some("TAKE_PROFIT_TARGET")
                    } else {
                        None
                    }
                }
            };
            if let Some(reason) = hit {
                to_close.push((sym.clone(), mark, reason));
            }
        }

        let slip_frac = self.config.slippage_bps / Decimal::from(10_000u32);
        let fee_frac = self.config.fee_bps / Decimal::from(10_000u32);
        let closed_count = to_close.len();

        for (sym, raw_mark, reason) in to_close {
            if let Some(pos) = self.ledger.open_positions.remove(&sym) {
                let exit_px = match pos.side {
                    Side::Long => (raw_mark * (Decimal::ONE - slip_frac)).round_dp(6),
                    Side::Short => (raw_mark * (Decimal::ONE + slip_frac)).round_dp(6),
                };
                let gross = match pos.side {
                    Side::Long => ((exit_px - pos.entry_price) * pos.qty).round_dp(4),
                    Side::Short => ((pos.entry_price - exit_px) * pos.qty).round_dp(4),
                };
                let exit_notional = (exit_px * pos.qty).round_dp(4);
                let exit_fee = (exit_notional * fee_frac).round_dp(4);
                let net_close = gross - exit_fee;
                let total_trade_fees = pos.entry_fee_usd + exit_fee;
                let net_round_trip = gross - total_trade_fees;

                self.ledger.cash_equity += net_close;
                self.ledger.total_fees_usd += exit_fee;
                self.ledger.realized_net_pnl_usd += net_close;

                let risk_usd = (pos.notional * self.config.stop_loss_pct).max(Decimal::ONE);
                let r_mult = f64_from_decimal((net_round_trip / risk_usd).round_dp(4));

                self.ledger.closed_trades.push(PaperClosedTrade {
                    symbol: sym.clone(),
                    owner_strategy: pos.owner_strategy.clone(),
                    side: pos.side,
                    entry_price: pos.entry_price,
                    exit_price: exit_px,
                    qty: pos.qty,
                    gross_pnl_usd: gross,
                    fees_usd: total_trade_fees,
                    realized_net_pnl_usd: net_round_trip,
                    exit_reason: reason.into(),
                    closed_at_ms: now_ms,
                    normalized_r: r_mult,
                });
                self.simulated_samples.push(ShadowSample {
                    id: format!("sim-{}", self.ledger.closed_trades.len()),
                    strategy: pos.owner_strategy,
                    symbol: sym.clone(),
                    regime: "simulated".into(),
                    decision_at_ms: pos.opened_at_ms,
                    outcome_at_ms: now_ms.max(pos.opened_at_ms + 1),
                    normalized_r: r_mult,
                    source: OutcomeSource::Simulated,
                });
                self.bus.publish(
                    correlation_id,
                    now_ms,
                    HeadId::Nexus,
                    DataProvenance::SimulatedPaperExecution,
                    SynapsePayload::NexusPaperExecution {
                        intent_id: pos.correlation_id,
                        symbol: sym,
                        action: reason.into(),
                        price: exit_px.to_string(),
                        qty: pos.qty.to_string(),
                        notional: exit_notional.to_string(),
                        fee_usd: exit_fee.to_string(),
                        realized_pnl_usd: Some(net_round_trip.to_string()),
                        open_positions: self.ledger.open_positions.len(),
                    },
                );
            }
        }
        Ok(closed_count)
    }

    /// Reproducible offline single step across deterministic synthetic BTC/ETH/SOL series
    /// so operators can verify the full 5-head Synapse nervous system with zero network access.
    pub fn run_deterministic_demo_step(&mut self) -> Result<CycleSummary> {
        let step_idx = self.cycle_count;
        let base_ms = 1_700_000_000_000_i64 + (step_idx as i64) * 60_000;
        let specs = [
            ("BTC", 64_000.0_f64, 0.0022_f64),
            ("ETH", 3_200.0_f64, 0.0028_f64),
            ("SOL", 145.0_f64, -0.0024_f64),
        ];

        let mut marks = BTreeMap::new();
        let mut markets = Vec::with_capacity(specs.len());
        for (sym, start_px, drift) in specs {
            let entry = self.demo_candles.entry(sym.into()).or_default();
            if entry.is_empty() {
                for k in 0..16i64 {
                    let t_ms = base_ms - (16 - k) * 60_000;
                    let ts = DateTime::from_timestamp_millis(t_ms).unwrap_or_else(Utc::now);
                    let px = start_px * (1.0 + drift * (k as f64));
                    entry.push(Candle {
                        ts,
                        open: px * 0.999,
                        high: px * 1.003,
                        low: px * 0.997,
                        close: px,
                        volume: 1_500.0,
                    });
                }
            }
            let prev = entry.last().map(|c| c.close).unwrap_or(start_px);
            let wave = ((step_idx as f64) * 0.45).sin() * 0.002;
            let next_px = (prev * (1.0 + drift + wave)).max(1.0);
            let prev_ts = entry.last().map(|c| c.ts).unwrap_or_else(Utc::now);
            let next_ts = prev_ts + ChronoDuration::minutes(1);
            entry.push(Candle {
                ts: next_ts,
                open: prev,
                high: next_px.max(prev) * 1.002,
                low: next_px.min(prev) * 0.998,
                close: next_px,
                volume: 1_650.0,
            });
            if entry.len() > 120 {
                entry.remove(0);
            }
            if let Some(d) = decimal_positive(next_px) {
                marks.insert(sym.into(), d);
            }
            markets.push(MarketObservation {
                symbol: sym.into(),
                observed_at_ms: next_ts.timestamp_millis(),
                volume_24h_usd: Some(25_000_000.0),
                depth_usd: Some(250_000.0),
                spread_bps: Some(3.0),
                volatility_pct: Some(2.5),
                directionality: Some(0.75),
                funding_bps: Some(1.0),
                open_interest_usd: Some(12_000_000.0),
                coverage_fraction: Some(0.99),
                age_days: Some(365.0),
                ema_ready: true,
            });
        }
        let candles = self.demo_candles.clone();
        self.step_cycle_with_provenance(
            base_ms + 60_000,
            DataProvenance::SyntheticTestFixture,
            &markets,
            &candles,
            &marks,
        )
    }

    /// Evaluates genuinely observed WebSocket/REST market snapshots (`HunterMarketState`)
    /// without fabricating missing L2 depth, spreads, completed candles, or market age.
    /// Any incomplete or stale snapshot stays ineligible rather than receiving placeholder numbers.
    pub fn step_cycle_from_observed(
        &mut self,
        now_ms: i64,
        snapshots: &[ObservedMarketSnapshot],
    ) -> Result<CycleSummary> {
        let mut marks = BTreeMap::new();
        let mut candles_map = BTreeMap::new();
        let mut markets = Vec::new();
        for snap in snapshots {
            if let Some(d) = decimal_positive(snap.mark_px) {
                marks.insert(snap.symbol.clone(), d);
            }
            if !snap.completed_candles.is_empty() {
                candles_map.insert(snap.symbol.clone(), snap.completed_candles.clone());
            }
            let obs_ts = snap.exchange_ts_ms.unwrap_or(snap.received_at_ms);
            let age_days = if snap.is_stale
                || snap.depth_usd.is_none()
                || snap.spread_bps.is_none()
                || snap.directionality.is_none()
                || snap.coverage_fraction.is_none()
                || snap.completed_candles.len() < 12
            {
                None
            } else {
                // Hyperliquid metaAndAssetCtxs does not report instrument listing date;
                // require minimum completed candle history instead of inventing an arbitrary age.
                Some(snap.completed_candles.len() as f64 / 24.0)
            };
            markets.push(MarketObservation {
                symbol: snap.symbol.clone(),
                observed_at_ms: obs_ts,
                volume_24h_usd: snap.volume_24h_usd,
                depth_usd: snap.depth_usd,
                spread_bps: snap.spread_bps,
                volatility_pct: snap.volatility_pct,
                directionality: snap.directionality,
                funding_bps: snap.funding_bps_1h,
                open_interest_usd: snap.open_interest_usd,
                coverage_fraction: snap.coverage_fraction,
                age_days,
                ema_ready: snap.completed_candles.len() >= 12,
            });
        }
        self.step_cycle_with_provenance(
            now_ms,
            DataProvenance::AuthoritativeExchange,
            &markets,
            &candles_map,
            &marks,
        )
    }
}

#[inline]
fn decimal_positive(v: f64) -> Option<Decimal> {
    if !v.is_finite() || v <= 0.0 {
        return None;
    }
    Decimal::try_from(v)
        .ok()
        .map(|d| d.round_dp(6))
        .filter(|d| *d > Decimal::ZERO)
}

#[inline]
fn f64_from_decimal(d: Decimal) -> f64 {
    d.to_string().parse::<f64>().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_demo_cycles_are_reproducible_and_connected() {
        let mut p1 = SynapsePipeline::new(SynapsePipelineConfig::default()).unwrap();
        let mut p2 = SynapsePipeline::new(SynapsePipelineConfig::default()).unwrap();
        for _ in 0..5 {
            let s1 = p1.run_deterministic_demo_step().unwrap();
            let s2 = p2.run_deterministic_demo_step().unwrap();
            assert_eq!(s1.watchlist, s2.watchlist);
            assert_eq!(s1.approved_entries, s2.approved_entries);
            assert_eq!(s1.mtm_equity_usd, s2.mtm_equity_usd);
        }
        assert!(p1.bus.total_published() >= 10);
        assert!(p1.bus.latest_for_head(HeadId::Hunter).is_some());
        assert!(p1.bus.latest_for_head(HeadId::Cognition).is_some());
        assert!(p1.bus.latest_for_head(HeadId::Fang).is_some());
        assert!(p1.bus.latest_for_head(HeadId::Aegis).is_some());
        assert!(p1.bus.latest_for_head(HeadId::Nexus).is_some());
        assert!(!p1.ledger.open_positions.is_empty() || !p1.ledger.closed_trades.is_empty());
    }

    #[test]
    fn stale_markets_are_rejected_without_opening_positions() {
        let mut pipe = SynapsePipeline::new(SynapsePipelineConfig::default()).unwrap();
        let now_ms = 1_700_005_000_000_i64;
        let stale_obs = MarketObservation {
            symbol: "BTC".into(),
            observed_at_ms: now_ms - 120_000, // 120s old > 60s stale limit
            volume_24h_usd: Some(50_000_000.0),
            depth_usd: Some(500_000.0),
            spread_bps: Some(2.0),
            volatility_pct: Some(2.0),
            directionality: Some(0.8),
            funding_bps: Some(1.0),
            open_interest_usd: Some(10_000_000.0),
            coverage_fraction: Some(1.0),
            age_days: Some(365.0),
            ema_ready: true,
        };
        let mut candles = BTreeMap::new();
        let mut bars = Vec::new();
        for k in 0..16i64 {
            let ts = DateTime::from_timestamp_millis(now_ms - (16 - k) * 60_000).unwrap();
            let px = 60_000.0 + k as f64 * 120.0;
            bars.push(Candle {
                ts,
                open: px * 0.999,
                high: px * 1.002,
                low: px * 0.998,
                close: px,
                volume: 1_000.0,
            });
        }
        candles.insert("BTC".into(), bars);
        let mut marks = BTreeMap::new();
        marks.insert("BTC".into(), Decimal::from(61_800u32));

        let sum = pipe
            .step_cycle(now_ms, &[stale_obs], &candles, &marks)
            .unwrap();
        assert_eq!(sum.approved_entries, 0);
        assert!(pipe.ledger.open_positions.is_empty());
    }

    #[test]
    fn drawdown_breaker_blocks_new_entries_while_permitting_protective_exits() {
        let mut pipe = SynapsePipeline::new(SynapsePipelineConfig {
            starting_equity: Decimal::from(10_000u32),
            max_drawdown_ratio: Decimal::new(5, 2), // tight 5% breaker for test
            stop_loss_pct: Decimal::new(3, 2),
            ..Default::default()
        })
        .unwrap();
        let now_ms = 1_700_005_000_000_i64;
        // Inject an existing open position + latch drawdown via steep adverse mark
        pipe.ledger.open_positions.insert(
            "ETH".into(),
            PaperPosition {
                symbol: "ETH".into(),
                owner_strategy: "test".into(),
                side: Side::Long,
                entry_price: Decimal::from(3_000u32),
                qty: Decimal::from(2u32),
                notional: Decimal::from(6_000u32),
                stop_price: Decimal::from(2_910u32),
                take_profit_price: Decimal::from(3_120u32),
                entry_fee_usd: Decimal::new(240, 2),
                opened_at_ms: now_ms - 60_000,
                correlation_id: "prior-1".into(),
            },
        );
        let mut marks = BTreeMap::new();
        marks.insert("ETH".into(), Decimal::from(2_700u32)); // breaches stop AND triggers 5% drawdown!
        let sum = pipe
            .step_cycle(now_ms, &[], &BTreeMap::new(), &marks)
            .unwrap();
        assert_eq!(sum.closed_trades, 1);
        assert!(pipe.ledger.open_positions.is_empty());
        assert!(pipe.ledger.breaker_reason.is_some());
        assert!(pipe.ledger.realized_net_pnl_usd < Decimal::ZERO);
    }
}
