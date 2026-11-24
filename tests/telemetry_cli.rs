use hydra::storage::Storage;
use std::process::Command;

#[test]
fn telemetry_reads_an_existing_database_without_loading_trading_config() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("telemetry.db");
    let storage = Storage::open(&path).unwrap();
    storage.migrate().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_hydra"))
        .current_dir(directory.path())
        .args(["--config", "nonexistent.toml", "telemetry", "--db"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert!(!directory.path().join("logs").exists());
}
