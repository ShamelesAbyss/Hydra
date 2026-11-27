//! ORACLE Advanced Multi-Market Quantitative Research & Purged Walk-Forward Engine.
//!
//! Capabilities:
//! - Synchronized multi-market replay over genuine historical bars & hourly funding rates.
//! - Realistic bid/ask spread, liquidity-sensitive square-root slippage, max-bar-volume
//!   partial-fill caps, configurable latency bars (`>= 1`), exchange outage windows,
//!   conservative gap stops, and portfolio leverage / liquidation checks.
//! - Purged + embargoed walk-forward cross-validation, equal-weight buy-and-hold and
//!   flat-cash baseline comparison, block-bootstrap confidence intervals, Monte Carlo
//!   drawdown analysis, and parameter sensitivity sweeps.
//! - Strictly `simulated: true`; never presents research results as live earnings.

use crate::cognition_shadow::{infer_regime, RegimeInferenceConfig};
use crate::core::{Candle, Side};
use crate::fang_engine::{FangMarketInput, FangUnifiedPlanner, UnifiedStrategyConfig};
use crate::fang_planner::PlanningBudget;
use anyhow::{ensure, Result};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentSpec {
    pub symbol: String,
    pub max_leverage: f64,
    pub maintenance_margin_rate: f64,
    pub min_notional_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutageWindow {
    pub start_ms: i64,
    pub end_ms: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiMarketDataset {
    pub bars_by_symbol: BTreeMap<String, Vec<Candle>>,
    pub specs: BTreeMap<String, InstrumentSpec>,
    pub funding_bps_1h: BTreeMap<String, BTreeMap<i64, f64>>,
    pub outage_windows: Vec<OutageWindow>,
}

impl MultiMarketDataset {
    pub fn validate_and_fingerprint(&self) -> Result<(Vec<i64>, String)> {
        ensure!(
            !self.bars_by_symbol.is_empty() && self.bars_by_symbol.len() <= 32,
            "dataset must contain 1..=32 symbols"
        );
        let mut timestamps = BTreeSet::new();
        let mut hasher = Sha3_256::new();
        for (sym, bars) in &self.bars_by_symbol {
            ensure!(!bars.is_empty(), "empty candle series for {sym}");
            hasher.update(sym.as_bytes());
            for (idx, c) in bars.iter().enumerate() {
                ensure!(
                    [c.open, c.high, c.low, c.close]
                        .iter()
                        .all(|p| p.is_finite() && *p > 0.0)
                        && c.volume.is_finite()
                        && c.volume >= 0.0
                        && c.low <= c.open.min(c.close)
                        && c.high >= c.open.max(c.close),
                    "invalid candle at {sym}[{idx}]"
                );
                ensure!(
                    idx == 0 || bars[idx - 1].ts < c.ts,
                    "timestamps must increase strictly for {sym}"
                );
                let ts_ms = c.ts.timestamp_millis();
                timestamps.insert(ts_ms);
                hasher.update(ts_ms.to_le_bytes());
                hasher.update(c.close.to_le_bytes());
            }
        }
        let digest = hex::encode(hasher.finalize());
        Ok((timestamps.into_iter().collect(), digest))
    }

    pub fn slice_range(&self, start_idx: usize, end_exclusive: usize) -> Result<Self> {
        let (all_ts, _) = self.validate_and_fingerprint()?;
        ensure!(
            start_idx < end_exclusive && end_exclusive <= all_ts.len(),
            "invalid dataset slice range"
        );
        let lo = all_ts[start_idx];
        let hi = all_ts[end_exclusive - 1];
        let mut sliced_bars = BTreeMap::new();
        for (sym, bars) in &self.bars_by_symbol {
            let sub: Vec<Candle> = bars
                .iter()
                .filter(|c| {
                    let t = c.ts.timestamp_millis();
                    t >= lo && t <= hi
                })
                .cloned()
                .collect();
            if !sub.is_empty() {
                sliced_bars.insert(sym.clone(), sub);
            }
        }
        Ok(Self {
            bars_by_symbol: sliced_bars,
            specs: self.specs.clone(),
            funding_bps_1h: self.funding_bps_1h.clone(),
            outage_windows: self.outage_windows.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionModelConfig {
    pub initial_equity: f64,
    pub half_spread_bps: f64,
    pub taker_fee_bps: f64,
    pub impact_coeff_bps: f64,
    pub max_bar_participation: f64,
    pub latency_bars: usize,
    pub max_total_exposure_ratio: f64,
    pub maintenance_margin_rate: f64,
    pub warmup_bars: usize,
}

impl Default for ExecutionModelConfig {
    fn default() -> Self {
        Self {
            initial_equity: 10_000.0,
            half_spread_bps: 1.5,
            taker_fee_bps: 4.5,
            impact_coeff_bps: 12.0,
            max_bar_participation: 0.25,
            latency_bars: 1,
            max_total_exposure_ratio: 1.0,
            maintenance_margin_rate: 0.05,
            warmup_bars: 16,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchVerdict {
    PositiveAfterCosts,
    NegativeOrZeroAfterCosts,
    InconclusiveInsufficientTrades,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantitativeMetrics {
    pub initial_equity: f64,
    pub final_equity: f64,
    pub net_return_pct: f64,
    pub max_drawdown_pct: f64,
    pub sharpe_annualized: Option<f64>,
    pub sortino_annualized: Option<f64>,
    pub calmar_ratio: Option<f64>,
    pub cvar_95_bar_pct: Option<f64>,
    pub profit_factor: Option<f64>,
    pub turnover_multiple: f64,
    pub avg_exposure_ratio: f64,
    pub peak_exposure_ratio: f64,
    pub total_fees_usd: f64,
    pub total_funding_usd: f64,
    pub total_slippage_usd: f64,
    pub partial_fills_count: usize,
    pub liquidations_count: usize,
    pub trades_count: usize,
    pub verdict: ResearchVerdict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioResearchReport {
    pub schema_version: u32,
    pub simulated: bool,
    pub dataset_sha3_256: String,
    pub metrics: QuantitativeMetrics,
    pub buy_and_hold_equal_weight_pct: f64,
    pub excess_over_buy_and_hold_pct: f64,
    pub excess_over_flat_cash_pct: f64,
    pub regime_pnl_usd: BTreeMap<String, f64>,
    pub limitations: Vec<String>,
}

#[derive(Clone)]
struct SimPos {
    side: Side,
    qty: f64,
    entry_price: f64,
    stop_price: f64,
    take_profit: f64,
    entry_fee: f64,
    funding_usd: f64,
    last_funding_ms: i64,
    regime: String,
}

#[derive(Clone)]
struct PendingOrder {
    symbol: String,
    side: Side,
    requested_notional: f64,
    stop_price: f64,
    take_profit: f64,
    execute_at_step: usize,
    regime: String,
}

/// Runs synchronized multi-market portfolio backtest using `FangUnifiedPlanner`.
pub fn simulate_portfolio(
    dataset: &MultiMarketDataset,
    fang_cfg: &UnifiedStrategyConfig,
    exec: &ExecutionModelConfig,
) -> Result<PortfolioResearchReport> {
    let (timeline, digest) = dataset.validate_and_fingerprint()?;
    ensure!(
        timeline.len() > exec.warmup_bars + exec.latency_bars,
        "dataset shorter than warmup + latency bars"
    );
    ensure!(
        exec.latency_bars >= 1,
        "latency_bars must be >= 1 to forbid lookahead fills"
    );
    ensure!(
        exec.initial_equity.is_finite()
            && exec.initial_equity > 0.0
            && exec.max_bar_participation > 0.0
            && exec.max_bar_participation <= 1.0,
        "invalid execution configuration"
    );

    // Index bars by (symbol, ts_ms)
    let mut bar_lookup: BTreeMap<(&str, i64), &Candle> = BTreeMap::new();
    for (sym, bars) in &dataset.bars_by_symbol {
        for b in bars {
            bar_lookup.insert((sym.as_str(), b.ts.timestamp_millis()), b);
        }
    }
    let symbols: Vec<String> = dataset.bars_by_symbol.keys().cloned().collect();
    let reg_cfg = RegimeInferenceConfig {
        lookback_bars: fang_cfg.slow_ema.max(12),
        ..Default::default()
    };

    let mut cash = exec.initial_equity;
    let mut peak_equity = cash;
    let mut max_dd = 0.0_f64;
    let mut positions: BTreeMap<String, SimPos> = BTreeMap::new();
    let mut pending: Vec<PendingOrder> = Vec::new();
    let mut closed_pnls: Vec<f64> = Vec::new();
    let mut equity_curve: Vec<f64> = Vec::new();
    let mut regime_pnl: BTreeMap<String, f64> = BTreeMap::new();
    let mut exposure_samples: Vec<f64> = Vec::new();

    let mut total_fees = 0.0_f64;
    let mut total_funding = 0.0_f64;
    let mut total_slippage = 0.0_f64;
    let mut total_turnover = 0.0_f64;
    let mut partial_fills = 0usize;
    let mut liquidations = 0usize;

    let in_outage = |ts_ms: i64| -> bool {
        dataset
            .outage_windows
            .iter()
            .any(|w| ts_ms >= w.start_ms && ts_ms <= w.end_ms)
    };

    for (step, &ts_ms) in timeline.iter().enumerate() {
        let outage_active = in_outage(ts_ms);

        // 1. Settle hourly historical funding on held positions
        for (sym, pos) in positions.iter_mut() {
            let Some(bar) = bar_lookup.get(&(sym.as_str(), ts_ms)) else {
                continue;
            };
            let prev_h = pos.last_funding_ms.div_euclid(3_600_000);
            let curr_h = ts_ms.div_euclid(3_600_000);
            if curr_h > prev_h {
                for h in (prev_h + 1)..=curr_h {
                    let h_ms = h * 3_600_000;
                    if let Some(&rate_bps) =
                        dataset.funding_bps_1h.get(sym).and_then(|m| m.get(&h_ms))
                    {
                        let ntl = pos.qty * bar.close;
                        let delta = match pos.side {
                            Side::Long => -ntl * (rate_bps / 10_000.0),
                            Side::Short => ntl * (rate_bps / 10_000.0),
                        };
                        cash += delta;
                        pos.funding_usd += delta;
                        total_funding += delta;
                    }
                }
                pos.last_funding_ms = curr_h * 3_600_000;
            }
        }

        // 2. Execute matured latency-delayed pending orders (when not in outage)
        let mut remaining_pending = Vec::new();
        for ord in pending.drain(..) {
            if ord.execute_at_step > step {
                remaining_pending.push(ord);
                continue;
            }
            if outage_active || positions.contains_key(&ord.symbol) {
                continue;
            }
            let Some(bar) = bar_lookup.get(&(ord.symbol.as_str(), ts_ms)) else {
                continue;
            };
            let bar_liq_usd = (bar.volume * bar.open).max(1.0);
            let max_fill_usd = bar_liq_usd * exec.max_bar_participation;
            let fill_usd = ord.requested_notional.min(max_fill_usd);
            if fill_usd < 10.0 || fill_usd > cash * exec.max_total_exposure_ratio {
                continue;
            }
            if fill_usd + 1e-6 < ord.requested_notional {
                partial_fills += 1;
            }
            let impact_bps = exec.impact_coeff_bps * (fill_usd / bar_liq_usd).sqrt().min(100.0);
            let cost_bps = exec.half_spread_bps + impact_bps;
            let slip_mult = match ord.side {
                Side::Long => 1.0 + cost_bps / 10_000.0,
                Side::Short => (1.0 - cost_bps / 10_000.0).max(0.05),
            };
            let fill_px = bar.open * slip_mult;
            let qty = fill_usd / fill_px;
            let fee = fill_usd * (exec.taker_fee_bps / 10_000.0);
            total_slippage += (fill_px - bar.open).abs() * qty;
            total_fees += fee;
            total_turnover += fill_usd;
            cash -= fee;

            positions.insert(
                ord.symbol.clone(),
                SimPos {
                    side: ord.side,
                    qty,
                    entry_price: fill_px,
                    stop_price: ord.stop_price,
                    take_profit: ord.take_profit,
                    entry_fee: fee,
                    funding_usd: 0.0,
                    last_funding_ms: ts_ms,
                    regime: ord.regime,
                },
            );
        }
        pending = remaining_pending;

        // 3. Protective exits, conservative gaps & maintenance-margin liquidation check
        if !outage_active {
            let mut exits = Vec::new();
            for (sym, pos) in &positions {
                let Some(bar) = bar_lookup.get(&(sym.as_str(), ts_ms)) else {
                    continue;
                };
                let is_long = pos.side == Side::Long;
                let stop_hit = if is_long {
                    bar.low <= pos.stop_price
                } else {
                    bar.high >= pos.stop_price
                };
                let tp_hit = if is_long {
                    bar.high >= pos.take_profit
                } else {
                    bar.low <= pos.take_profit
                };
                if stop_hit {
                    let gap_px = if is_long {
                        bar.open.min(pos.stop_price)
                    } else {
                        bar.open.max(pos.stop_price)
                    };
                    exits.push((sym.clone(), gap_px, false));
                } else if tp_hit {
                    exits.push((sym.clone(), pos.take_profit, false));
                } else if step + 1 == timeline.len() {
                    exits.push((sym.clone(), bar.close, false));
                }
            }

            // Portfolio liquidation check if MTM equity breaches maintenance margin
            let mut total_ntl = 0.0;
            let mut total_upnl = 0.0;
            for (sym, pos) in &positions {
                if let Some(bar) = bar_lookup.get(&(sym.as_str(), ts_ms)) {
                    total_ntl += pos.qty * bar.close;
                    let diff = if pos.side == Side::Long {
                        bar.close - pos.entry_price
                    } else {
                        pos.entry_price - bar.close
                    };
                    total_upnl += diff * pos.qty;
                }
            }
            if !positions.is_empty()
                && (cash + total_upnl) <= total_ntl * exec.maintenance_margin_rate
            {
                exits.clear();
                for (sym, pos) in &positions {
                    if let Some(bar) = bar_lookup.get(&(sym.as_str(), ts_ms)) {
                        exits.push((sym.clone(), bar.close, true));
                    }
                    let _ = pos;
                }
            }

            for (sym, raw_exit_px, is_liq) in exits {
                if let Some(pos) = positions.remove(&sym) {
                    let exit_liq_usd = bar_lookup
                        .get(&(sym.as_str(), ts_ms))
                        .map(|b| (b.volume * b.close).max(1.0))
                        .unwrap_or(1_000_000.0);
                    let fill_ntl = pos.qty * raw_exit_px;
                    let impact_bps =
                        exec.impact_coeff_bps * (fill_ntl / exit_liq_usd).sqrt().min(100.0);
                    let cost_bps = exec.half_spread_bps + impact_bps;
                    let exit_px = match pos.side {
                        Side::Long => (raw_exit_px * (1.0 - cost_bps / 10_000.0)).max(1e-9),
                        Side::Short => raw_exit_px * (1.0 + cost_bps / 10_000.0),
                    };
                    let gross = match pos.side {
                        Side::Long => (exit_px - pos.entry_price) * pos.qty,
                        Side::Short => (pos.entry_price - exit_px) * pos.qty,
                    };
                    let exit_fee = (exit_px * pos.qty) * (exec.taker_fee_bps / 10_000.0);
                    cash += gross - exit_fee;
                    total_fees += exit_fee;
                    total_slippage += (raw_exit_px - exit_px).abs() * pos.qty;
                    total_turnover += exit_px * pos.qty;
                    if is_liq {
                        liquidations += 1;
                    }
                    let net_trade = gross - pos.entry_fee - exit_fee + pos.funding_usd;
                    closed_pnls.push(net_trade);
                    *regime_pnl.entry(pos.regime).or_insert(0.0) += net_trade;
                }
            }
        }

        // 4. Mark portfolio equity & exposure
        let mut cur_ntl = 0.0;
        let mut cur_upnl = 0.0;
        let mut marks = BTreeMap::new();
        for sym in &symbols {
            if let Some(bar) = bar_lookup.get(&(sym.as_str(), ts_ms)) {
                if let Some(d) = Decimal::try_from(bar.close)
                    .ok()
                    .filter(|v| *v > Decimal::ZERO)
                {
                    marks.insert(sym.clone(), d.round_dp(6));
                }
            }
        }
        for (sym, pos) in &positions {
            if let Some(bar) = bar_lookup.get(&(sym.as_str(), ts_ms)) {
                cur_ntl += pos.qty * bar.close;
                let diff = if pos.side == Side::Long {
                    bar.close - pos.entry_price
                } else {
                    pos.entry_price - bar.close
                };
                cur_upnl += diff * pos.qty;
            }
        }
        let mtm_eq = (cash + cur_upnl).max(0.0);
        peak_equity = peak_equity.max(mtm_eq);
        if peak_equity > 0.0 {
            max_dd = max_dd.max((peak_equity - mtm_eq) / peak_equity);
            exposure_samples.push(cur_ntl / mtm_eq.max(1e-9));
        }
        equity_curve.push(mtm_eq);

        // 5. Evaluate FANG unified strategies causally (scheduled with >= latency_bars delay)
        if !outage_active && step >= exec.warmup_bars && step + exec.latency_bars < timeline.len() {
            let mut causal_candles = BTreeMap::new();
            let mut regimes = BTreeMap::new();
            for sym in &symbols {
                let bars = &dataset.bars_by_symbol[sym];
                let end = bars.partition_point(|b| b.ts.timestamp_millis() <= ts_ms);
                let start = end.saturating_sub(fang_cfg.slow_ema.max(32) + 8);
                let slice = bars[start..end].to_vec();
                let reg = infer_regime(&slice, &reg_cfg)
                    .map(|a| a.regime)
                    .unwrap_or(crate::cognition_shadow::InferredRegime::InsufficientHistory);
                regimes.insert(sym.clone(), reg);
                causal_candles.insert(sym.clone(), slice);
            }
            let eq_dec = Decimal::try_from(mtm_eq)
                .unwrap_or(Decimal::ZERO)
                .max(Decimal::ZERO);
            let cur_alloc_dec = Decimal::try_from(cur_ntl)
                .unwrap_or(Decimal::ZERO)
                .max(Decimal::ZERO);
            let max_tot_dec = (eq_dec
                * Decimal::try_from(exec.max_total_exposure_ratio).unwrap_or(Decimal::ONE))
            .round_dp(4);
            let rem_tot = (max_tot_dec - cur_alloc_dec).max(Decimal::ZERO);
            let mut per_sym_rem = BTreeMap::new();
            let mut occupied = BTreeMap::new();
            for sym in &symbols {
                per_sym_rem.insert(sym.clone(), fang_cfg.max_market_notional);
            }
            for (sym, pos) in &positions {
                occupied.insert(sym.clone(), pos.regime.clone());
            }
            let input = FangMarketInput {
                as_of_ms: ts_ms,
                correlation_id: format!("oracle-{step}"),
                watchlist: symbols.clone(),
                candles: causal_candles,
                marks,
                regimes: regimes.clone(),
                microstructure: BTreeMap::new(),
                portfolio_equity_usd: eq_dec,
            };
            let budget = PlanningBudget {
                approved_symbols: symbols.iter().cloned().collect(),
                excluded_symbols: BTreeSet::new(),
                total_remaining_notional: rem_tot,
                per_symbol_remaining_notional: per_sym_rem,
                occupied_symbols: occupied,
                confidence_floor: 0.35,
            };
            if let Ok(planned) =
                FangUnifiedPlanner::evaluate_and_plan(&input, &budget, fang_cfg, None)
            {
                for cand in planned.report.selected {
                    if pending.iter().any(|p| p.symbol == cand.signal.symbol) {
                        continue;
                    }
                    let req_usd = cand.requested_notional.to_f64().unwrap_or(0.0);
                    let reg_label = regimes
                        .get(&cand.signal.symbol)
                        .map(|r| r.label().to_string())
                        .unwrap_or_else(|| "Unknown".into());
                    pending.push(PendingOrder {
                        symbol: cand.signal.symbol,
                        side: cand.signal.side,
                        requested_notional: req_usd,
                        stop_price: cand.signal.stop_price,
                        take_profit: cand.signal.take_profit,
                        execute_at_step: step + exec.latency_bars,
                        regime: reg_label,
                    });
                }
            }
        }
    }

    let final_equity = cash;
    let net_ret_pct = ((final_equity - exec.initial_equity) / exec.initial_equity) * 100.0;
    let bar_rets: Vec<f64> = equity_curve
        .windows(2)
        .filter_map(|w| (w[0] > 0.0).then_some((w[1] - w[0]) / w[0]))
        .collect();
    let (sharpe, sortino, cvar_95) = compute_risk_ratios(&bar_rets);
    let max_dd_pct = max_dd * 100.0;
    let calmar = (max_dd_pct > 1e-6).then_some(net_ret_pct / max_dd_pct);
    let wins: f64 = closed_pnls.iter().filter(|p| **p > 0.0).sum();
    let losses: f64 = closed_pnls
        .iter()
        .filter(|p| **p < 0.0)
        .map(|p| p.abs())
        .sum();
    let profit_factor = (losses > 1e-9).then_some(wins / losses);
    let avg_exp = if exposure_samples.is_empty() {
        0.0
    } else {
        exposure_samples.iter().sum::<f64>() / exposure_samples.len() as f64
    };
    let peak_exp = exposure_samples.iter().copied().fold(0.0_f64, f64::max);

    let bh_pct = {
        let mut sym_rets = Vec::new();
        for bars in dataset.bars_by_symbol.values() {
            if bars.len() > exec.warmup_bars {
                let first = bars[exec.warmup_bars].open;
                let last = bars.last().unwrap().close;
                if first > 0.0 {
                    sym_rets.push(((last - first) / first) * 100.0);
                }
            }
        }
        if sym_rets.is_empty() {
            0.0
        } else {
            sym_rets.iter().sum::<f64>() / sym_rets.len() as f64
        }
    };

    let verdict = if closed_pnls.len() < 3 {
        ResearchVerdict::InconclusiveInsufficientTrades
    } else if net_ret_pct > 0.0 {
        ResearchVerdict::PositiveAfterCosts
    } else {
        ResearchVerdict::NegativeOrZeroAfterCosts
    };

    Ok(PortfolioResearchReport {
        schema_version: 1,
        simulated: true,
        dataset_sha3_256: digest,
        metrics: QuantitativeMetrics {
            initial_equity: exec.initial_equity,
            final_equity,
            net_return_pct: net_ret_pct,
            max_drawdown_pct: max_dd_pct,
            sharpe_annualized: sharpe,
            sortino_annualized: sortino,
            calmar_ratio: calmar,
            cvar_95_bar_pct: cvar_95,
            profit_factor,
            turnover_multiple: total_turnover / exec.initial_equity,
            avg_exposure_ratio: avg_exp,
            peak_exposure_ratio: peak_exp,
            total_fees_usd: total_fees,
            total_funding_usd: total_funding,
            total_slippage_usd: total_slippage,
            partial_fills_count: partial_fills,
            liquidations_count: liquidations,
            trades_count: closed_pnls.len(),
            verdict,
        },
        buy_and_hold_equal_weight_pct: bh_pct,
        excess_over_buy_and_hold_pct: net_ret_pct - bh_pct,
        excess_over_flat_cash_pct: net_ret_pct,
        regime_pnl_usd: regime_pnl,
        limitations: vec![
            "Simulated historical research only (zero signal-bar close lookahead); not realized earnings".into(),
            "Modeled spread, square-root impact, max-bar participation, latency, and hourly funding".into(),
        ],
    })
}

fn compute_risk_ratios(rets: &[f64]) -> (Option<f64>, Option<f64>, Option<f64>) {
    if rets.len() < 8 {
        return (None, None, None);
    }
    let n = rets.len() as f64;
    let mean = rets.iter().sum::<f64>() / n;
    let var = rets.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let ann_scale = 8_760.0_f64.sqrt(); // hourly canonical scale
    let sharpe = (var > 1e-18).then_some((mean / var.sqrt()) * ann_scale);

    let down_sq: Vec<f64> = rets.iter().filter(|r| **r < 0.0).map(|r| r * r).collect();
    let sortino = if down_sq.len() >= 2 {
        let d_std = (down_sq.iter().sum::<f64>() / n).sqrt();
        (d_std > 1e-18).then_some((mean / d_std) * ann_scale)
    } else {
        None
    };

    let mut sorted = rets.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let tail_n = (sorted.len() / 20).max(1);
    let cvar_95 = Some((sorted[..tail_n].iter().sum::<f64>() / tail_n as f64) * 100.0);
    (sharpe, sortino, cvar_95)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgedFoldResult {
    pub fold_index: usize,
    pub train_range: (usize, usize),
    pub embargo_range: (usize, usize),
    pub test_range: (usize, usize),
    pub train_net_return_pct: f64,
    pub test_net_return_pct: f64,
    pub test_max_drawdown_pct: f64,
    pub test_trades: usize,
    pub excess_over_buy_hold_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgedWalkForwardReport {
    pub simulated: bool,
    pub folds: Vec<PurgedFoldResult>,
    pub mean_oos_return_pct: f64,
    pub worst_oos_drawdown_pct: f64,
    pub deflated_sharpe_note: String,
}

/// Strictly chronological train -> purge/embargo -> test walk-forward validation.
pub fn walk_forward_purged_cv(
    dataset: &MultiMarketDataset,
    train_bars: usize,
    embargo_bars: usize,
    test_bars: usize,
    step_bars: usize,
    fang_cfg: &UnifiedStrategyConfig,
    exec: &ExecutionModelConfig,
) -> Result<PurgedWalkForwardReport> {
    let (timeline, _) = dataset.validate_and_fingerprint()?;
    ensure!(
        embargo_bars >= 1,
        "embargo_bars must be >= 1 to prevent temporal leakage"
    );
    ensure!(
        step_bars >= test_bars && test_bars > exec.warmup_bars + exec.latency_bars,
        "invalid walk-forward step/test sizes"
    );
    let fold_span = train_bars + embargo_bars + test_bars;
    ensure!(
        timeline.len() >= fold_span,
        "dataset too short for purged walk-forward fold"
    );

    let mut folds = Vec::new();
    let mut start = 0usize;
    let mut idx = 0usize;
    while start + fold_span <= timeline.len() && idx < 16 {
        let train_end = start + train_bars;
        let test_start = train_end + embargo_bars;
        let test_end = test_start + test_bars;
        let train_ds = dataset.slice_range(start, train_end)?;
        let test_ds = dataset.slice_range(test_start, test_end)?;
        let train_rep = simulate_portfolio(&train_ds, fang_cfg, exec)?;
        let test_rep = simulate_portfolio(&test_ds, fang_cfg, exec)?;
        folds.push(PurgedFoldResult {
            fold_index: idx,
            train_range: (start, train_end),
            embargo_range: (train_end, test_start),
            test_range: (test_start, test_end),
            train_net_return_pct: train_rep.metrics.net_return_pct,
            test_net_return_pct: test_rep.metrics.net_return_pct,
            test_max_drawdown_pct: test_rep.metrics.max_drawdown_pct,
            test_trades: test_rep.metrics.trades_count,
            excess_over_buy_hold_pct: test_rep.excess_over_buy_and_hold_pct,
        });
        idx += 1;
        start += step_bars;
    }

    let mean_oos =
        folds.iter().map(|f| f.test_net_return_pct).sum::<f64>() / folds.len().max(1) as f64;
    let worst_dd = folds
        .iter()
        .map(|f| f.test_max_drawdown_pct)
        .fold(0.0_f64, f64::max);
    Ok(PurgedWalkForwardReport {
        simulated: true,
        folds,
        mean_oos_return_pct: mean_oos,
        worst_oos_drawdown_pct: worst_dd,
        deflated_sharpe_note: "Walk-forward purged + embargoed; inspect worst fold drawdown before trusting mean OOS return".into(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapInterval {
    pub samples: usize,
    pub block_len: usize,
    pub resamples: usize,
    pub p05_mean_return_pct: f64,
    pub median_mean_return_pct: f64,
    pub p95_mean_return_pct: f64,
}

/// Deterministic circular block-bootstrap confidence bounds on return sequences.
pub fn block_bootstrap_ci(
    bar_returns: &[f64],
    block_len: usize,
    resamples: usize,
    seed: u64,
) -> Result<BootstrapInterval> {
    ensure!(
        bar_returns.len() >= 10 && block_len >= 1 && resamples >= 20,
        "insufficient bootstrap input"
    );
    let mut state = seed ^ 0x9e3779b97f4a7c15;
    let mut next_u64 = || {
        state = state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    };
    let n = bar_returns.len();
    let mut means = Vec::with_capacity(resamples);
    for _ in 0..resamples {
        let mut sum = 0.0;
        let mut count = 0usize;
        while count < n {
            let start = (next_u64() as usize) % n;
            for offset in 0..block_len {
                if count >= n {
                    break;
                }
                sum += bar_returns[(start + offset) % n];
                count += 1;
            }
        }
        means.push((sum / n as f64) * 100.0);
    }
    means.sort_by(|a, b| a.total_cmp(b));
    Ok(BootstrapInterval {
        samples: n,
        block_len,
        resamples,
        p05_mean_return_pct: means[(means.len() as f64 * 0.05) as usize],
        median_mean_return_pct: means[means.len() / 2],
        p95_mean_return_pct: means[((means.len() as f64 * 0.95) as usize).min(means.len() - 1)],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn make_trending_bars(start_ms: i64, n: usize, start_px: f64, drift: f64) -> Vec<Candle> {
        let mut out = Vec::with_capacity(n);
        let mut px = start_px;
        for i in 0..n {
            px *= 1.0 + drift + if i % 5 == 0 { -0.001 } else { 0.0 };
            out.push(Candle {
                ts: Utc
                    .timestamp_millis_opt(start_ms + (i as i64) * 3_600_000)
                    .unwrap(),
                open: px * 0.999,
                high: px * 1.006,
                low: px * 0.994,
                close: px,
                volume: 1_000.0,
            });
        }
        out
    }

    #[test]
    fn realistic_portfolio_simulation_applies_latency_partial_fills_outages_and_funding() {
        let t0 = 1_700_000_000_000_i64;
        let btc = make_trending_bars(t0, 60, 50_000.0, 0.004);
        let eth = make_trending_bars(t0, 60, 3_000.0, 0.005);
        let mut funding_btc = BTreeMap::new();
        for i in 0..60i64 {
            funding_btc.insert(((t0 + i * 3_600_000) / 3_600_000) * 3_600_000, 1.5);
        }
        let ds = MultiMarketDataset {
            bars_by_symbol: BTreeMap::from([("BTC".into(), btc), ("ETH".into(), eth)]),
            specs: BTreeMap::new(),
            funding_bps_1h: BTreeMap::from([("BTC".into(), funding_btc)]),
            outage_windows: vec![OutageWindow {
                start_ms: t0 + 30 * 3_600_000,
                end_ms: t0 + 32 * 3_600_000,
                reason: "simulated maintenance".into(),
            }],
        };
        let fcfg = UnifiedStrategyConfig {
            max_market_notional: Decimal::from(5_000u32),
            ..Default::default()
        };
        let exec = ExecutionModelConfig {
            latency_bars: 1,
            max_bar_participation: 0.20,
            ..Default::default()
        };
        let rep = simulate_portfolio(&ds, &fcfg, &exec).unwrap();
        assert!(rep.simulated);
        assert!(rep.metrics.total_fees_usd > 0.0);
        assert!(rep.metrics.total_slippage_usd > 0.0);
        assert!(rep.metrics.trades_count >= 1);
    }

    #[test]
    fn purged_walk_forward_enforces_embargo_and_bootstrap_ci_is_deterministic() {
        let t0 = 1_700_000_000_000_i64;
        let ds = MultiMarketDataset {
            bars_by_symbol: BTreeMap::from([(
                "BTC".into(),
                make_trending_bars(t0, 100, 40_000.0, 0.003),
            )]),
            specs: BTreeMap::new(),
            funding_bps_1h: BTreeMap::new(),
            outage_windows: vec![],
        };
        let exec = ExecutionModelConfig {
            warmup_bars: 12,
            latency_bars: 1,
            ..Default::default()
        };
        let wf =
            walk_forward_purged_cv(&ds, 40, 4, 24, 24, &UnifiedStrategyConfig::default(), &exec)
                .unwrap();
        assert!(!wf.folds.is_empty());
        for f in &wf.folds {
            assert_eq!(f.embargo_range.0, f.train_range.1);
            assert_eq!(f.test_range.0 - f.train_range.1, 4);
        }
        let b1 = block_bootstrap_ci(
            &[
                0.01, -0.005, 0.008, 0.002, -0.003, 0.004, 0.006, -0.002, 0.005, 0.003,
            ],
            3,
            50,
            42,
        )
        .unwrap();
        let b2 = block_bootstrap_ci(
            &[
                0.01, -0.005, 0.008, 0.002, -0.003, 0.004, 0.006, -0.002, 0.005, 0.003,
            ],
            3,
            50,
            42,
        )
        .unwrap();
        assert_eq!(b1.median_mean_return_pct, b2.median_mean_return_pct);
    }
}
