//! Synthetic REST-decoded native acknowledgment coverage. No network or orders.
use chrono::Utc;
use hydra::{
    core::{OpenPosition, OrderIntent, OrderResult, Side},
    execution_facts::{self, PreciseFill},
    ledger::{
        AccountScope, Evidence, EvidenceSource, ExchangeFill, FillSide, FinancialLedger,
        LedgerEnvironment, PnlConvention, TokenAmount,
    },
    storage::Storage,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";
fn setup_at(
    db: Storage,
    quantity: &str,
    reduce: bool,
) -> (Storage, OrderIntent, OrderResult, FinancialLedger) {
    db.migrate().unwrap();
    db.bind_trading_account(ACCOUNT, false).unwrap();
    let order = OrderIntent {
        id: uuid::Uuid::new_v4().to_string(),
        venue: "hyperliquid".into(),
        symbol: "ETH".into(),
        side: if reduce { Side::Short } else { Side::Long },
        size: 2.0,
        limit_price: Some(100.0),
        reduce_only: reduce,
        stop_price: None,
        take_profit: None,
        source_signal: None,
        created_at: Utc::now(),
    };
    db.reserve_intent(&order).unwrap();
    let f = PreciseFill {
        quantity: quantity.into(),
        average_price: "100.00".into(),
        order_id: "42".into(),
    };
    let result = OrderResult {
        intent_id: order.id.clone(),
        accepted: true,
        venue_order_id: Some("42".into()),
        filled_size: quantity.parse().unwrap(),
        avg_fill_price: Some(100.0),
        message: "offline native fixture".into(),
    };
    execution_facts::record_receipt(&db, "filled", &result, Some(&f)).unwrap();
    let ledger = FinancialLedger::new(
        db.clone(),
        AccountScope::hyperliquid(LedgerEnvironment::Mainnet, ACCOUNT).unwrap(),
    )
    .unwrap();
    ledger.bind_intent(&order.id).unwrap();
    (db, order, result, ledger)
}
fn setup(quantity: &str, reduce: bool) -> (Storage, OrderIntent, OrderResult, FinancialLedger) {
    setup_at(Storage::open(":memory:").unwrap(), quantity, reduce)
}
fn fill(order: &OrderIntent, id: &str, quantity: Decimal, cloid: bool) -> ExchangeFill {
    ExchangeFill {
        fill_id: id.into(),
        order_id: 42,
        client_order_id: cloid.then(|| order.id.clone()),
        symbol: "ETH".into(),
        side: if order.side.is_buy() {
            FillSide::Buy
        } else {
            FillSide::Sell
        },
        quantity,
        price: dec!(100),
        closed_pnl_usdc: Some(dec!(0)),
        pnl_convention: PnlConvention::Unverified,
        fee: Some(TokenAmount {
            token: "USDC".into(),
            amount: dec!(0.01),
        }),
        timestamp_ms: 1_800_000_000_000,
    }
}
fn ingest(ledger: &FinancialLedger, fills: &[ExchangeFill]) {
    ledger
        .ingest(
            fills,
            &[],
            Evidence {
                source: EvidenceSource::RestSnapshot,
                observed_at_ms: 1_800_000_000_001,
            },
        )
        .unwrap();
}
fn assert_held(db: &Storage, id: &str) {
    let state: String = db
        .with_conn(|c| {
            c.query_row(
                "SELECT state FROM execution_intents WHERE intent_id=?1",
                [id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(state, "filled");
    assert!(db.ensure_execution_resolved().is_err());
}
#[test]
fn complete_native_quantity_deduplicates_cloid_oid_arms_and_preserves_unknown_net() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("coverage.db");
    let (db, order, _, ledger) = setup_at(Storage::open(&path).unwrap(), "2.0000", false);
    ingest(
        &ledger,
        &[
            fill(&order, "a", dec!(0.5), true),
            fill(&order, "b", dec!(1.5), false),
        ],
    );
    let result = ledger.acknowledged_order_evidence(&order.id).unwrap();
    assert_eq!(result.acknowledged_quantity, dec!(2));
    assert_eq!(result.evidence.fill_ids, vec!["a", "b"]);
    assert_eq!(result.evidence.fees_usdc, Some(dec!(0.02)));
    assert_eq!(result.evidence.net_before_funding_usdc, None);
    assert!(!result.round_trip_settlement_verified);
    assert!(result.scope.starts_with("mainnet:hyperliquid:"));
    assert_held(&db, &order.id);
    drop(ledger);
    drop(db);
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    let ledger = FinancialLedger::new(
        db,
        AccountScope::hyperliquid(LedgerEnvironment::Mainnet, ACCOUNT).unwrap(),
    )
    .unwrap();
    assert_eq!(
        ledger.acknowledged_order_evidence(&order.id).unwrap(),
        result
    );
}
#[test]
fn partial_sets_remain_partial_and_excess_acknowledged_quantity_is_rejected() {
    let (_, order, _, ledger) = setup("2", false);
    ingest(&ledger, &[fill(&order, "partial", dec!(1), true)]);
    assert_eq!(
        ledger.order_evidence(&order.id).unwrap().filled_quantity,
        dec!(1)
    );
    assert!(ledger.acknowledged_order_evidence(&order.id).is_err());
    let (_, order, _, ledger) = setup("1", false);
    ingest(&ledger, &[fill(&order, "excess", dec!(1.5), true)]);
    assert!(ledger.order_evidence(&order.id).is_err());
    assert!(ledger.acknowledged_order_evidence(&order.id).is_err());
}
#[test]
fn legacy_float_receipt_is_not_relabelled_as_exact_native_acknowledgment() {
    let (db, order, _, ledger) = setup("2", false);
    db.with_conn(|c| {
        c.execute(
            "DELETE FROM execution_fill_facts WHERE intent_id=?1",
            [&order.id],
        )
    })
    .unwrap();
    ingest(&ledger, &[fill(&order, "legacy", dec!(2), true)]);
    assert_eq!(
        ledger.order_evidence(&order.id).unwrap().filled_quantity,
        dec!(2)
    );
    assert!(ledger.acknowledged_order_evidence(&order.id).is_err());
    assert_held(&db, &order.id);
}
#[test]
fn replay_and_other_account_scopes_cannot_certify_mainnet_native_facts() {
    let (db, order, _, _) = setup("2", false);
    for (env, account) in [
        (LedgerEnvironment::Replay, ACCOUNT),
        (
            LedgerEnvironment::Mainnet,
            "0x2222222222222222222222222222222222222222",
        ),
    ] {
        let wrong =
            FinancialLedger::new(db.clone(), AccountScope::hyperliquid(env, account).unwrap())
                .unwrap();
        assert!(wrong.acknowledged_order_evidence(&order.id).is_err());
    }
    assert_held(&db, &order.id);
}
#[test]
fn reduction_uses_exact_native_quantity_instead_of_reconstructed_float() {
    let (db, order, res, ledger) = setup("1.999999999999999999", true);
    let position = OpenPosition {
        id: 0,
        venue: "hyperliquid".into(),
        symbol: "ETH".into(),
        side: "LONG".into(),
        size: 2.0,
        entry_price: 99.0,
        stop_price: None,
        take_profit: None,
        opened_at: Utc::now(),
        strategy: None,
        trailing_high: None,
    };
    let id = db.insert_open_position(&position).unwrap();
    db.with_conn(|c|c.execute("INSERT INTO confirmed_reductions(intent_id,position_id,filled_size,exit_price,remaining_size,gross_pnl,reason,confirmed_at) VALUES(?1,?2,?3,100,0,0,'offline',?4)",rusqlite::params![order.id,id,res.filled_size,Utc::now().to_rfc3339()])).unwrap();
    ingest(
        &ledger,
        &[fill(&order, "exact", dec!(1.999999999999999999), true)],
    );
    assert_eq!(
        ledger.verify_reduction(&order.id).unwrap().filled_quantity,
        dec!(1.999999999999999999)
    );
    let net: Option<f64> = db
        .with_conn(|c| {
            c.query_row(
                "SELECT net_pnl FROM confirmed_reductions WHERE intent_id=?1",
                [&order.id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(net, None);
    db.with_conn(|c| {
        c.execute(
            "UPDATE confirmed_reductions SET filled_size=1 WHERE intent_id=?1",
            [&order.id],
        )
    })
    .unwrap();
    assert!(ledger.verify_reduction(&order.id).is_err());
}
#[test]
fn unknown_foreign_and_rebate_fees_keep_their_economic_meaning() {
    for variant in 0..4 {
        let (db, order, _, ledger) = setup("2", false);
        let mut f = fill(&order, "cost", dec!(2), true);
        f.fee = match variant {
            0 => None,
            1 => Some(TokenAmount {
                token: "USDT".into(),
                amount: dec!(1),
            }),
            2 => Some(TokenAmount {
                token: "USDC".into(),
                amount: dec!(-0.01),
            }),
            _ => Some(TokenAmount {
                token: "USDC".into(),
                amount: dec!(0),
            }),
        };
        ingest(&ledger, &[f]);
        let result = ledger.acknowledged_order_evidence(&order.id).unwrap();
        assert_eq!(
            result.evidence.fees_usdc,
            match variant {
                0 | 1 => None,
                2 => Some(dec!(-0.01)),
                _ => Some(dec!(0)),
            }
        );
        assert_eq!(result.evidence.net_before_funding_usdc, None);
        assert!(!result.round_trip_settlement_verified);
        assert_held(&db, &order.id);
    }
}
#[test]
fn malformed_fill_payload_or_index_provenance_never_certifies_coverage() {
    for variant in 0..4 {
        let (db, order, _, ledger) = setup("2", false);
        let mut f = fill(&order, "corrupt", dec!(2), true);
        ingest(&ledger, &[f.clone()]);
        match variant {
            0 => {
                f.quantity = dec!(-2);
                db.with_conn(|c| {
                    c.execute(
                        "UPDATE ledger_fills SET payload_json=?1",
                        [serde_json::to_string(&f).unwrap()],
                    )
                })
                .unwrap();
            }
            1 => {
                db.with_conn(|c| c.execute("UPDATE ledger_fills SET order_id='99'", []))
                    .unwrap();
            }
            3 => {
                db.with_conn(|c| c.execute("UPDATE ledger_fills SET symbol=?1", ["x".repeat(161)]))
                    .unwrap();
            }
            _ => {
                db.with_conn(|c| {
                    c.execute(
                        "UPDATE ledger_fills SET payload_json=?1",
                        [" ".repeat(32769)],
                    )
                })
                .unwrap();
            }
        };
        assert!(ledger.acknowledged_order_evidence(&order.id).is_err());
        assert_held(&db, &order.id);
    }
}
#[test]
fn native_binding_and_durable_result_conflicts_remain_held() {
    for variant in 0..2 {
        let (db, order, mut res, ledger) = setup("2", false);
        ingest(&ledger, &[fill(&order, "filled", dec!(2), true)]);
        if variant == 0 {
            db.with_conn(|c| {
                c.execute(
                    "UPDATE meta_kv SET value=?1 WHERE key='trading_account_binding'",
                    ["0x2222222222222222222222222222222222222222"],
                )
            })
            .unwrap();
        } else {
            res.avg_fill_price = Some(101.0);
            db.with_conn(|c| {
                c.execute(
                    "UPDATE execution_intents SET result_json=?1 WHERE intent_id=?2",
                    rusqlite::params![serde_json::to_string(&res).unwrap(), order.id],
                )
            })
            .unwrap();
        }
        assert!(ledger.acknowledged_order_evidence(&order.id).is_err());
        assert_held(&db, &order.id);
    }
}
