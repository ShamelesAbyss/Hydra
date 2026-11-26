use chrono::{TimeZone, Utc};
use hydra::{
    config::AppConfig,
    core::Candle,
    genesis::{self, SearchOptions},
};
#[test]
fn optimizer_step_resumes_offline_and_never_overwrites_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let candles = dir.path().join("synthetic-bars.json");
    let options = dir.path().join("options.json");
    let bars: Vec<_> = (0..300)
        .map(|i| Candle {
            ts: Utc.timestamp_millis_opt(1700000000000 + i * 60000).unwrap(),
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 1.0,
        })
        .collect();
    let search = SearchOptions {
        population: 2,
        generations: 2,
        max_evaluations: 4,
        ..Default::default()
    };
    std::fs::write(&candles, serde_json::to_vec(&bars).unwrap()).unwrap();
    std::fs::write(&options, serde_json::to_vec(&search).unwrap()).unwrap();
    let first = dir.path().join("generation-1.json");
    let second = dir.path().join("generation-2.json");
    let run = |input: Option<&std::path::Path>, output: &std::path::Path| {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_hydra"));
        cmd.current_dir(dir.path())
            .env("ENABLE_LIVE_TRADING", "true")
            .env("HYPERLIQUID_PRIVATE_KEY", "invalid-never-used")
            .args([
                "--config",
                concat!(env!("CARGO_MANIFEST_DIR"), "/config/strategies.toml"),
                "optimize-step",
                "--candles",
            ])
            .arg(&candles)
            .arg("--options")
            .arg(&options)
            .arg("--output")
            .arg(output);
        if let Some(p) = input {
            cmd.arg("--checkpoint").arg(p);
        }
        cmd.output().unwrap()
    };
    let a = run(None, &first);
    assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
    let initial = std::fs::read(&first).unwrap();
    let a: serde_json::Value = serde_json::from_slice(&a.stdout).unwrap();
    assert_eq!(a["complete"], false);
    assert_eq!(a["auto_deployment_authorized"], false);
    let b = run(Some(&first), &second);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    let result: serde_json::Value = serde_json::from_slice(&b.stdout).unwrap();
    assert_eq!(result["complete"], true);
    let cfg = AppConfig::load_research(
        concat!(env!("CARGO_MANIFEST_DIR"), "/config/strategies.toml"),
        None,
    )
    .unwrap();
    assert_eq!(
        result["report"],
        serde_json::to_value(genesis::optimize(&cfg, &bars, &search).unwrap()).unwrap()
    );
    assert!(!run(Some(&first), &first).status.success());
    assert_eq!(initial, std::fs::read(&first).unwrap());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 4);
}
