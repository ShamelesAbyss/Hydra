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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub id: String,
    pub parameters: Parameters,
    pub objectives: Objectives,
    pub trade_count: usize,
    pub first_generation: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    Ok(advance(cfg, candles, options, None, options.generations)?.report)
}

/// Advance complete generations without re-evaluating prior candidates.
/// Checkpoints are caller-owned research state, not authenticated financial evidence.
pub fn advance(
    cfg: &AppConfig,
    candles: &[Candle],
    options: &SearchOptions,
    checkpoint: Option<&SearchCheckpoint>,
    generations_to_run: usize,
) -> Result<SearchProgress> {
    validate(cfg, candles, options)?;
    ensure!(
        (1..=100).contains(&generations_to_run),
        "step generation budget must be 1..100"
    );
    let context = context_digest(cfg, candles, options)?;
    let (mut random, mut evaluated, mut generations, mut parents, mut termination) =
        if let Some(c) = checkpoint {
            validate_checkpoint(c, &context, options)?;
            (
                Random(c.state.random_state),
                c.state
                    .candidates
                    .iter()
                    .cloned()
                    .map(|candidate| (candidate.parameters.clone(), candidate))
                    .collect::<BTreeMap<_, _>>(),
                c.state.generations.clone(),
                c.state.parents.clone(),
                c.state.termination.clone(),
            )
        } else {
            (
                Random(options.seed),
                BTreeMap::<Parameters, Candidate>::new(),
                Vec::new(),
                Vec::<Parameters>::new(),
                "generation_budget".to_string(),
            )
        };
    let first_generation = generations.len();
    let already_complete = evaluated.len() >= options.max_evaluations
        || termination == "unique_candidate_attempt_budget";
    let end_generation = if already_complete {
        first_generation
    } else {
        first_generation
            .saturating_add(generations_to_run)
            .min(options.generations)
    };
    for generation in first_generation..end_generation {
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
            let candidate_config = configure_candidate(cfg, &parameters);
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
    if generations.len() < options.generations && evaluated.len() >= options.max_evaluations {
        termination = "evaluation_budget".into();
    }
    let complete = generations.len() >= options.generations
        || evaluated.len() >= options.max_evaluations
        || termination == "unique_candidate_attempt_budget";
    let mut checkpoint = SearchCheckpoint {
        schema_version: 1,
        context_sha3_256: context,
        state: SearchState {
            random_state: random.0,
            candidates: evaluated.values().cloned().collect(),
            generations: generations.clone(),
            parents,
            termination: termination.clone(),
        },
        checksum_sha3_256: String::new(),
    };
    checkpoint.checksum_sha3_256 = state_digest(&checkpoint)?;
    let candidates: Vec<_> = evaluated.into_values().collect();
    let frontier = pareto_indices(
        &candidates
            .iter()
            .map(|c| c.objectives.clone())
            .collect::<Vec<_>>(),
    )?;
    let report = SearchReport{schema_version:1,algorithm:"seeded-pareto-parent-mutation-crossover-v1".into(),simulated:true,
        options:options.clone(),strategy_profile:"momentum-only; remaining strategy configuration inherited".into(),
        bar_count:candles.len(),data_start_ms:candles.first().unwrap().ts.timestamp_millis(),
        data_end_ms:candles.last().unwrap().ts.timestamp_millis(),pareto_candidate_ids:frontier.iter().map(|index|candidates[*index].id.clone()).collect(),
        candidates,generations,termination,limitations:vec![
            "Pareto-parent evolutionary search with bounded retries; NSGA-II crowding/ranking is not implemented".into(),
            "Single input dataset; out-of-sample acceptance and walk-forward validation remain operator research work".into(),
            "Oracle candle execution assumptions and omitted funding/liquidation models apply".into(),
            "No live configuration is modified or deployed".into()]};
    Ok(SearchProgress {
        research_only: true,
        auto_deployment_authorized: false,
        complete,
        checkpoint,
        report,
    })
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchState {
    pub random_state: u64,
    pub candidates: Vec<Candidate>,
    pub generations: Vec<Generation>,
    pub parents: Vec<Parameters>,
    pub termination: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchCheckpoint {
    pub schema_version: u32,
    pub context_sha3_256: String,
    pub state: SearchState,
    pub checksum_sha3_256: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct SearchProgress {
    pub research_only: bool,
    pub auto_deployment_authorized: bool,
    pub complete: bool,
    pub checkpoint: SearchCheckpoint,
    pub report: SearchReport,
}
fn context_digest(cfg: &AppConfig, candles: &[Candle], options: &SearchOptions) -> Result<String> {
    use sha3::{Digest, Sha3_256};
    let mut hash = Sha3_256::new();
    hash.update(b"hydra-genesis-context-v1\0");
    hash.update(env!("CARGO_PKG_VERSION").as_bytes());
    // StrategyConfig contains a HashMap; normalize its order without touching keys.
    // No AppConfig Debug/serialization occurs: wallet/account/paths are excluded.
    let ordered: BTreeMap<_, _> = cfg.strategies.per_symbol.iter().collect();
    let mut strategies = cfg.strategies.clone();
    strategies.per_symbol.clear();
    hash.update(format!("{strategies:?}|{ordered:?}").as_bytes());
    hash.update(serde_json::to_vec(options)?);
    hash.update(serde_json::to_vec(candles)?);
    Ok(format!("{:x}", hash.finalize()))
}
fn state_digest(c: &SearchCheckpoint) -> Result<String> {
    use sha3::{Digest, Sha3_256};
    let payload = serde_json::to_vec(&(c.schema_version, &c.context_sha3_256, &c.state))?;
    Ok(format!("{:x}", Sha3_256::digest(payload)))
}
fn validate_checkpoint(c: &SearchCheckpoint, context: &str, o: &SearchOptions) -> Result<()> {
    ensure!(
        c.schema_version == 1 && c.context_sha3_256 == context,
        "checkpoint dataset/config/options/build context mismatch"
    );
    ensure!(
        c.state.candidates.len() <= o.max_evaluations
            && c.state.generations.len() <= o.generations
            && c.state.parents.len() <= o.max_evaluations,
        "checkpoint resource budget exceeded"
    );
    ensure!(
        c.checksum_sha3_256 == state_digest(c)?,
        "checkpoint checksum mismatch (not an authenticity proof)"
    );
    ensure!(
        [
            "generation_budget",
            "evaluation_budget",
            "unique_candidate_attempt_budget"
        ]
        .contains(&c.state.termination.as_str()),
        "invalid checkpoint termination"
    );
    let mut seen = std::collections::BTreeSet::new();
    for candidate in &c.state.candidates {
        let p = &candidate.parameters;
        let in_range = |v: u32, r: &ParameterRange| v >= r.min && v <= r.max;
        ensure!(
            candidate.id == p.id()
                && seen.insert(p.clone())
                && p.fast_ema < p.slow_ema
                && in_range(p.fast_ema, &o.bounds.fast_ema)
                && in_range(p.slow_ema, &o.bounds.slow_ema)
                && in_range(p.atr_period, &o.bounds.atr_period)
                && in_range(p.min_atr_milli_pct, &o.bounds.min_atr_milli_pct)
                && candidate.first_generation < c.state.generations.len()
                && candidate.trade_count <= o.replay.max_trades,
            "invalid checkpoint candidate"
        );
        validate_objectives(&candidate.objectives)?;
    }
    ensure!(
        c.state
            .candidates
            .windows(2)
            .all(|pair| pair[0].parameters < pair[1].parameters),
        "checkpoint candidates must use canonical ordering"
    );
    let mut count = 0;
    for (index, generation) in c.state.generations.iter().enumerate() {
        let new: Vec<_> = c
            .state
            .candidates
            .iter()
            .filter(|v| v.first_generation == index)
            .map(|v| v.id.clone())
            .collect();
        count += new.len();
        ensure!(
            generation.index == index
                && !new.is_empty()
                && new.len() <= o.population
                && generation.new_candidate_ids == new
                && generation.cumulative_evaluations == count
                && generation.pareto_candidate_ids.len() <= count,
            "invalid checkpoint generation history"
        );
    }
    ensure!(
        count == c.state.candidates.len(),
        "checkpoint history coverage mismatch"
    );
    let frontier = pareto_indices(
        &c.state
            .candidates
            .iter()
            .map(|v| v.objectives.clone())
            .collect::<Vec<_>>(),
    )?;
    let parents: Vec<_> = frontier
        .iter()
        .map(|i| c.state.candidates[*i].parameters.clone())
        .collect();
    ensure!(
        c.state.parents == parents,
        "checkpoint parent frontier mismatch"
    );
    if let Some(last) = c.state.generations.last() {
        let ids: Vec<_> = frontier
            .iter()
            .map(|i| c.state.candidates[*i].id.clone())
            .collect();
        ensure!(
            last.pareto_candidate_ids == ids,
            "checkpoint final frontier mismatch"
        );
    }
    Ok(())
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    fn fixture() -> (AppConfig, Vec<Candle>, SearchOptions) {
        let cfg = AppConfig::load_research(
            concat!(env!("CARGO_MANIFEST_DIR"), "/config/strategies.toml"),
            None,
        )
        .unwrap();
        let bars = (0..300)
            .map(|i| Candle {
                ts: Utc
                    .timestamp_millis_opt(1_700_000_000_000 + i * 60000)
                    .unwrap(),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 1.0,
            })
            .collect();
        let o = SearchOptions {
            population: 3,
            generations: 3,
            max_evaluations: 9,
            ..Default::default()
        };
        (cfg, bars, o)
    }
    #[test]
    fn serialized_resume_matches_uninterrupted_search_exactly() {
        let (c, b, o) = fixture();
        let expected = optimize(&c, &b, &o).unwrap();
        let mut progress = advance(&c, &b, &o, None, 1).unwrap();
        assert!(!progress.complete);
        for _ in 0..2 {
            let checkpoint: SearchCheckpoint =
                serde_json::from_slice(&serde_json::to_vec(&progress.checkpoint).unwrap()).unwrap();
            progress = advance(&c, &b, &o, Some(&checkpoint), 1).unwrap();
        }
        assert!(progress.complete);
        assert_eq!(
            serde_json::to_value(expected).unwrap(),
            serde_json::to_value(&progress.report).unwrap()
        );
        let done = advance(&c, &b, &o, Some(&progress.checkpoint), 1).unwrap();
        assert_eq!(
            serde_json::to_value(done.report).unwrap(),
            serde_json::to_value(progress.report).unwrap()
        );
    }
    #[test]
    fn altered_state_context_and_unsafe_step_are_rejected() {
        let (c, mut b, o) = fixture();
        let p = advance(&c, &b, &o, None, 1).unwrap();
        let mut changed = p.checkpoint.clone();
        changed.state.random_state ^= 1;
        assert!(advance(&c, &b, &o, Some(&changed), 1)
            .unwrap_err()
            .to_string()
            .contains("checksum"));
        b[0].close = 100.5;
        assert!(advance(&c, &b, &o, Some(&p.checkpoint), 1)
            .unwrap_err()
            .to_string()
            .contains("context"));
        assert!(advance(&c, &b, &o, None, 0).is_err());
        let mut different = o.clone();
        different.seed += 1;
        assert!(advance(&c, &b, &different, Some(&p.checkpoint), 1).is_err());
    }
    #[test]
    fn impossible_rechecksummed_state_is_rejected() {
        let (c, b, o) = fixture();
        let mut p = advance(&c, &b, &o, None, 1).unwrap().checkpoint;
        p.state.candidates[0].parameters.fast_ema = 0;
        p.checksum_sha3_256 = state_digest(&p).unwrap();
        assert!(advance(&c, &b, &o, Some(&p), 1).is_err());
    }
    #[test]
    fn checkpoint_publication_preserves_previous_file_and_cleans_temporary_link() {
        let (c, b, o) = fixture();
        let checkpoint = advance(&c, &b, &o, None, 1).unwrap().checkpoint;
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("generation-1.json");
        crate::genesis_checkpoint::save_new(&path, &checkpoint).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(crate::genesis_checkpoint::save_new(&path, &checkpoint).is_err());
        assert_eq!(before, std::fs::read(&path).unwrap());
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
        let loaded: SearchCheckpoint = serde_json::from_slice(&before).unwrap();
        assert!(advance(&c, &b, &o, Some(&loaded), 1).is_ok());
    }
    #[test]
    fn strategy_map_order_is_normalized_and_secrets_are_excluded() {
        let (mut a, b, o) = fixture();
        let mut c = a.clone();
        a.strategies.per_symbol.clear();
        c.strategies.per_symbol.clear();
        for key in ["ETH", "BTC"] {
            a.strategies
                .per_symbol
                .insert(key.into(), Default::default());
        }
        for key in ["BTC", "ETH"] {
            c.strategies
                .per_symbol
                .insert(key.into(), Default::default());
        }
        c.hl_private_key = Some("must-not-be-serialized".into());
        let p = advance(&a, &b, &o, None, 1).unwrap();
        assert_eq!(
            context_digest(&a, &b, &o).unwrap(),
            context_digest(&c, &b, &o).unwrap()
        );
        assert!(!serde_json::to_string(&p)
            .unwrap()
            .contains("must-not-be-serialized"));
        assert!(advance(&c, &b, &o, Some(&p.checkpoint), 1).is_ok());
    }
}

pub(crate) fn configure_candidate(cfg: &AppConfig, parameters: &Parameters) -> AppConfig {
    let mut candidate_config = cfg.clone();
    candidate_config.strategies.global.enabled_strategies = vec!["momentum".into()];
    candidate_config.strategies.momentum.enabled = true;
    candidate_config.strategies.momentum.fast_ema = parameters.fast_ema as usize;
    candidate_config.strategies.momentum.slow_ema = parameters.slow_ema as usize;
    candidate_config.strategies.momentum.atr_period = parameters.atr_period as usize;
    candidate_config.strategies.momentum.min_atr_pct = parameters.min_atr_milli_pct as f64 / 1000.0;
    candidate_config
}
