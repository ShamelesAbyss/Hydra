//! Bounded CPU research search. Results are proposals and never write live config.
use crate::{
    config::AppConfig,
    core::Candle,
    oracle::{replay, ReplayOptions},
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterRange {
    pub min: u32,
    pub max: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterBounds {
    pub fast_ema: ParameterRange,
    pub slow_ema: ParameterRange,
    pub atr_period: ParameterRange,
    /// One unit equals 0.001 percentage points, so 180 means 0.18 percent.
    pub min_atr_milli_pct: ParameterRange,
}
impl Default for ParameterBounds {
    fn default() -> Self {
        Self {
            fast_ema: ParameterRange { min: 4, max: 24 },
            slow_ema: ParameterRange { min: 32, max: 96 },
            atr_period: ParameterRange { min: 7, max: 28 },
            min_atr_milli_pct: ParameterRange { min: 50, max: 500 },
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchOptions {
    pub seed: u64,
    pub population: usize,
    pub generations: usize,
    pub max_evaluations: usize,
    pub mutation_bps: u32,
    pub bounds: ParameterBounds,
    pub replay: ReplayOptions,
}
impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            seed: 1,
            population: 8,
            generations: 4,
            max_evaluations: 32,
            mutation_bps: 2500,
            bounds: ParameterBounds::default(),
            replay: ReplayOptions::default(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Parameters {
    pub fast_ema: u32,
    pub slow_ema: u32,
    pub atr_period: u32,
    pub min_atr_milli_pct: u32,
}
impl Parameters {
    pub fn id(&self) -> String {
        format!(
            "momentum-v1-{}-{}-{}-{}",
            self.fast_ema, self.slow_ema, self.atr_period, self.min_atr_milli_pct
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Objectives {
    pub net_return: f64,
    pub drawdown_fraction: f64,
    pub fee_fraction: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub id: String,
    pub parameters: Parameters,
    pub objectives: Objectives,
    pub trade_count: usize,
    pub first_generation: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct Generation {
    pub index: usize,
    pub new_candidate_ids: Vec<String>,
    pub cumulative_evaluations: usize,
    pub pareto_candidate_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct SearchReport {
    pub schema_version: u32,
    pub algorithm: String,
    pub simulated: bool,
    pub options: SearchOptions,
    pub strategy_profile: String,
    pub bar_count: usize,
    pub data_start_ms: i64,
    pub data_end_ms: i64,
    pub candidates: Vec<Candidate>,
    pub generations: Vec<Generation>,
    pub pareto_candidate_ids: Vec<String>,
    pub termination: String,
    pub limitations: Vec<String>,
}

/// Maximize return and minimize drawdown/cost. Equality is nondominance.
pub fn dominates(left: &Objectives, right: &Objectives) -> Result<bool> {
    validate_objectives(left)?;
    validate_objectives(right)?;
    Ok(left.net_return >= right.net_return
        && left.drawdown_fraction <= right.drawdown_fraction
        && left.fee_fraction <= right.fee_fraction
        && (left.net_return > right.net_return
            || left.drawdown_fraction < right.drawdown_fraction
            || left.fee_fraction < right.fee_fraction))
}
pub fn pareto_indices(objectives: &[Objectives]) -> Result<Vec<usize>> {
    ensure!(
        objectives.len() <= 4096,
        "Pareto input resource limit exceeded"
    );
    for objective in objectives {
        validate_objectives(objective)?;
    }
    let mut result = Vec::new();
    for (index, objective) in objectives.iter().enumerate() {
        let mut dominated = false;
        for (other_index, other) in objectives.iter().enumerate() {
            if index != other_index && dominates(other, objective)? {
                dominated = true;
                break;
            }
        }
        if !dominated {
            result.push(index);
        }
    }
    Ok(result)
}
fn validate_objectives(o: &Objectives) -> Result<()> {
    ensure!(
        [o.net_return, o.drawdown_fraction, o.fee_fraction]
            .iter()
            .all(|n| n.is_finite())
            && o.drawdown_fraction >= 0.0
            && o.fee_fraction >= 0.0,
        "invalid optimizer objective"
    );
    Ok(())
}

pub fn optimize(
    cfg: &AppConfig,
    candles: &[Candle],
    options: &SearchOptions,
) -> Result<SearchReport> {
    validate(cfg, candles, options)?;
    let mut random = Random(options.seed);
    let mut evaluated = BTreeMap::<Parameters, Candidate>::new();
    let mut generations = Vec::new();
    let mut parents = Vec::<Parameters>::new();
    let mut termination = "generation_budget".to_string();
    for generation in 0..options.generations {
        let remaining = options.max_evaluations - evaluated.len();
        if remaining == 0 {
            termination = "evaluation_budget".into();
            break;
        }
        let target = options.population.min(remaining);
        let mut proposed = BTreeMap::new();
        if generation == 0 {
            // A valid bound-corner seed guarantees that dataset/Oracle validation
            // executes even when random draws repeatedly violate EMA ordering.
            proposed.insert(
                Parameters {
                    fast_ema: options.bounds.fast_ema.min,
                    slow_ema: options.bounds.slow_ema.max,
                    atr_period: options.bounds.atr_period.min,
                    min_atr_milli_pct: options.bounds.min_atr_milli_pct.min,
                },
                (),
            );
        }
        for _ in 0..target * 128 {
            if proposed.len() >= target {
                break;
            }
            let candidate = if parents.is_empty() {
                random_parameters(&mut random, &options.bounds)
            } else {
                let first = &parents[random.index(parents.len())];
                let second = &parents[random.index(parents.len())];
                breed(
                    &mut random,
                    first,
                    second,
                    &options.bounds,
                    options.mutation_bps,
                )
            };
            if candidate.fast_ema >= candidate.slow_ema || evaluated.contains_key(&candidate) {
                continue;
            }
            proposed.insert(candidate, ());
            if proposed.len() == target {
                break;
            }
        }
        if proposed.is_empty() {
            termination = "unique_candidate_attempt_budget".into();
            break;
        }
        let mut new_ids = Vec::new();
        for parameters in proposed.into_keys() {
            let mut candidate_config = cfg.clone();
            candidate_config.strategies.global.enabled_strategies = vec!["momentum".into()];
            candidate_config.strategies.momentum.enabled = true;
            candidate_config.strategies.momentum.fast_ema = parameters.fast_ema as usize;
            candidate_config.strategies.momentum.slow_ema = parameters.slow_ema as usize;
            candidate_config.strategies.momentum.atr_period = parameters.atr_period as usize;
            candidate_config.strategies.momentum.min_atr_pct =
                parameters.min_atr_milli_pct as f64 / 1000.0;
            let result = replay(&candidate_config, candles, &options.replay)?;
            let objectives = Objectives {
                net_return: (result.final_equity - result.initial_equity) / result.initial_equity,
                drawdown_fraction: result.max_drawdown_fraction,
                fee_fraction: result.total_fees / result.initial_equity,
            };
            validate_objectives(&objectives)?;
            let id = parameters.id();
            new_ids.push(id.clone());
            evaluated.insert(
                parameters.clone(),
                Candidate {
                    id,
                    parameters,
                    objectives,
                    trade_count: result.trades.len(),
                    first_generation: generation,
                },
            );
        }
        let all: Vec<_> = evaluated.values().cloned().collect();
        let frontier =
            pareto_indices(&all.iter().map(|c| c.objectives.clone()).collect::<Vec<_>>())?;
        parents = frontier
            .iter()
            .map(|index| all[*index].parameters.clone())
            .collect();
        generations.push(Generation {
            index: generation,
            new_candidate_ids: new_ids,
            cumulative_evaluations: evaluated.len(),
            pareto_candidate_ids: frontier
                .iter()
                .map(|index| all[*index].id.clone())
                .collect(),
        });
    }
    let candidates: Vec<_> = evaluated.into_values().collect();
    let frontier = pareto_indices(
        &candidates
            .iter()
            .map(|c| c.objectives.clone())
            .collect::<Vec<_>>(),
    )?;
    Ok(SearchReport{schema_version:1,algorithm:"seeded-pareto-parent-mutation-crossover-v1".into(),simulated:true,
        options:options.clone(),strategy_profile:"momentum-only; remaining strategy configuration inherited".into(),
        bar_count:candles.len(),data_start_ms:candles.first().unwrap().ts.timestamp_millis(),
        data_end_ms:candles.last().unwrap().ts.timestamp_millis(),pareto_candidate_ids:frontier.iter().map(|index|candidates[*index].id.clone()).collect(),
        candidates,generations,termination,limitations:vec![
            "Pareto-parent evolutionary search with bounded retries; NSGA-II crowding/ranking is not implemented".into(),
            "Single input dataset; out-of-sample acceptance and walk-forward validation remain operator research work".into(),
            "Oracle candle execution assumptions and omitted funding/liquidation models apply".into(),
            "No live configuration is modified or deployed".into()]})
}

fn validate(cfg: &AppConfig, candles: &[Candle], o: &SearchOptions) -> Result<()> {
    ensure!(
        o.population > 0 && o.population <= 128 && o.generations > 0 && o.generations <= 100,
        "invalid population/generation budget"
    );
    ensure!(
        o.max_evaluations > 0 && o.max_evaluations <= 4096 && o.mutation_bps <= 10_000,
        "invalid evaluation/mutation budget"
    );
    let pareto_work = o
        .max_evaluations
        .checked_mul(o.max_evaluations)
        .and_then(|work| work.checked_mul(o.generations));
    ensure!(
        pareto_work.is_some_and(|work| work <= 50_000_000),
        "optimizer Pareto comparison resource budget exceeded"
    );
    ensure!(
        candles.len() > o.replay.warmup_bars,
        "insufficient research data"
    );
    let work = (candles.len() - o.replay.warmup_bars)
        .checked_mul(cfg.strategies.global.lookback_candles.min(candles.len()))
        .and_then(|n| n.checked_mul(o.max_evaluations));
    ensure!(
        work.is_some_and(|n| n <= 100_000_000),
        "optimizer cumulative simulation-work budget exceeded"
    );
    for range in [&o.bounds.fast_ema, &o.bounds.slow_ema, &o.bounds.atr_period] {
        ensure!(
            range.min >= 2 && range.min <= range.max && range.max <= 2048,
            "invalid indicator parameter range"
        );
    }
    ensure!(
        o.bounds.fast_ema.min < o.bounds.slow_ema.max,
        "EMA bounds have no valid ordering"
    );
    let required_history = (o.bounds.slow_ema.max as usize)
        .checked_add(cfg.strategies.momentum.breakout_lookback)
        .and_then(|length| length.checked_add(5));
    ensure!(
        required_history.is_some_and(|length| length <= cfg.strategies.global.lookback_candles)
            && o.bounds.atr_period.max as usize <= cfg.strategies.global.lookback_candles,
        "parameter bounds exceed available strategy lookback"
    );
    ensure!(
        o.bounds.min_atr_milli_pct.min <= o.bounds.min_atr_milli_pct.max
            && o.bounds.min_atr_milli_pct.max <= 100_000,
        "invalid volatility threshold range"
    );
    Ok(())
}
// SplitMix64 defines a stable, non-cryptographic research random stream.
struct Random(u64);
impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        value ^ (value >> 31)
    }
    fn index(&mut self, len: usize) -> usize {
        (self.next() % len as u64) as usize
    }
    fn range(&mut self, r: &ParameterRange) -> u32 {
        r.min + (self.next() % (u64::from(r.max - r.min) + 1)) as u32
    }
}
fn random_parameters(r: &mut Random, b: &ParameterBounds) -> Parameters {
    Parameters {
        fast_ema: r.range(&b.fast_ema),
        slow_ema: r.range(&b.slow_ema),
        atr_period: r.range(&b.atr_period),
        min_atr_milli_pct: r.range(&b.min_atr_milli_pct),
    }
}
fn breed(
    r: &mut Random,
    a: &Parameters,
    b: &Parameters,
    bounds: &ParameterBounds,
    mutation: u32,
) -> Parameters {
    fn gene(r: &mut Random, a: u32, b: u32, bounds: &ParameterRange, mutation: u32) -> u32 {
        if r.next() % 10_000 < u64::from(mutation) {
            r.range(bounds)
        } else if r.next() % 2 == 0 {
            a
        } else {
            b
        }
    }
    Parameters {
        fast_ema: gene(r, a.fast_ema, b.fast_ema, &bounds.fast_ema, mutation),
        slow_ema: gene(r, a.slow_ema, b.slow_ema, &bounds.slow_ema, mutation),
        atr_period: gene(r, a.atr_period, b.atr_period, &bounds.atr_period, mutation),
        min_atr_milli_pct: gene(
            r,
            a.min_atr_milli_pct,
            b.min_atr_milli_pct,
            &bounds.min_atr_milli_pct,
            mutation,
        ),
    }
}
