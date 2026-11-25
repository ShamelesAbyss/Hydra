use hydra::hunter::{HunterPolicy, HunterRequest};
#[test]
fn hunter_cli_is_offline_and_does_not_open_configuration_or_database() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("snapshot.json");
    let request = HunterRequest {
        as_of_ms: 0,
        source: "test snapshot".into(),
        approved: Default::default(),
        excluded: Default::default(),
        current_watchlist: vec![],
        policy: HunterPolicy::default(),
        markets: vec![],
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    std::fs::write(&input, &bytes).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hydra"))
        .current_dir(dir.path())
        .args(["--config", "missing.toml", "hunter", "--snapshot"])
        .arg(&input)
        .env("ENABLE_LIVE_TRADING", "true")
        .env("HYPERLIQUID_PRIVATE_KEY", "invalid-never-used")
        .env("DB_PATH", dir.path().join("must-not-exist.sqlite"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["read_only"], true);
    assert_eq!(report["source_is_live"], false);
    assert_eq!(report["trading_authorized"], false);
    assert_eq!(std::fs::read(&input).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
