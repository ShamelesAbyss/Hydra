//! COGNITION adaptive statistical intelligence, Bayesian expectancy shrinkage,
//! empirical probability calibration, and Page-Hinkley deterioration detection.
//!
//! Safety & truthfulness:
//! - Strictly separates `OutcomeSource::Simulated` from verified live outcomes
//!   (`OutcomeSource::VerifiedNet` remains rejected until settlement certification).
//! - Refuses to emit uncalibrated heuristic confidence values as verified probabilities.
//! - Recommends bounded ensemble weights only; never overrides independent AEGIS limits.

use crate::cognition_shadow::{OutcomeSource, ShadowPolicy, ShadowSample};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CalibrationStatus {
    UncalibratedPrior {
        reason: String,
    },
    CalibratedEmpirical {
        brier_score: f64,
        ece: f64,
        sample_count: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationBin {
    pub lower_p: f64,
    pub upper_p: f64,
    pub mean_predicted_p: f64,
    pub empirical_frequency: f64,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationReport {
    pub status: CalibrationStatus,
    pub bins: Vec<CalibrationBin>,
    pub raw_brier_score: Option<f64>,
    pub calibrated_brier_score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BayesianExpectancyEstimate {
    pub strategy: String,
    pub regime: String,
    pub source: OutcomeSource,
    pub raw_samples: usize,
    pub effective_samples: f64,
    pub empirical_mean_r: Option<f64>,
    pub posterior_mean_r: Option<f64>,
    pub posterior_std_r: Option<f64>,
    pub credible_lower_bound_r: Option<f64>,
    pub win_probability_posterior: Option<f64>,
    pub deterioration_detected: bool,
    pub cost_adjusted_ev_bps: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeteriorationAlert {
    pub strategy: String,
    pub detected: bool,
    pub peak_statistic: f64,
    pub threshold: f64,
    pub recent_mean_r: Option<f64>,
    pub triggered_at_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnsembleWeightEntry {
    pub strategy: String,
    pub weight: f64,
    pub posterior_mean_r: Option<f64>,
    pub credible_lower_bound_r: Option<f64>,
    pub cost_adjusted_ev_bps: Option<f64>,
    pub deteriorated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnsembleRanking {
    pub as_of_ms: i64,
    pub regime: String,
    pub simulated_only: bool,
    pub calibration_status: CalibrationStatus,
    pub entries: Vec<EnsembleWeightEntry>,
    pub weights: BTreeMap<String, f64>,
    pub limitations: Vec<String>,
}

/// Computes cost-adjusted expected value in basis points given posterior mean R,
/// initial stop distance in bps, and explicit round-trip execution/funding costs.
pub fn cost_adjusted_ev_bps(
    posterior_mean_r: f64,
    stop_loss_bps: f64,
    round_trip_fee_bps: f64,
    spread_bps: f64,
    slippage_bps: f64,
    expected_funding_bps: f64,
) -> f64 {
    let gross_ev = posterior_mean_r * stop_loss_bps.max(0.0);
    let total_cost = round_trip_fee_bps.max(0.0)
        + spread_bps.max(0.0)
        + slippage_bps.max(0.0)
        + expected_funding_bps.abs();
    gross_ev - total_cost
}

/// Conjugate Normal-Inverse-Gamma + Beta-Binomial shrinkage over strictly causal prior outcomes.
#[allow(clippy::too_many_arguments)]
pub fn bayesian_strategy_expectancy(
    strategy: &str,
    regime: &str,
    source: OutcomeSource,
    as_of_ms: i64,
    samples: &[ShadowSample],
    policy: &ShadowPolicy,
    stop_loss_bps: f64,
    round_trip_cost_bps: f64,
) -> Result<BayesianExpectancyEstimate> {
    ensure!(
        policy.half_life_ms > 0 && policy.lookback_ms > 0,
        "invalid policy window"
    );
    for s in samples {
        ensure!(
            s.source != OutcomeSource::VerifiedNet,
            "COGNITION cannot ingest unverified VerifiedNet claims"
        );
    }
    let cutoff = as_of_ms.saturating_sub(policy.lookback_ms);
    let mut matched: Vec<&ShadowSample> = samples
        .iter()
        .filter(|s| {
            s.strategy == strategy
                && (regime == "*" || s.regime == regime)
                && s.source == source
                && s.outcome_at_ms >= cutoff
                && s.outcome_at_ms < as_of_ms
                && s.normalized_r.is_finite()
        })
        .collect();
    matched.sort_by_key(|s| s.outcome_at_ms);

    let alert = detect_strategy_deterioration(strategy, &matched, 0.08, 2.5);
    if matched.is_empty() {
        return Ok(BayesianExpectancyEstimate {
            strategy: strategy.into(),
            regime: regime.into(),
            source,
            raw_samples: 0,
            effective_samples: 0.0,
            empirical_mean_r: None,
            posterior_mean_r: None,
            posterior_std_r: None,
            credible_lower_bound_r: None,
            win_probability_posterior: None,
            deterioration_detected: false,
            cost_adjusted_ev_bps: None,
        });
    }

    let mut sum_w = 0.0;
    let mut sum_w2 = 0.0;
    let mut sum_wr = 0.0;
    let mut weighted_wins = 0.0;
    for s in &matched {
        let w = (-(as_of_ms - s.outcome_at_ms) as f64 / policy.half_life_ms as f64).exp2();
        sum_w += w;
        sum_w2 += w * w;
        sum_wr += w * s.normalized_r;
        if s.normalized_r > 0.0 {
            weighted_wins += w;
        }
    }
    let n_eff = (sum_w * sum_w / sum_w2.max(1e-12)).min(matched.len() as f64);
    let emp_mean = sum_wr / sum_w.max(1e-12);

    // Skeptical shrinkage prior: N(0, sigma^2 / kappa_0) with kappa_0 = 5.0, prior variance s0^2 = 1.0.
    let kappa_0 = 5.0;
    let prior_mean = 0.0;
    let post_mean = (kappa_0 * prior_mean + n_eff * emp_mean) / (kappa_0 + n_eff);

    let emp_var = if matched.len() >= 2 && sum_w > sum_w2 / sum_w {
        let denom = sum_w - sum_w2 / sum_w;
        matched
            .iter()
            .map(|s| {
                let w = (-(as_of_ms - s.outcome_at_ms) as f64 / policy.half_life_ms as f64).exp2();
                w * (s.normalized_r - emp_mean).powi(2)
            })
            .sum::<f64>()
            / denom.max(1e-12)
    } else {
        1.0
    };
    let post_var_obs = (kappa_0 * 1.0 + n_eff * emp_var.max(0.05)) / (kappa_0 + n_eff);
    let post_std = (post_var_obs / (kappa_0 + n_eff)).sqrt();
    let credible_lb = post_mean - policy.uncertainty_multiplier * post_std;

    // Conservative Beta(2, 2) win probability posterior
    let win_rate_scaled = (weighted_wins / sum_w.max(1e-12)) * n_eff;
    let win_prob = (2.0 + win_rate_scaled) / (4.0 + n_eff);

    let ev_bps = cost_adjusted_ev_bps(post_mean, stop_loss_bps, round_trip_cost_bps, 0.0, 0.0, 0.0);

    Ok(BayesianExpectancyEstimate {
        strategy: strategy.into(),
        regime: regime.into(),
        source,
        raw_samples: matched.len(),
        effective_samples: n_eff,
        empirical_mean_r: Some(emp_mean),
        posterior_mean_r: Some(post_mean),
        posterior_std_r: Some(post_std),
        credible_lower_bound_r: Some(credible_lb),
        win_probability_posterior: Some(win_prob),
        deterioration_detected: alert.detected,
        cost_adjusted_ev_bps: Some(ev_bps),
    })
}

/// Two-sided / downward Page-Hinkley change-point detector monitoring strategy R-multiples.
pub fn detect_strategy_deterioration(
    strategy: &str,
    ordered_samples: &[&ShadowSample],
    delta: f64,
    threshold: f64,
) -> DeteriorationAlert {
    if ordered_samples.len() < 4 {
        return DeteriorationAlert {
            strategy: strategy.into(),
            detected: false,
            peak_statistic: 0.0,
            threshold,
            recent_mean_r: None,
            triggered_at_index: None,
        };
    }
    let mut running_mean = 0.0;
    let mut cum_drop = 0.0;
    let mut min_cum = 0.0;
    let mut max_ph = 0.0;
    let mut triggered_at = None;

    for (idx, s) in ordered_samples.iter().enumerate() {
        let r = s.normalized_r;
        running_mean += (r - running_mean) / (idx + 1) as f64;
        // Downward drift increases cumulative shortfall (running_mean - r - delta)
        cum_drop += running_mean - r - delta;
        if cum_drop < min_cum {
            min_cum = cum_drop;
        }
        let ph = cum_drop - min_cum;
        if ph > max_ph {
            max_ph = ph;
        }
        if ph >= threshold && triggered_at.is_none() {
            triggered_at = Some(idx);
        }
    }
    let tail_n = ordered_samples.len().min(6);
    let recent_mean = ordered_samples[ordered_samples.len() - tail_n..]
        .iter()
        .map(|s| s.normalized_r)
        .sum::<f64>()
        / tail_n as f64;

    DeteriorationAlert {
        strategy: strategy.into(),
        detected: triggered_at.is_some(),
        peak_statistic: max_ph,
        threshold,
        recent_mean_r: Some(recent_mean),
        triggered_at_index: triggered_at,
    }
}

/// Evaluates empirical out-of-sample reliability calibration (Brier & ECE).
/// Refuses to mark fewer than 20 samples as calibrated.
pub fn evaluate_empirical_calibration(
    predictions: &[(f64, bool)],
    num_bins: usize,
) -> Result<CalibrationReport> {
    let bins_n = num_bins.clamp(2, 20);
    for (p, _) in predictions {
        ensure!(
            p.is_finite() && (0.0..=1.0).contains(p),
            "predicted probability must be in [0, 1]"
        );
    }
    if predictions.len() < 20 {
        return Ok(CalibrationReport {
            status: CalibrationStatus::UncalibratedPrior {
                reason: format!(
                    "need >= 20 samples for empirical calibration (got {})",
                    predictions.len()
                ),
            },
            bins: Vec::new(),
            raw_brier_score: None,
            calibrated_brier_score: None,
        });
    }

    let brier = predictions
        .iter()
        .map(|(p, y)| {
            let yf = if *y { 1.0 } else { 0.0 };
            (p - yf).powi(2)
        })
        .sum::<f64>()
        / predictions.len() as f64;

    let mut bins = Vec::with_capacity(bins_n);
    let mut ece = 0.0;
    let mut cal_brier = 0.0;
    let total = predictions.len() as f64;
    for b in 0..bins_n {
        let lo = b as f64 / bins_n as f64;
        let hi = (b + 1) as f64 / bins_n as f64;
        let items: Vec<(f64, bool)> = predictions
            .iter()
            .copied()
            .filter(|(p, _)| {
                if b + 1 == bins_n {
                    *p >= lo && *p <= hi
                } else {
                    *p >= lo && *p < hi
                }
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        let mean_p = items.iter().map(|(p, _)| *p).sum::<f64>() / items.len() as f64;
        let emp_f = items.iter().filter(|(_, y)| *y).count() as f64 / items.len() as f64;
        ece += (items.len() as f64 / total) * (mean_p - emp_f).abs();
        for &(_, y) in &items {
            let yf = if y { 1.0 } else { 0.0 };
            cal_brier += (emp_f - yf).powi(2);
        }
        bins.push(CalibrationBin {
            lower_p: lo,
            upper_p: hi,
            mean_predicted_p: mean_p,
            empirical_frequency: emp_f,
            count: items.len(),
        });
    }
    cal_brier /= total;

    Ok(CalibrationReport {
        status: CalibrationStatus::CalibratedEmpirical {
            brier_score: brier,
            ece,
            sample_count: predictions.len(),
        },
        bins,
        raw_brier_score: Some(brier),
        calibrated_brier_score: Some(cal_brier),
    })
}

/// Computes bounded, cost-adjusted, causal adaptive ensemble weights across strategies.
#[allow(clippy::too_many_arguments)]
pub fn rank_and_weight_ensemble(
    as_of_ms: i64,
    regime: &str,
    source: OutcomeSource,
    candidate_strategies: &[String],
    samples: &[ShadowSample],
    policy: &ShadowPolicy,
    stop_loss_bps: f64,
    round_trip_cost_bps: f64,
) -> Result<EnsembleRanking> {
    ensure!(
        !candidate_strategies.is_empty(),
        "ensemble requires at least one strategy"
    );
    let mut entries = Vec::with_capacity(candidate_strategies.len());
    let mut raw_scores = Vec::with_capacity(candidate_strategies.len());

    for strat in candidate_strategies {
        let mut est = bayesian_strategy_expectancy(
            strat,
            regime,
            source,
            as_of_ms,
            samples,
            policy,
            stop_loss_bps,
            round_trip_cost_bps,
        )?;
        if est.raw_samples < 3 {
            est = bayesian_strategy_expectancy(
                strat,
                "*",
                source,
                as_of_ms,
                samples,
                policy,
                stop_loss_bps,
                round_trip_cost_bps,
            )?;
        }
        let base_score = if est.deterioration_detected {
            0.05
        } else if let Some(ev) = est.cost_adjusted_ev_bps {
            if ev > 0.0 {
                (1.0 + ev / 25.0).min(4.0)
            } else {
                (1.0 + (ev / 50.0).max(-0.8)).max(0.2)
            }
        } else {
            1.0
        };
        raw_scores.push((strat.clone(), base_score));
        entries.push(EnsembleWeightEntry {
            strategy: strat.clone(),
            weight: 0.0,
            posterior_mean_r: est.posterior_mean_r,
            credible_lower_bound_r: est.credible_lower_bound_r,
            cost_adjusted_ev_bps: est.cost_adjusted_ev_bps,
            deteriorated: est.deterioration_detected,
        });
    }

    let sum_scores: f64 = raw_scores.iter().map(|(_, s)| *s).sum::<f64>().max(1e-9);
    let mut weights = BTreeMap::new();
    for (idx, (strat, sc)) in raw_scores.into_iter().enumerate() {
        let w = (sc / sum_scores).clamp(0.05, 0.70);
        weights.insert(strat, w);
        entries[idx].weight = w;
    }
    let norm: f64 = weights.values().sum::<f64>().max(1e-9);
    for (idx, entry) in entries.iter_mut().enumerate() {
        let w = weights[&entry.strategy] / norm;
        weights.insert(entry.strategy.clone(), w);
        entry.weight = w;
        let _ = idx;
    }

    Ok(EnsembleRanking {
        as_of_ms,
        regime: regime.into(),
        simulated_only: true,
        calibration_status: CalibrationStatus::UncalibratedPrior {
            reason: "Ensemble weights reflect bounded Bayesian R shrinkage over historical samples"
                .into(),
        },
        entries,
        weights,
        limitations: vec![
            "Advisory Bayesian ensemble weights only; never alters independent AEGIS risk caps"
                .into(),
            "Simulated outcomes strictly separated from VerifiedNet financial settlement".into(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_sample(id: usize, strat: &str, regime: &str, t: i64, r: f64) -> ShadowSample {
        ShadowSample {
            id: format!("s{id}"),
            strategy: strat.into(),
            symbol: "BTC".into(),
            regime: regime.into(),
            decision_at_ms: t,
            outcome_at_ms: t + 1,
            normalized_r: r,
            source: OutcomeSource::Simulated,
        }
    }

    #[test]
    fn bayesian_shrinkage_dampens_tiny_samples_and_detects_deterioration() {
        let pol = ShadowPolicy::default();
        // Single lucky +3.0R trade must shrink strongly toward 0 with kappa_0=5.0 -> 3 / 6 = 0.50R.
        let one = vec![mk_sample(0, "mom", "TrendingUp", 100, 3.0)];
        let est = bayesian_strategy_expectancy(
            "mom",
            "TrendingUp",
            OutcomeSource::Simulated,
            1_000,
            &one,
            &pol,
            200.0,
            14.0,
        )
        .unwrap();
        assert!((est.posterior_mean_r.unwrap() - 0.5).abs() < 0.05);
        assert!(!est.deterioration_detected);

        // Positive streak followed by sustained severe deterioration -> Page-Hinkley triggers.
        let mut seq = Vec::new();
        for i in 0..8 {
            seq.push(mk_sample(i, "mom", "TrendingUp", 100 + i as i64 * 10, 1.2));
        }
        for i in 8..20 {
            seq.push(mk_sample(i, "mom", "TrendingUp", 100 + i as i64 * 10, -1.5));
        }
        let est_det = bayesian_strategy_expectancy(
            "mom",
            "TrendingUp",
            OutcomeSource::Simulated,
            1_000,
            &seq,
            &pol,
            200.0,
            14.0,
        )
        .unwrap();
        assert!(est_det.deterioration_detected);
    }

    #[test]
    fn empirical_calibration_and_ensemble_ranking_behave_conservatively() {
        let too_few = vec![(0.8, true); 10];
        let rep_few = evaluate_empirical_calibration(&too_few, 5).unwrap();
        assert!(matches!(
            rep_few.status,
            CalibrationStatus::UncalibratedPrior { .. }
        ));

        let preds: Vec<(f64, bool)> = (0..40)
            .map(|i| {
                if i < 20 {
                    (0.25, i % 4 == 0)
                } else {
                    (0.75, i % 4 != 0)
                }
            })
            .collect();
        let rep = evaluate_empirical_calibration(&preds, 4).unwrap();
        match rep.status {
            CalibrationStatus::CalibratedEmpirical {
                ece, brier_score, ..
            } => {
                assert!(ece < 0.05);
                assert!(brier_score < 0.25);
            }
            _ => panic!("expected calibrated report"),
        }

        let pol = ShadowPolicy::default();
        let mut samples = Vec::new();
        for i in 0..10 {
            samples.push(mk_sample(
                i,
                "winner",
                "TrendingUp",
                10 + i as i64 * 10,
                1.1,
            ));
            samples.push(mk_sample(
                100 + i,
                "loser",
                "TrendingUp",
                10 + i as i64 * 10,
                -0.6,
            ));
        }
        let ranking = rank_and_weight_ensemble(
            1_000,
            "TrendingUp",
            OutcomeSource::Simulated,
            &["winner".into(), "loser".into()],
            &samples,
            &pol,
            200.0,
            14.0,
        )
        .unwrap();
        assert!(ranking.weights["winner"] > ranking.weights["loser"]);
        assert!((ranking.weights.values().sum::<f64>() - 1.0).abs() < 1e-9);
    }
}
