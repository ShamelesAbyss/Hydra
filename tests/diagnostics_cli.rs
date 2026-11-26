use hydra::diagnostics::{self, DiagnosticKind};
#[test]
fn diagnostics_bypass_wallet_engine_config_and_storage() {
    let dir = tempfile::tempdir().unwrap();
    for (kind, name) in [
        (DiagnosticKind::Fang, "fang"),
        (DiagnosticKind::Cognition, "cognition"),
        (DiagnosticKind::Aegis, "aegis"),
    ] {
        let input = dir.path().join(format!("{name}.json"));
        let bytes = diagnostics::template(kind).unwrap();
        std::fs::write(&input, &bytes).unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_hydra"))
            .current_dir(dir.path())
            .args([
                "--config",
                "missing.toml",
                "diagnose",
                "--kind",
                name,
                "--snapshot",
            ])
            .arg(&input)
            .env("ENABLE_LIVE_TRADING", "true")
            .env("HYPERLIQUID_PRIVATE_KEY", "invalid-never-used")
            .env("DB_PATH", dir.path().join("must-not-exist.sqlite"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["kind"], name);
        let r = &report["report"];
        if name == "cognition" {
            assert_eq!(r["trading_authorized"], false);
            assert_eq!(r["financial_evidence_verified"], false);
        } else {
            assert_eq!(r["execution_authorized"], false);
        }
        if name == "aegis" {
            assert!(r["within_supplied_budgets"].is_null());
            assert_eq!(r["risk_release_authorized"], false);
        }
        assert_eq!(std::fs::read_to_string(&input).unwrap(), bytes);
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
}
#[test]
fn empty_anchor_template_cannot_become_an_invented_price() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("anchor.json");
    std::fs::write(
        &p,
        diagnostics::template(DiagnosticKind::EmaAnchor).unwrap(),
    )
    .unwrap();
    assert!(diagnostics::evaluate(DiagnosticKind::EmaAnchor, &p).is_err());
}
