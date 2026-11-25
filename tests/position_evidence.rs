//! Offline proof boundaries for complete default-perpetual position snapshots.
use hydra::adapters::position_evidence::parse_hyperliquid;
use rust_decimal_macros::dec;
use serde_json::json;
const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";
const NOW: i64 = 1_800_000_000_000;
fn row(symbol: &str, size: &str, price: Option<&str>) -> serde_json::Value {
    json!({"position":{"coin":symbol,"szi":size,"entryPx":price}})
}
fn parse(
    rows: Vec<serde_json::Value>,
) -> anyhow::Result<hydra::adapters::position_evidence::PositionSnapshot> {
    parse_hyperliquid(
        ACCOUNT,
        &json!({"time":NOW,"assetPositions":rows}).to_string(),
        NOW,
        NOW,
    )
}
#[test]
fn preserves_signed_decimal_facts_and_explicit_empty_snapshot() {
    let p = parse(vec![row(
        "BTC",
        "-1.000000",
        "100.000000000000000001".into(),
    )])
    .unwrap();
    assert_eq!(p.positions[0].signed_quantity, dec!(-1));
    assert_eq!(p.positions[0].entry_price, dec!(100.000000000000000001));
    assert!(parse(vec![]).unwrap().positions.is_empty());
    assert!(parse_hyperliquid(ACCOUNT, r#"{"time":1800000000000}"#, NOW, NOW).is_err());
}
#[test]
fn a_bad_or_duplicate_row_invalidates_the_complete_snapshot() {
    for rows in [
        vec![row("BTC", "0", None), row("BTC", "1", Some("100"))],
        vec![row("BTC", "1", Some("100")), row("BTC", "-1", Some("100"))],
        vec![row("BTC", "1", None)],
        vec![row("BTC", "1", Some("0"))],
        vec![row("BTC", "NaN", Some("100"))],
        vec![row("dex:BTC", "1", Some("100"))],
        vec![row("BTC", "1.00000000000000000000000000001", Some("100"))],
    ] {
        assert!(parse(rows).is_err());
    }
}
#[test]
fn zero_rows_need_no_entry_price_but_do_not_hide_duplicate_symbols() {
    let p = parse(vec![row("BTC", "0", None), row("ETH", "2", Some("99"))]).unwrap();
    assert_eq!(p.positions.len(), 1);
    assert_eq!(p.positions[0].symbol, "ETH");
}
#[test]
fn identity_environment_and_default_dex_are_mandatory() {
    let original = parse(vec![]).unwrap();
    for mutate in 0..5 {
        let mut p = original.clone();
        match mutate {
            0 => p.account = "invalid".into(),
            1 => p.venue = "other".into(),
            2 => p.environment = "testnet".into(),
            3 => p.perp_dex = "other".into(),
            _ => p
                .positions
                .push(hydra::adapters::position_evidence::ExactPosition {
                    symbol: "BTC".into(),
                    signed_quantity: dec!(0),
                    entry_price: dec!(1),
                }),
        };
        assert!(p.validate(NOW).is_err());
    }
}
#[test]
fn timestamps_cannot_authorize_stale_future_or_slow_reads() {
    let original = parse(vec![]).unwrap();
    for mutate in 0..7 {
        let mut p = original.clone();
        match mutate {
            0 => p.acquired_started_ms = NOW + 1,
            1 => p.acquired_started_ms = NOW - 30_001,
            2 => p.acquired_finished_ms = NOW + 1,
            3 => p.exchange_time_ms = NOW + 1,
            4 => p.exchange_time_ms = NOW - 30_001,
            5 => p.exchange_time_ms = 0,
            _ => p.acquired_started_ms = NOW - 15_001,
        };
        assert!(p.validate(NOW).is_err());
    }
    let mut p = original;
    p.exchange_time_ms = NOW - 30_000;
    p.acquired_started_ms = NOW - 15_000;
    p.validate(NOW).unwrap();
}
#[test]
fn payload_and_row_budgets_fail_closed_before_empty_proof() {
    assert!(parse_hyperliquid(ACCOUNT, &" ".repeat(4 * 1024 * 1024 + 1), NOW, NOW).is_err());
    assert!(parse(
        (0..1025)
            .map(|n| row(&format!("COIN{n}"), "0", None))
            .collect()
    )
    .is_err());
    let mut p = parse(vec![row("BTC", "1", Some("100"))]).unwrap();
    p.positions.push(p.positions[0].clone());
    assert!(p.validate(NOW).is_err());
}
