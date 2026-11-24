//! Decimal financial evidence tests. No network clients or credentials are used.
use chrono::Utc;
use hydra::core::{OrderIntent, OrderResult, Side};
use hydra::ledger::{
    AccountScope, Evidence, EvidenceSource, ExchangeFill, FillSide, FinancialLedger,
    FundingPayment, LedgerEnvironment, PnlConvention, TokenAmount,
};
use hydra::storage::Storage;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";
const OTHER: &str = "0x2222222222222222222222222222222222222222";
fn storage() -> Storage {
    let db = Storage::open(":memory:").unwrap();
    db.migrate().unwrap();
    db
}
fn make_ledger(db: &Storage) -> FinancialLedger {
    FinancialLedger::new(
        db.clone(),
        AccountScope::hyperliquid(LedgerEnvironment::Replay, ACCOUNT).unwrap(),
    )
    .unwrap()
}
fn evidence(source: EvidenceSource) -> Evidence {
    Evidence {
        source,
        observed_at_ms: 1_800_000_000_000,
    }
}
fn money(amount: Decimal) -> TokenAmount {
    TokenAmount {
        token: "USDC".into(),
        amount,
    }
}
fn fill(id: &str) -> ExchangeFill {
    ExchangeFill {
        fill_id: id.into(),
        order_id: 42,
        client_order_id: None,
        symbol: "ETH".into(),
        side: FillSide::Sell,
        quantity: dec!(0.1),
        price: dec!(100.1),
        closed_pnl_usdc: Some(dec!(0.01)),
        pnl_convention: PnlConvention::BeforeFees,
        fee: Some(money(dec!(0.001))),
        timestamp_ms: 1_799_999_999_000,
    }
}
fn funding(id: &str, amount: Decimal) -> FundingPayment {
    FundingPayment {
        funding_id: id.into(),
        symbol: "ETH".into(),
        payment: Some(money(amount)),
        timestamp_ms: 1_799_999_999_500,
    }
}
fn reserve(db: &Storage, id: &str, oid: Option<&str>) -> OrderIntent {
    let intent = OrderIntent {
        id: id.into(),
        venue: "hyperliquid".into(),
        symbol: "ETH".into(),
        side: Side::Short,
        size: 0.2,
        limit_price: None,
        reduce_only: true,
        stop_price: None,
        take_profit: None,
        source_signal: None,
        created_at: Utc::now(),
    };
    db.reserve_intent(&intent).unwrap();
    if let Some(oid) = oid {
        let res = OrderResult {
            intent_id: id.into(),
            accepted: true,
            venue_order_id: Some(oid.into()),
            filled_size: 0.2,
            avg_fill_price: Some(100.1),
            message: "offline fixture".into(),
        };
        db.finish_intent(id, "filled", Some(&res)).unwrap();
    }
    intent
}

#[test]
fn account_identity_is_canonical_and_separate_from_environment() {
    assert!(AccountScope::hyperliquid(LedgerEnvironment::Mainnet, "not-an-account").is_err());
    assert!(AccountScope::hyperliquid(
        LedgerEnvironment::Mainnet,
        "0x0000000000000000000000000000000000000000"
    )
    .is_err());
    let upper = "0xABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD";
    assert_eq!(
        AccountScope::hyperliquid(LedgerEnvironment::Replay, upper)
            .unwrap()
            .account(),
        upper.to_ascii_lowercase()
    );
    assert_ne!(
        AccountScope::hyperliquid(LedgerEnvironment::Mainnet, ACCOUNT)
            .unwrap()
            .key(),
        AccountScope::hyperliquid(LedgerEnvironment::Replay, ACCOUNT)
            .unwrap()
            .key()
    );
}

