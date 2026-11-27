use chrono::{DateTime, Duration, Utc};
use hydra::core::Candle;
use hydra::hunter::MarketObservation;
use hydra::paper_pipeline::{SynapsePipeline, SynapsePipelineConfig, SIMULATION_ONLY};
use hydra::synapse_bus::{DataProvenance, HeadId, SynapseBus};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::process::Command;

#[test]
fn connected_multi_head_paper_pipeline_is_reproducible_and_emits_typed_bus() {
    assert!(SIMULATION_ONLY);
    let mut pipe = SynapsePipeline::new(SynapsePipelineConfig::default()).unwrap();
    let base_ts: DateTime<Utc> = DateTime::from_timestamp_millis(1_700_000_000_000).unwrap();
    let mut bars = Vec::new();
    let mut candle_map = BTreeMap::new();

    for step in 0..24i64 {
        let ts = base_ts + Duration::minutes(step);
        let px = if step < 18 {
            50_000.0 + (step as f64) * 280.0
        } else {
            55_040.0 + ((step - 18) as f64) * 650.0
        };
        bars.push(Candle {
            ts,
            open: px * 0.999,
            high: px * 1.002,
            low: px * 0.998,
            close: px,
            volume: 2_500.0,
        });
        candle_map.insert("BTC".to_string(), bars.clone());
        let mut marks = BTreeMap::new();
        marks.insert(
            "BTC".to_string(),
            Decimal::try_from(px).unwrap().round_dp(4),
        );
        let obs = MarketObservation {
            symbol: "BTC".into(),
            observed_at_ms: ts.timestamp_millis(),
            volume_24h_usd: Some(40_000_000.0),
            depth_usd: Some(400_000.0),
            spread_bps: Some(2.0),
            volatility_pct: Some(2.0),
            directionality: Some(0.82),
            funding_bps: Some(1.0),
            open_interest_usd: Some(15_000_000.0),
            coverage_fraction: Some(1.0),
            age_days: Some(365.0),
            observed_history_minutes: None,
            ema_ready: bars.len() >= 12,
        };
        pipe.step_cycle(ts.timestamp_millis(), &[obs], &candle_map, &marks)
            .unwrap();
    }

    assert!(pipe.bus.latest_for_head(HeadId::Hunter).is_some());
    assert!(pipe.bus.latest_for_head(HeadId::Cognition).is_some());
    assert!(pipe.bus.latest_for_head(HeadId::Fang).is_some());
    assert!(pipe.bus.latest_for_head(HeadId::Aegis).is_some());
    assert!(pipe.bus.latest_for_head(HeadId::Nexus).is_some());
    assert!(pipe.ledger.simulation_only);
    assert!(!pipe.ledger.closed_trades.is_empty() || !pipe.ledger.open_positions.is_empty());

    let jsonl = pipe.bus.to_jsonl().unwrap();
    let decoded = SynapseBus::from_jsonl(&jsonl).unwrap();
    assert_eq!(decoded.len(), pipe.bus.len());

    // Missing mark for an existing open position must fail closed on new entries.
    if let Some(_open_sym) = pipe.ledger.open_positions.keys().next().cloned() {
        let missing_marks = BTreeMap::new();
        let before_open = pipe.ledger.open_positions.len();
        let sum = pipe
            .step_cycle(
                (base_ts + Duration::minutes(25)).timestamp_millis(),
                &[],
                &candle_map,
                &missing_marks,
            )
            .unwrap();
        assert_eq!(sum.approved_entries, 0);
        assert_eq!(pipe.ledger.open_positions.len(), before_open);
        let latest_aegis = pipe.bus.latest_for_head(HeadId::Aegis).unwrap();
        assert_eq!(latest_aegis.provenance, DataProvenance::OperationalFault);
    }

    // Synthetic demo cycles must explicitly carry SyntheticTestFixture provenance.
    let mut demo = SynapsePipeline::new(SynapsePipelineConfig::default()).unwrap();
    demo.run_deterministic_demo_step().unwrap();
    let demo_hunter = demo.bus.latest_for_head(HeadId::Hunter).unwrap();
    assert_eq!(demo_hunter.provenance, DataProvenance::SyntheticTestFixture);
}

#[test]
fn synapse_paper_cli_runs_offline_without_wallet_or_config() {
    let dir = tempfile::tempdir().unwrap();
    let candles_path = dir.path().join("candles.json");
    let base_ts: DateTime<Utc> = DateTime::from_timestamp_millis(1_700_000_000_000).unwrap();
    let bars: Vec<Candle> = (0..22i64)
        .map(|k| {
            let px = 3_000.0 + k as f64 * 22.0;
            Candle {
                ts: base_ts + Duration::minutes(k),
                open: px * 0.999,
                high: px * 1.002,
                low: px * 0.998,
                close: px,
                volume: 1_200.0,
            }
        })
        .collect();
    std::fs::write(&candles_path, serde_json::to_string(&bars).unwrap()).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_hydra"))
        .arg("--no-banner")
        .arg("synapse-paper")
        .arg("--candles")
        .arg(&candles_path)
        .arg("--symbol")
        .arg("ETH")
        .arg("--json")
        .current_dir(dir.path())
        .output()
        .expect("execute hydra synapse-paper");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON output");
    assert_eq!(parsed["simulation_only"], true);
    assert_eq!(parsed["trading_authorized"], false);
    assert_eq!(parsed["cycles"], 22);
    assert!(parsed["events_emitted"].as_u64().unwrap_or(0) > 20);
}
