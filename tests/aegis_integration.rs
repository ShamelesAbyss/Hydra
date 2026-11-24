use hydra::{
    aegis::{AccountSnapshot, Aegis, RearmAuthorization, RiskDecision, RiskLimits},
    storage::Storage,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::{
    collections::BTreeMap,
    sync::{Arc, Barrier},
};

const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";
const OTHER: &str = "0x2222222222222222222222222222222222222222";
const NOW: i64 = 1_000_000;
fn limits() -> RiskLimits {
    RiskLimits {
        max_total_exposure_ratio: dec!(0.5),
        max_market_notional: dec!(300),
        max_leverage: dec!(2),
        max_drawdown_ratio: dec!(0.15),
        max_snapshot_age_ms: 5_000,
    }
}
fn snapshot(revision: i64, equity: Decimal) -> AccountSnapshot {
    AccountSnapshot {
        revision,
        observed_at_ms: NOW,
        equity,
        exposure_by_market: BTreeMap::new(),
    }
}
fn storage() -> Storage {
    let db = Storage::open(":memory:").unwrap();
    db.migrate().unwrap();
    db
}
fn guard(db: &Storage) -> Aegis {
    Aegis::new(db.clone(), ACCOUNT, limits()).unwrap()
}
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn approved(decision: RiskDecision) -> bool {
    matches!(decision, RiskDecision::Approved(_))
}

#[test]
fn pending_orders_and_reservations_share_hard_exposure_budget() {
    let db = storage();
    let aegis = guard(&db);
    let mut account = snapshot(1, dec!(1000));
    account.exposure_by_market.insert("BTC".into(), dec!(200));
    assert!(approved(
        aegis
            .reserve_entry(&id(), "ETH", dec!(250), dec!(1), &account, NOW)
            .unwrap()
    ));
    assert!(!approved(
        aegis
            .reserve_entry(&id(), "SOL", dec!(51), dec!(1), &account, NOW)
            .unwrap()
    ));
    assert!(approved(
        aegis
            .reserve_entry(&id(), "SOL", dec!(50), dec!(1), &account, NOW)
            .unwrap()
    ));
    assert_eq!(aegis.status().unwrap().reserved_notional, dec!(300));
}

#[test]
fn same_market_and_requested_leverage_limits_are_independent() {
    let db = storage();
    let aegis = guard(&db);
    let account = snapshot(1, dec!(1000));
    assert!(!approved(
        aegis
            .reserve_entry(&id(), "BTC", dec!(100), dec!(3), &account, NOW)
            .unwrap()
    ));
    assert!(approved(
        aegis
            .reserve_entry(&id(), "BTC", dec!(250), dec!(2), &account, NOW)
            .unwrap()
    ));
    assert!(!approved(
        aegis
            .reserve_entry(&id(), "BTC", dec!(51), dec!(1), &account, NOW)
            .unwrap()
    ));
    assert_eq!(aegis.status().unwrap().active_reservations, 1);
}

#[test]
fn concurrent_reservations_cannot_overspend() {
    let db = storage();
    let aegis = guard(&db);
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|i| {
            let gate = aegis.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                approved(
                    gate.reserve_entry(
                        &id(),
                        &format!("ASSET{i}"),
                        dec!(100),
                        dec!(1),
                        &snapshot(1, dec!(1000)),
                        NOW,
                    )
                    .unwrap(),
                )
            })
        })
        .collect();
    let accepted = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .filter(|accepted| *accepted)
        .count();
    assert_eq!(accepted, 5);
    assert_eq!(aegis.status().unwrap().reserved_notional, dec!(500));
}

#[test]
fn independent_connections_use_atomic_sqlite_reservations() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("risk.db");
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    let aegis = guard(&db);
    let other = Storage::open(&path).unwrap();
    let second = guard(&other);
    let barrier = Arc::new(Barrier::new(2));
    let workers: Vec<_> = [aegis, second]
        .into_iter()
        .enumerate()
        .map(|(i, gate)| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                approved(
                    gate.reserve_entry(
                        &id(),
                        &format!("ASSET{i}"),
                        dec!(300),
                        dec!(1),
                        &snapshot(1, dec!(1000)),
                        NOW,
                    )
                    .unwrap(),
                )
            })
        })
        .collect();
    assert_eq!(
        workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .filter(|accepted| *accepted)
            .count(),
        1
    );
}

