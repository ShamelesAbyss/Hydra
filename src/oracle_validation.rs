//! Bounded rolling held-out research. Parameters remain fixed; no model is trained.
use crate::{
    config::AppConfig,
    core::Candle,
    oracle::{self, ReplayOptions, ReplayReport},
    strategies::make_strategies,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostScenario {
    pub name: String,
    pub fee_bps: f64,
    pub slippage_bps: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::{MarketSnapshot, Regime, Side, Signal},
        strategies::Strategy,
    };
    use chrono::{TimeZone, Utc};
    use std::sync::atomic::{AtomicBool, Ordering};

    fn config() -> AppConfig {
        let mut c = AppConfig::load_research(
            concat!(env!("CARGO_MANIFEST_DIR"), "/config/strategies.toml"),
            None,
        )
        .unwrap();
        c.strategies.global.regime_required = false;
        c
    }
    fn bars(count: usize) -> Vec<Candle> {
        (0..count)
            .map(|i| Candle {
                ts: Utc
                    .timestamp_millis_opt(1_700_000_000_000 + i as i64 * 60_000)
                    .unwrap(),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 1.0,
            })
            .collect()
    }
    fn options() -> ValidationOptions {
        ValidationOptions {
            history_bars: 3,
            test_bars: 4,
            step_bars: 4,
            max_folds: 3,
            replay: ReplayOptions {
                max_trades: 10,
                ..Default::default()
            },
            ..Default::default()
        }
    }
    struct Once(AtomicBool);
    impl Strategy for Once {
        fn name(&self) -> &'static str {
            "validation_test"
        }
        fn evaluate(&self, s: &MarketSnapshot, _: Regime, _: &AppConfig) -> Option<Signal> {
            assert!(s.candles.iter().all(|c| c.ts <= s.last_update));
            if self.0.swap(true, Ordering::SeqCst) {
                return None;
            }
            Some(Signal {
                strategy: self.name().into(),
                venue: s.venue.clone(),
                symbol: s.symbol.clone(),
                side: Side::Long,
                confidence: 1.0,
                expected_edge_bps: 100.0,
                stop_price: 90.0,
                take_profit: 120.0,
                reason: "offline fixture".into(),
                created_at: s.last_update,
            })
        }
    }
    fn synthetic(c: &AppConfig, b: &[Candle], o: &ReplayOptions) -> Result<ReplayReport> {
        oracle::replay_with_strategies(c, b, o, &[Box::new(Once(AtomicBool::new(false)))])
    }
    #[test]
    fn repeatable_public_evaluation_and_exact_disjoint_ranges() {
        let c = config();
        let b = bars(18);
        let o = options();
        let a = evaluate(&c, &b, &o).unwrap();
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::to_value(evaluate(&c, &b, &o).unwrap()).unwrap()
        );
        assert!(a.simulated);
        assert_eq!(a.unused_trailing_bars, 3);
        let folds = &a.scenarios[0].folds;
        assert_eq!(folds.len(), 3);
        for (i, f) in folds.iter().enumerate() {
            assert_eq!(f.test_start_index, 3 + i * 4);
            assert_eq!(f.test_end_index_exclusive, 7 + i * 4);
            assert_eq!(f.replay.equity.len(), 4);
            assert!(f
                .replay
                .equity
                .iter()
                .all(|p| p.time_ms >= f.test_start_ms && p.time_ms <= f.test_end_ms));
            assert_eq!(f.replay.initial_equity, o.replay.initial_equity);
        }
    }
    #[test]
    fn fresh_state_cost_sensitivity_and_forced_fold_close() {
        let r = evaluate_using(&config(), &bars(15), &options(), synthetic).unwrap();
        for s in &r.scenarios {
            assert_eq!(s.summary.trade_count, 3);
            for f in &s.folds {
                let t = &f.replay.trades[0];
                assert_eq!(t.signal_time_ms, f.test_start_ms);
                assert!(t.entry_time_ms > f.test_start_ms);
                assert_eq!(t.exit_time_ms, f.test_end_ms);
                assert_eq!(t.exit_reason, "end_of_data");
            }
            assert!(
                (s.summary.pooled_net_pnl - s.summary.mean_fold_return * 3.0 * 1000.0).abs() < 1e-8
            );
        }
        assert!(r.scenarios[1].summary.pooled_net_pnl < r.scenarios[0].summary.pooled_net_pnl);
        assert!(r.scenarios[1].summary.pooled_fees > r.scenarios[0].summary.pooled_fees);
    }
    #[test]
    fn future_prices_cannot_change_an_earlier_fold() {
        let c = config();
        let o = options();
        let mut b = bars(15);
        let before = evaluate_using(&c, &b, &o, synthetic).unwrap();
        for bar in &mut b[7..] {
            bar.open *= 2.0;
            bar.high *= 2.0;
            bar.low *= 2.0;
            bar.close *= 2.0;
        }
        let after = evaluate_using(&c, &b, &o, synthetic).unwrap();
        for (a, z) in before.scenarios.iter().zip(after.scenarios.iter()) {
            assert_eq!(
                serde_json::to_value(&a.folds[0]).unwrap(),
                serde_json::to_value(&z.folds[0]).unwrap()
            );
        }
    }
    #[test]
    fn invalid_overlap_and_costs_are_rejected_before_replay() {
        let c = config();
        let b = bars(15);
        let mut o = options();
        o.step_bars = 3;
        assert!(evaluate(&c, &b, &o).is_err());
        o = options();
        o.scenarios[0].fee_bps = f64::NAN;
        assert!(evaluate(&c, &b, &o).is_err());
        o = options();
        o.scenarios[1].name = o.scenarios[0].name.clone();
        assert!(evaluate(&c, &b, &o).is_err());
        o = options();
        o.history_bars = usize::MAX;
        assert!(evaluate(&c, &b, &o).is_err());
    }
    #[test]
    fn aggregate_compute_memory_and_call_limits_are_enforced() {
        let mut c = config();
        let b = bars(20_000);
        let mut o = options();
        o.history_bars = 10_000;
        o.test_bars = 10_000;
        o.step_bars = 10_000;
        c.strategies.global.lookback_candles = 10_000;
        assert!(evaluate(&c, &b, &o)
            .unwrap_err()
            .to_string()
            .contains("evaluation budget"));
        o = options();
        o.max_folds = 32;
        o.scenarios = (0..8)
            .map(|i| CostScenario {
                name: i.to_string(),
                fee_bps: 0.0,
                slippage_bps: 0.0,
            })
            .collect();
        assert!(evaluate(&c, &b, &o)
            .unwrap_err()
            .to_string()
            .contains("combined"));
        o = options();
        o.max_folds = 32;
        o.replay.max_trades = 100_000;
        assert!(evaluate(&c, &b, &o)
            .unwrap_err()
            .to_string()
            .contains("memory"));
    }
    #[test]
    fn rejects_global_time_reversal_and_unknown_option_keys() {
        let mut b = bars(15);
        b[10].ts = b[9].ts;
        assert!(evaluate(&config(), &b, &options()).is_err());
        let mut value = serde_json::to_value(options()).unwrap();
        value["train_and_deploy"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ValidationOptions>(value).is_err());
    }

    #[test]
    fn dataset_integrity_audit_detects_gaps_ohlc_inversions_and_produces_stable_sha256() {
        let clean = bars(20);
        let a = audit_candle_dataset(&clean, 60_000).unwrap();
        let b = audit_candle_dataset(&clean, 60_000).unwrap();
        assert!(a.integrity_ok);
        assert_eq!(a.missing_interval_gaps, 0);
        assert_eq!(a.invalid_ohlc_bars, 0);
        assert_eq!(a.dataset_sha256, b.dataset_sha256);
        assert_eq!(a.dataset_sha256.len(), 64);

        // Inject a missing-bar gap and an OHLC inversion (high < low).
        let mut dirty = bars(10);
        dirty[5].ts = dirty[4].ts + chrono::Duration::minutes(5);
        dirty[7].high = dirty[7].low - 1.0;
        let rep = audit_candle_dataset(&dirty, 60_000).unwrap();
        assert!(!rep.integrity_ok);
        assert_eq!(rep.missing_interval_gaps, 1);
        assert_eq!(rep.invalid_ohlc_bars, 1);
        assert_ne!(rep.dataset_sha256, a.dataset_sha256);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationOptions {
    /// Prior indicator history, not a parameter-fitting or training interval.
    pub history_bars: usize,
    pub test_bars: usize,
    pub step_bars: usize,
    pub max_folds: usize,
    /// warmup_bars is replaced by history_bars; costs by each scenario.
    pub replay: ReplayOptions,
    pub scenarios: Vec<CostScenario>,
}
impl Default for ValidationOptions {
    fn default() -> Self {
        Self {
            history_bars: 240,
            test_bars: 240,
            step_bars: 240,
            max_folds: 8,
            replay: ReplayOptions::default(),
            scenarios: vec![
                CostScenario {
                    name: "baseline".into(),
                    fee_bps: 4.5,
                    slippage_bps: 5.0,
                },
                CostScenario {
                    name: "higher_cost".into(),
                    fee_bps: 9.0,
                    slippage_bps: 10.0,
                },
            ],
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct ValidationFold {
    pub index: usize,
    /// Dataset indices use half-open intervals [start, end).
    pub history_start_index: usize,
    pub test_start_index: usize,
    pub test_end_index_exclusive: usize,
    pub history_start_ms: i64,
    pub history_end_ms: i64,
    pub test_start_ms: i64,
    pub test_end_ms: i64,
    pub replay: ReplayReport,
}
#[derive(Clone, Debug, Serialize)]
pub struct ScenarioSummary {
    pub fold_count: usize,
    pub trade_count: usize,
    pub profitable_folds: usize,
    /// Arithmetic mean of independent fold returns; never compounded.
    pub mean_fold_return: f64,
    pub worst_fold_return: f64,
    pub worst_fold_drawdown_fraction: f64,
    /// Sum of independently reset fold net outcomes, not a continuous portfolio.
    pub pooled_net_pnl: f64,
    pub pooled_fees: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct ScenarioReport {
    pub scenario: CostScenario,
    pub summary: ScenarioSummary,
    pub folds: Vec<ValidationFold>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ValidationReport {
    pub schema_version: u32,
    pub simulated: bool,
    pub method: String,
    pub options: ValidationOptions,
    pub input_bar_count: usize,
    pub estimated_history_evaluations: usize,
    pub unused_trailing_bars: usize,
    pub scenarios: Vec<ScenarioReport>,
    pub limitations: Vec<String>,
}

pub fn evaluate(
    cfg: &AppConfig,
    candles: &[Candle],
    options: &ValidationOptions,
) -> Result<ValidationReport> {
    evaluate_using(cfg, candles, options, oracle::replay)
}

fn evaluate_using<F>(
    cfg: &AppConfig,
    candles: &[Candle],
    o: &ValidationOptions,
    mut run: F,
) -> Result<ValidationReport>
where
    F: FnMut(&AppConfig, &[Candle], &ReplayOptions) -> Result<ReplayReport>,
{
    ensure!(
        o.history_bars > 0 && o.test_bars >= 2,
        "positive history and at least two test bars required"
    );
    ensure!(
        o.step_bars >= o.test_bars,
        "test intervals must not overlap"
    );
    ensure!((1..=32).contains(&o.max_folds), "fold budget must be 1..32");
    ensure!(
        (1..=8).contains(&o.scenarios.len()),
        "scenario budget must be 1..8"
    );
    ensure!(
        o.replay.max_bars > 0
            && o.replay.max_bars <= 1_000_000
            && candles.len() <= o.replay.max_bars,
        "input bar budget exceeded"
    );
    ensure!(
        (1..=100_000).contains(&o.replay.max_trades),
        "invalid per-fold trade budget"
    );
    let window = o
        .history_bars
        .checked_add(o.test_bars)
        .filter(|v| *v <= candles.len())
        .ok_or_else(|| anyhow::anyhow!("insufficient history/test data"))?;
    let fold_count = (1 + (candles.len() - window) / o.step_bars).min(o.max_folds);
    let calls = fold_count
        .checked_mul(o.scenarios.len())
        .unwrap_or(usize::MAX);
    ensure!(calls <= 64, "combined fold/scenario budget exceeded");
    let entries = o
        .test_bars
        .checked_add(o.replay.max_trades)
        .and_then(|v| v.checked_mul(calls));
    ensure!(
        entries.is_some_and(|v| v <= 1_000_000),
        "aggregate equity/trade memory budget exceeded"
    );
    let lookback = cfg.strategies.global.lookback_candles;
    ensure!(
        (1..=10_000).contains(&lookback),
        "invalid lookback resource budget"
    );
    let strategy_count = make_strategies(&cfg.strategies)?.len();
    let work = o
        .test_bars
        .checked_mul(lookback.min(window))
        .and_then(|v| v.checked_mul(strategy_count))
        .and_then(|v| v.checked_mul(calls));
    ensure!(
        work.is_some_and(|v| v <= 100_000_000),
        "aggregate history evaluation budget exceeded"
    );
    for (i, scenario) in o.scenarios.iter().enumerate() {
        ensure!(
            !scenario.name.trim().is_empty() && scenario.name.len() <= 64,
            "invalid scenario name"
        );
        ensure!(
            !o.scenarios[..i].iter().any(|s| s.name == scenario.name),
            "duplicate scenario name"
        );
        ensure!(
            [scenario.fee_bps, scenario.slippage_bps]
                .iter()
                .all(|v| v.is_finite() && (0.0..=1000.0).contains(v)),
            "invalid scenario costs"
        );
    }
    // Check ordering across even the gaps between windows, without feeding future bars to a replay.
    ensure!(
        candles.windows(2).all(|w| w[0].ts < w[1].ts),
        "timestamps must increase strictly"
    );
    let mut scenarios = Vec::with_capacity(o.scenarios.len());
    for scenario in &o.scenarios {
        let mut folds = Vec::with_capacity(fold_count);
        for index in 0..fold_count {
            let start = index * o.step_bars;
            let test_start = start + o.history_bars;
            let end = start + window;
            let mut replay_options = o.replay.clone();
            replay_options.warmup_bars = o.history_bars;
            replay_options.fee_bps = scenario.fee_bps;
            replay_options.slippage_bps = scenario.slippage_bps;
            // replay constructs fresh strategies, cash and positions on every call.
            let report = run(cfg, &candles[start..end], &replay_options)?;
            folds.push(ValidationFold {
                index,
                history_start_index: start,
                test_start_index: test_start,
                test_end_index_exclusive: end,
                history_start_ms: candles[start].ts.timestamp_millis(),
                history_end_ms: candles[test_start - 1].ts.timestamp_millis(),
                test_start_ms: candles[test_start].ts.timestamp_millis(),
                test_end_ms: candles[end - 1].ts.timestamp_millis(),
                replay: report,
            });
        }
        let returns: Vec<f64> = folds
            .iter()
            .map(|f| (f.replay.final_equity - f.replay.initial_equity) / f.replay.initial_equity)
            .collect();
        let summary = ScenarioSummary {
            fold_count,
            trade_count: folds.iter().map(|f| f.replay.trades.len()).sum(),
            profitable_folds: returns.iter().filter(|v| **v > 0.0).count(),
            mean_fold_return: returns.iter().sum::<f64>() / fold_count as f64,
            worst_fold_return: returns.iter().copied().fold(f64::INFINITY, f64::min),
            worst_fold_drawdown_fraction: folds
                .iter()
                .map(|f| f.replay.max_drawdown_fraction)
                .fold(0.0, f64::max),
            pooled_net_pnl: folds
                .iter()
                .map(|f| f.replay.final_equity - f.replay.initial_equity)
                .sum(),
            pooled_fees: folds.iter().map(|f| f.replay.total_fees).sum(),
        };
        ensure!(
            [
                summary.mean_fold_return,
                summary.worst_fold_return,
                summary.worst_fold_drawdown_fraction,
                summary.pooled_net_pnl,
                summary.pooled_fees
            ]
            .iter()
            .all(|v| v.is_finite()),
            "aggregate accounting overflow"
        );
        scenarios.push(ScenarioReport {
            scenario: scenario.clone(),
            summary,
            folds,
        });
    }
    let mut limitations = scenarios[0].folds[0].replay.model_limitations.clone();
    limitations.extend([
        "Fixed configuration rolling held-out evaluation; history warms indicators only, no training, parameter selection or nested optimization".into(),
        "Each fold resets cash, positions and strategy state; positions close at fold end. Pooled net PnL/fees sum separate experiments, mean return is arithmetic, no continuous portfolio or compounded equity is claimed".into(),
        "Test intervals are disjoint but prior-history windows may include earlier test bars; scenario comparisons share the same data and are not independent statistical evidence".into(),
        "The first test bar can generate a signal for the next bar; history does not seed a pending order. Gaps, capped folds and incomplete trailing windows are excluded from evaluation".into(),
        "User-supplied dataset provenance and prior parameter-selection bias are not verified; held-out here means excluded from this fold's history, not certified unseen data".into(),
    ]);
    Ok(ValidationReport {
        schema_version: 1,
        simulated: true,
        method: "fixed-configuration-rolling-held-out-v1".into(),
        options: o.clone(),
        input_bar_count: candles.len(),
        estimated_history_evaluations: work.unwrap(),
        unused_trailing_bars: candles.len() - ((fold_count - 1) * o.step_bars + window),
        scenarios,
        limitations,
    })
}

// ============================================================================
// ORACLE Historical Dataset Integrity & SHA-256 Provenance Audit
// ============================================================================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DatasetIntegrityReport {
    pub bar_count: usize,
    pub expected_interval_ms: i64,
    pub first_bar_ms: Option<i64>,
    pub last_bar_ms: Option<i64>,
    pub coverage_fraction: f64,
    pub duplicate_or_reversed_timestamps: usize,
    pub missing_interval_gaps: usize,
    pub invalid_ohlc_bars: usize,
    pub extreme_return_spikes: usize,
    pub dataset_sha256: String,
    pub integrity_ok: bool,
    pub findings: Vec<String>,
}

/// Audits historical OHLCV candle series for chronology, interval gaps, OHLC
/// inversions, and extreme single-bar anomalies, returning a deterministic
/// FIPS 180-4 SHA-256 content fingerprint for reproducible ORACLE/GENESIS runs.
pub fn audit_candle_dataset(
    candles: &[Candle],
    expected_interval_ms: i64,
) -> Result<DatasetIntegrityReport> {
    ensure!(
        expected_interval_ms > 0,
        "expected_interval_ms must be positive"
    );
    ensure!(candles.len() <= 250_000, "dataset audit bar cap exceeded");

    let mut duplicate_or_reversed_timestamps = 0usize;
    let mut missing_interval_gaps = 0usize;
    let mut invalid_ohlc_bars = 0usize;
    let mut extreme_return_spikes = 0usize;
    let mut findings = Vec::new();
    let mut canonical_buf = Vec::with_capacity(candles.len() * 48);

    for (i, c) in candles.iter().enumerate() {
        let ts_ms = c.ts.timestamp_millis();
        canonical_buf.extend_from_slice(&ts_ms.to_le_bytes());
        for v in [c.open, c.high, c.low, c.close, c.volume] {
            canonical_buf.extend_from_slice(&v.to_bits().to_le_bytes());
        }
        let ohlc_ok = [c.open, c.high, c.low, c.close]
            .iter()
            .all(|p| p.is_finite() && *p > 0.0)
            && c.volume.is_finite()
            && c.volume >= 0.0
            && c.low <= c.open.min(c.close)
            && c.high >= c.open.max(c.close);
        if !ohlc_ok {
            invalid_ohlc_bars += 1;
            if findings.len() < 16 {
                findings.push(format!("Bar {i} failed OHLC/volume validity checks"));
            }
        }
        if i > 0 {
            let prev = &candles[i - 1];
            let dt = ts_ms - prev.ts.timestamp_millis();
            if dt <= 0 {
                duplicate_or_reversed_timestamps += 1;
                if findings.len() < 16 {
                    findings.push(format!(
                        "Bar {i} has non-monotonic timestamp delta ({dt} ms)"
                    ));
                }
            } else if dt > expected_interval_ms {
                missing_interval_gaps += 1;
                if findings.len() < 16 {
                    findings.push(format!(
                        "Bar {i} has interval gap of {dt} ms (expected {expected_interval_ms} ms)"
                    ));
                }
            }
            if prev.close.is_finite() && prev.close > 0.0 && c.close.is_finite() && c.close > 0.0 {
                let jump = ((c.close - prev.close) / prev.close).abs();
                if jump > 0.50 {
                    extreme_return_spikes += 1;
                    if findings.len() < 16 {
                        findings.push(format!(
                            "Bar {i} has extreme single-bar move ({:.1}%)",
                            jump * 100.0
                        ));
                    }
                }
            }
        }
    }

    let first_bar_ms = candles.first().map(|c| c.ts.timestamp_millis());
    let last_bar_ms = candles.last().map(|c| c.ts.timestamp_millis());
    let coverage_fraction = match (first_bar_ms, last_bar_ms) {
        (Some(start), Some(end)) if end > start => {
            let expected_bars = ((end - start) / expected_interval_ms) as f64 + 1.0;
            (candles.len() as f64 / expected_bars).clamp(0.0, 1.0)
        }
        (Some(_), Some(_)) => 1.0,
        _ => 0.0,
    };

    let dataset_sha256 = crate::evolution::sha256_bytes(&canonical_buf);
    let integrity_ok = !candles.is_empty()
        && duplicate_or_reversed_timestamps == 0
        && missing_interval_gaps == 0
        && invalid_ohlc_bars == 0;

    Ok(DatasetIntegrityReport {
        bar_count: candles.len(),
        expected_interval_ms,
        first_bar_ms,
        last_bar_ms,
        coverage_fraction,
        duplicate_or_reversed_timestamps,
        missing_interval_gaps,
        invalid_ohlc_bars,
        extreme_return_spikes,
        dataset_sha256,
        integrity_ok,
        findings,
    })
}
