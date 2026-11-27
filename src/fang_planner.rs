//! Pure research planning kernels. No execution or independent risk authorization.
//! Future live/replay consumers must separately enforce AEGIS and venue contracts.
use crate::core::{Candle, Side, Signal};
use anyhow::{ensure, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanCandidate {
    pub id: String,
    pub signal: Signal,
    pub reference_price: f64,
    /// Explicit requested quote notional, not a quantity or leverage instruction.
    pub requested_notional: Decimal,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanningBudget {
    pub approved_symbols: BTreeSet<String>,
    pub excluded_symbols: BTreeSet<String>,
    /// Remaining capacity supplied by caller, after accounting for existing exposure.
    pub total_remaining_notional: Decimal,
    pub per_symbol_remaining_notional: BTreeMap<String, Decimal>,
    /// Any occupied symbol is blocked: adding/scaling ownership is not implemented.
    pub occupied_symbols: BTreeMap<String, String>,
    pub confidence_floor: f64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectionReason {
    InvalidSignal,
    ExcludedSymbol,
    UnapprovedSymbol,
    ConfidenceBelowFloor,
    ExistingOwner,
    SymbolAlreadySelected,
    MissingSymbolBudget,
    SymbolBudgetExceeded,
    TotalBudgetExceeded,
}
#[derive(Clone, Debug, Serialize)]
pub struct RejectedCandidate {
    pub id: String,
    pub reason: RejectionReason,
}
#[derive(Clone, Debug, Serialize)]
pub struct PlanningReport {
    pub schema_version: u32,
    pub research_only: bool,
    pub execution_authorized: bool,
    pub selected: Vec<PlanCandidate>,
    pub rejected: Vec<RejectedCandidate>,
    pub allocated_notional: Decimal,
    pub remaining_notional: Decimal,
    pub limitations: Vec<String>,
}

/// Priority: confidence descending, expected edge descending, oldest signal,
/// strategy name, symbol and unique candidate ID ascending. Input order has no effect.
pub fn plan(candidates: &[PlanCandidate], budget: &PlanningBudget) -> Result<PlanningReport> {
    ensure!(candidates.len() <= 10_000, "candidate budget exceeded");
    ensure!(
        budget.approved_symbols.len() <= 1_000
            && budget.excluded_symbols.len() <= 1_000
            && budget.per_symbol_remaining_notional.len() <= 1_000
            && budget.occupied_symbols.len() <= 1_000,
        "symbol budget exceeded"
    );
    ensure!(
        budget.confidence_floor.is_finite() && (0.0..=1.0).contains(&budget.confidence_floor),
        "invalid confidence floor"
    );
    ensure!(
        budget.total_remaining_notional >= Decimal::ZERO
            && budget
                .per_symbol_remaining_notional
                .values()
                .all(|v| *v >= Decimal::ZERO),
        "negative remaining budget"
    );
    let mut ids = BTreeSet::new();
    for c in candidates {
        ensure!(
            !c.id.is_empty() && c.id.len() <= 128 && ids.insert(c.id.as_str()),
            "candidate IDs must be bounded, nonempty and unique"
        );
    }
    let mut ranked: Vec<&PlanCandidate> = candidates.iter().collect();
    ranked.sort_by(|a, b| {
        b.signal
            .confidence
            .total_cmp(&a.signal.confidence)
            .then_with(|| {
                b.signal
                    .expected_edge_bps
                    .total_cmp(&a.signal.expected_edge_bps)
            })
            .then_with(|| a.signal.created_at.cmp(&b.signal.created_at))
            .then_with(|| a.signal.strategy.cmp(&b.signal.strategy))
            .then_with(|| a.signal.symbol.cmp(&b.signal.symbol))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut selected = Vec::new();
    let mut rejected = Vec::new();
    let mut claimed = BTreeSet::new();
    let mut remaining = budget.total_remaining_notional;
    for c in ranked {
        let s = &c.signal;
        let valid = !s.strategy.is_empty()
            && s.strategy.len() <= 128
            && !s.symbol.is_empty()
            && s.symbol.len() <= 64
            && s.venue == "hyperliquid"
            && s.reason.len() <= 4096
            && s.confidence.is_finite()
            && (0.0..=1.0).contains(&s.confidence)
            && s.expected_edge_bps.is_finite()
            && s.expected_edge_bps >= 0.0
            && [c.reference_price, s.stop_price, s.take_profit]
                .iter()
                .all(|p| p.is_finite() && *p > 0.0)
            && c.requested_notional > Decimal::ZERO
            && if s.side.is_buy() {
                s.stop_price < c.reference_price && s.take_profit > c.reference_price
            } else {
                s.stop_price > c.reference_price && s.take_profit < c.reference_price
            };
        let reason = if !valid {
            Some(RejectionReason::InvalidSignal)
        } else if budget.excluded_symbols.contains(&s.symbol) {
            Some(RejectionReason::ExcludedSymbol)
        } else if !budget.approved_symbols.contains(&s.symbol) {
            Some(RejectionReason::UnapprovedSymbol)
        } else if s.confidence < budget.confidence_floor {
            Some(RejectionReason::ConfidenceBelowFloor)
        } else if budget.occupied_symbols.contains_key(&s.symbol) {
            Some(RejectionReason::ExistingOwner)
        } else if claimed.contains(&s.symbol) {
            Some(RejectionReason::SymbolAlreadySelected)
        } else {
            match budget.per_symbol_remaining_notional.get(&s.symbol) {
                None => Some(RejectionReason::MissingSymbolBudget),
                Some(v) if c.requested_notional > *v => Some(RejectionReason::SymbolBudgetExceeded),
                _ if c.requested_notional > remaining => Some(RejectionReason::TotalBudgetExceeded),
                _ => None,
            }
        };
        if let Some(reason) = reason {
            rejected.push(RejectedCandidate {
                id: c.id.clone(),
                reason,
            });
        } else {
            let next = remaining
                .checked_sub(c.requested_notional)
                .ok_or_else(|| anyhow::anyhow!("budget subtraction overflow"))?;
            ensure!(
                remaining.checked_sub(next) == Some(c.requested_notional)
                    && next.checked_add(c.requested_notional) == Some(remaining),
                "budget subtraction loses decimal precision"
            );
            remaining = next;
            claimed.insert(s.symbol.clone());
            selected.push(c.clone());
        }
    }
    Ok(PlanningReport { schema_version: 1, research_only: true, execution_authorized: false,
        allocated_notional: budget.total_remaining_notional.checked_sub(remaining).ok_or_else(|| anyhow::anyhow!("allocation overflow"))?,
        remaining_notional: remaining, selected, rejected,
        limitations: vec![
            "Research arbitration only; no order construction, submission, credentials, persistence or live authorization".into(),
            "External budgets are assumed remaining quote-notional capacities, not a certified account snapshot; AEGIS must independently enforce risk before any future execution".into(),
            "Single native Hyperliquid symbol owner; existing positions, scaling, reversal, lot/tick precision, minimums, freshness, fees, margin and protective-order verification remain external requirements".into(),
        ] })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmaAnchorOptions {
    pub fast_span: usize,
    pub slow_span: usize,
    pub offset_bps: f64,
    /// Maximum distance of suggested quote from the latest completed close.
    pub max_quote_distance_bps: f64,
}
impl Default for EmaAnchorOptions {
    fn default() -> Self {
        Self {
            fast_span: 12,
            slow_span: 48,
            offset_bps: 10.0,
            max_quote_distance_bps: 100.0,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct EmaAnchorReport {
    pub research_only: bool,
    pub execution_authorized: bool,
    pub as_of_ms: i64,
    pub bar_count: usize,
    pub fast_ema: f64,
    pub slow_ema: f64,
    pub anchor: f64,
    /// Positive means current price is favorable to this side relative to anchor.
    pub directional_deviation_bps: f64,
    /// Present only when the last completed close crosses the directional offset.
    pub suggested_entry_price: Option<f64>,
    pub limitations: Vec<String>,
}
/// Hydra-owned basic EMA-band research math, not a Passivbot port or parity claim.
/// Caller supplies only completed bars as of its evaluation time; no wall clock is read.
pub fn ema_anchor(candles: &[Candle], side: Side, o: &EmaAnchorOptions) -> Result<EmaAnchorReport> {
    ensure!(
        (1..=10_000).contains(&o.fast_span) && o.fast_span < o.slow_span && o.slow_span <= 10_000,
        "invalid EMA spans"
    );
    ensure!(
        candles.len() >= o.slow_span && candles.len() <= 100_000,
        "insufficient EMA warmup or bar budget exceeded"
    );
    ensure!(
        [o.offset_bps, o.max_quote_distance_bps]
            .iter()
            .all(|v| v.is_finite() && (0.0..=1000.0).contains(v)),
        "invalid anchor bounds"
    );
    for (i, c) in candles.iter().enumerate() {
        ensure!(
            [c.open, c.high, c.low, c.close]
                .iter()
                .all(|p| p.is_finite() && *p > 0.0)
                && c.volume.is_finite()
                && c.volume >= 0.0
                && c.low <= c.open.min(c.close)
                && c.high >= c.open.max(c.close)
                && (i == 0 || candles[i - 1].ts < c.ts),
            "invalid chronological OHLCV"
        );
    }
    fn ema(bars: &[Candle], span: usize) -> Result<f64> {
        // Scale before summation so a positive finite seed cannot overflow.
        let mut value: f64 = bars[..span].iter().map(|c| c.close / span as f64).sum();
        let alpha = 2.0 / (span as f64 + 1.0);
        for c in &bars[span..] {
            value = value * (1.0 - alpha) + c.close * alpha;
        }
        ensure!(
            value.is_finite() && value > 0.0,
            "EMA numerical overflow/underflow"
        );
        Ok(value)
    }
    let fast = ema(candles, o.fast_span)?;
    let slow = ema(candles, o.slow_span)?;
    let anchor = if side.is_buy() {
        fast.min(slow)
    } else {
        fast.max(slow)
    };
    let last = candles.last().unwrap();
    let deviation = if side.is_buy() {
        1.0 - last.close / anchor
    } else {
        last.close / anchor - 1.0
    } * 10_000.0;
    let raw_quote = anchor
        * if side.is_buy() {
            1.0 - o.offset_bps / 10_000.0
        } else {
            1.0 + o.offset_bps / 10_000.0
        };
    let distance = o.max_quote_distance_bps / 10_000.0;
    let lower = last.close * (1.0 - distance);
    let upper = last.close * (1.0 + distance);
    ensure!(
        [deviation, raw_quote, lower, upper]
            .iter()
            .all(|v| v.is_finite())
            && lower > 0.0,
        "anchor numerical overflow/underflow"
    );
    let crossed = if side.is_buy() {
        last.close <= raw_quote
    } else {
        last.close >= raw_quote
    };
    let quote = if side.is_buy() {
        raw_quote.min(last.close)
    } else {
        raw_quote.max(last.close)
    }
    .clamp(lower, upper);
    Ok(EmaAnchorReport { research_only: true, execution_authorized: false,
        as_of_ms: last.ts.timestamp_millis(), bar_count: candles.len(), fast_ema: fast, slow_ema: slow, anchor,
        directional_deviation_bps: deviation, suggested_entry_price: crossed.then_some(quote),
        limitations: vec!["Basic fixed-offset EMA-band research only; no Passivbot parity, inventory shift, adaptive grids, volatility offsets, scaling, rounding, sizing or live integration".into(),
            "Suggestions are prices, not orders; completed-bar provenance and independent AEGIS/venue checks are caller responsibilities".into()] })
}

// ============================================================================
// FANG Bounded Deterministic Grid Planner (Anti-Martingale / Hard-Cap Enforced)
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GridSpacingMode {
    Arithmetic,
    Geometric,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundedGridConfig {
    pub symbol: String,
    pub side: Side,
    pub anchor_price: Decimal,
    /// Step distance in basis points between consecutive grid rungs (`5..=2500`).
    pub step_bps: Decimal,
    /// Number of bounded entry rungs (`1..=20`).
    pub levels: usize,
    pub spacing: GridSpacingMode,
    /// Hard cumulative quote notional budget across ALL grid rungs combined.
    pub max_total_notional_usd: Decimal,
    /// Per-level size multiplier strictly bounded to `0.5..=1.5` (no unbounded Martingale).
    pub size_multiplier: Decimal,
    /// Take-profit distance in basis points from cumulative weighted average entry (`10..=5000`).
    pub take_profit_bps: Decimal,
    /// Protective stop buffer in basis points beyond the deepest grid rung (`10..=5000`).
    pub stop_buffer_bps: Decimal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GridOrderLevel {
    pub level_index: usize,
    pub entry_price: Decimal,
    pub level_notional_usd: Decimal,
    pub level_quantity: Decimal,
    pub cumulative_notional_usd: Decimal,
    pub cumulative_quantity: Decimal,
    pub weighted_avg_entry: Decimal,
    pub take_profit_price: Decimal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoundedGridPlan {
    pub research_only: bool,
    pub execution_authorized: bool,
    pub symbol: String,
    pub side: Side,
    pub spacing: GridSpacingMode,
    pub levels: Vec<GridOrderLevel>,
    pub total_planned_notional_usd: Decimal,
    pub max_total_notional_usd: Decimal,
    pub worst_case_avg_entry: Decimal,
    pub protective_stop_price: Decimal,
    pub max_grid_span_bps: Decimal,
    pub limitations: Vec<String>,
}

/// Computes a bounded, deterministic entry/take-profit grid strictly capped by
/// `max_total_notional_usd` and an anti-Martingale multiplier limit (`<= 1.5`).
pub fn plan_bounded_grid(cfg: &BoundedGridConfig) -> Result<BoundedGridPlan> {
    ensure!(
        !cfg.symbol.trim().is_empty() && cfg.symbol.len() <= 64,
        "invalid grid symbol"
    );
    ensure!(
        (1..=20).contains(&cfg.levels),
        "grid levels must be bounded between 1 and 20"
    );
    ensure!(
        cfg.anchor_price > Decimal::ZERO,
        "anchor_price must be positive"
    );
    let ten_k = Decimal::from(10_000u32);
    ensure!(
        cfg.step_bps >= Decimal::from(5u32) && cfg.step_bps <= Decimal::from(2_500u32),
        "step_bps must be within 5..=2500 bps"
    );
    ensure!(
        cfg.max_total_notional_usd > Decimal::ZERO,
        "max_total_notional_usd must be positive"
    );
    let min_mult = Decimal::new(5, 1); // 0.5
    let max_mult = Decimal::new(15, 1); // 1.5 (hard anti-Martingale ceiling)
    ensure!(
        cfg.size_multiplier >= min_mult && cfg.size_multiplier <= max_mult,
        "size_multiplier must be within 0.5..=1.5 (unbounded Martingale is prohibited)"
    );
    ensure!(
        cfg.take_profit_bps >= Decimal::from(10u32)
            && cfg.take_profit_bps <= Decimal::from(5_000u32),
        "take_profit_bps must be within 10..=5000 bps"
    );
    ensure!(
        cfg.stop_buffer_bps >= Decimal::from(10u32)
            && cfg.stop_buffer_bps <= Decimal::from(5_000u32),
        "stop_buffer_bps must be within 10..=5000 bps"
    );

    // Compute normalized weights so the sum of all rungs stays <= max_total_notional_usd.
    let mut weights = Vec::with_capacity(cfg.levels);
    let mut w = Decimal::ONE;
    let mut weight_sum = Decimal::ZERO;
    for _ in 0..cfg.levels {
        weights.push(w);
        weight_sum = weight_sum
            .checked_add(w)
            .ok_or_else(|| anyhow::anyhow!("grid weight overflow"))?;
        w = w
            .checked_mul(cfg.size_multiplier)
            .ok_or_else(|| anyhow::anyhow!("grid multiplier overflow"))?;
    }

    let step_frac = cfg
        .step_bps
        .checked_div(ten_k)
        .ok_or_else(|| anyhow::anyhow!("step fraction error"))?;
    let tp_frac = cfg
        .take_profit_bps
        .checked_div(ten_k)
        .ok_or_else(|| anyhow::anyhow!("tp fraction error"))?;
    let stop_frac = cfg
        .stop_buffer_bps
        .checked_div(ten_k)
        .ok_or_else(|| anyhow::anyhow!("stop fraction error"))?;

    let mut out_levels = Vec::with_capacity(cfg.levels);
    let mut cum_notional = Decimal::ZERO;
    let mut cum_qty = Decimal::ZERO;
    let mut geo_price = cfg.anchor_price;

    for (idx, weight) in weights.iter().enumerate() {
        let entry_price = if idx == 0 {
            cfg.anchor_price
        } else {
            match cfg.spacing {
                GridSpacingMode::Arithmetic => {
                    let offset = step_frac
                        .checked_mul(Decimal::from(idx as u64))
                        .ok_or_else(|| anyhow::anyhow!("arithmetic step overflow"))?;
                    let factor = if cfg.side.is_buy() {
                        Decimal::ONE.checked_sub(offset)
                    } else {
                        Decimal::ONE.checked_add(offset)
                    }
                    .ok_or_else(|| anyhow::anyhow!("grid factor overflow"))?;
                    cfg.anchor_price
                        .checked_mul(factor)
                        .ok_or_else(|| anyhow::anyhow!("grid price overflow"))?
                }
                GridSpacingMode::Geometric => {
                    let factor = if cfg.side.is_buy() {
                        Decimal::ONE.checked_sub(step_frac)
                    } else {
                        Decimal::ONE.checked_add(step_frac)
                    }
                    .ok_or_else(|| anyhow::anyhow!("geometric factor overflow"))?;
                    geo_price = geo_price
                        .checked_mul(factor)
                        .ok_or_else(|| anyhow::anyhow!("geometric price overflow"))?;
                    geo_price
                }
            }
        }
        .round_dp(6);

        ensure!(
            entry_price > Decimal::ZERO,
            "grid rung {} reached non-positive price; reduce levels or step_bps",
            idx + 1
        );

        let raw_notional = cfg
            .max_total_notional_usd
            .checked_mul(*weight)
            .and_then(|v| v.checked_div(weight_sum))
            .ok_or_else(|| anyhow::anyhow!("grid notional overflow"))?
            .round_dp(2);
        // Clamp final rung so rounding never exceeds max_total_notional_usd.
        let remaining_cap = cfg
            .max_total_notional_usd
            .checked_sub(cum_notional)
            .unwrap_or(Decimal::ZERO);
        let level_notional_usd = raw_notional.min(remaining_cap);
        ensure!(
            level_notional_usd > Decimal::ZERO,
            "grid level notional rounded to zero"
        );

        let level_quantity = level_notional_usd
            .checked_div(entry_price)
            .ok_or_else(|| anyhow::anyhow!("level quantity overflow"))?
            .round_dp(6);
        ensure!(
            level_quantity > Decimal::ZERO,
            "level quantity rounded to zero"
        );

        cum_notional = cum_notional
            .checked_add(level_notional_usd)
            .ok_or_else(|| anyhow::anyhow!("cumulative notional overflow"))?;
        cum_qty = cum_qty
            .checked_add(level_quantity)
            .ok_or_else(|| anyhow::anyhow!("cumulative quantity overflow"))?;
        let weighted_avg_entry = cum_notional
            .checked_div(cum_qty)
            .ok_or_else(|| anyhow::anyhow!("average entry overflow"))?
            .round_dp(6);
        let tp_factor = if cfg.side.is_buy() {
            Decimal::ONE.checked_add(tp_frac)
        } else {
            Decimal::ONE.checked_sub(tp_frac)
        }
        .ok_or_else(|| anyhow::anyhow!("take-profit factor overflow"))?;
        let take_profit_price = weighted_avg_entry
            .checked_mul(tp_factor)
            .ok_or_else(|| anyhow::anyhow!("take-profit price overflow"))?
            .round_dp(6);
        ensure!(
            take_profit_price > Decimal::ZERO,
            "non-positive take-profit price"
        );

        out_levels.push(GridOrderLevel {
            level_index: idx + 1,
            entry_price,
            level_notional_usd,
            level_quantity,
            cumulative_notional_usd: cum_notional,
            cumulative_quantity: cum_qty,
            weighted_avg_entry,
            take_profit_price,
        });
    }

    ensure!(
        cum_notional <= cfg.max_total_notional_usd,
        "cumulative grid notional exceeded AEGIS cap"
    );

    let deepest = out_levels.last().unwrap();
    let stop_factor = if cfg.side.is_buy() {
        Decimal::ONE.checked_sub(stop_frac)
    } else {
        Decimal::ONE.checked_add(stop_frac)
    }
    .ok_or_else(|| anyhow::anyhow!("stop factor overflow"))?;
    let protective_stop_price = deepest
        .entry_price
        .checked_mul(stop_factor)
        .ok_or_else(|| anyhow::anyhow!("stop price overflow"))?
        .round_dp(6);
    ensure!(
        protective_stop_price > Decimal::ZERO,
        "protective stop price fell to zero or below"
    );

    let span_bps = (deepest.entry_price - cfg.anchor_price)
        .abs()
        .checked_div(cfg.anchor_price)
        .and_then(|r| r.checked_mul(ten_k))
        .unwrap_or(Decimal::ZERO)
        .round_dp(2);

    Ok(BoundedGridPlan {
        research_only: true,
        execution_authorized: false,
        symbol: cfg.symbol.clone(),
        side: cfg.side,
        spacing: cfg.spacing,
        worst_case_avg_entry: deepest.weighted_avg_entry,
        total_planned_notional_usd: cum_notional,
        max_total_notional_usd: cfg.max_total_notional_usd,
        protective_stop_price,
        max_grid_span_bps: span_bps,
        levels: out_levels,
        limitations: vec![
            "Pure deterministic bounded grid planning kernel; no live order placement or signing authority.".into(),
            "Hard anti-Martingale ceiling (multiplier <= 1.5) and strict max_total_notional_usd cap enforced; AEGIS must independently authorize live exposure.".into(),
            "Exchange tick/lot rounding, margin/liquidation checks, and fee/funding impact remain separate execution-stage responsibilities.".into(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    #[test]
    fn large_budget_cannot_silently_round_away_small_allocation() {
        let c = candidate("tiny", "ETH");
        let b = PlanningBudget {
            approved_symbols: ["ETH".into()].into(),
            excluded_symbols: Default::default(),
            total_remaining_notional: Decimal::MAX,
            per_symbol_remaining_notional: [("ETH".into(), Decimal::MAX)].into(),
            occupied_symbols: Default::default(),
            confidence_floor: 0.0,
        };
        assert!(plan(&[c], &b)
            .unwrap_err()
            .to_string()
            .contains("precision"));
    }
    fn candidate(id: &str, symbol: &str) -> PlanCandidate {
        PlanCandidate {
            id: id.into(),
            reference_price: 100.0,
            requested_notional: Decimal::new(1, 1),
            signal: Signal {
                strategy: "fixture".into(),
                venue: "hyperliquid".into(),
                symbol: symbol.into(),
                side: Side::Long,
                confidence: 0.9,
                expected_edge_bps: 20.0,
                stop_price: 90.0,
                take_profit: 110.0,
                reason: "offline fixture".into(),
                created_at: Utc.timestamp_millis_opt(1_700_000_000_000).unwrap(),
            },
        }
    }
    fn budget() -> PlanningBudget {
        PlanningBudget {
            approved_symbols: ["ETH", "BTC", "SOL"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            excluded_symbols: BTreeSet::new(),
            total_remaining_notional: Decimal::new(3, 1),
            per_symbol_remaining_notional: ["ETH", "BTC", "SOL"]
                .into_iter()
                .map(|s| (s.into(), Decimal::ONE))
                .collect(),
            occupied_symbols: BTreeMap::new(),
            confidence_floor: 0.5,
        }
    }
    #[test]
    fn permutation_invariant_ties_one_owner_and_exact_decimal_allocation() {
        let items = vec![
            candidate("z", "ETH"),
            candidate("a", "ETH"),
            candidate("b", "BTC"),
            candidate("c", "SOL"),
        ];
        let r = plan(&items, &budget()).unwrap();
        let mut reversed = items.clone();
        reversed.reverse();
        assert_eq!(
            serde_json::to_value(&r).unwrap(),
            serde_json::to_value(plan(&reversed, &budget()).unwrap()).unwrap()
        );
        assert_eq!(r.selected.len(), 3);
        assert!(r.selected.iter().any(|c| c.id == "a"));
        assert_eq!(r.rejected[0].reason, RejectionReason::SymbolAlreadySelected);
        assert_eq!(r.allocated_notional, Decimal::new(3, 1));
        assert_eq!(r.remaining_notional, Decimal::ZERO);
        assert!(!r.execution_authorized);
    }
    #[test]
    fn eligibility_confidence_ownership_and_capacity_are_explained() {
        let c = candidate("a", "ETH");
        let mut b = budget();
        b.excluded_symbols.insert("ETH".into());
        assert_eq!(
            plan(&[c.clone()], &b).unwrap().rejected[0].reason,
            RejectionReason::ExcludedSymbol
        );
        b = budget();
        b.approved_symbols.remove("ETH");
        assert_eq!(
            plan(&[c.clone()], &b).unwrap().rejected[0].reason,
            RejectionReason::UnapprovedSymbol
        );
        b = budget();
        b.confidence_floor = 0.95;
        assert_eq!(
            plan(&[c.clone()], &b).unwrap().rejected[0].reason,
            RejectionReason::ConfidenceBelowFloor
        );
        b = budget();
        b.occupied_symbols.insert("ETH".into(), "other".into());
        assert_eq!(
            plan(&[c.clone()], &b).unwrap().rejected[0].reason,
            RejectionReason::ExistingOwner
        );
        b = budget();
        b.per_symbol_remaining_notional.remove("ETH");
        assert_eq!(
            plan(&[c.clone()], &b).unwrap().rejected[0].reason,
            RejectionReason::MissingSymbolBudget
        );
        b.per_symbol_remaining_notional
            .insert("ETH".into(), Decimal::ZERO);
        assert_eq!(
            plan(&[c.clone()], &b).unwrap().rejected[0].reason,
            RejectionReason::SymbolBudgetExceeded
        );
        b = budget();
        b.total_remaining_notional = Decimal::ZERO;
        assert_eq!(
            plan(&[c], &b).unwrap().rejected[0].reason,
            RejectionReason::TotalBudgetExceeded
        );
    }
    #[test]
    fn invalid_signals_cannot_claim_symbols_and_bad_inputs_fail_closed() {
        let mut invalid = candidate("a", "ETH");
        invalid.signal.confidence = f64::NAN;
        let good = candidate("b", "ETH");
        let r = plan(&[invalid.clone(), good.clone()], &budget()).unwrap();
        assert_eq!(r.selected[0].id, "b");
        assert_eq!(r.rejected[0].reason, RejectionReason::InvalidSignal);
        invalid = candidate("a", "ETH");
        invalid.signal.stop_price = 101.0;
        assert_eq!(
            plan(&[invalid], &budget()).unwrap().rejected[0].reason,
            RejectionReason::InvalidSignal
        );
        assert!(plan(&[good.clone(), good], &budget()).is_err());
        let mut b = budget();
        b.total_remaining_notional = -Decimal::ONE;
        assert!(plan(&[], &b).is_err());
        b = budget();
        b.confidence_floor = f64::INFINITY;
        assert!(plan(&[], &b).is_err());
    }
    #[test]
    fn priority_is_confidence_then_edge_before_tie_keys() {
        let mut a = candidate("a", "ETH");
        let mut b = candidate("b", "ETH");
        a.signal.confidence = 0.8;
        b.signal.expected_edge_bps = 0.0;
        assert_eq!(
            plan(&[a.clone(), b.clone()], &budget()).unwrap().selected[0].id,
            "b"
        );
        a.signal.confidence = b.signal.confidence;
        a.signal.expected_edge_bps = 30.0;
        assert_eq!(plan(&[a, b], &budget()).unwrap().selected[0].id, "a");
    }
    fn bars(prices: &[f64]) -> Vec<Candle> {
        prices
            .iter()
            .enumerate()
            .map(|(i, &p)| Candle {
                ts: Utc
                    .timestamp_millis_opt(1_700_000_000_000 + i as i64 * 60_000)
                    .unwrap(),
                open: p,
                high: p,
                low: p,
                close: p,
                volume: 1.0,
            })
            .collect()
    }
    fn anchor_options() -> EmaAnchorOptions {
        EmaAnchorOptions {
            fast_span: 2,
            slow_span: 4,
            offset_bps: 10.0,
            max_quote_distance_bps: 50.0,
        }
    }
    #[test]
    fn anchor_warmup_seed_and_directional_recommendations_are_bounded() {
        let o = anchor_options();
        assert!(ema_anchor(&bars(&[100.0; 3]), Side::Long, &o).is_err());
        let flat = ema_anchor(&bars(&[100.0; 4]), Side::Long, &o).unwrap();
        assert_eq!(flat.fast_ema, 100.0);
        assert_eq!(flat.slow_ema, 100.0);
        assert!(flat.suggested_entry_price.is_none());
        for (prices, side) in [
            ([100.0, 100.0, 100.0, 90.0], Side::Long),
            ([100.0, 100.0, 100.0, 110.0], Side::Short),
        ] {
            let r = ema_anchor(&bars(&prices), side, &o).unwrap();
            let quote = r.suggested_entry_price.unwrap();
            let last = prices[3];
            assert!(r.directional_deviation_bps > 0.0);
            assert!(quote >= last * 0.995 && quote <= last * 1.005);
            assert!(!r.execution_authorized);
        }
    }
    #[test]
    fn anchor_uses_only_given_prefix_and_rejects_invalid_series() {
        let o = anchor_options();
        let mut b = bars(&[100.0, 101.0, 102.0, 99.0, 105.0]);
        let before = ema_anchor(&b[..4], Side::Long, &o).unwrap();
        b[4].close = 1e9;
        b[4].high = 1e9;
        assert_eq!(
            serde_json::to_value(before).unwrap(),
            serde_json::to_value(ema_anchor(&b[..4], Side::Long, &o).unwrap()).unwrap()
        );
        b[2].ts = b[1].ts;
        assert!(ema_anchor(&b, Side::Long, &o).is_err());
        b = bars(&[100.0; 4]);
        b[0].close = f64::NAN;
        assert!(ema_anchor(&b, Side::Long, &o).is_err());
        let mut bad = o;
        bad.offset_bps = f64::INFINITY;
        assert!(ema_anchor(&bars(&[100.0; 4]), Side::Long, &bad).is_err());
    }

    #[test]
    fn bounded_grid_enforces_cap_anti_martingale_and_monotonic_geometry() {
        let cfg = BoundedGridConfig {
            symbol: "ETH".into(),
            side: Side::Long,
            anchor_price: Decimal::from(3000u32),
            step_bps: Decimal::from(100u32), // 1%
            levels: 5,
            spacing: GridSpacingMode::Geometric,
            max_total_notional_usd: Decimal::from(1000u32),
            size_multiplier: Decimal::new(12, 1), // 1.2x bounded
            take_profit_bps: Decimal::from(150u32),
            stop_buffer_bps: Decimal::from(200u32),
        };
        let grid = plan_bounded_grid(&cfg).unwrap();
        assert!(grid.research_only && !grid.execution_authorized);
        assert_eq!(grid.levels.len(), 5);
        assert!(grid.total_planned_notional_usd <= cfg.max_total_notional_usd);
        for w in grid.levels.windows(2) {
            assert!(w[1].entry_price < w[0].entry_price);
            assert!(w[1].weighted_avg_entry < w[0].weighted_avg_entry);
            assert!(w[1].take_profit_price > w[1].weighted_avg_entry);
        }
        assert!(grid.protective_stop_price < grid.levels.last().unwrap().entry_price);

        // Short side geometry moves upward on entry rungs and downward on take-profit.
        let mut short_cfg = cfg.clone();
        short_cfg.side = Side::Short;
        short_cfg.spacing = GridSpacingMode::Arithmetic;
        let short_grid = plan_bounded_grid(&short_cfg).unwrap();
        assert!(short_grid.levels[4].entry_price > short_grid.levels[0].entry_price);
        assert!(short_grid.protective_stop_price > short_grid.levels[4].entry_price);
        assert!(short_grid.levels[4].take_profit_price < short_grid.levels[4].weighted_avg_entry);

        // Unbounded Martingale (> 1.5) and excessive level counts (> 20) are strictly rejected.
        let mut bad_mult = cfg.clone();
        bad_mult.size_multiplier = Decimal::from(2u32);
        assert!(plan_bounded_grid(&bad_mult).is_err());

        let mut bad_levels = cfg;
        bad_levels.levels = 21;
        assert!(plan_bounded_grid(&bad_levels).is_err());
    }
}
