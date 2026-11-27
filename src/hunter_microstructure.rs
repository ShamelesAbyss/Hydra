//! HUNTER advanced market microstructure analysis over genuine L2 book,
//! trade flow, funding, open-interest, and completed candle history.
//!
//! Safety & truthfulness:
//! - Strictly `DataProvenance::DerivedMarketStat` (never fabricates missing fields).
//! - All indicators respect point-in-time causality (`ts <= as_of_ms`).
//! - Evaluates whether microstructure signals actually lift returns beyond baselines.

use crate::core::Candle;
use crate::hunter_stream::{L2BookObservation, L2DepthSample, TradeObservation};
use crate::synapse_bus::DataProvenance;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MicrostructureSnapshot {
    pub symbol: String,
    pub as_of_ms: i64,
    pub provenance: DataProvenance,
    pub order_book_imbalance: Option<f64>,
    pub top_level_imbalance: Option<f64>,
    pub trade_flow_imbalance: Option<f64>,
    pub buyer_volume_usd: f64,
    pub seller_volume_usd: f64,
    pub liquidity_migration_bps: Option<f64>,
    pub liquidity_deterioration_ratio: Option<f64>,
    pub spread_current_bps: Option<f64>,
    pub spread_baseline_bps: Option<f64>,
    pub spread_widening_ratio: Option<f64>,
    pub depth_impact_buy_bps: Option<f64>,
    pub depth_impact_sell_bps: Option<f64>,
    pub realized_volatility_ann_pct: Option<f64>,
    pub funding_bps_1h: Option<f64>,
    pub open_interest_delta_pct: Option<f64>,
    pub adverse_selection_score: Option<f64>,
    pub sample_counts: MicrostructureSampleCounts,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrostructureSampleCounts {
    pub bid_levels: usize,
    pub ask_levels: usize,
    pub causal_trades: usize,
    pub depth_history: usize,
    pub completed_candles: usize,
    pub oi_samples: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrelationMatrix {
    pub symbols: Vec<String>,
    pub overlap_counts: Vec<Vec<usize>>,
    pub correlations: Vec<Vec<Option<f64>>>,
    pub lookback_bars: usize,
    pub provenance: DataProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UtilityVerdict {
    PositiveLift,
    NegativeOrDegraded,
    InconclusiveInsufficientSamples,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MicrostructureUtilityReport {
    pub baseline_samples: usize,
    pub filtered_samples: usize,
    pub baseline_mean_bps: Option<f64>,
    pub filtered_mean_bps: Option<f64>,
    pub lift_bps: Option<f64>,
    pub baseline_hit_rate: Option<f64>,
    pub filtered_hit_rate: Option<f64>,
    pub verdict: UtilityVerdict,
    pub limitations: Vec<String>,
}

/// Computes exact execution impact in basis points for buying or selling `probe_notional_usd`
/// by walking L2 `(price, size)` levels. Returns `None` if cumulative book depth is insufficient.
pub fn estimate_l2_walk_impact_bps(
    mid: f64,
    levels: &[(f64, f64)],
    is_buy: bool,
    probe_notional_usd: f64,
) -> Option<f64> {
    if !mid.is_finite()
        || mid <= 0.0
        || !probe_notional_usd.is_finite()
        || probe_notional_usd <= 0.0
    {
        return None;
    }
    let mut remaining_usd = probe_notional_usd;
    let mut total_qty = 0.0;
    let mut total_usd = 0.0;
    for &(px, sz) in levels {
        if !px.is_finite() || !sz.is_finite() || px <= 0.0 || sz <= 0.0 {
            continue;
        }
        let level_usd = px * sz;
        if level_usd >= remaining_usd {
            let take_qty = remaining_usd / px;
            total_qty += take_qty;
            total_usd += remaining_usd;
            remaining_usd = 0.0;
            break;
        } else {
            total_qty += sz;
            total_usd += level_usd;
            remaining_usd -= level_usd;
        }
    }
    if remaining_usd > 1e-6 || total_qty <= 0.0 {
        return None;
    }
    let vwap = total_usd / total_qty;
    let bps = if is_buy {
        ((vwap - mid) / mid) * 10_000.0
    } else {
        ((mid - vwap) / mid) * 10_000.0
    };
    Some(bps.max(0.0))
}

#[allow(clippy::too_many_arguments)]
pub fn compute_microstructure_snapshot(
    symbol: &str,
    as_of_ms: i64,
    book: Option<&L2BookObservation>,
    depth_history: Option<&VecDeque<L2DepthSample>>,
    trades: &[TradeObservation],
    candles: &[Candle],
    funding_bps_1h: Option<f64>,
    oi_history: Option<&VecDeque<(i64, f64)>>,
    probe_notional_usd: f64,
) -> MicrostructureSnapshot {
    let valid_book = book.filter(|b| b.received_at_ms <= as_of_ms);

    let order_book_imbalance = valid_book.and_then(|b| {
        let denom = b.bid_depth_usd + b.ask_depth_usd;
        (denom > 0.0 && denom.is_finite())
            .then_some(((b.bid_depth_usd - b.ask_depth_usd) / denom).clamp(-1.0, 1.0))
    });

    let top_level_imbalance = valid_book.and_then(|b| {
        let top_bid = b.bids.first().map(|(p, s)| p * s).unwrap_or(0.0);
        let top_ask = b.asks.first().map(|(p, s)| p * s).unwrap_or(0.0);
        let denom = top_bid + top_ask;
        (denom > 0.0 && denom.is_finite()).then_some(((top_bid - top_ask) / denom).clamp(-1.0, 1.0))
    });

    let depth_impact_buy_bps = valid_book
        .and_then(|b| estimate_l2_walk_impact_bps(b.mid, &b.asks, true, probe_notional_usd));
    let depth_impact_sell_bps = valid_book
        .and_then(|b| estimate_l2_walk_impact_bps(b.mid, &b.bids, false, probe_notional_usd));

    let causal_trades: Vec<&TradeObservation> = trades
        .iter()
        .filter(|t| t.received_at_ms <= as_of_ms && t.exchange_ts_ms <= as_of_ms)
        .collect();
    let mut buyer_volume_usd = 0.0;
    let mut seller_volume_usd = 0.0;
    for t in &causal_trades {
        let ntl = t.px * t.sz;
        if !ntl.is_finite() || ntl <= 0.0 {
            continue;
        }
        match t.side.as_str() {
            "B" | "BUY" | "buy" => buyer_volume_usd += ntl,
            "A" | "S" | "SELL" | "sell" => seller_volume_usd += ntl,
            _ => {}
        }
    }
    let trade_total = buyer_volume_usd + seller_volume_usd;
    let trade_flow_imbalance = (trade_total > 0.0)
        .then_some(((buyer_volume_usd - seller_volume_usd) / trade_total).clamp(-1.0, 1.0));

    let causal_depth: Vec<&L2DepthSample> = depth_history
        .map(|dq| dq.iter().filter(|s| s.received_at_ms <= as_of_ms).collect())
        .unwrap_or_default();
    let spread_current_bps = valid_book.map(|b| b.spread_bps);
    let (
        spread_baseline_bps,
        spread_widening_ratio,
        liquidity_migration_bps,
        liquidity_deterioration_ratio,
    ) = if causal_depth.len() >= 3 {
        let mut spreads: Vec<f64> = causal_depth.iter().map(|d| d.spread_bps).collect();
        spreads.sort_by(|a, b| a.total_cmp(b));
        let baseline_sp = spreads[spreads.len() / 2];
        let widening = spread_current_bps
            .filter(|_| baseline_sp > 0.0)
            .map(|cur| cur / baseline_sp);

        let mid_idx = causal_depth.len() / 2;
        let mean_imb = |slice: &[&L2DepthSample]| -> Option<f64> {
            if slice.is_empty() {
                return None;
            }
            let vals: Vec<f64> = slice
                .iter()
                .filter_map(|s| {
                    let d = s.bid_depth_usd + s.ask_depth_usd;
                    (d > 0.0).then_some((s.bid_depth_usd - s.ask_depth_usd) / d)
                })
                .collect();
            (!vals.is_empty()).then_some(vals.iter().sum::<f64>() / vals.len() as f64)
        };
        let migration = match (
            mean_imb(&causal_depth[..mid_idx]),
            mean_imb(&causal_depth[mid_idx..]),
        ) {
            (Some(earlier), Some(recent)) => Some((recent - earlier) * 10_000.0),
            _ => None,
        };

        let mut mins: Vec<f64> = causal_depth
            .iter()
            .map(|d| d.bid_depth_usd.min(d.ask_depth_usd))
            .collect();
        mins.sort_by(|a, b| a.total_cmp(b));
        let median_depth = mins[mins.len() / 2];
        let cur_min_depth = valid_book.map(|b| b.bid_depth_usd.min(b.ask_depth_usd));
        let det = cur_min_depth
            .filter(|_| median_depth > 0.0)
            .map(|cur| cur / median_depth);

        (Some(baseline_sp), widening, migration, det)
    } else {
        (None, None, None, None)
    };

    let causal_candles: Vec<&Candle> = candles
        .iter()
        .filter(|c| c.ts.timestamp_millis() <= as_of_ms && c.close > 0.0)
        .collect();
    let realized_volatility_ann_pct = if causal_candles.len() >= 12 {
        let mut rets = Vec::with_capacity(causal_candles.len() - 1);
        for w in causal_candles.windows(2) {
            rets.push((w[1].close / w[0].close).ln());
        }
        if rets.len() >= 2 {
            let n = rets.len() as f64;
            let mean = rets.iter().sum::<f64>() / n;
            let var = rets.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
            Some((var.sqrt() * 525_600.0_f64.sqrt() * 100.0).max(0.0))
        } else {
            None
        }
    } else {
        None
    };

    let causal_oi: Vec<(i64, f64)> = oi_history
        .map(|dq| {
            dq.iter()
                .copied()
                .filter(|(ts, _)| *ts <= as_of_ms)
                .collect()
        })
        .unwrap_or_default();
    let open_interest_delta_pct = if causal_oi.len() >= 2 && causal_oi[0].1 > 0.0 {
        let first = causal_oi[0].1;
        let last = causal_oi.last().unwrap().1;
        Some(((last - first) / first) * 100.0)
    } else {
        None
    };

    // Adverse selection proxy in [0, 1]: high when taker flow is strongly one-sided
    // AND spread widens or liquidity deteriorates against passive quotes.
    let adverse_selection_score = match (
        trade_flow_imbalance,
        spread_widening_ratio,
        liquidity_deterioration_ratio,
    ) {
        (Some(tfi), Some(sw), Some(det)) => {
            let flow_pressure = tfi.abs();
            let widen_term = ((sw - 1.0).max(0.0) / 1.5).clamp(0.0, 1.0);
            let thin_term = ((1.0 - det).max(0.0) / 0.5).clamp(0.0, 1.0);
            Some((0.45 * flow_pressure + 0.30 * widen_term + 0.25 * thin_term).clamp(0.0, 1.0))
        }
        (Some(tfi), Some(sw), None) => {
            let widen_term = ((sw - 1.0).max(0.0) / 1.5).clamp(0.0, 1.0);
            Some((0.60 * tfi.abs() + 0.40 * widen_term).clamp(0.0, 1.0))
        }
        (Some(tfi), None, None) => Some(tfi.abs().clamp(0.0, 1.0)),
        _ => None,
    };

    MicrostructureSnapshot {
        symbol: symbol.to_string(),
        as_of_ms,
        provenance: DataProvenance::DerivedMarketStat,
        order_book_imbalance,
        top_level_imbalance,
        trade_flow_imbalance,
        buyer_volume_usd,
        seller_volume_usd,
        liquidity_migration_bps,
        liquidity_deterioration_ratio,
        spread_current_bps,
        spread_baseline_bps,
        spread_widening_ratio,
        depth_impact_buy_bps,
        depth_impact_sell_bps,
        realized_volatility_ann_pct,
        funding_bps_1h,
        open_interest_delta_pct,
        adverse_selection_score,
        sample_counts: MicrostructureSampleCounts {
            bid_levels: valid_book.map_or(0, |b| b.levels_bid),
            ask_levels: valid_book.map_or(0, |b| b.levels_ask),
            causal_trades: causal_trades.len(),
            depth_history: causal_depth.len(),
            completed_candles: causal_candles.len(),
            oi_samples: causal_oi.len(),
        },
        limitations: vec![
            "Read-only DerivedMarketStat features; never independently authorizes live exposure"
                .into(),
            "Unavailable L2 depth or history fields remain None without synthetic imputation"
                .into(),
        ],
    }
}

/// Computes timestamp-aligned pairwise Pearson correlations across completed candle log-returns.
pub fn compute_correlation_matrix(
    candles_by_symbol: &BTreeMap<String, Vec<Candle>>,
    lookback_bars: usize,
) -> Result<CorrelationMatrix> {
    ensure!(
        (2..=2_000).contains(&lookback_bars),
        "lookback_bars must be in 2..=2000"
    );
    let symbols: Vec<String> = candles_by_symbol.keys().cloned().collect();
    let mut returns_by_sym: BTreeMap<String, BTreeMap<i64, f64>> = BTreeMap::new();
    for (sym, bars) in candles_by_symbol {
        let take_n = bars.len().min(lookback_bars + 1);
        let slice = &bars[bars.len() - take_n..];
        let mut rmap = BTreeMap::new();
        for w in slice.windows(2) {
            if w[0].close > 0.0 && w[1].close > 0.0 {
                rmap.insert(w[1].ts.timestamp_millis(), (w[1].close / w[0].close).ln());
            }
        }
        returns_by_sym.insert(sym.clone(), rmap);
    }

    let n = symbols.len();
    let mut overlap_counts = vec![vec![0usize; n]; n];
    let mut correlations = vec![vec![None; n]; n];

    for i in 0..n {
        for j in i..n {
            if i == j {
                let len = returns_by_sym[&symbols[i]].len();
                overlap_counts[i][j] = len;
                correlations[i][j] = (len >= 12).then_some(1.0);
                continue;
            }
            let r1 = &returns_by_sym[&symbols[i]];
            let r2 = &returns_by_sym[&symbols[j]];
            let pairs: Vec<(f64, f64)> = r1
                .iter()
                .filter_map(|(ts, &x)| r2.get(ts).map(|&y| (x, y)))
                .collect();
            overlap_counts[i][j] = pairs.len();
            overlap_counts[j][i] = pairs.len();
            if pairs.len() >= 12 {
                let k = pairs.len() as f64;
                let mx = pairs.iter().map(|(x, _)| *x).sum::<f64>() / k;
                let my = pairs.iter().map(|(_, y)| *y).sum::<f64>() / k;
                let mut num = 0.0;
                let mut vx = 0.0;
                let mut vy = 0.0;
                for (x, y) in pairs {
                    let dx = x - mx;
                    let dy = y - my;
                    num += dx * dy;
                    vx += dx * dx;
                    vy += dy * dy;
                }
                let denom = (vx * vy).sqrt();
                let corr = (denom > 1e-12).then_some((num / denom).clamp(-1.0, 1.0));
                correlations[i][j] = corr;
                correlations[j][i] = corr;
            }
        }
    }

    Ok(CorrelationMatrix {
        symbols,
        overlap_counts,
        correlations,
        lookback_bars,
        provenance: DataProvenance::DerivedMarketStat,
    })
}

/// Truthfully evaluates whether filtering a simple price baseline by microstructure
/// features delivers measurable out-of-sample lift (reports negative/inconclusive results).
pub fn evaluate_microstructure_utility(
    baseline_returns_bps: &[f64],
    microstructure_selected: &[bool],
) -> Result<MicrostructureUtilityReport> {
    ensure!(
        baseline_returns_bps.len() == microstructure_selected.len(),
        "baseline and filter mask lengths must match"
    );
    ensure!(
        baseline_returns_bps.iter().all(|v| v.is_finite()),
        "baseline returns must be finite"
    );
    let baseline_samples = baseline_returns_bps.len();
    let filtered: Vec<f64> = baseline_returns_bps
        .iter()
        .zip(microstructure_selected.iter())
        .filter_map(|(&r, &keep)| keep.then_some(r))
        .collect();
    let filtered_samples = filtered.len();

    let baseline_mean_bps = (!baseline_returns_bps.is_empty())
        .then_some(baseline_returns_bps.iter().sum::<f64>() / baseline_samples as f64);
    let filtered_mean_bps =
        (!filtered.is_empty()).then_some(filtered.iter().sum::<f64>() / filtered_samples as f64);
    let baseline_hit_rate = (!baseline_returns_bps.is_empty()).then_some(
        baseline_returns_bps.iter().filter(|r| **r > 0.0).count() as f64 / baseline_samples as f64,
    );
    let filtered_hit_rate = (!filtered.is_empty())
        .then_some(filtered.iter().filter(|r| **r > 0.0).count() as f64 / filtered_samples as f64);
    let lift_bps = match (filtered_mean_bps, baseline_mean_bps) {
        (Some(f), Some(b)) => Some(f - b),
        _ => None,
    };

    let verdict = if baseline_samples < 10 || filtered_samples < 5 {
        UtilityVerdict::InconclusiveInsufficientSamples
    } else if lift_bps.is_some_and(|l| l > 0.5) {
        UtilityVerdict::PositiveLift
    } else {
        UtilityVerdict::NegativeOrDegraded
    };

    Ok(MicrostructureUtilityReport {
        baseline_samples,
        filtered_samples,
        baseline_mean_bps,
        filtered_mean_bps,
        lift_bps,
        baseline_hit_rate,
        filtered_hit_rate,
        verdict,
        limitations: vec![
            "Descriptive baseline comparison only; multiple-testing and unseen regime shifts can erode apparent lift".into(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn l2_walk_impact_and_imbalance_respect_real_depth_and_insufficiency() {
        let asks = vec![(100.1, 2.0), (100.5, 4.0)]; // $200.2 + $402.0 = $602.2
        let bids = vec![(99.9, 5.0), (99.5, 5.0)];
        let mid = 100.0;
        let small_impact = estimate_l2_walk_impact_bps(mid, &asks, true, 200.2).unwrap();
        assert!((small_impact - 10.0).abs() < 1e-6);
        let deep_impact = estimate_l2_walk_impact_bps(mid, &asks, true, 401.2).unwrap();
        assert!(deep_impact > small_impact);
        // More notional than book depth -> None rather than guessing linear extrapolation.
        assert!(estimate_l2_walk_impact_bps(mid, &asks, true, 2_000.0).is_none());
        assert!(estimate_l2_walk_impact_bps(mid, &bids, false, 300.0).is_some());
    }

    #[test]
    fn causal_microstructure_never_leaks_future_trades_and_measures_utility() {
        let book = L2BookObservation {
            symbol: "BTC".into(),
            best_bid: 64995.0,
            best_ask: 65005.0,
            mid: 65000.0,
            spread_bps: 1.538,
            top_depth_usd: 130_000.0,
            bid_depth_usd: 300_000.0,
            ask_depth_usd: 100_000.0,
            levels_bid: 2,
            levels_ask: 2,
            bids: vec![(64995.0, 2.0), (64990.0, 2.0)],
            asks: vec![(65005.0, 1.0), (65010.0, 1.0)],
            exchange_ts_ms: Some(1_000_000),
            received_at_ms: 1_000_010,
        };
        let trades = vec![
            TradeObservation {
                symbol: "BTC".into(),
                px: 65005.0,
                sz: 1.0,
                side: "B".into(),
                exchange_ts_ms: 999_900,
                received_at_ms: 999_920,
                tid: 1,
                hash: None,
            },
            // Future trade after as_of_ms must be ignored!
            TradeObservation {
                symbol: "BTC".into(),
                px: 65000.0,
                sz: 10.0,
                side: "A".into(),
                exchange_ts_ms: 1_000_500,
                received_at_ms: 1_000_520,
                tid: 2,
                hash: None,
            },
        ];
        let snap = compute_microstructure_snapshot(
            "BTC",
            1_000_100,
            Some(&book),
            None,
            &trades,
            &[],
            Some(1.0),
            None,
            50_000.0,
        );
        assert!((snap.order_book_imbalance.unwrap() - 0.5).abs() < 1e-6);
        assert!((snap.trade_flow_imbalance.unwrap() - 1.0).abs() < 1e-6);
        assert_eq!(snap.sample_counts.causal_trades, 1);
        assert!(snap.realized_volatility_ann_pct.is_none());

        let base = vec![-2.0; 10]
            .into_iter()
            .chain(vec![5.0; 10])
            .collect::<Vec<_>>();
        let good_mask = vec![false; 10]
            .into_iter()
            .chain(vec![true; 10])
            .collect::<Vec<_>>();
        let pos = evaluate_microstructure_utility(&base, &good_mask).unwrap();
        assert_eq!(pos.verdict, UtilityVerdict::PositiveLift);

        let bad_mask = vec![true; 10]
            .into_iter()
            .chain(vec![false; 10])
            .collect::<Vec<_>>();
        let neg = evaluate_microstructure_utility(&base, &bad_mask).unwrap();
        assert_eq!(neg.verdict, UtilityVerdict::NegativeOrDegraded);
    }

    #[test]
    fn correlation_matrix_aligns_timestamps_accurately() {
        let make = |vals: &[f64]| -> Vec<Candle> {
            vals.iter()
                .enumerate()
                .map(|(i, &c)| Candle {
                    ts: Utc
                        .timestamp_millis_opt(1_700_000_000_000 + i as i64 * 60_000)
                        .unwrap(),
                    open: c,
                    high: c * 1.001,
                    low: c * 0.999,
                    close: c,
                    volume: 100.0,
                })
                .collect()
        };
        let a: Vec<f64> = (0..20)
            .map(|i| 100.0 * (1.01_f64).powi(i) * (1.0 + 0.008 * ((i as f64) * 0.7).sin()))
            .collect();
        let b: Vec<f64> = (0..20)
            .map(|i| 200.0 * (1.01_f64).powi(i) * (1.0 + 0.008 * ((i as f64) * 0.7).sin()))
            .collect();
        let map = BTreeMap::from([("A".into(), make(&a)), ("B".into(), make(&b))]);
        let cm = compute_correlation_matrix(&map, 20).unwrap();
        assert_eq!(cm.correlations[0][0], Some(1.0));
        assert!(cm.correlations[0][1].is_some_and(|c| c > 0.99));
    }
}
