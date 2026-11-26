use chrono::{TimeZone, Utc};
use hydra::{
    config::AppConfig,
    core::Candle,
    genesis::{self, SearchOptions},
};
#[test]
fn frozen_candidate_cli_uses_only_local_research_files() {
    let dir = tempfile::tempdir().unwrap();
    let train = dir.path().join("training.json");
    let held = dir.path().join("held-out.json");
    let opt = dir.path().join("options.json");
    let cp = dir.path().join("checkpoint.json");
    let bars = |start: i64, n: usize| -> Vec<Candle> {
        (0..n)
            .map(|i| Candle {
                ts: Utc.timestamp_millis_opt(start + i as i64 * 60000).unwrap(),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 1.0,
            })
            .collect()
    };
    let training = bars(1700000000000, 300);
    let hold = bars(1700000000000 + 300 * 60000, 500);
    let options = SearchOptions {
        population: 2,
        generations: 1,
        max_evaluations: 2,
        ..Default::default()
    };
    let cfg = AppConfig::load_research(
        concat!(env!("CARGO_MANIFEST_DIR"), "/config/strategies.toml"),
        None,
    )
    .unwrap();
    let checkpoint = genesis::advance(&cfg, &training, &options, None, 1)
        .unwrap()
        .checkpoint;
    for (p, value) in [
        (&train, serde_json::to_vec(&training).unwrap()),
        (&held, serde_json::to_vec(&hold).unwrap()),
        (&opt, serde_json::to_vec(&options).unwrap()),
        (&cp, serde_json::to_vec(&checkpoint).unwrap()),
    ] {
        std::fs::write(p, value).unwrap();
    }
    let before = std::fs::read(&cp).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_hydra"))
        .current_dir(dir.path())
        .env("ENABLE_LIVE_TRADING", "true")
        .env("HYPERLIQUID_PRIVATE_KEY", "invalid-never-used")
        .args([
            "--config",
            concat!(env!("CARGO_MANIFEST_DIR"), "/config/strategies.toml"),
            "assess-candidates",
            "--training",
        ])
        .arg(&train)
        .arg("--held-out")
        .arg(&held)
        .arg("--options")
        .arg(&opt)
        .arg("--checkpoint")
        .arg(&cp)
        .arg("--limit")
        .arg("2")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["simulated"], true);
    assert_eq!(report["auto_deployment_authorized"], false);
    assert_eq!(report["financial_evidence_verified"], false);
    assert_eq!(report["assessments"].as_array().unwrap().len(), 2);
    assert_eq!(before, std::fs::read(&cp).unwrap());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 4);
}
