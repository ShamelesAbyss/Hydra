//! Bounded assessment of a frozen training frontier on chronologically later data.
use crate::{
    config::AppConfig,
    core::Candle,
    genesis::{self, Objectives, Parameters, SearchCheckpoint, SearchOptions},
    oracle_validation::{self, ValidationOptions, ValidationReport},
};
use anyhow::{ensure, Result};
use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
pub struct CandidateAssessment {
    pub candidate_id: String,
    pub parameters: Parameters,
    pub training_objectives: Objectives,
    pub held_out: ValidationReport,
}
#[derive(Debug, Clone, Serialize)]
pub struct AssessmentReport {
    pub schema_version: u32,
    pub simulated: bool,
    pub auto_deployment_authorized: bool,
    pub financial_evidence_verified: bool,
    pub caller_state_authenticated: bool,
    pub training_context_sha3_256: String,
    pub training_end_ms: i64,
    pub held_out_start_ms: i64,
    pub held_out_end_ms: i64,
    pub selection_basis: String,
    pub frontier_candidates: usize,
    pub unassessed_frontier_candidates: usize,
    pub assessments: Vec<CandidateAssessment>,
    pub limitations: Vec<String>,
}
pub fn evaluate(
    cfg: &AppConfig,
    training: &[Candle],
    held_out: &[Candle],
    search: &SearchOptions,
    checkpoint: &SearchCheckpoint,
    validation: &ValidationOptions,
    max_candidates: usize,
) -> Result<AssessmentReport> {
    ensure!(
        (1..=16).contains(&max_candidates),
        "candidate assessment limit must be 1..16"
    );
    ensure!(
        !training.is_empty() && !held_out.is_empty(),
        "training and held-out data required"
    );
    ensure!(
        training.windows(2).all(|w| w[0].ts < w[1].ts),
        "training chronology must increase strictly"
    );
    ensure!(
        held_out[0].ts > training.last().unwrap().ts,
        "all held-out history/test bars must be later than training data"
    );
    ensure!(
        checkpoint.state.generations.len() >= search.generations
            || checkpoint.state.candidates.len() >= search.max_evaluations
            || checkpoint.state.termination == "unique_candidate_attempt_budget",
        "freeze a completed training search before held-out assessment"
    );
    // Complete checkpoints undergo context/checksum/structural checks without replay.
    let frozen = genesis::advance(cfg, training, search, Some(checkpoint), 1)?;
    ensure!(frozen.complete, "training search is incomplete");
    let selected: Vec<_> = frozen
        .report
        .candidates
        .iter()
        .filter(|c| frozen.report.pareto_candidate_ids.contains(&c.id))
        .take(max_candidates)
        .collect();
    ensure!(!selected.is_empty(), "training frontier has no candidates");
    ensure!(
        validation.history_bars > 0
            && validation.test_bars >= 2
            && validation.step_bars >= validation.test_bars
            && (1..=32).contains(&validation.max_folds)
            && (1..=8).contains(&validation.scenarios.len()),
        "invalid validation window/scenario budgets"
    );
    let window = validation
        .history_bars
        .checked_add(validation.test_bars)
        .filter(|n| *n <= held_out.len())
        .ok_or_else(|| anyhow::anyhow!("insufficient held-out history/test data"))?;
    let folds = (1 + (held_out.len() - window) / validation.step_bars).min(validation.max_folds);
    let calls = folds
        .checked_mul(validation.scenarios.len())
        .and_then(|n| n.checked_mul(selected.len()))
        .ok_or_else(|| anyhow::anyhow!("combined assessment budget overflow"))?;
    ensure!(
        calls <= 64,
        "combined candidate/fold/scenario budget exceeds 64"
    );
    let work = validation
        .test_bars
        .checked_mul(cfg.strategies.global.lookback_candles.min(window))
        .and_then(|n| n.checked_mul(calls));
    ensure!(
        work.is_some_and(|n| n <= 100_000_000),
        "aggregate candidate history-work budget exceeded"
    );
    let entries = validation
        .test_bars
        .checked_add(validation.replay.max_trades)
        .and_then(|n| n.checked_mul(calls));
    ensure!(
        entries.is_some_and(|n| n <= 1_000_000),
        "aggregate candidate report-memory budget exceeded"
    );
    let mut assessments = Vec::with_capacity(selected.len());
    for candidate in selected {
        let config = genesis::configure_candidate(cfg, &candidate.parameters);
        let held_out = oracle_validation::evaluate(&config, held_out, validation)?;
        assessments.push(CandidateAssessment {
            candidate_id: candidate.id.clone(),
            parameters: candidate.parameters.clone(),
            training_objectives: candidate.objectives.clone(),
            held_out,
        });
    }
    Ok(AssessmentReport {schema_version:1,simulated:true,auto_deployment_authorized:false,financial_evidence_verified:false,caller_state_authenticated:false,
        training_context_sha3_256:checkpoint.context_sha3_256.clone(),training_end_ms:training.last().unwrap().ts.timestamp_millis(),held_out_start_ms:held_out[0].ts.timestamp_millis(),held_out_end_ms:held_out.last().unwrap().ts.timestamp_millis(),
        selection_basis:"training Pareto frontier in canonical parameter order; bounded prefix chosen before held-out evaluation".into(),frontier_candidates:frozen.report.pareto_candidate_ids.len(),unassessed_frontier_candidates:frozen.report.pareto_candidate_ids.len()-assessments.len(),assessments,
        limitations:vec![
            "Simulated fixed-parameter assessment, not model training, calibrated confidence, portfolio compounding or automatic candidate selection/deployment.".into(),
            "Every held-out history/test bar is later than training; timestamps and context binding cannot prove that the caller never inspected or tuned against held-out data.".into(),
            "Checkpoint hashes detect corruption/context mismatch, not authenticity. Caller-supplied training objectives are not certified financial evidence.".into(),
            "Candidate limit uses a canonical prefix of the training frontier, not held-out profitability ranking; unassessed candidates remain explicit.".into(),
            "Original Oracle funding, partial-fill, latency, queue, liquidation and portfolio-model omissions apply. Preserve original data/config/checkpoint for reproduction.".into(),
            "Comparing multiple candidates/scenarios creates selection bias and multiplicity; no significance or unseen-data guarantee is provided.".into(),
        ]})
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    fn bars(start: i64, count: usize) -> Vec<Candle> {
        (0..count)
            .map(|i| Candle {
                ts: Utc.timestamp_millis_opt(start + i as i64 * 60000).unwrap(),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 1.0,
            })
            .collect()
    }
    fn fixture() -> (
        AppConfig,
        Vec<Candle>,
        Vec<Candle>,
        SearchOptions,
        SearchCheckpoint,
        ValidationOptions,
    ) {
        let c = AppConfig::load_research(
            concat!(env!("CARGO_MANIFEST_DIR"), "/config/strategies.toml"),
            None,
        )
        .unwrap();
        let train = bars(1700000000000, 300);
        let held = bars(1700000000000 + 300 * 60000, 500);
        let search = SearchOptions {
            population: 2,
            generations: 1,
            max_evaluations: 2,
            ..Default::default()
        };
        let checkpoint = genesis::advance(&c, &train, &search, None, 1)
            .unwrap()
            .checkpoint;
        (
            c,
            train,
            held,
            search,
            checkpoint,
            ValidationOptions::default(),
        )
    }
    #[test]
    fn later_candidate_assessment_is_repeatable_and_never_authorizes_deployment() {
        let (c, t, h, s, cp, v) = fixture();
        let a = evaluate(&c, &t, &h, &s, &cp, &v, 2).unwrap();
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::to_value(evaluate(&c, &t, &h, &s, &cp, &v, 2).unwrap()).unwrap()
        );
        assert!(
            !a.auto_deployment_authorized
                && !a.financial_evidence_verified
                && !a.caller_state_authenticated
                && a.simulated
        );
        assert_eq!(a.assessments.len(), 2);
        assert!(a.held_out_start_ms > a.training_end_ms);
        assert!(a
            .assessments
            .iter()
            .all(|c| c.held_out.scenarios.len() == 2));
    }
    #[test]
    fn overlap_changed_context_and_incomplete_search_are_refused() {
        let (c, t, mut h, s, cp, v) = fixture();
        h[0].ts = t.last().unwrap().ts;
        assert!(evaluate(&c, &t, &h, &s, &cp, &v, 2)
            .unwrap_err()
            .to_string()
            .contains("later"));
        let mut changed = t.clone();
        changed[0].close = 100.1;
        let h = bars(1700000000000 + 300 * 60000, 500);
        assert!(evaluate(&c, &changed, &h, &s, &cp, &v, 2).is_err());
        let mut incomplete = s.clone();
        incomplete.generations = 3;
        incomplete.max_evaluations = 6;
        let p = genesis::advance(&c, &t, &incomplete, None, 1).unwrap();
        assert!(!p.complete);
        assert!(evaluate(&c, &t, &h, &incomplete, &p.checkpoint, &v, 2).is_err());
    }
    #[test]
    fn held_out_prices_cannot_select_training_candidates() {
        let (c, t, mut h, s, cp, v) = fixture();
        let a = evaluate(&c, &t, &h, &s, &cp, &v, 1).unwrap();
        for b in &mut h {
            b.open *= 1.1;
            b.high *= 1.1;
            b.low *= 1.1;
            b.close *= 1.1;
        }
        let b = evaluate(&c, &t, &h, &s, &cp, &v, 1).unwrap();
        assert_eq!(a.assessments[0].candidate_id, b.assessments[0].candidate_id);
        assert_eq!(a.unassessed_frontier_candidates, 1);
        assert_eq!(
            cp.checksum_sha3_256,
            genesis::advance(&c, &t, &s, Some(&cp), 1)
                .unwrap()
                .checkpoint
                .checksum_sha3_256
        );
    }
    #[test]
    fn combined_candidate_work_is_bounded_before_replays() {
        let (c, t, h, s, cp, mut v) = fixture();
        v.history_bars = 3;
        v.test_bars = 3;
        v.step_bars = 3;
        v.max_folds = 32;
        v.scenarios = (0..8)
            .map(|i| crate::oracle_validation::CostScenario {
                name: i.to_string(),
                fee_bps: 0.0,
                slippage_bps: 0.0,
            })
            .collect();
        assert!(evaluate(&c, &t, &h, &s, &cp, &v, 2)
            .unwrap_err()
            .to_string()
            .contains("combined"));
        assert!(evaluate(&c, &t, &h, &s, &cp, &v, 0).is_err());
    }
}
