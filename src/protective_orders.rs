//! Exchange-Native Protective Stop-Loss Architecture (`v1.14.0 | PROJECT IMMORTALITY`).
//!
//! Safety invariants:
//! - Every open position must carry a validated `reduce_only: true` exchange protective stop.
//! - Partial fills adjust stop quantities cleanly via replacement; closed positions cancel orphans.
//! - Unverifiable or failed stop protection immediately halts NEW exposure entries while keeping
//!   protective management authorized (never abandons existing positions when entries are blocked).

use crate::core::Side;
use anyhow::{ensure, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenProtectedPosition {
    pub symbol: String,
    pub owner_strategy: String,
    pub side: Side,
    pub qty: Decimal,
    pub entry_price: Decimal,
    pub stop_price: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectiveOrderSpec {
    pub symbol: String,
    pub owner_strategy: String,
    pub position_side: Side,
    pub protective_side: Side,
    pub qty: Decimal,
    pub trigger_price: Decimal,
    pub reduce_only: bool,
    pub cloid: String,
    pub exchange_oid: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ProtectiveReconcileAction {
    PlaceReduceOnlyStop(ProtectiveOrderSpec),
    ReplaceReduceOnlyStop {
        replace_exchange_oid: Option<u64>,
        old_cloid: String,
        new_spec: ProtectiveOrderSpec,
    },
    CancelOrphanStop {
        symbol: String,
        exchange_oid: Option<u64>,
        cloid: String,
    },
    HaltEntriesUnprotected {
        symbol: String,
        reason: String,
    },
    ConfirmedProtected {
        symbol: String,
        qty: Decimal,
        trigger_price: Decimal,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectiveReconcileReport {
    pub entries_blocked_by_caller: bool,
    pub protection_management_permitted: bool,
    pub protection_verified: bool,
    pub halt_new_entries: bool,
    pub actions: Vec<ProtectiveReconcileAction>,
}

pub struct ProtectiveOrderManager;

impl ProtectiveOrderManager {
    pub fn expected_protective_side(position_side: Side) -> Side {
        match position_side {
            Side::Long => Side::Short,
            Side::Short => Side::Long,
        }
    }

    pub fn reconcile_positions_and_stops(
        positions: &[OpenProtectedPosition],
        exchange_stops: &[ProtectiveOrderSpec],
        unverified_or_failed_symbols: &BTreeSet<String>,
        entries_blocked_by_caller: bool,
    ) -> Result<ProtectiveReconcileReport> {
        let mut pos_by_sym: BTreeMap<String, &OpenProtectedPosition> = BTreeMap::new();
        for p in positions {
            ensure!(
                p.qty > Decimal::ZERO
                    && p.entry_price > Decimal::ZERO
                    && p.stop_price > Decimal::ZERO,
                "invalid protected position geometry for {}",
                p.symbol
            );
            let expected_stop_valid = match p.side {
                Side::Long => p.stop_price < p.entry_price,
                Side::Short => p.stop_price > p.entry_price,
            };
            ensure!(
                expected_stop_valid,
                "stop price {} invalid for {:?} entry {} on {}",
                p.stop_price,
                p.side,
                p.entry_price,
                p.symbol
            );
            let sym = p.symbol.to_ascii_uppercase();
            ensure!(
                pos_by_sym.insert(sym, p).is_none(),
                "duplicate open position for {}",
                p.symbol
            );
        }

        let mut stops_by_sym: BTreeMap<String, Vec<&ProtectiveOrderSpec>> = BTreeMap::new();
        for s in exchange_stops {
            stops_by_sym
                .entry(s.symbol.to_ascii_uppercase())
                .or_default()
                .push(s);
        }

        let failed_upper: BTreeSet<String> = unverified_or_failed_symbols
            .iter()
            .map(|s| s.to_ascii_uppercase())
            .collect();

        let mut actions = Vec::new();
        let mut protection_verified = true;
        let mut halt_new_entries = false;

        for (sym, pos) in &pos_by_sym {
            let expected_side = Self::expected_protective_side(pos.side);
            let target_spec = ProtectiveOrderSpec {
                symbol: sym.clone(),
                owner_strategy: pos.owner_strategy.clone(),
                position_side: pos.side,
                protective_side: expected_side,
                qty: pos.qty,
                trigger_price: pos.stop_price,
                reduce_only: true,
                cloid: format!("protect-{}-{}-{}", sym, pos.owner_strategy, pos.qty),
                exchange_oid: None,
            };

            if failed_upper.contains(sym) {
                protection_verified = false;
                halt_new_entries = true;
                actions.push(ProtectiveReconcileAction::HaltEntriesUnprotected {
                    symbol: sym.clone(),
                    reason: "Protective stop verification/placement failed; halting new entries"
                        .into(),
                });
                // Still queue replacement/placement so existing position gets protected ASAP.
                actions.push(ProtectiveReconcileAction::PlaceReduceOnlyStop(target_spec));
                continue;
            }

            let existing = stops_by_sym.get(sym).map(Vec::as_slice).unwrap_or(&[]);
            if existing.is_empty() {
                protection_verified = false;
                halt_new_entries = true;
                actions.push(ProtectiveReconcileAction::HaltEntriesUnprotected {
                    symbol: sym.clone(),
                    reason: "No exchange reduce-only stop present; placing protective order & blocking entries".into(),
                });
                actions.push(ProtectiveReconcileAction::PlaceReduceOnlyStop(target_spec));
            } else {
                let first = existing[0];
                if existing.len() > 1 {
                    for extra in &existing[1..] {
                        actions.push(ProtectiveReconcileAction::CancelOrphanStop {
                            symbol: sym.clone(),
                            exchange_oid: extra.exchange_oid,
                            cloid: extra.cloid.clone(),
                        });
                    }
                }
                if first.reduce_only
                    && first.protective_side == expected_side
                    && first.qty == pos.qty
                    && first.trigger_price == pos.stop_price
                {
                    actions.push(ProtectiveReconcileAction::ConfirmedProtected {
                        symbol: sym.clone(),
                        qty: pos.qty,
                        trigger_price: pos.stop_price,
                    });
                } else {
                    actions.push(ProtectiveReconcileAction::ReplaceReduceOnlyStop {
                        replace_exchange_oid: first.exchange_oid,
                        old_cloid: first.cloid.clone(),
                        new_spec: target_spec,
                    });
                }
            }
        }

        // Cancel orphaned exchange stops whose positions have closed.
        for (sym, stops) in &stops_by_sym {
            if !pos_by_sym.contains_key(sym) {
                for s in stops {
                    actions.push(ProtectiveReconcileAction::CancelOrphanStop {
                        symbol: sym.clone(),
                        exchange_oid: s.exchange_oid,
                        cloid: s.cloid.clone(),
                    });
                }
            }
        }

        Ok(ProtectiveReconcileReport {
            entries_blocked_by_caller,
            protection_management_permitted: true,
            protection_verified,
            halt_new_entries,
            actions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manages_reduce_only_stops_resizes_on_partial_fill_and_permits_protection_when_entries_blocked(
    ) {
        let positions = vec![
            OpenProtectedPosition {
                symbol: "BTC".into(),
                owner_strategy: "mom".into(),
                side: Side::Long,
                qty: Decimal::from(2),
                entry_price: Decimal::from(60_000),
                stop_price: Decimal::from(58_800),
            },
            OpenProtectedPosition {
                symbol: "ETH".into(),
                owner_strategy: "ema".into(),
                side: Side::Short,
                qty: Decimal::from(5),
                entry_price: Decimal::from(3_000),
                stop_price: Decimal::from(3_090),
            },
        ];
        // BTC stop has stale pre-partial-fill qty=1; SOL stop is orphan (closed position)
        let exchange_stops = vec![
            ProtectiveOrderSpec {
                symbol: "BTC".into(),
                owner_strategy: "mom".into(),
                position_side: Side::Long,
                protective_side: Side::Short,
                qty: Decimal::from(1),
                trigger_price: Decimal::from(58_800),
                reduce_only: true,
                cloid: "old-btc".into(),
                exchange_oid: Some(77),
            },
            ProtectiveOrderSpec {
                symbol: "SOL".into(),
                owner_strategy: "ema".into(),
                position_side: Side::Long,
                protective_side: Side::Short,
                qty: Decimal::from(10),
                trigger_price: Decimal::from(140),
                reduce_only: true,
                cloid: "orphan-sol".into(),
                exchange_oid: Some(88),
            },
        ];

        // Even with entries blocked (`true`), protection management runs unblocked!
        let rep = ProtectiveOrderManager::reconcile_positions_and_stops(
            &positions,
            &exchange_stops,
            &BTreeSet::new(),
            true,
        )
        .unwrap();
        assert!(rep.protection_management_permitted);
        assert!(rep.actions.iter().any(|a| matches!(
            a,
            ProtectiveReconcileAction::ReplaceReduceOnlyStop { new_spec, .. }
                if new_spec.symbol == "BTC" && new_spec.qty == Decimal::from(2) && new_spec.reduce_only
        )));
        assert!(rep.actions.iter().any(|a| matches!(
            a,
            ProtectiveReconcileAction::PlaceReduceOnlyStop(s)
                if s.symbol == "ETH" && s.protective_side == Side::Long && s.reduce_only
        )));
        assert!(rep.actions.iter().any(|a| matches!(
            a,
            ProtectiveReconcileAction::CancelOrphanStop { symbol, .. } if symbol == "SOL"
        )));
        assert!(rep.halt_new_entries);
    }
}