#[test]
fn drawdown_breaker_survives_restart_and_requires_explicit_rearm() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("risk.db");
    let db = Storage::open(&path).unwrap();
    db.migrate().unwrap();
    let aegis = guard(&db);
    aegis
        .apply_reconciled_snapshot(&snapshot(1, dec!(1000)), &[], "initial", NOW)
        .unwrap();
    let decline = snapshot(2, dec!(800));
    let blocked = aegis
        .reserve_entry(&id(), "ETH", dec!(10), dec!(1), &decline, NOW)
        .unwrap();
    assert!(matches!(
        blocked,
        RiskDecision::Blocked {
            breaker_latched: true,
            ..
        }
    ));
    drop(aegis);
    drop(db);
    let reopened = Storage::open(path).unwrap();
    let aegis = guard(&reopened);
    assert!(aegis.status().unwrap().breaker_reason.is_some());
    assert!(!approved(
        aegis
            .reserve_entry(
                &id(),
                "ETH",
                dec!(10),
                dec!(1),
                &snapshot(3, dec!(1000)),
                NOW
            )
            .unwrap()
    ));
    assert!(RearmAuthorization::explicit("", "acknowledged", true).is_err());
    aegis
        .rearm(
            RearmAuthorization::explicit("operator", "reviewed account and risk", false).unwrap(),
            &snapshot(3, dec!(1000)),
            NOW,
        )
        .unwrap();
    assert!(approved(
        aegis
            .reserve_entry(
                &id(),
                "ETH",
                dec!(10),
                dec!(1),
                &snapshot(3, dec!(1000)),
                NOW
            )
            .unwrap()
    ));
}

#[test]
fn rearm_below_high_water_requires_explicit_peak_reset() {
    let db = storage();
    let aegis = guard(&db);
    aegis
        .apply_reconciled_snapshot(&snapshot(1, dec!(1000)), &[], "initial", NOW)
        .unwrap();
    aegis
        .apply_reconciled_snapshot(&snapshot(2, dec!(800)), &[], "decline", NOW)
        .unwrap();
    assert!(aegis
        .rearm(
            RearmAuthorization::explicit("operator", "reviewed", false).unwrap(),
            &snapshot(2, dec!(800)),
            NOW
        )
        .is_err());
    aegis
        .rearm(
            RearmAuthorization::explicit("operator", "explicitly accepting new equity peak", true)
                .unwrap(),
            &snapshot(2, dec!(800)),
            NOW,
        )
        .unwrap();
    assert_eq!(aegis.status().unwrap().high_water_equity, dec!(800));
    assert!(aegis.status().unwrap().breaker_reason.is_none());
}

#[test]
fn stale_future_and_changed_snapshot_revisions_fail_closed() {
    let db = storage();
    let aegis = guard(&db);
    let valid = snapshot(1, dec!(1000));
    for observed in [NOW - 5_001, NOW + 1] {
        let mut stale = valid.clone();
        stale.observed_at_ms = observed;
        assert!(aegis
            .reserve_entry(&id(), "ETH", dec!(10), dec!(1), &stale, NOW)
            .is_err());
    }
    assert!(approved(
        aegis
            .reserve_entry(&id(), "ETH", dec!(10), dec!(1), &valid, NOW)
            .unwrap()
    ));
    assert!(aegis
        .reserve_entry(
            &id(),
            "SOL",
            dec!(10),
            dec!(1),
            &snapshot(1, dec!(999)),
            NOW
        )
        .is_err());
    let mut regressed = valid.clone();
    regressed.revision = 0;
    assert!(aegis
        .reserve_entry(&id(), "SOL", dec!(10), dec!(1), &regressed, NOW)
        .is_err());
    assert_eq!(aegis.status().unwrap().active_reservations, 1);
}

