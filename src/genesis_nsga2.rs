//! GENESIS Constraint-Aware Multi-Objective NSGA-II Strategy & Ensemble Discovery.
//!
//! Capabilities:
//! - True NSGA-II fast non-dominated sorting + crowding distance + Deb feasibility rules.
//! - Bounded deterministic parallel CPU evaluation via `std::thread::scope`.
//! - Evolves FANG strategy parameters + risk stops under realistic ORACLE execution costs.
//! - Enforces strict chronologically-later embargoed held-out validation (`auto_deployment_authorized: false`).

use crate::fang_engine::UnifiedStrategyConfig;
use crate::oracle_research::{
    simulate_portfolio, ExecutionModelConfig, MultiMarketDataset, PortfolioResearchReport,
};
use anyhow::{ensure, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Nsga2Params {
    pub fast_ema: usize,
    pub slow_ema: usize,
    pub offset_bps: u32,
    pub stop_loss_bps: u32,
    pub take_profit_bps: u32,
}

impl Nsga2Params {
    pub fn id(&self) -> String {
        format!(
            "nsga2-f{}-s{}-o{}-sl{}-tp{}",
            self.fast_ema, self.slow_ema, self.offset_bps, self.stop_loss_bps, self.take_profit_bps
        )
    }

    pub fn to_unified_config(&self) -> UnifiedStrategyConfig {
        UnifiedStrategyConfig {
            fast_ema: self.fast_ema,
            slow_ema: self.slow_ema,
            atr_period: self.slow_ema,
            offset_bps: self.offset_bps as f64,
            base_stop_loss_pct: Decimal::new(self.stop_loss_bps as i64, 4),
            base_take_profit_pct: Decimal::new(self.take_profit_bps as i64, 4),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nsga2Objectives {
    /// Higher is better
    pub net_return_pct: f64,
    /// Lower is better (penalty)
    pub max_drawdown_pct: f64,
    /// Lower is better (cost drag)
    pub cost_drag_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nsga2Candidate {
    pub id: String,
    pub params: Nsga2Params,
    pub objectives: Nsga2Objectives,
    pub constraint_violation: f64,
    pub trades_count: usize,
    pub rank: usize,
    pub crowding_distance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nsga2Options {
    pub seed: u64,
    pub population: usize,
    pub generations: usize,
    pub max_workers: usize,
    pub min_trades_required: usize,
    pub max_drawdown_cap_pct: f64,
    pub exec: ExecutionModelConfig,
}

impl Default for Nsga2Options {
    fn default() -> Self {
        Self {
            seed: 7,
            population: 8,
            generations: 3,
            max_workers: 2,
            min_trades_required: 1,
            max_drawdown_cap_pct: 25.0,
            exec: ExecutionModelConfig {
                warmup_bars: 12,
                latency_bars: 1,
                ..Default::default()
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nsga2Checkpoint {
    pub schema_version: u32,
    pub dataset_sha3_256: String,
    #[serde(default)]
    pub experiment_fingerprint: String,
    pub rng_state: u64,
    pub generations_completed: usize,
    pub evaluated: Vec<Nsga2Candidate>,
}

pub fn compute_experiment_fingerprint(
    train_digest: &str,
    opts: &Nsga2Options,
    train_bars: usize,
    embargo_bars: usize,
) -> Result<String> {
    let mut h = Sha3_256::new();
    h.update(train_digest.as_bytes());
    h.update(crate::fang_engine::REGISTRY_VERSION.as_bytes());
    h.update(serde_json::to_vec(opts)?);
    h.update((train_bars as u64).to_le_bytes());
    h.update((embargo_bars as u64).to_le_bytes());
    Ok(hex::encode(h.finalize()))
}

#[derive(Debug, Clone, Serialize)]
pub struct Nsga2HeldOutResult {
    pub candidate_id: String,
    pub training_net_return_pct: f64,
    pub held_out: PortfolioResearchReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct Nsga2CampaignReport {
    pub schema_version: u32,
    pub algorithm: String,
    pub simulated: bool,
    pub auto_deployment_authorized: bool,
    pub workers_used: usize,
    pub train_bars: usize,
    pub embargo_bars: usize,
    pub held_out_bars: usize,
    pub pareto_front: Vec<Nsga2Candidate>,
    pub held_out_assessments: Vec<Nsga2HeldOutResult>,
    pub checkpoint: Nsga2Checkpoint,
    pub limitations: Vec<String>,
}

/// Deb's constraint-domination: feasible dominates infeasible; smaller violation dominates larger
/// violation; feasible vs feasible uses multi-objective dominance `(max return, min drawdown, min cost)`.
pub fn constraint_dominates(a: &Nsga2Candidate, b: &Nsga2Candidate) -> bool {
    let a_feas = a.constraint_violation <= 0.0;
    let b_feas = b.constraint_violation <= 0.0;
    match (a_feas, b_feas) {
        (true, false) => true,
        (false, true) => false,
        (false, false) => a.constraint_violation < b.constraint_violation,
        (true, true) => {
            a.objectives.net_return_pct >= b.objectives.net_return_pct
                && a.objectives.max_drawdown_pct <= b.objectives.max_drawdown_pct
                && a.objectives.cost_drag_pct <= b.objectives.cost_drag_pct
                && (a.objectives.net_return_pct > b.objectives.net_return_pct
                    || a.objectives.max_drawdown_pct < b.objectives.max_drawdown_pct
                    || a.objectives.cost_drag_pct < b.objectives.cost_drag_pct)
        }
    }
}

pub fn fast_non_dominated_sort(pop: &mut [Nsga2Candidate]) -> Vec<Vec<usize>> {
    let n = pop.len();
    let mut dominated: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut dom_count: Vec<usize> = vec![0; n];
    let mut fronts: Vec<Vec<usize>> = Vec::new();
    let mut first = Vec::new();

    for p in 0..n {
        for q in 0..n {
            if p == q {
                continue;
            }
            if constraint_dominates(&pop[p], &pop[q]) {
                dominated[p].push(q);
            } else if constraint_dominates(&pop[q], &pop[p]) {
                dom_count[p] += 1;
            }
        }
        if dom_count[p] == 0 {
            pop[p].rank = 0;
            first.push(p);
        }
    }
    fronts.push(first);
    let mut i = 0usize;
    while !fronts[i].is_empty() {
        let mut next = Vec::new();
        for &p in &fronts[i] {
            for &q in &dominated[p] {
                dom_count[q] = dom_count[q].saturating_sub(1);
                if dom_count[q] == 0 {
                    pop[q].rank = i + 1;
                    next.push(q);
                }
            }
        }
        i += 1;
        fronts.push(next);
    }
    if fronts.last().is_some_and(|f| f.is_empty()) {
        fronts.pop();
    }
    for f in &fronts {
        assign_crowding_distance(pop, f);
    }
    fronts
}

fn assign_crowding_distance(pop: &mut [Nsga2Candidate], front: &[usize]) {
    for &idx in front {
        pop[idx].crowding_distance = 0.0;
    }
    if front.len() <= 2 {
        for &idx in front {
            pop[idx].crowding_distance = f64::INFINITY;
        }
        return;
    }
    let getters: [fn(&Nsga2Objectives) -> f64; 3] = [
        |o| o.net_return_pct,
        |o| o.max_drawdown_pct,
        |o| o.cost_drag_pct,
    ];
    for get in getters {
        let mut sorted = front.to_vec();
        sorted.sort_by(|&i, &j| get(&pop[i].objectives).total_cmp(&get(&pop[j].objectives)));
        pop[sorted[0]].crowding_distance = f64::INFINITY;
        pop[sorted[sorted.len() - 1]].crowding_distance = f64::INFINITY;
        let min_v = get(&pop[sorted[0]].objectives);
        let max_v = get(&pop[sorted[sorted.len() - 1]].objectives);
        let span = max_v - min_v;
        if span <= 1e-12 {
            continue;
        }
        for k in 1..(sorted.len() - 1) {
            if pop[sorted[k]].crowding_distance.is_finite() {
                let d = (get(&pop[sorted[k + 1]].objectives) - get(&pop[sorted[k - 1]].objectives))
                    / span;
                pop[sorted[k]].crowding_distance += d;
            }
        }
    }
}

struct Rng64(u64);
impl Rng64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut v = self.0;
        v = (v ^ (v >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        v = (v ^ (v >> 27)).wrapping_mul(0x94d049bb133111eb);
        v ^ (v >> 31)
    }
    fn usize_range(&mut self, lo: usize, hi: usize) -> usize {
        lo + (self.next() as usize % (hi - lo + 1))
    }
    fn u32_range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.next() as u32 % (hi - lo + 1))
    }
}

fn evaluate_single(
    params: Nsga2Params,
    train_ds: &MultiMarketDataset,
    opts: &Nsga2Options,
) -> Result<Nsga2Candidate> {
    let fcfg = params.to_unified_config();
    let rep = simulate_portfolio(train_ds, &fcfg, &opts.exec)?;
    let mut violation = 0.0;
    if rep.metrics.trades_count < opts.min_trades_required {
        violation += (opts.min_trades_required - rep.metrics.trades_count) as f64 * 10.0;
    }
    if rep.metrics.max_drawdown_pct > opts.max_drawdown_cap_pct {
        violation += rep.metrics.max_drawdown_pct - opts.max_drawdown_cap_pct;
    }
    let cost_drag = ((rep.metrics.total_fees_usd + rep.metrics.total_slippage_usd)
        / rep.metrics.initial_equity.max(1.0))
        * 100.0;
    Ok(Nsga2Candidate {
        id: params.id(),
        params,
        objectives: Nsga2Objectives {
            net_return_pct: rep.metrics.net_return_pct,
            max_drawdown_pct: rep.metrics.max_drawdown_pct,
            cost_drag_pct: cost_drag,
        },
        constraint_violation: violation,
        trades_count: rep.metrics.trades_count,
        rank: 0,
        crowding_distance: 0.0,
    })
}

/// Runs bounded deterministic parallel NSGA-II on `[..train_bars]` followed by
/// embargoed held-out validation on `[train_bars + embargo_bars..]`.
pub fn run_nsga2_campaign(
    dataset: &MultiMarketDataset,
    train_bars: usize,
    embargo_bars: usize,
    opts: &Nsga2Options,
    resume: Option<&Nsga2Checkpoint>,
) -> Result<Nsga2CampaignReport> {
    let (timeline, _) = dataset.validate_and_fingerprint()?;
    ensure!(
        opts.population >= 4
            && opts.population <= 64
            && opts.generations >= 1
            && opts.generations <= 50,
        "NSGA-II population must be 4..=64 and generations 1..=50"
    );
    ensure!(embargo_bars >= 1, "embargo_bars must be >= 1");
    ensure!(
        timeline.len() > train_bars + embargo_bars + opts.exec.warmup_bars + opts.exec.latency_bars,
        "dataset too small for train + embargo + held-out validation"
    );
    let workers = opts.max_workers.clamp(1, 4);

    let train_ds = dataset.slice_range(0, train_bars)?;
    let (_, train_digest) = train_ds.validate_and_fingerprint()?;
    let exp_fingerprint =
        compute_experiment_fingerprint(&train_digest, opts, train_bars, embargo_bars)?;
    let held_out_ds = dataset.slice_range(train_bars + embargo_bars, timeline.len())?;

    let mut evaluated_map: BTreeMap<Nsga2Params, Nsga2Candidate> = BTreeMap::new();
    let mut rng = if let Some(cp) = resume {
        ensure!(
            cp.dataset_sha3_256 == train_digest,
            "checkpoint dataset mismatch"
        );
        ensure!(
            cp.experiment_fingerprint.is_empty() || cp.experiment_fingerprint == exp_fingerprint,
            "checkpoint experiment fingerprint mismatch (code/config/options changed)"
        );
        for c in &cp.evaluated {
            evaluated_map.insert(c.params.clone(), c.clone());
        }
        Rng64(cp.rng_state)
    } else {
        Rng64(opts.seed)
    };
    let start_gen = resume.map_or(0, |cp| cp.generations_completed);

    let mut population_params: Vec<Nsga2Params> = Vec::new();
    if evaluated_map.is_empty() {
        population_params.push(Nsga2Params {
            fast_ema: 4,
            slow_ema: 12,
            offset_bps: 4,
            stop_loss_bps: 200,
            take_profit_bps: 400,
        });
        while population_params.len() < opts.population {
            let f = rng.usize_range(3, 8);
            let s = rng.usize_range(f + 2, 16);
            population_params.push(Nsga2Params {
                fast_ema: f,
                slow_ema: s,
                offset_bps: rng.u32_range(2, 20),
                stop_loss_bps: rng.u32_range(100, 400),
                take_profit_bps: rng.u32_range(200, 800),
            });
        }
    }

    for _gen in start_gen..opts.generations {
        let missing: Vec<Nsga2Params> = population_params
            .iter()
            .filter(|p| !evaluated_map.contains_key(*p))
            .cloned()
            .collect();

        if !missing.is_empty() {
            // Bounded deterministic parallel evaluation: each chunk returns ordered results.
            let chunk_sz = missing.len().div_ceil(workers).max(1);
            let mut results_buf: Vec<Result<Nsga2Candidate>> = Vec::with_capacity(missing.len());
            std::thread::scope(|scope| -> Result<()> {
                let mut handles = Vec::new();
                for chunk in missing.chunks(chunk_sz) {
                    handles.push(scope.spawn(|| {
                        chunk
                            .iter()
                            .cloned()
                            .map(|p| evaluate_single(p, &train_ds, opts))
                            .collect::<Vec<_>>()
                    }));
                }
                for h in handles {
                    let part = h
                        .join()
                        .map_err(|_| anyhow::anyhow!("NSGA-II evaluation worker panicked"))?;
                    results_buf.extend(part);
                }
                Ok(())
            })?;
            for res in results_buf {
                let cand = res?;
                evaluated_map.insert(cand.params.clone(), cand);
            }
        }

        let mut all_cands: Vec<Nsga2Candidate> = evaluated_map.values().cloned().collect();
        let _fronts = fast_non_dominated_sort(&mut all_cands);
        // Tournament / crowded sorting for survivor selection
        all_cands.sort_by(|a, b| {
            a.rank
                .cmp(&b.rank)
                .then_with(|| b.crowding_distance.total_cmp(&a.crowding_distance))
                .then_with(|| a.id.cmp(&b.id))
        });
        let parents: Vec<Nsga2Params> = all_cands
            .iter()
            .take(opts.population)
            .map(|c| c.params.clone())
            .collect();

        let mut next_pop = parents.clone();
        while next_pop.len() < opts.population * 2 {
            let p1 = &parents[rng.next() as usize % parents.len()];
            let p2 = &parents[rng.next() as usize % parents.len()];
            let fast_raw = if rng.next().is_multiple_of(2) {
                p1.fast_ema
            } else {
                p2.fast_ema
            };
            let slow_raw = if rng.next().is_multiple_of(2) {
                p1.slow_ema
            } else {
                p2.slow_ema
            };
            let f = fast_raw
                .saturating_add_signed(if rng.next() % 3 == 0 { 1 } else { 0 })
                .clamp(3, 10);
            let s = slow_raw.max(f + 2).clamp(6, 20);
            next_pop.push(Nsga2Params {
                fast_ema: f,
                slow_ema: s,
                offset_bps: ((p1.offset_bps + p2.offset_bps) / 2).clamp(1, 30),
                stop_loss_bps: ((p1.stop_loss_bps + p2.stop_loss_bps) / 2).clamp(50, 500),
                take_profit_bps: ((p1.take_profit_bps + p2.take_profit_bps) / 2).clamp(100, 1000),
            });
        }
        population_params = next_pop;
    }

    let mut all_cands: Vec<Nsga2Candidate> = evaluated_map.values().cloned().collect();
    let fronts = fast_non_dominated_sort(&mut all_cands);
    let pareto_front: Vec<Nsga2Candidate> = fronts
        .first()
        .map(|f| f.iter().map(|&idx| all_cands[idx].clone()).collect())
        .unwrap_or_default();

    let mut held_out_assessments = Vec::new();
    for cand in pareto_front.iter().take(4) {
        let rep = simulate_portfolio(&held_out_ds, &cand.params.to_unified_config(), &opts.exec)?;
        held_out_assessments.push(Nsga2HeldOutResult {
            candidate_id: cand.id.clone(),
            training_net_return_pct: cand.objectives.net_return_pct,
            held_out: rep,
        });
    }

    let mut hasher = Sha3_256::new();
    hasher.update(train_digest.as_bytes());
    let _ = hasher;

    let checkpoint = Nsga2Checkpoint {
        schema_version: 1,
        dataset_sha3_256: train_digest,
        experiment_fingerprint: exp_fingerprint,
        rng_state: rng.0,
        generations_completed: opts.generations,
        evaluated: all_cands,
    };

    Ok(Nsga2CampaignReport {
        schema_version: 1,
        algorithm: "constraint-aware-nsga2-v1".into(),
        simulated: true,
        auto_deployment_authorized: false,
        workers_used: workers,
        train_bars,
        embargo_bars,
        held_out_bars: timeline.len() - (train_bars + embargo_bars),
        pareto_front,
        held_out_assessments,
        checkpoint,
        limitations: vec![
            "Research proposals only (`auto_deployment_authorized = false`); never writes live trading configuration".into(),
            "Evaluated with strict embargo gap separating training selection from held-out assessment".into(),
        ],
    })
}

/// Persists a non-executable GENESIS research event record into durable SQLite `research_events`.
pub fn persist_genesis_research_event(
    storage: &crate::storage::Storage,
    experiment_id: &str,
    report: &Nsga2CampaignReport,
) -> Result<()> {
    ensure!(
        !report.auto_deployment_authorized,
        "GENESIS proposals remain non-executable without explicit owner authorization"
    );
    let best_id = report
        .pareto_front
        .first()
        .map(|c| c.id.clone())
        .unwrap_or_else(|| "none".into());
    storage.record_research_event(&crate::storage::ResearchEventRecord {
        experiment_id: experiment_id.into(),
        head: "GENESIS".into(),
        data_fingerprint: report.checkpoint.experiment_fingerprint.clone(),
        policy_id: best_id,
        provenance: "evolution_proposal".into(),
        activation_state: "proposal_only_non_executable".into(),
        payload_json: serde_json::to_string(report)?,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Candle;
    use chrono::{TimeZone, Utc};

    fn sample_dataset() -> MultiMarketDataset {
        let t0 = 1_700_000_000_000_i64;
        let mut bars = Vec::new();
        let mut px = 50_000.0;
        for i in 0..80i64 {
            px *= if i % 7 == 0 { 0.996 } else { 1.003 };
            bars.push(Candle {
                ts: Utc.timestamp_millis_opt(t0 + i * 3_600_000).unwrap(),
                open: px * 0.999,
                high: px * 1.005,
                low: px * 0.995,
                close: px,
                volume: 800.0,
            });
        }
        MultiMarketDataset {
            bars_by_symbol: BTreeMap::from([("BTC".into(), bars)]),
            specs: BTreeMap::new(),
            funding_bps_1h: BTreeMap::new(),
            outage_windows: vec![],
        }
    }

    #[test]
    fn parallel_worker_counts_produce_identical_nsga2_frontiers_and_prevent_leakage() {
        let ds = sample_dataset();
        let opts1 = Nsga2Options {
            max_workers: 1,
            population: 6,
            generations: 2,
            ..Default::default()
        };
        let opts4 = Nsga2Options {
            max_workers: 4,
            ..opts1.clone()
        };
        let r1 = run_nsga2_campaign(&ds, 40, 4, &opts1, None).unwrap();
        let r4 = run_nsga2_campaign(&ds, 40, 4, &opts4, None).unwrap();
        assert!(!r1.auto_deployment_authorized);
        assert_eq!(r1.pareto_front.len(), r4.pareto_front.len());
        assert_eq!(
            r1.pareto_front.iter().map(|c| &c.id).collect::<Vec<_>>(),
            r4.pareto_front.iter().map(|c| &c.id).collect::<Vec<_>>()
        );

        // Mutating held-out bars beyond index 44 must NEVER alter training Pareto front.
        let mut ds_shock = ds.clone();
        for b in ds_shock
            .bars_by_symbol
            .get_mut("BTC")
            .unwrap()
            .iter_mut()
            .skip(44)
        {
            b.close *= 0.2;
            b.open *= 0.2;
            b.high *= 0.2;
            b.low *= 0.2;
        }
        let r_shock = run_nsga2_campaign(&ds_shock, 40, 4, &opts1, None).unwrap();
        assert_eq!(
            r1.pareto_front
                .iter()
                .map(|c| c.objectives.net_return_pct)
                .collect::<Vec<_>>(),
            r_shock
                .pareto_front
                .iter()
                .map(|c| c.objectives.net_return_pct)
                .collect::<Vec<_>>()
        );
    }
}
