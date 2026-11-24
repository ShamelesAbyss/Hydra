use hydra::ledger::{
    AccountScope, Evidence, EvidenceSource, FinancialLedger, LedgerEnvironment, PnlConvention,
};
use hydra::ledger_hyperliquid::{parse_fills, parse_funding};
use hydra::storage::Storage;
use rust_decimal_macros::dec;
use serde_json::json;

fn fill() -> serde_json::Value {
    json!({"tid":12345678901234567u64,"oid":42,"coin":"ETH","side":"A","px":"100.000000000001",
        "sz":"0.100000000001","time":1800000000000i64,"closedPnl":"1.2345","fee":"0.003","feeToken":"USDC",
        "builderFee":"0.001","dir":"Close Long","crossed":true})
}
fn funding() -> serde_json::Value {
    json!({"time":1800000000000i64,"hash":format!("0x{}","ab".repeat(32)),"delta":{"type":"funding","coin":"ETH","usdc":" -0.012345","szi":"0.1","fundingRate":"0.01"}})
}

#[test]
fn raw_fill_preserves_exact_ids_and_decimals_without_double_builder_fee() {
    let rows = parse_fills(&json!([fill()]).to_string()).unwrap();
    let row = &rows[0];
    assert_eq!(row.fill_id, "12345678901234567");
    assert_eq!(row.quantity, dec!(0.100000000001));
    assert_eq!(row.price, dec!(100.000000000001));
    assert_eq!(row.closed_pnl_usdc, Some(dec!(1.2345)));
    assert_eq!(row.fee.as_ref().unwrap().amount, dec!(0.003));
    assert_eq!(row.pnl_convention, PnlConvention::Unverified);
}

#[test]
fn missing_fees_or_denomination_are_unknown_and_never_assumed_usdc() {
    for field in ["fee", "feeToken"] {
        let mut row = fill();
        row.as_object_mut().unwrap().remove(field);
        assert!(parse_fills(&json!([row]).to_string()).unwrap()[0]
            .fee
            .is_none());
    }
}

#[test]
fn invalid_or_numeric_financial_fields_and_missing_identity_reject_whole_page() {
    for (field, bad) in [
        ("px", json!(100.2)),
        ("sz", json!("NaN")),
        ("side", json!("LONG")),
        ("fee", json!("inf")),
        ("oid", json!(0)),
    ] {
        let mut row = fill();
        row[field] = bad;
        assert!(parse_fills(&json!([fill(), row]).to_string()).is_err());
    }
    let mut row = fill();
    row.as_object_mut().unwrap().remove("tid");
    assert!(parse_fills(&json!([row]).to_string()).is_err());
}

#[test]
fn funding_keys_separate_coins_with_same_transaction_hash_and_time() {
    let mut other = funding();
    other["delta"]["coin"] = json!("BTC");
    let rows = parse_funding(&json!([funding(), other]).to_string()).unwrap();
    assert_ne!(rows[0].funding_id, rows[1].funding_id);
    assert_eq!(rows[0].payment.as_ref().unwrap().amount, dec!(-0.012345));
}

#[test]
fn malformed_or_nonfunding_update_cannot_enter_funding_ledger() {
    for field in ["hash", "delta"] {
        let mut row = funding();
        row[field] = if field == "hash" {
            json!("missing")
        } else {
            json!({"type":"deposit","coin":"ETH","usdc":"3"})
        };
        assert!(parse_funding(&json!([row]).to_string()).is_err());
    }
}

#[test]
fn unsupported_collateral_mapping_fails_visibly_for_spot_and_hip3() {
    for coin in ["@107", "xyz:XYZ100"] {
        let mut row = fill();
        row["coin"] = json!(coin);
        assert!(parse_fills(&json!([row]).to_string()).is_err());
        let mut row = funding();
        row["delta"]["coin"] = json!(coin);
        assert!(parse_funding(&json!([row]).to_string()).is_err());
    }
}

#[test]
fn importing_raw_api_pnl_does_not_certify_fee_convention_or_unlock_execution() {
    let db = Storage::open(":memory:").unwrap();
    db.migrate().unwrap();
    let ledger = FinancialLedger::new(
        db,
        AccountScope::hyperliquid(
            LedgerEnvironment::Replay,
            "0x1111111111111111111111111111111111111111",
        )
        .unwrap(),
    )
    .unwrap();
    let fills = parse_fills(&json!([fill()]).to_string()).unwrap();
    ledger
        .ingest(
            &fills,
            &parse_funding(&json!([funding()]).to_string()).unwrap(),
            Evidence {
                source: EvidenceSource::OfflineFixture,
                observed_at_ms: 1800000000001,
            },
        )
        .unwrap();
    let flow = ledger.realized_cash_flow().unwrap();
    assert_eq!(flow.net_usdc, None);
    assert_eq!(flow.unknown_pnl_count, 1);
    assert_eq!(flow.known_fees_usdc, dec!(0.003));
    assert_eq!(flow.known_funding_usdc, dec!(-0.012345));
}
