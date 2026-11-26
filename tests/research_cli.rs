//! Synthetic flat candles are used only to verify the offline command boundary.
use chrono::{TimeZone, Utc};
use hydra::core::Candle;
use std::process::Command;

#[test]
fn research_commands_emit_simulation_json_without_creating_live_storage() {
    let directory = tempfile::tempdir().unwrap();
    let candles: Vec<Candle> = (0..500)
        .map(|i| Candle {
            ts: Utc
                .timestamp_millis_opt(1_700_000_000_000 + i * 60_000)
                .unwrap(),
            open: 100.0,
            high: 100.0,
            low: 100.0,
            close: 100.0,
            volume: 1.0,
        })
        .collect();
    let input = directory.path().join("synthetic-test-only.json");
    std::fs::write(&input, serde_json::to_vec(&candles).unwrap()).unwrap();
    for command in ["backtest", "optimize", "validate-research"] {
        let output = Command::new(env!("CARGO_BIN_EXE_hydra"))
            .current_dir(directory.path())
            .env("ENABLE_LIVE_TRADING", "true")
            .env(
                "HYPERLIQUID_PRIVATE_KEY",
                "OFFLINE_INVALID_KEY_MUST_NEVER_BE_USED",
            )
            .args([
                "--config",
                concat!(env!("CARGO_MANIFEST_DIR"), "/config/strategies.toml"),
                command,
                "--candles",
            ])
            .arg(&input)
            .output()
            .unwrap();
        assert!(output.status.success(), "{} {:?}", command, output.stderr);
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["simulated"], true);
        assert!(!directory.path().join("data").exists());
        assert!(!directory.path().join("logs").exists());
    }
}
