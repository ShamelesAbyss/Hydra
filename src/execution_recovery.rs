//! NEXUS Execution & Recovery Architecture (`v1.14.0 | PROJECT IMMORTALITY`).
//!
//! Safety invariants:
//! - Explicitly separates `Prepared -> Signed -> Submitted -> Acknowledged -> PartiallyFilled ->
//!   Filled | Cancelled | Rejected | Unresolved`.
//! - Never retries uncertain (`Submitted` / `Unresolved`) submissions blindly.
//! - Rejects duplicate `cloid`s and checks REST/WS order stream consistency before resolving.

use crate::core::Side;
use crate::ledger::EvidenceSource;
use anyhow::{ensure, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderLifecycleState {
    Prepared,
    Signed,
    Submitted,
    Acknowledged,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentOrderConstraints {
    pub symbol: String,
    pub tick_size: Decimal,
    pub lot_size: Decimal,
    pub min_notional_usd: Decimal,
    pub max_leverage: Decimal,
}

impl InstrumentOrderConstraints {
    /// Rounds price/quantity conservatively to valid exchange tick/lot steps and verifies
    /// minimum notional & leverage ceilings.
    pub fn validate_and_round(
        &self,
        price: Decimal,
        qty: Decimal,
        leverage: Decimal,
    ) -> Result<(Decimal, Decimal)> {
        ensure!(
            self.tick_size > Decimal::ZERO
                && self.lot_size > Decimal::ZERO
                && self.min_notional_usd >= Decimal::ZERO
                && self.max_leverage >= Decimal::ONE,
            "invalid instrument order constraints for {}",
            self.symbol
        );
        ensure!(
            price > Decimal::ZERO && qty > Decimal::ZERO,
            "order price and qty must be positive"
        );
        ensure!(
            leverage >= Decimal::ONE && leverage <= self.max_leverage,
            "requested leverage {leverage} exceeds max {} for {}",
            self.max_leverage,
            self.symbol
        );
        let px_steps = (price / self.tick_size).round();
        let rounded_px = px_steps * self.tick_size;
        let qty_steps = (qty / self.lot_size).floor();
        let rounded_qty = qty_steps * self.lot_size;
        ensure!(
            rounded_px > Decimal::ZERO && rounded_qty > Decimal::ZERO,
            "rounded price or lot size became zero for {}",
            self.symbol
        );
        let notional = rounded_px * rounded_qty;
        ensure!(
            notional >= self.min_notional_usd,
            "rounded notional {notional} below min_notional_usd {} for {}",
            self.min_notional_usd,
            self.symbol
        );
        Ok((rounded_px, rounded_qty))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionJournalRecord {
    pub cloid: String,
    pub exchange_oid: Option<u64>,
    pub symbol: String,
    pub side: Side,
    pub owner_strategy: String,
    pub requested_qty: Decimal,
    pub limit_price: Option<Decimal>,
    pub filled_qty: Decimal,
    pub state: OrderLifecycleState,
    pub submitted_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeOrderSnapshot {
    pub source: EvidenceSource,
    pub cloid: Option<String>,
    pub exchange_oid: u64,
    pub symbol: String,
    pub side: Side,
    pub state: OrderLifecycleState,
    pub filled_qty: Decimal,
    pub avg_fill_px: Option<Decimal>,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeOpenPositionSnapshot {
    pub symbol: String,
    pub side: Side,
    pub qty: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestWsConsistencyReport {
    pub consistent: bool,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecoveryAction {
    MarkAcknowledged {
        cloid: String,
        exchange_oid: u64,
    },
    ApplyPartialFill {
        cloid: String,
        exchange_oid: u64,
        delta_qty: Decimal,
        total_filled_qty: Decimal,
    },
    MarkFilled {
        cloid: String,
        exchange_oid: u64,
        total_filled_qty: Decimal,
    },
    MarkCancelled {
        cloid: String,
        exchange_oid: Option<u64>,
    },
    MarkRejected {
        cloid: String,
        reason: String,
    },
    HoldUnresolved {
        cloid: String,
        reason: String,
    },
    RefuseBlindRetry {
        cloid: String,
        state: OrderLifecycleState,
    },
    ReconcileOwnership {
        symbol: String,
        owner_strategy: String,
        qty: Decimal,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryReport {
    pub consistency: RestWsConsistencyReport,
    pub actions: Vec<RecoveryAction>,
    pub unresolved_holds: usize,
}

pub fn check_rest_ws_consistency(
    rest_orders: &[ExchangeOrderSnapshot],
    ws_orders: &[ExchangeOrderSnapshot],
) -> RestWsConsistencyReport {
    let mut rest_by_oid: BTreeMap<u64, &ExchangeOrderSnapshot> = BTreeMap::new();
    for r in rest_orders {
        rest_by_oid.insert(r.exchange_oid, r);
    }
    let mut conflicts = Vec::new();
    for w in ws_orders {
        if let Some(r) = rest_by_oid.get(&w.exchange_oid) {
            if r.state != w.state || r.filled_qty != w.filled_qty {
                // Only flag as conflict when both observed identically close timestamps or diverged terminal status.
                if (r.observed_at_ms - w.observed_at_ms).abs() <= 5_000
                    || (matches!(
                        r.state,
                        OrderLifecycleState::Filled
                            | OrderLifecycleState::Cancelled
                            | OrderLifecycleState::Rejected
                    ) && r.state != w.state)
                {
                    conflicts.push(format!(
                        "oid {} mismatch: REST({:?}, filled={}) vs WS({:?}, filled={})",
                        w.exchange_oid, r.state, r.filled_qty, w.state, w.filled_qty
                    ));
                }
            }
        }
    }
    RestWsConsistencyReport {
        consistent: conflicts.is_empty(),
        conflicts,
    }
}

pub fn reconcile_execution_state(
    journal: &[ExecutionJournalRecord],
    rest_orders: &[ExchangeOrderSnapshot],
    ws_orders: &[ExchangeOrderSnapshot],
    exchange_positions: &[ExchangeOpenPositionSnapshot],
) -> Result<RecoveryReport> {
    let mut seen_cloids = BTreeSet::new();
    for rec in journal {
        ensure!(
            !rec.cloid.trim().is_empty(),
            "journal record cannot have empty cloid"
        );
        ensure!(
            seen_cloids.insert(rec.cloid.clone()),
            "duplicate client order id rejected: {}",
            rec.cloid
        );
    }

    let consistency = check_rest_ws_consistency(rest_orders, ws_orders);
    let mut latest_by_cloid: BTreeMap<&str, &ExchangeOrderSnapshot> = BTreeMap::new();
    let mut latest_by_oid: BTreeMap<u64, &ExchangeOrderSnapshot> = BTreeMap::new();

    for snap in rest_orders.iter().chain(ws_orders.iter()) {
        if let Some(ref c) = snap.cloid {
            latest_by_cloid
                .entry(c.as_str())
                .and_modify(|cur| {
                    if snap.observed_at_ms >= cur.observed_at_ms {
                        *cur = snap;
                    }
                })
                .or_insert(snap);
        }
        latest_by_oid
            .entry(snap.exchange_oid)
            .and_modify(|cur| {
                if snap.observed_at_ms >= cur.observed_at_ms {
                    *cur = snap;
                }
            })
            .or_insert(snap);
    }

    let mut actions = Vec::new();
    let mut unresolved_holds = 0usize;
    let mut symbol_owners: BTreeMap<String, String> = BTreeMap::new();

    for rec in journal {
        symbol_owners
            .entry(rec.symbol.to_ascii_uppercase())
            .or_insert_with(|| rec.owner_strategy.clone());

        let matched = latest_by_cloid
            .get(rec.cloid.as_str())
            .copied()
            .or_else(|| {
                rec.exchange_oid
                    .and_then(|o| latest_by_oid.get(&o).copied())
            });

        match matched {
            Some(ex) if !consistency.consistent => {
                unresolved_holds += 1;
                actions.push(RecoveryAction::HoldUnresolved {
                    cloid: rec.cloid.clone(),
                    reason: format!(
                        "REST/WS conflict prevented automatic resolution (oid {})",
                        ex.exchange_oid
                    ),
                });
            }
            Some(ex) => {
                if ex.filled_qty > rec.filled_qty {
                    actions.push(RecoveryAction::ApplyPartialFill {
                        cloid: rec.cloid.clone(),
                        exchange_oid: ex.exchange_oid,
                        delta_qty: ex.filled_qty - rec.filled_qty,
                        total_filled_qty: ex.filled_qty,
                    });
                }
                match ex.state {
                    OrderLifecycleState::Acknowledged => {
                        actions.push(RecoveryAction::MarkAcknowledged {
                            cloid: rec.cloid.clone(),
                            exchange_oid: ex.exchange_oid,
                        });
                    }
                    OrderLifecycleState::Filled => {
                        actions.push(RecoveryAction::MarkFilled {
                            cloid: rec.cloid.clone(),
                            exchange_oid: ex.exchange_oid,
                            total_filled_qty: ex.filled_qty,
                        });
                    }
                    OrderLifecycleState::Cancelled => {
                        actions.push(RecoveryAction::MarkCancelled {
                            cloid: rec.cloid.clone(),
                            exchange_oid: Some(ex.exchange_oid),
                        });
                    }
                    OrderLifecycleState::Rejected => {
                        actions.push(RecoveryAction::MarkRejected {
                            cloid: rec.cloid.clone(),
                            reason: "Exchange snapshot reported Rejected".into(),
                        });
                    }
                    _ => {}
                }
            }
            None => {
                if matches!(
                    rec.state,
                    OrderLifecycleState::Submitted | OrderLifecycleState::Unresolved
                ) {
                    unresolved_holds += 1;
                    actions.push(RecoveryAction::RefuseBlindRetry {
                        cloid: rec.cloid.clone(),
                        state: rec.state,
                    });
                    actions.push(RecoveryAction::HoldUnresolved {
                        cloid: rec.cloid.clone(),
                        reason: "Ambiguous submission absent from REST/WS snapshots; blind retry prohibited"
                            .into(),
                    });
                }
            }
        }
    }

    for pos in exchange_positions {
        let sym = pos.symbol.to_ascii_uppercase();
        if pos.qty > Decimal::ZERO {
            if let Some(owner) = symbol_owners.get(&sym) {
                actions.push(RecoveryAction::ReconcileOwnership {
                    symbol: sym,
                    owner_strategy: owner.clone(),
                    qty: pos.qty,
                });
            } else {
                unresolved_holds += 1;
                actions.push(RecoveryAction::HoldUnresolved {
                    cloid: format!("orphan-pos-{sym}"),
                    reason: format!("Open exchange position on {sym} has no durable local owner"),
                });
            }
        }
    }

    Ok(RecoveryReport {
        consistency,
        actions,
        unresolved_holds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_constraints_and_rejects_duplicate_cloid_or_blind_retry() {
        let c = InstrumentOrderConstraints {
            symbol: "ETH".into(),
            tick_size: Decimal::new(5, 1), // 0.5
            lot_size: Decimal::new(1, 2),  // 0.01
            min_notional_usd: Decimal::from(10),
            max_leverage: Decimal::from(20),
        };
        let (px, qty) = c
            .validate_and_round(
                Decimal::new(25004, 1),
                Decimal::new(1234, 3),
                Decimal::from(5),
            )
            .unwrap();
        assert_eq!(px, Decimal::new(25005, 1)); // rounded to 0.5 step
        assert_eq!(qty, Decimal::new(123, 2)); // floored to 0.01 step
        assert!(c
            .validate_and_round(Decimal::from(2500), Decimal::from(1), Decimal::from(25))
            .is_err());

        // Duplicate cloid rejected
        let dup = vec![
            ExecutionJournalRecord {
                cloid: "c1".into(),
                exchange_oid: None,
                symbol: "ETH".into(),
                side: Side::Long,
                owner_strategy: "mom".into(),
                requested_qty: Decimal::ONE,
                limit_price: None,
                filled_qty: Decimal::ZERO,
                state: OrderLifecycleState::Submitted,
                submitted_at_ms: 100,
            },
            ExecutionJournalRecord {
                cloid: "c1".into(),
                exchange_oid: None,
                symbol: "ETH".into(),
                side: Side::Long,
                owner_strategy: "mom".into(),
                requested_qty: Decimal::ONE,
                limit_price: None,
                filled_qty: Decimal::ZERO,
                state: OrderLifecycleState::Prepared,
                submitted_at_ms: 101,
            },
        ];
        assert!(reconcile_execution_state(&dup, &[], &[], &[]).is_err());

        // Ambiguous submitted order absent from REST/WS holds & refuses blind retry
        let rep = reconcile_execution_state(&dup[..1], &[], &[], &[]).unwrap();
        assert_eq!(rep.unresolved_holds, 1);
        assert!(rep
            .actions
            .iter()
            .any(|a| matches!(a, RecoveryAction::RefuseBlindRetry { cloid, .. } if cloid == "c1")));
    }

    #[test]
    fn reconciles_partial_fill_ownership_and_detects_rest_ws_conflict() {
        let j = vec![ExecutionJournalRecord {
            cloid: "c-eth-1".into(),
            exchange_oid: Some(9001),
            symbol: "ETH".into(),
            side: Side::Long,
            owner_strategy: "adaptive_ema_anchor".into(),
            requested_qty: Decimal::from(2),
            limit_price: Some(Decimal::from(2500)),
            filled_qty: Decimal::ZERO,
            state: OrderLifecycleState::Acknowledged,
            submitted_at_ms: 100,
        }];
        let rest = vec![ExchangeOrderSnapshot {
            source: EvidenceSource::RestSnapshot,
            cloid: Some("c-eth-1".into()),
            exchange_oid: 9001,
            symbol: "ETH".into(),
            side: Side::Long,
            state: OrderLifecycleState::PartiallyFilled,
            filled_qty: Decimal::ONE,
            avg_fill_px: Some(Decimal::from(2500)),
            observed_at_ms: 200,
        }];
        let ws = vec![ExchangeOrderSnapshot {
            source: EvidenceSource::UserWebSocket,
            cloid: Some("c-eth-1".into()),
            exchange_oid: 9001,
            symbol: "ETH".into(),
            side: Side::Long,
            state: OrderLifecycleState::PartiallyFilled,
            filled_qty: Decimal::ONE,
            avg_fill_px: Some(Decimal::from(2500)),
            observed_at_ms: 210,
        }];
        let pos = vec![ExchangeOpenPositionSnapshot {
            symbol: "ETH".into(),
            side: Side::Long,
            qty: Decimal::ONE,
        }];
        let rep = reconcile_execution_state(&j, &rest, &ws, &pos).unwrap();
        assert!(rep.consistency.consistent);
        assert_eq!(rep.unresolved_holds, 0);
        assert!(rep.actions.iter().any(|a| matches!(
            a,
            RecoveryAction::ApplyPartialFill { delta_qty, .. } if *delta_qty == Decimal::ONE
        )));
        assert!(rep.actions.iter().any(|a| matches!(
            a,
            RecoveryAction::ReconcileOwnership { owner_strategy, .. } if owner_strategy == "adaptive_ema_anchor"
        )));

        // Conflicting REST vs WS terminal states fail safe into unresolved hold
        let mut ws_bad = ws;
        ws_bad[0].state = OrderLifecycleState::Filled;
        ws_bad[0].filled_qty = Decimal::from(2);
        let rep_bad = reconcile_execution_state(&j, &rest, &ws_bad, &pos).unwrap();
        assert!(!rep_bad.consistency.consistent);
        assert_eq!(rep_bad.unresolved_holds, 1);
    }
}