#[test]
fn snapshot_websocket_race_and_replay_apply_each_cash_flow_once() {
    let db = storage();
    let ledger = make_ledger(&db);
    let fills = [fill("tid-1"), fill("tid-2")];
    let funding = [funding("fund-1", dec!(-0.002))];
    let initial = ledger
        .ingest(&fills, &funding, evidence(EvidenceSource::UserWebSocket))
        .unwrap();
    assert_eq!(initial.new_fills, 2);
    assert_eq!(initial.new_funding, 1);
    let replay = ledger
        .ingest(&fills, &funding, evidence(EvidenceSource::RestSnapshot))
        .unwrap();
    assert_eq!(replay.replays, 3);
    assert_eq!(replay.new_fills, 0);
    let flow = ledger.realized_cash_flow().unwrap();
    assert_eq!(flow.known_closed_pnl_usdc, dec!(0.02));
    assert_eq!(flow.known_fees_usdc, dec!(0.002));
    assert_eq!(flow.known_funding_usdc, dec!(-0.002));
    assert_eq!(flow.net_usdc, Some(dec!(0.016)));
    let observations: i64 = db
        .with_conn(|c| c.query_row("SELECT COUNT(*) FROM ledger_observations", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(observations, 6); // bounded rows per event/source, not per replay
}

#[test]
fn identifiers_are_scoped_to_account_and_environment() {
    let db = storage();
    let replay = make_ledger(&db);
    let other = FinancialLedger::new(
        db.clone(),
        AccountScope::hyperliquid(LedgerEnvironment::Replay, OTHER).unwrap(),
    )
    .unwrap();
    let paper = FinancialLedger::new(
        db.clone(),
        AccountScope::hyperliquid(LedgerEnvironment::Paper, ACCOUNT).unwrap(),
    )
    .unwrap();
    for ledger in [&replay, &other, &paper] {
        assert_eq!(
            ledger
                .ingest(
                    &[fill("same-tid")],
                    &[],
                    evidence(EvidenceSource::OfflineFixture)
                )
                .unwrap()
                .new_fills,
            1
        );
        assert_eq!(ledger.realized_cash_flow().unwrap().fill_count, 1);
    }
}

#[test]
fn offline_fixture_cannot_contaminate_mainnet_make_ledger() {
    let db = storage();
    let ledger = FinancialLedger::new(
        db,
        AccountScope::hyperliquid(LedgerEnvironment::Mainnet, ACCOUNT).unwrap(),
    )
    .unwrap();
    assert!(ledger
        .ingest(&[fill("id")], &[], evidence(EvidenceSource::OfflineFixture))
        .is_err());
    assert_eq!(ledger.realized_cash_flow().unwrap().fill_count, 0);
}

#[test]
fn unknown_fees_pnl_and_funding_never_become_zero_cost() {
    let db = storage();
    let ledger = make_ledger(&db);
    let mut f = fill("id");
    f.fee = None;
    f.closed_pnl_usdc = None;
    let mut payment = funding("fund", dec!(1));
    payment.payment = None;
    ledger
        .ingest(&[f], &[payment], evidence(EvidenceSource::OfflineFixture))
        .unwrap();
    let flow = ledger.realized_cash_flow().unwrap();
    assert_eq!(
        (
            flow.unknown_fee_count,
            flow.unknown_pnl_count,
            flow.unknown_funding_count
        ),
        (1, 1, 1)
    );
    assert_eq!(flow.net_usdc, None);
}

#[test]
fn later_authoritative_data_enriches_missing_costs_without_double_counting() {
    let db = storage();
    let ledger = make_ledger(&db);
    let full = fill("id");
    let full_funding = funding("fund", dec!(-0.001));
    let mut incomplete = full.clone();
    incomplete.fee = None;
    incomplete.closed_pnl_usdc = None;
    let mut incomplete_funding = full_funding.clone();
    incomplete_funding.payment = None;
    ledger
        .ingest(
            &[incomplete.clone()],
            &[incomplete_funding.clone()],
            evidence(EvidenceSource::UserWebSocket),
        )
        .unwrap();
    let enriched = ledger
        .ingest(
            &[full],
            &[full_funding],
            evidence(EvidenceSource::RestSnapshot),
        )
        .unwrap();
    assert_eq!(enriched.enriched, 2);
    assert_eq!(
        ledger.realized_cash_flow().unwrap().net_usdc,
        Some(dec!(0.008))
    );
    let replay = ledger
        .ingest(
            &[incomplete],
            &[incomplete_funding],
            evidence(EvidenceSource::UserWebSocket),
        )
        .unwrap();
    assert_eq!(replay.replays, 2);
    assert_eq!(
        ledger.realized_cash_flow().unwrap().net_usdc,
        Some(dec!(0.008))
    );
}

#[test]
fn non_usdc_fees_require_explicit_conversion_and_rebates_remain_signed() {
    let db = storage();
    let ledger = make_ledger(&db);
    let mut f = fill("id");
    f.fee = Some(TokenAmount {
        token: "USDT".into(),
        amount: dec!(0.001),
    });
    ledger
        .ingest(&[f], &[], evidence(EvidenceSource::OfflineFixture))
        .unwrap();
    let flow = ledger.realized_cash_flow().unwrap();
    assert_eq!(flow.net_usdc, None);
    assert!(flow.unsupported_tokens.contains("USDT"));
    let db = storage();
    let ledger = make_ledger(&db);
    let mut rebate = fill("rebate");
    rebate.fee = Some(money(dec!(-0.001)));
    ledger
        .ingest(&[rebate], &[], evidence(EvidenceSource::OfflineFixture))
        .unwrap();
    assert_eq!(
        ledger.realized_cash_flow().unwrap().net_usdc,
        Some(dec!(0.011))
    );
}

#[test]
fn conflicting_identifier_rolls_back_the_whole_batch() {
    let db = storage();
    let ledger = make_ledger(&db);
    ledger
        .ingest(
            &[fill("existing")],
            &[],
            evidence(EvidenceSource::OfflineFixture),
        )
        .unwrap();
    let mut conflict = fill("existing");
    conflict.quantity = dec!(0.2);
    assert!(ledger
        .ingest(
            &[fill("new"), conflict],
            &[funding("new-fund", dec!(1))],
            evidence(EvidenceSource::OfflineFixture)
        )
        .is_err());
    let flow = ledger.realized_cash_flow().unwrap();
    assert_eq!(flow.fill_count, 1);
    assert_eq!(flow.funding_count, 0);
}

#[test]
fn known_fee_correction_is_not_silently_overwritten() {
    let db = storage();
    let ledger = make_ledger(&db);
    ledger
        .ingest(&[fill("id")], &[], evidence(EvidenceSource::OfflineFixture))
        .unwrap();
    let mut conflict = fill("id");
    conflict.fee = Some(money(dec!(0.02)));
    assert!(ledger
        .ingest(&[conflict], &[], evidence(EvidenceSource::RestSnapshot))
        .is_err());
    assert_eq!(
        ledger.realized_cash_flow().unwrap().known_fees_usdc,
        dec!(0.001)
    );
}

#[test]
fn invalid_and_overflowing_fills_fail_before_any_persistence() {
    for mut f in [fill("zero"), fill("overflow"), fill("bad-cloid")] {
        match f.fill_id.as_str() {
            "zero" => f.quantity = Decimal::ZERO,
            "overflow" => {
                f.quantity = Decimal::MAX;
                f.price = dec!(2)
            }
            _ => f.client_order_id = Some("bad".into()),
        }
        let db = storage();
        let ledger = make_ledger(&db);
        assert!(ledger
            .ingest(
                &[fill("valid"), f],
                &[],
                evidence(EvidenceSource::OfflineFixture)
            )
            .is_err());
        assert_eq!(ledger.realized_cash_flow().unwrap().fill_count, 0);
    }
}

#[test]
fn sql_failure_rolls_back_evidence_and_provenance_together() {
    let db = storage();
    let ledger = make_ledger(&db);
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_funding BEFORE INSERT ON ledger_funding BEGIN SELECT RAISE(ABORT,'injected failure'); END;")).unwrap();
    assert!(ledger
        .ingest(
            &[fill("id")],
            &[funding("fund", dec!(1))],
            evidence(EvidenceSource::OfflineFixture)
        )
        .is_err());
    assert_eq!(ledger.realized_cash_flow().unwrap().fill_count, 0);
    let n: i64 = db
        .with_conn(|c| c.query_row("SELECT COUNT(*) FROM ledger_observations", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn fills_and_cash_flow_survive_restart_with_exact_decimal_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    {
        let db = Storage::open(&path).unwrap();
        db.migrate().unwrap();
        make_ledger(&db)
            .ingest(
                &[fill("id")],
                &[funding("fund", dec!(0.003))],
                evidence(EvidenceSource::OfflineFixture),
            )
            .unwrap();
    }
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    let ledger = make_ledger(&db);
    assert_eq!(
        ledger.realized_cash_flow().unwrap().net_usdc,
        Some(dec!(0.012))
    );
    assert_eq!(
        ledger
            .ingest(&[fill("id")], &[], evidence(EvidenceSource::OfflineFixture))
            .unwrap()
            .replays,
        1
    );
}

#[test]
fn unknown_ack_can_recover_partial_fills_by_cloid_after_explicit_account_binding() {
    let db = storage();
    let ledger = make_ledger(&db);
    let id = uuid::Uuid::new_v4().to_string();
    reserve(&db, &id, None);
    let mut one = fill("1");
    one.client_order_id = Some(format!("0x{}", id.replace('-', "")));
    let mut two = fill("2");
    two.client_order_id = Some(id.clone());
    two.price = dec!(100.3);
    ledger
        .ingest(&[one, two], &[], evidence(EvidenceSource::OfflineFixture))
        .unwrap();
    assert!(ledger.order_evidence(&id).is_err());
    ledger.bind_intent(&id).unwrap();
    let proof = ledger.order_evidence(&id).unwrap();
    assert_eq!(proof.filled_quantity, dec!(0.2));
    assert_eq!(proof.weighted_average_price, dec!(100.2));
    assert_eq!(proof.net_before_funding_usdc, Some(dec!(0.018)));
    assert!(db.ensure_execution_resolved().is_err()); // evidence does not silently bypass recovery
}

#[test]
fn account_binding_cannot_be_reassigned_and_mismatched_fill_is_rejected() {
    let db = storage();
    let ledger = make_ledger(&db);
    let id = uuid::Uuid::new_v4().to_string();
    reserve(&db, &id, Some("42"));
    ledger.bind_intent(&id).unwrap();
    let other = FinancialLedger::new(
        db.clone(),
        AccountScope::hyperliquid(LedgerEnvironment::Replay, OTHER).unwrap(),
    )
    .unwrap();
    assert!(other.bind_intent(&id).is_err());
    let mut f = fill("id");
    f.symbol = "BTC".into();
    ledger
        .ingest(&[f], &[], evidence(EvidenceSource::OfflineFixture))
        .unwrap();
    assert!(ledger.order_evidence(&id).is_err());
}

#[test]
fn order_id_cloid_conflicts_and_excess_fill_quantity_do_not_certify_execution() {
    let db = storage();
    let ledger = make_ledger(&db);
    let id = uuid::Uuid::new_v4().to_string();
    reserve(&db, &id, Some("42"));
    ledger.bind_intent(&id).unwrap();
    let mut f = fill("id");
    f.client_order_id = Some(uuid::Uuid::new_v4().to_string());
    ledger
        .ingest(&[f], &[], evidence(EvidenceSource::OfflineFixture))
        .unwrap();
    assert!(ledger.order_evidence(&id).is_err());
    let db = storage();
    let ledger = make_ledger(&db);
    let id = uuid::Uuid::new_v4().to_string();
    reserve(&db, &id, Some("42"));
    ledger.bind_intent(&id).unwrap();
    let mut f = fill("id");
    f.quantity = dec!(0.3);
    ledger
        .ingest(&[f], &[], evidence(EvidenceSource::OfflineFixture))
        .unwrap();
    assert!(ledger.order_evidence(&id).is_err());
}

#[test]
fn pnl_conventions_never_double_subtract_fees_or_assume_unknown_semantics() {
    for (convention, reported, expected) in [
        (PnlConvention::Unverified, dec!(0.01), None),
        (PnlConvention::BeforeFees, dec!(0.01), Some(dec!(0.009))),
        (PnlConvention::AfterFees, dec!(0.009), Some(dec!(0.009))),
    ] {
        let db = storage();
        let ledger = make_ledger(&db);
        let mut f = fill("id");
        f.closed_pnl_usdc = Some(reported);
        f.pnl_convention = convention;
        ledger
            .ingest(&[f], &[], evidence(EvidenceSource::OfflineFixture))
            .unwrap();
        assert_eq!(ledger.realized_cash_flow().unwrap().net_usdc, expected);
    }
}

#[test]
fn unknown_convention_can_be_verified_but_known_conflicting_convention_is_rejected() {
    let db = storage();
    let ledger = make_ledger(&db);
    let mut f = fill("id");
    f.pnl_convention = PnlConvention::Unverified;
    ledger
        .ingest(&[f.clone()], &[], evidence(EvidenceSource::UserWebSocket))
        .unwrap();
    f.pnl_convention = PnlConvention::BeforeFees;
    assert_eq!(
        ledger
            .ingest(&[f.clone()], &[], evidence(EvidenceSource::RestSnapshot))
            .unwrap()
            .enriched,
        1
    );
    assert_eq!(
        ledger.realized_cash_flow().unwrap().net_usdc,
        Some(dec!(0.009))
    );
    f.pnl_convention = PnlConvention::AfterFees;
    assert!(ledger
        .ingest(&[f], &[], evidence(EvidenceSource::RestSnapshot))
        .is_err());
}
