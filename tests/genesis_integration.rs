use chrono::{TimeZone, Utc};
use hydra::{
    config::AppConfig,
    core::Candle,
    genesis::{dominates, optimize, pareto_indices, Objectives, SearchOptions},
    oracle::ReplayOptions,
};

fn cfg() -> AppConfig {
    AppConfig::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/config/strategies.toml"),
        None,
    )
    .unwrap()
}
fn candles() -> Vec<Candle> {
    (0..300)
        .map(|i| {
            let close = 100.0 + (i as f64 / 12.0).sin() * 3.0 + i as f64 * 0.02;
            Candle {
                ts: Utc
                    .timestamp_millis_opt(1_700_000_000_000 + i * 60_000)
                    .unwrap(),
                open: close,
                high: close + 0.8,
                low: close - 0.8,
                close,
                volume: 1000.0,
            }
        })
        .collect()
}
fn options() -> SearchOptions {
    SearchOptions {
        population: 4,
        generations: 2,
        max_evaluations: 8,
        replay: ReplayOptions {
            warmup_bars: 200,
            ..Default::default()
        },
        ..Default::default()
    }
}
#[test]
fn seeded_evaluation_is_reproducible_and_preserves_input_config() {
    let cfg = cfg();
    let before = format!("{:?}", cfg.strategies.momentum);
    let first = optimize(&cfg, &candles(), &options()).unwrap();
    let second = optimize(&cfg, &candles(), &options()).unwrap();
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );
    assert_eq!(format!("{:?}", cfg.strategies.momentum), before);
    assert!(!first.candidates.is_empty());
    assert!(first.candidates.len() <= 8);
    let ids: std::collections::BTreeSet<_> = first.candidates.iter().map(|c| &c.id).collect();
    assert_eq!(ids.len(), first.candidates.len());
    assert!(first
        .candidates
        .iter()
        .all(|c| c.parameters.fast_ema < c.parameters.slow_ema));
}
#[test]
fn another_seed_changes_the_candidate_population() {
    let one = optimize(&cfg(), &candles(), &options()).unwrap();
    let mut settings = options();
    settings.seed = 99;
    let two = optimize(&cfg(), &candles(), &settings).unwrap();
    assert_ne!(
        one.candidates.iter().map(|c| &c.id).collect::<Vec<_>>(),
        two.candidates.iter().map(|c| &c.id).collect::<Vec<_>>()
    );
}
#[test]
fn pareto_front_preserves_tradeoffs_and_equal_objectives() {
    let a = Objectives {
        net_return: 0.1,
        drawdown_fraction: 0.1,
        fee_fraction: 0.01,
    };
    let dominated = Objectives {
        net_return: 0.05,
        drawdown_fraction: 0.2,
        fee_fraction: 0.02,
    };
    let tradeoff = Objectives {
        net_return: 0.2,
        drawdown_fraction: 0.3,
        fee_fraction: 0.01,
    };
    assert!(dominates(&a, &dominated).unwrap());
    assert!(!dominates(&a, &a).unwrap());
    assert_eq!(
        pareto_indices(&[a.clone(), dominated, tradeoff, a]).unwrap(),
        vec![0, 2, 3]
    );
}
#[test]
fn invalid_inputs_and_search_budgets_fail_closed() {
    let mut settings = options();
    settings.max_evaluations = 4097;
    assert!(optimize(&cfg(), &candles(), &settings).is_err());
    settings = options();
    settings.bounds.fast_ema.min = 200;
    settings.bounds.slow_ema.max = 100;
    assert!(optimize(&cfg(), &candles(), &settings).is_err());
    let invalid = Objectives {
        net_return: f64::NAN,
        drawdown_fraction: 0.0,
        fee_fraction: 0.0,
    };
    assert!(pareto_indices(&[invalid]).is_err());
}
#[test]
fn fixed_parameter_space_stops_without_duplicate_evaluations() {
    let mut settings = options();
    settings.bounds.fast_ema.min = 10;
    settings.bounds.fast_ema.max = 10;
    settings.bounds.slow_ema.min = 40;
    settings.bounds.slow_ema.max = 40;
    settings.bounds.atr_period.min = 14;
    settings.bounds.atr_period.max = 14;
    settings.bounds.min_atr_milli_pct.min = 100;
    settings.bounds.min_atr_milli_pct.max = 100;
    let report = optimize(&cfg(), &candles(), &settings).unwrap();
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.termination, "unique_candidate_attempt_budget");
}
