//! Pure offline, prequential COGNITION diagnostics. No live learning authority.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeSource {
    Simulated,
    CallerSuppliedNormalizedR,
    /// This module cannot authenticate settlement; this claim is rejected.
    VerifiedNet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowSample {
    pub id: String,
    pub strategy: String,
    pub symbol: String,
    /// Caller-assigned regime at decision time, not an independently inferred regime.
    pub regime: String,
    pub decision_at_ms: i64,
    pub outcome_at_ms: i64,
    /// Caller-normalized outcome per unit initial risk; units/costs are not verified.
    pub normalized_r: f64,
    pub source: OutcomeSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowPolicy {
    pub lookback_ms: i64,
    pub half_life_ms: i64,
    pub min_samples: usize,
    pub min_effective_samples: f64,
    pub min_lower_bound_r: f64,
    /// Width multiplier for a descriptive normal approximation, not a probability.
    pub uncertainty_multiplier: f64,
}
impl Default for ShadowPolicy {
    fn default() -> Self {
        Self {
            lookback_ms: 30 * 86_400_000,
            half_life_ms: 7 * 86_400_000,
            min_samples: 30,
            min_effective_samples: 20.0,
            min_lower_bound_r: 0.0,
            uncertainty_multiplier: 1.96,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowRequest {
    pub as_of_ms: i64,
    pub policy: ShadowPolicy,
    pub samples: Vec<ShadowSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfidenceEstimate {
    pub samples: usize,
    pub effective_samples: f64,
    pub weighted_mean_r: Option<f64>,
    pub weighted_sample_variance_r: Option<f64>,
    pub approximate_standard_error_r: Option<f64>,
    pub approximate_lower_bound_r: Option<f64>,
    pub approximate_upper_bound_r: Option<f64>,
    pub sufficient_samples: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributionGroup {
    pub strategy: String,
    pub regime: String,
    pub source: OutcomeSource,
    pub symbols: Vec<String>,
    pub estimate: ConfidenceEstimate,
    pub normalized_r_sum: f64,
    pub positive_outcomes: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowDecision {
    pub sample_id: String,
    pub decision_at_ms: i64,
    pub shadow_pass: bool,
    pub reason: String,
    pub prior_estimate: ConfidenceEstimate,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyComparison {
    pub source: OutcomeSource,
    pub baseline_samples: usize,
    pub baseline_normalized_r_sum: f64,
    pub shadow_passed_samples: usize,
    pub shadow_normalized_r_sum: f64,
    pub shadow_skipped_samples: usize,
    /// Per supplied opportunity, skipped outcomes contribute zero. Not a portfolio return.
    pub baseline_mean_r_per_opportunity: Option<f64>,
    pub shadow_mean_r_per_opportunity: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowReport {
    pub read_only: bool,
    pub trading_authorized: bool,
    pub auto_deployment_authorized: bool,
    pub financial_evidence_verified: bool,
    pub caller_provenance_unverified: bool,
    pub as_of_ms: i64,
    pub policy: ShadowPolicy,
    pub supplied_samples: usize,
    pub evaluated_opportunities: usize,
    pub outside_decision_window: usize,
    pub groups: Vec<AttributionGroup>,
    pub decisions: Vec<ShadowDecision>,
    pub comparisons: Vec<PolicyComparison>,
    pub limitations: Vec<String>,
}

fn label_ok(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 64
        && label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
}
type GroupKey = (String, String, OutcomeSource);
fn key(s: &ShadowSample) -> GroupKey {
    (s.strategy.clone(), s.regime.clone(), s.source)
}

/// Kish effective sample size and unbiased weighted sample variance. The normal
/// interval is descriptive under independent observations and fixed weights;
/// serial dependence, heavy tails and model selection invalidate coverage claims.
fn estimate(samples: &[&ShadowSample], at: i64, p: &ShadowPolicy) -> ConfidenceEstimate {
    let cutoff = at.saturating_sub(p.lookback_ms);
    let selected: Vec<_> = samples
        .iter()
        .copied()
        .filter(|s| s.outcome_at_ms >= cutoff && s.outcome_at_ms < at)
        .collect();
    let mut out = ConfidenceEstimate {
        samples: selected.len(),
        effective_samples: 0.0,
        weighted_mean_r: None,
        weighted_sample_variance_r: None,
        approximate_standard_error_r: None,
        approximate_lower_bound_r: None,
        approximate_upper_bound_r: None,
        sufficient_samples: false,
    };
    if selected.is_empty() {
        return out;
    }
    let weight = |s: &ShadowSample| (-(at - s.outcome_at_ms) as f64 / p.half_life_ms as f64).exp2();
    let weighted: Vec<_> = selected.iter().map(|s| (*s, weight(s))).collect();
    let sum_w = weighted.iter().map(|(_, w)| *w).sum::<f64>();
    let sum_w2 = weighted.iter().map(|(_, w)| w.powi(2)).sum::<f64>();
    let mean = weighted
        .iter()
        .map(|(s, w)| w * s.normalized_r)
        .sum::<f64>()
        / sum_w;
    let n_eff = (sum_w * sum_w / sum_w2).min(selected.len() as f64);
    out.effective_samples = n_eff;
    out.weighted_mean_r = Some(mean);
    let denominator = sum_w - sum_w2 / sum_w;
    if selected.len() >= 2 && denominator > 0.0 && n_eff > 1.0 {
        let variance = weighted
            .iter()
            .map(|(s, w)| w * (s.normalized_r - mean).powi(2))
            .sum::<f64>()
            / denominator;
        let se = (variance / n_eff).sqrt();
        out.weighted_sample_variance_r = Some(variance);
        out.approximate_standard_error_r = Some(se);
        out.approximate_lower_bound_r = Some(mean - p.uncertainty_multiplier * se);
        out.approximate_upper_bound_r = Some(mean + p.uncertainty_multiplier * se);
    }
    out.sufficient_samples = selected.len() >= p.min_samples
        && n_eff >= p.min_effective_samples
        && out.approximate_standard_error_r.is_some();
    out
}

/// Evaluate pass-all baseline versus a fixed, strictly prequential shadow gate.
/// No sample can inform its own decision or another decision before its outcome
/// was available. Same-timestamp outcomes are deliberately unavailable.
/// Work is bounded by 10,000 samples (worst-case quadratic history diagnostics).
pub fn evaluate(request: &ShadowRequest) -> Result<ShadowReport> {
    let p = &request.policy;
    ensure!(request.as_of_ms >= 0, "as_of_ms must be nonnegative");
    ensure!(
        request.samples.len() <= 10_000,
        "sample budget exceeds 10000"
    );
    ensure!(
        p.lookback_ms > 0
            && p.half_life_ms > 0
            && p.lookback_ms as f64 / p.half_life_ms as f64 <= 50.0,
        "positive lookback/half-life required with lookback at most 50 half-lives"
    );
    ensure!(
        (2..=10_000).contains(&p.min_samples),
        "min_samples must be 2..10000"
    );
    ensure!(
        p.min_effective_samples.is_finite() && (2.0..=10_000.0).contains(&p.min_effective_samples),
        "invalid effective sample minimum"
    );
    ensure!(
        p.min_lower_bound_r.is_finite() && p.min_lower_bound_r.abs() <= 1_000_000.0,
        "invalid lower-bound threshold"
    );
    ensure!(
        p.uncertainty_multiplier.is_finite() && (1.0..=10.0).contains(&p.uncertainty_multiplier),
        "uncertainty multiplier must be 1..10"
    );
    let mut ids = BTreeSet::new();
    for s in &request.samples {
        ensure!(
            label_ok(&s.id) && label_ok(&s.strategy) && label_ok(&s.symbol) && label_ok(&s.regime),
            "invalid sample identifier or label"
        );
        ensure!(ids.insert(s.id.as_str()), "duplicate sample id");
        ensure!(s.source!=OutcomeSource::VerifiedNet, "verified_net is refused: this module cannot authenticate authoritative settlement or net financial outcomes");
        ensure!(
            s.normalized_r.is_finite() && s.normalized_r.abs() <= 1_000_000.0,
            "normalized_r must be finite and within +/-1000000"
        );
        ensure!(
            s.decision_at_ms >= 0
                && s.outcome_at_ms >= s.decision_at_ms
                && s.outcome_at_ms <= request.as_of_ms,
            "invalid chronology or future financial outcome"
        );
    }
    let cutoff = request.as_of_ms.saturating_sub(p.lookback_ms);
    let mut buckets: BTreeMap<GroupKey, Vec<&ShadowSample>> = BTreeMap::new();
    for s in &request.samples {
        buckets.entry(key(s)).or_default().push(s);
    }
    for samples in buckets.values_mut() {
        samples.sort_by(|a, b| a.outcome_at_ms.cmp(&b.outcome_at_ms).then(a.id.cmp(&b.id)));
    }
    let mut candidates: Vec<_> = request
        .samples
        .iter()
        .filter(|s| s.decision_at_ms >= cutoff)
        .collect();
    candidates.sort_by(|a, b| {
        a.decision_at_ms
            .cmp(&b.decision_at_ms)
            .then(a.id.cmp(&b.id))
    });
    let mut comparisons: BTreeMap<OutcomeSource, PolicyComparison> = BTreeMap::new();
    let mut decisions = Vec::with_capacity(candidates.len());
    for s in &candidates {
        let prior = estimate(&buckets[&key(s)], s.decision_at_ms, p);
        let pass = prior.sufficient_samples
            && prior
                .approximate_lower_bound_r
                .is_some_and(|v| v > p.min_lower_bound_r);
        let reason = if !prior.sufficient_samples {
            "insufficient prior count/effective samples"
        } else if pass {
            "prior approximate lower bound exceeds fixed threshold"
        } else {
            "prior approximate lower bound does not exceed fixed threshold"
        };
        let comparison = comparisons.entry(s.source).or_insert(PolicyComparison {
            source: s.source,
            baseline_samples: 0,
            baseline_normalized_r_sum: 0.0,
            shadow_passed_samples: 0,
            shadow_normalized_r_sum: 0.0,
            shadow_skipped_samples: 0,
            baseline_mean_r_per_opportunity: None,
            shadow_mean_r_per_opportunity: None,
        });
        comparison.baseline_samples += 1;
        comparison.baseline_normalized_r_sum += s.normalized_r;
        if pass {
            comparison.shadow_passed_samples += 1;
            comparison.shadow_normalized_r_sum += s.normalized_r;
        } else {
            comparison.shadow_skipped_samples += 1;
        }
        decisions.push(ShadowDecision {
            sample_id: s.id.clone(),
            decision_at_ms: s.decision_at_ms,
            shadow_pass: pass,
            reason: reason.into(),
            prior_estimate: prior,
        });
    }
    for c in comparisons.values_mut() {
        c.baseline_mean_r_per_opportunity =
            Some(c.baseline_normalized_r_sum / c.baseline_samples as f64);
        c.shadow_mean_r_per_opportunity =
            Some(c.shadow_normalized_r_sum / c.baseline_samples as f64);
    }
    let mut groups = Vec::new();
    for ((strategy, regime, source), samples) in buckets {
        // The same strict-before convention applies to the as-of attribution.
        let recent: Vec<_> = samples
            .iter()
            .copied()
            .filter(|s| s.outcome_at_ms >= cutoff && s.outcome_at_ms < request.as_of_ms)
            .collect();
        if recent.is_empty() {
            continue;
        }
        groups.push(AttributionGroup {
            strategy,
            regime,
            source,
            symbols: recent
                .iter()
                .map(|s| s.symbol.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            estimate: estimate(&samples, request.as_of_ms, p),
            normalized_r_sum: recent.iter().map(|s| s.normalized_r).sum(),
            positive_outcomes: recent.iter().filter(|s| s.normalized_r > 0.0).count(),
        });
    }
    Ok(ShadowReport { read_only:true,trading_authorized:false,auto_deployment_authorized:false,financial_evidence_verified:false,caller_provenance_unverified:true,as_of_ms:request.as_of_ms,policy:p.clone(),supplied_samples:request.samples.len(),evaluated_opportunities:candidates.len(),outside_decision_window:request.samples.len()-candidates.len(),groups,decisions,comparisons:comparisons.into_values().collect(),limitations:vec![
        "All normalized R, costs, risk denominators, labels and decision-time regimes are caller supplied and unverified. verified_net claims are refused; no settlement is certified.".into(),
        "Simulated and caller-supplied outcomes remain separate. Neither is verified financial evidence.".into(),
        "Kish effective sample size describes recency-weight concentration, not independence or a calibrated probability of profit. Approximate normal intervals can be unreliable for sparse, serially dependent or heavy-tailed outcomes; constant samples can have zero estimated variance.".into(),
        "Baseline and shadow compare the same supplied opportunities with skipped outcomes set to zero. Sums of normalized R are attribution statistics, not portfolio returns, account equity, or deployable capital.".into(),
        "Only outcomes strictly before each decision train its gate. Caller-provided opportunity sets and regime labels can still contain leakage, survivorship bias or selection bias; completeness is not verified.".into(),
        "Thresholds must be fixed before evaluating held-out data. Repeated threshold search, overlapping trades and multiple strategy/regime comparisons introduce data-snooping and multiplicity bias; no adjustment or causal performance claim is made.".into(),
        "No online learning, regime inference, sizing, execution, auto-deployment, risk override, or network/storage access exists in this module.".into(),
    ] })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(id: usize, at: i64, r: f64) -> ShadowSample {
        ShadowSample {
            id: format!("s{id}"),
            strategy: "ema".into(),
            symbol: "BTC".into(),
            regime: "trend".into(),
            decision_at_ms: at,
            outcome_at_ms: at + 1,
            normalized_r: r,
            source: OutcomeSource::Simulated,
        }
    }
    fn request() -> ShadowRequest {
        ShadowRequest {
            as_of_ms: 100,
            policy: ShadowPolicy {
                lookback_ms: 100,
                half_life_ms: 100,
                min_samples: 2,
                min_effective_samples: 2.0,
                min_lower_bound_r: 0.0,
                uncertainty_multiplier: 1.96,
            },
            samples: vec![],
        }
    }
    #[test]
    fn sparse_data_never_passes_or_fabricates_confidence() {
        let mut r = request();
        r.samples.push(sample(0, 10, 1.0));
        let out = evaluate(&r).unwrap();
        assert!(!out.decisions[0].shadow_pass);
        assert_eq!(out.groups[0].estimate.approximate_lower_bound_r, None);
        assert_eq!(out.groups[0].estimate.samples, 1);
        assert!(
            out.read_only
                && !out.trading_authorized
                && !out.auto_deployment_authorized
                && !out.financial_evidence_verified
                && out.caller_provenance_unverified
        );
    }
    #[test]
    fn no_lookahead_even_with_unsorted_or_same_timestamp_outcomes() {
        let mut r = request();
        r.policy.min_effective_samples = 2.0;
        r.samples = vec![sample(2, 30, 50.0), sample(0, 10, 1.0), sample(1, 20, 1.0)];
        r.samples[0].outcome_at_ms = 31;
        let a = evaluate(&r).unwrap();
        assert_eq!(a.decisions[0].prior_estimate.samples, 0);
        assert_eq!(a.decisions[1].prior_estimate.samples, 1);
        assert_eq!(a.decisions[2].prior_estimate.samples, 2);
        r.samples[0].normalized_r = -100.0;
        let b = evaluate(&r).unwrap();
        assert_eq!(
            serde_json::to_value(&a.decisions).unwrap(),
            serde_json::to_value(&b.decisions).unwrap()
        );
        r.samples[1].outcome_at_ms = 20;
        let c = evaluate(&r).unwrap();
        assert_eq!(c.decisions[1].prior_estimate.samples, 0);
    }
    #[test]
    fn recency_weighting_and_cutoff_are_explicit() {
        let mut r = request();
        r.policy.half_life_ms = 10;
        r.samples = vec![sample(0, 0, -1.0), sample(1, 90, 1.0)];
        let out = evaluate(&r).unwrap();
        assert!(out.groups[0].estimate.weighted_mean_r.unwrap() > 0.9);
        assert!(out.groups[0].estimate.effective_samples < 1.1);
        r.policy.lookback_ms = 20;
        let out = evaluate(&r).unwrap();
        assert_eq!(out.groups[0].estimate.samples, 1);
        assert_eq!(out.outside_decision_window, 1);
    }
    #[test]
    fn deterministic_source_separation_and_input_preservation() {
        let mut r = request();
        r.samples = vec![sample(0, 10, 1.0), sample(1, 20, -1.0)];
        r.samples[1].source = OutcomeSource::CallerSuppliedNormalizedR;
        let before = serde_json::to_value(&r).unwrap();
        let a = evaluate(&r).unwrap();
        assert_eq!(before, serde_json::to_value(&r).unwrap());
        assert_eq!(a.groups.len(), 2);
        assert_eq!(a.comparisons.len(), 2);
        r.samples.reverse();
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(evaluate(&r).unwrap()).unwrap()
        );
    }
    #[test]
    fn refuses_future_nonfinite_duplicate_and_unverifiable_claims() {
        let mut r = request();
        r.samples = vec![sample(0, 100, 1.0)];
        assert!(evaluate(&r).is_err());
        r.samples[0] = sample(0, 1, f64::NAN);
        assert!(evaluate(&r).is_err());
        r.samples[0] = sample(0, 1, 1.0);
        r.samples[0].source = OutcomeSource::VerifiedNet;
        assert!(evaluate(&r).is_err());
        r.samples[0].source = OutcomeSource::Simulated;
        r.samples.push(r.samples[0].clone());
        assert!(evaluate(&r).is_err());
        r.samples = vec![sample(0, 1, 1.0); 10001];
        assert!(evaluate(&r).is_err());
    }
    #[test]
    fn threshold_gate_uses_prior_data_and_shared_denominator() {
        let mut r = request();
        r.policy.half_life_ms = 10000;
        r.policy.min_effective_samples = 2.0;
        r.samples = (0..5).map(|i| sample(i, 10 + i as i64 * 10, 1.0)).collect();
        let out = evaluate(&r).unwrap();
        assert!(!out.decisions[0].shadow_pass);
        assert!(!out.decisions[1].shadow_pass);
        assert!(out.decisions[3].shadow_pass);
        let c = &out.comparisons[0];
        assert_eq!(c.baseline_samples, 5);
        assert_eq!(c.shadow_passed_samples + c.shadow_skipped_samples, 5);
        assert_eq!(
            c.shadow_mean_r_per_opportunity,
            Some(c.shadow_normalized_r_sum / 5.0)
        );
    }
    #[test]
    fn finite_extremes_and_unknown_schema_fail_closed() {
        let mut r = request();
        r.samples = vec![sample(0, 10, 1e6), sample(1, 20, -1e6)];
        let out = evaluate(&r).unwrap();
        assert!(out.groups[0]
            .estimate
            .approximate_standard_error_r
            .unwrap()
            .is_finite());
        assert!(serde_json::to_string(&out).is_ok());
        let mut value = serde_json::to_value(r).unwrap();
        value["future_field"] = true.into();
        assert!(serde_json::from_value::<ShadowRequest>(value).is_err());
    }

    #[test]
    fn causal_regime_inference_classifies_regimes_without_future_leakage() {
        use crate::core::Candle;
        use chrono::{TimeZone, Utc};

        let make_candles = |closes: &[f64]| -> Vec<Candle> {
            closes
                .iter()
                .enumerate()
                .map(|(i, &c)| Candle {
                    ts: Utc
                        .timestamp_opt(1_700_000_000 + (i as i64) * 3600, 0)
                        .unwrap(),
                    open: c,
                    high: c * 1.002,
                    low: c * 0.998,
                    close: c,
                    volume: 100.0,
                })
                .collect()
        };

        let cfg = RegimeInferenceConfig::default();
        // Insufficient history
        let short = make_candles(&[100.0; 10]);
        assert_eq!(
            infer_regime(&short, &cfg).unwrap().regime,
            InferredRegime::InsufficientHistory
        );

        // Steady uptrend -> TrendingUp
        let up: Vec<f64> = (0..30).map(|i| 100.0 + i as f64 * 1.5).collect();
        let up_bars = make_candles(&up);
        let up_res = infer_regime(&up_bars, &cfg).unwrap();
        assert_eq!(up_res.regime, InferredRegime::TrendingUp);
        assert!(up_res.efficiency_ratio > 0.8);

        // Steady downtrend -> TrendingDown
        let down: Vec<f64> = (0..30).map(|i| 200.0 - i as f64 * 2.0).collect();
        assert_eq!(
            infer_regime(&make_candles(&down), &cfg).unwrap().regime,
            InferredRegime::TrendingDown
        );

        // Tight oscillating range -> MeanRevertingRange
        let range: Vec<f64> = (0..30)
            .map(|i| if i % 2 == 0 { 100.0 } else { 100.6 })
            .collect();
        assert_eq!(
            infer_regime(&make_candles(&range), &cfg).unwrap().regime,
            InferredRegime::MeanRevertingRange
        );

        // Violent high-volatility swings -> HighVolatilityTurbulent
        let wild: Vec<f64> = (0..30)
            .map(|i| if i % 2 == 0 { 100.0 } else { 114.0 })
            .collect();
        assert_eq!(
            infer_regime(&make_candles(&wild), &cfg).unwrap().regime,
            InferredRegime::HighVolatilityTurbulent
        );

        // Prefix causality check: appending huge future crash must not alter earlier prefix inference.
        let mut extended = up_bars.clone();
        extended.push(Candle {
            ts: Utc.timestamp_opt(1_700_000_000 + 30 * 3600, 0).unwrap(),
            open: 10.0,
            high: 10.0,
            low: 10.0,
            close: 10.0,
            volume: 100.0,
        });
        let prefix_res = infer_regime(&extended[..30], &cfg).unwrap();
        assert_eq!(prefix_res.regime, up_res.regime);
        assert_eq!(prefix_res.efficiency_ratio, up_res.efficiency_ratio);
    }
}

// ============================================================================
// COGNITION Causal Market-Regime Inference (Zero-Lookahead Completed Bars)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferredRegime {
    TrendingUp,
    TrendingDown,
    MeanRevertingRange,
    HighVolatilityTurbulent,
    InsufficientHistory,
}

impl InferredRegime {
    pub fn label(self) -> &'static str {
        match self {
            Self::TrendingUp => "TRENDING_UP",
            Self::TrendingDown => "TRENDING_DOWN",
            Self::MeanRevertingRange => "MEAN_REVERTING_RANGE",
            Self::HighVolatilityTurbulent => "HIGH_VOLATILITY_TURBULENT",
            Self::InsufficientHistory => "INSUFFICIENT_HISTORY",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegimeInferenceConfig {
    pub lookback_bars: usize,
    /// Minimum Kaufman efficiency ratio (`0..=1`) to classify as directional trend.
    pub trend_efficiency_threshold: f64,
    /// Per-bar log-return standard deviation threshold above which regime is turbulent.
    pub high_volatility_bar_std: f64,
}

impl Default for RegimeInferenceConfig {
    fn default() -> Self {
        Self {
            lookback_bars: 24,
            trend_efficiency_threshold: 0.42,
            high_volatility_bar_std: 0.035,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeAssessment {
    pub read_only: bool,
    pub trading_authorized: bool,
    pub regime: InferredRegime,
    pub bars_evaluated: usize,
    pub net_return_pct: f64,
    pub efficiency_ratio: f64,
    pub bar_log_return_std: f64,
    pub as_of_ms: Option<i64>,
}

/// Infers market regime deterministically from strictly completed historical candles.
/// Never reads wall clock, never leaks future bars, and never modifies live policy.
pub fn infer_regime(
    candles: &[crate::core::Candle],
    cfg: &RegimeInferenceConfig,
) -> Result<RegimeAssessment> {
    ensure!(
        (4..=10_000).contains(&cfg.lookback_bars),
        "lookback_bars must be within 4..=10000"
    );
    ensure!(
        cfg.trend_efficiency_threshold.is_finite()
            && (0.0..=1.0).contains(&cfg.trend_efficiency_threshold)
            && cfg.high_volatility_bar_std.is_finite()
            && cfg.high_volatility_bar_std > 0.0,
        "invalid regime inference thresholds"
    );
    ensure!(candles.len() <= 100_000, "candle budget exceeded");
    for (i, c) in candles.iter().enumerate() {
        ensure!(
            c.close.is_finite() && c.close > 0.0 && (i == 0 || candles[i - 1].ts < c.ts),
            "invalid or non-chronological candles for regime inference"
        );
    }

    if candles.len() < cfg.lookback_bars {
        return Ok(RegimeAssessment {
            read_only: true,
            trading_authorized: false,
            regime: InferredRegime::InsufficientHistory,
            bars_evaluated: candles.len(),
            net_return_pct: 0.0,
            efficiency_ratio: 0.0,
            bar_log_return_std: 0.0,
            as_of_ms: candles.last().map(|c| c.ts.timestamp_millis()),
        });
    }

    let window = &candles[candles.len() - cfg.lookback_bars..];
    let first = window.first().unwrap().close;
    let last = window.last().unwrap().close;
    let net_move = (last - first).abs();
    let net_return_pct = ((last - first) / first) * 100.0;

    let mut path_sum = 0.0;
    let mut log_returns = Vec::with_capacity(window.len() - 1);
    for pair in window.windows(2) {
        path_sum += (pair[1].close - pair[0].close).abs();
        log_returns.push((pair[1].close / pair[0].close).ln());
    }
    let efficiency_ratio = if path_sum > 1e-12 {
        (net_move / path_sum).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let n = log_returns.len() as f64;
    let mean = log_returns.iter().sum::<f64>() / n;
    let var = if log_returns.len() > 1 {
        log_returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0)
    } else {
        0.0
    };
    let bar_log_return_std = var.sqrt();

    let regime = if bar_log_return_std >= cfg.high_volatility_bar_std {
        InferredRegime::HighVolatilityTurbulent
    } else if efficiency_ratio >= cfg.trend_efficiency_threshold {
        if last >= first {
            InferredRegime::TrendingUp
        } else {
            InferredRegime::TrendingDown
        }
    } else {
        InferredRegime::MeanRevertingRange
    };

    Ok(RegimeAssessment {
        read_only: true,
        trading_authorized: false,
        regime,
        bars_evaluated: window.len(),
        net_return_pct,
        efficiency_ratio,
        bar_log_return_std,
        as_of_ms: Some(window.last().unwrap().ts.timestamp_millis()),
    })
}