#[test]
fn only_newer_reconciliation_can_release_reserved_exposure() {
    let db = storage();
    let aegis = guard(&db);
    let order = id();
    let first = snapshot(1, dec!(1000));
    assert!(approved(
        aegis
            .reserve_entry(&order, "ETH", dec!(200), dec!(1), &first, NOW)
            .unwrap()
    ));
    assert!(aegis
        .apply_reconciled_snapshot(&first, std::slice::from_ref(&order), "too-early", NOW)
        .is_err());
    let mut filled = snapshot(2, dec!(1000));
    filled.exposure_by_market.insert("ETH".into(), dec!(200));
    aegis
        .apply_reconciled_snapshot(
            &filled,
            std::slice::from_ref(&order),
            "exchange-fill-proof",
            NOW,
        )
        .unwrap();
    assert_eq!(aegis.status().unwrap().active_reservations, 0);
    aegis
        .apply_reconciled_snapshot(
            &filled,
            std::slice::from_ref(&order),
            "exchange-fill-proof",
            NOW,
        )
        .unwrap();
    assert!(aegis
        .apply_reconciled_snapshot(
            &filled,
            std::slice::from_ref(&order),
            "different-proof",
            NOW
        )
        .is_err());
    assert!(!approved(
        aegis
            .reserve_entry(&id(), "ETH", dec!(101), dec!(1), &filled, NOW)
            .unwrap()
    ));
    assert!(aegis
        .reserve_entry(&order, "ETH", dec!(10), dec!(1), &filled, NOW)
        .is_err());
}

#[test]
fn reconciliation_is_account_scoped_and_atomic() {
    let db = storage();
    let aegis = guard(&db);
    let other = Aegis::new(db.clone(), OTHER, limits()).unwrap();
    let first = id();
    let second = id();
    let initial = snapshot(1, dec!(1000));
    aegis
        .reserve_entry(&first, "ETH", dec!(100), dec!(1), &initial, NOW)
        .unwrap();
    other
        .reserve_entry(&second, "ETH", dec!(100), dec!(1), &initial, NOW)
        .unwrap();
    assert!(aegis
        .apply_reconciled_snapshot(
            &snapshot(2, dec!(1000)),
            &[first, second],
            "mixed-accounts",
            NOW
        )
        .is_err());
    assert_eq!(aegis.status().unwrap().active_reservations, 1);
    assert_eq!(aegis.status().unwrap().latest_snapshot_revision, Some(1));
}

#[test]
fn protective_reductions_ignore_entry_breakers_but_cannot_reverse_positions() {
    let db = storage();
    let aegis = guard(&db);
    aegis.trip("operator emergency", NOW).unwrap();
    assert!(Aegis::validate_reduction(dec!(2), dec!(2)).is_ok());
    assert!(Aegis::validate_reduction(dec!(2), dec!(0.5)).is_ok());
    for requested in [dec!(-1), dec!(0), dec!(2.1)] {
        assert!(Aegis::validate_reduction(dec!(2), requested).is_err());
    }
    assert!(aegis.status().unwrap().breaker_reason.is_some());
}

#[test]
fn risk_policy_cannot_be_silently_loosened_by_another_guard() {
    let db = storage();
    assert!(Aegis::new(
        db.clone(),
        "0x0000000000000000000000000000000000000000",
        limits()
    )
    .is_err());
    let _aegis = guard(&db);
    let mut looser = limits();
    looser.max_total_exposure_ratio = dec!(10);
    assert!(Aegis::new(db.clone(), ACCOUNT, looser).is_err());
    let mut invalid = limits();
    invalid.max_drawdown_ratio = Decimal::ONE;
    assert!(Aegis::new(db, OTHER, invalid).is_err());
}

#[test]
fn database_errors_and_decimal_overflow_never_approve() {
    let db = storage();
    let aegis = guard(&db);
    let mut account = snapshot(1, Decimal::MAX);
    account
        .exposure_by_market
        .insert("ETH".into(), Decimal::MAX);
    assert!(!approved(
        aegis
            .reserve_entry(&id(), "ETH", dec!(1), dec!(1), &account, NOW)
            .unwrap_or_else(|_| RiskDecision::Blocked {
                reason: "overflow".into(),
                breaker_latched: false
            })
    ));
    db.with_conn(|c| c.execute("DROP TABLE aegis_reservations", []))
        .unwrap();
    assert!(aegis
        .reserve_entry(
            &id(),
            "ETH",
            dec!(1),
            dec!(1),
            &snapshot(2, dec!(1000)),
            NOW
        )
        .is_err());
}
