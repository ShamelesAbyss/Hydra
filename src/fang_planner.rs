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
}
