//! NEXUS Authoritative Financial Settlement Engine (`v1.14.0 | PROJECT IMMORTALITY`).
//!
//! Safety & truthfulness invariants:
//! - Exact `rust_decimal::Decimal` accounting over exchange fills, fees, maker rebates,
//!   position-specific funding payments, and external deposits/withdrawals.
//! - Never infers missing fees, funding coverage, or closed-PnL conventions as zero.
//! - Idempotent fill/funding/cash-flow ingestion prevents duplicate accounting after
//!   reconnects or restarts.
//! - Retains explicit `SettlementHoldReason`s and omits `verified_net_pnl_usdc` until
//!   every economic component is backed by authoritative evidence.
//! - Emits each verified round-trip outcome at most once for downstream learning.

use crate::ledger::{ExchangeFill, FillSide, FundingPayment, PnlConvention};
use crate::storage::Storage;
use anyhow::{bail, ensure, Context, Result};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalCashFlowKind {
    Deposit,
    Withdrawal,
    InternalTransfer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalCashFlow {
    pub cash_flow_id: String,
    pub kind: ExternalCashFlowKind,
    pub token: String,
    /// Positive for credits/deposits into this perp scope, negative for withdrawals out.
    pub signed_amount: Decimal,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundingCoverageWindow {
    pub symbol: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub complete: bool,
}

/// Durable obligation tracking a missed/unresolved funding interval independently of forward
/// cursor progress, including already-closed positions awaiting backfill before settlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingFundingObligation {
    pub obligation_id: String,
    pub symbol: String,
    pub interval_start_ms: i64,
    pub interval_end_ms: i64,
    pub position_qty: Decimal,
    pub side_label: String,
    pub resolved: bool,
    pub funding_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementHoldReason {
    UnknownFee,
    UnverifiedPnlConvention,
    MissingFundingCoverage,
    UnsupportedToken,
    UnmatchedExitQuantity,
    ExternalCashFlowGap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementStatus {
    VerifiedSettled,
    HeldIncompleteEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettledRoundTrip {
    pub round_trip_id: String,
    pub symbol: String,
    pub owner_strategy: String,
    pub side_label: String,
    pub qty: Decimal,
    pub entry_vwap: Decimal,
    pub exit_vwap: Decimal,
    pub opened_at_ms: i64,
    pub closed_at_ms: i64,
    pub gross_realized_pnl_usdc: Decimal,
    pub entry_fee_usdc: Option<Decimal>,
    pub exit_fee_usdc: Option<Decimal>,
    pub funding_usdc: Option<Decimal>,
    pub verified_net_pnl_usdc: Option<Decimal>,
    pub status: SettlementStatus,
    pub holds: BTreeSet<SettlementHoldReason>,
    pub entry_fill_ids: Vec<String>,
    pub exit_fill_ids: Vec<String>,
    pub funding_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquityReconciliationReport {
    pub start_equity_usdc: Decimal,
    pub end_equity_usdc: Decimal,
    pub verified_realized_net_pnl_usdc: Option<Decimal>,
    pub unrealized_pnl_delta_usdc: Decimal,
    pub external_net_cash_flow_usdc: Decimal,
    pub expected_end_equity_usdc: Option<Decimal>,
    pub discrepancy_usdc: Option<Decimal>,
    pub verified: bool,
    pub holds: BTreeSet<SettlementHoldReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedLearningOutcome {
    pub exactly_once_seq: u64,
    pub round_trip_id: String,
    pub symbol: String,
    pub owner_strategy: String,
    pub opened_at_ms: i64,
    pub closed_at_ms: i64,
    pub qty: Decimal,
    pub entry_fee_usdc: Decimal,
    pub exit_fee_usdc: Decimal,
    pub funding_usdc: Decimal,
    pub verified_net_pnl_usdc: Decimal,
}

#[derive(Debug, Clone)]
struct OpenLot {
    side: FillSide,
    owner_strategy: String,
    qty_initial: Decimal,
    qty_remaining: Decimal,
    price: Decimal,
    opened_at_ms: i64,
    fill_id: String,
    entry_fee_remaining: Option<Decimal>,
    entry_fee_hold: Option<SettlementHoldReason>,
    /// Cumulative signed funding already attributed to still-remaining qty.
    attributed_funding_remaining: Decimal,
    funding_ids: Vec<String>,
}

/// Idempotent financial settlement state machine preserving exact `Decimal` evidence.
#[derive(Debug, Clone, Default)]
pub struct SettlementEngine {
    fills_by_id: BTreeMap<String, ExchangeFill>,
    fill_strategies: BTreeMap<String, String>,
    funding_by_id: BTreeMap<String, FundingPayment>,
    cash_flows_by_id: BTreeMap<String, ExternalCashFlow>,
    funding_coverage: BTreeMap<String, Vec<FundingCoverageWindow>>,
    pending_funding: BTreeMap<String, PendingFundingObligation>,
    round_trips: Vec<SettledRoundTrip>,
    round_trip_keys: BTreeSet<String>,
    emitted_round_trips: BTreeSet<String>,
    next_learning_seq: u64,
}

impl SettlementEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ingest_fill(
        &mut self,
        fill: ExchangeFill,
        owner_strategy: Option<&str>,
    ) -> Result<bool> {
        ensure!(
            fill.quantity > Decimal::ZERO && fill.price > Decimal::ZERO && fill.timestamp_ms > 0,
            "fill quantity, price, and timestamp must be positive"
        );
        if let Some(prev) = self.fills_by_id.get(&fill.fill_id) {
            ensure!(
                prev == &fill,
                "conflicting duplicate fill record for fill_id '{}'",
                fill.fill_id
            );
            if let Some(strat) = owner_strategy {
                if let Some(prev_strat) = self.fill_strategies.get(&fill.fill_id) {
                    ensure!(
                        prev_strat == strat,
                        "conflicting strategy ownership for fill_id '{}'",
                        fill.fill_id
                    );
                }
            }
            return Ok(false);
        }
        let strat = owner_strategy.unwrap_or("unattributed").trim();
        ensure!(!strat.is_empty(), "owner_strategy cannot be empty");
        self.fill_strategies
            .insert(fill.fill_id.clone(), strat.to_string());
        self.fills_by_id.insert(fill.fill_id.clone(), fill);
        Ok(true)
    }

    pub fn ingest_funding(&mut self, payment: FundingPayment) -> Result<bool> {
        ensure!(
            payment.timestamp_ms > 0,
            "funding timestamp must be positive"
        );
        if let Some(prev) = self.funding_by_id.get(&payment.funding_id) {
            ensure!(
                prev == &payment,
                "conflicting duplicate funding record for funding_id '{}'",
                payment.funding_id
            );
            return Ok(false);
        }
        self.funding_by_id
            .insert(payment.funding_id.clone(), payment);
        Ok(true)
    }

    pub fn ingest_external_cash_flow(&mut self, flow: ExternalCashFlow) -> Result<bool> {
        ensure!(
            !flow.cash_flow_id.trim().is_empty() && flow.timestamp_ms > 0,
            "invalid external cash flow id or timestamp"
        );
        if let Some(prev) = self.cash_flows_by_id.get(&flow.cash_flow_id) {
            ensure!(
                prev == &flow,
                "conflicting duplicate external cash flow '{}'",
                flow.cash_flow_id
            );
            return Ok(false);
        }
        self.cash_flows_by_id
            .insert(flow.cash_flow_id.clone(), flow);
        Ok(true)
    }

    pub fn record_funding_coverage(&mut self, window: FundingCoverageWindow) -> Result<()> {
        ensure!(
            window.end_ms >= window.start_ms && !window.symbol.trim().is_empty(),
            "invalid funding coverage window"
        );
        let sym = window.symbol.trim().to_ascii_uppercase();
        self.funding_coverage
            .entry(sym)
            .or_default()
            .push(FundingCoverageWindow {
                symbol: window.symbol.trim().to_ascii_uppercase(),
                ..window
            });
        Ok(())
    }

    pub fn record_pending_funding_obligation(
        &mut self,
        obligation: PendingFundingObligation,
    ) -> Result<()> {
        ensure!(
            !obligation.obligation_id.trim().is_empty()
                && !obligation.symbol.trim().is_empty()
                && obligation.interval_end_ms >= obligation.interval_start_ms
                && obligation.position_qty > Decimal::ZERO,
            "invalid pending funding obligation"
        );
        let mut norm = obligation;
        norm.symbol = norm.symbol.trim().to_ascii_uppercase();
        self.pending_funding
            .insert(norm.obligation_id.clone(), norm);
        Ok(())
    }

    pub fn resolve_pending_funding_obligation(
        &mut self,
        obligation_id: &str,
        funding_id: &str,
    ) -> Result<()> {
        let ob = self
            .pending_funding
            .get_mut(obligation_id)
            .with_context(|| format!("missing pending funding obligation '{obligation_id}'"))?;
        ob.resolved = true;
        ob.funding_id = Some(funding_id.to_string());
        Ok(())
    }

    pub fn restore_emitted_learning(&mut self, round_trip_id: String, seq: u64) {
        self.emitted_round_trips.insert(round_trip_id);
        self.next_learning_seq = self.next_learning_seq.max(seq);
    }

    pub fn unresolved_pending_funding_count(&self) -> usize {
        self.pending_funding
            .values()
            .filter(|p| !p.resolved)
            .count()
    }

    /// Recomputes deterministic FIFO partial-fill round trips from the full idempotent
    /// ledger of fills + funding + coverage windows.
    pub fn settle_all(&mut self) -> Result<&[SettledRoundTrip]> {
        let mut ordered_fills: Vec<(&ExchangeFill, &str)> = self
            .fills_by_id
            .values()
            .map(|f| {
                let strat = self
                    .fill_strategies
                    .get(&f.fill_id)
                    .map(String::as_str)
                    .unwrap_or("unattributed");
                (f, strat)
            })
            .collect();
        ordered_fills.sort_by(|a, b| {
            a.0.timestamp_ms
                .cmp(&b.0.timestamp_ms)
                .then_with(|| a.0.order_id.cmp(&b.0.order_id))
                .then_with(|| a.0.fill_id.cmp(&b.0.fill_id))
        });

        let mut ordered_funding: Vec<&FundingPayment> = self.funding_by_id.values().collect();
        ordered_funding.sort_by(|a, b| {
            a.timestamp_ms
                .cmp(&b.timestamp_ms)
                .then_with(|| a.funding_id.cmp(&b.funding_id))
        });
        let mut applied_funding: BTreeSet<String> = BTreeSet::new();
        let mut open_queues: BTreeMap<String, VecDeque<OpenLot>> = BTreeMap::new();

        for (fill, strat) in ordered_fills {
            let sym = fill.symbol.trim().to_ascii_uppercase();
            // Attribute all funding strictly prior to or at `fill.timestamp_ms` to open lots on `sym`.
            for fp in &ordered_funding {
                if applied_funding.contains(&fp.funding_id) {
                    continue;
                }
                if fp.symbol.trim().to_ascii_uppercase() == sym
                    && fp.timestamp_ms <= fill.timestamp_ms
                {
                    if let Some(q) = open_queues.get_mut(&sym) {
                        if !q.is_empty() {
                            if let Some(tok) = &fp.payment {
                                if tok.token == "USDC" {
                                    let total_open: Decimal =
                                        q.iter().map(|l| l.qty_remaining).sum();
                                    if total_open > Decimal::ZERO {
                                        let mut allocated = Decimal::ZERO;
                                        let len = q.len();
                                        for (i, lot) in q.iter_mut().enumerate() {
                                            if fp.timestamp_ms >= lot.opened_at_ms {
                                                let share = if i + 1 == len {
                                                    tok.amount - allocated
                                                } else {
                                                    ((tok.amount * lot.qty_remaining) / total_open)
                                                        .round_dp(8)
                                                };
                                                allocated += share;
                                                lot.attributed_funding_remaining += share;
                                                lot.funding_ids.push(fp.funding_id.clone());
                                            }
                                        }
                                        applied_funding.insert(fp.funding_id.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let (fee_val, fee_hold) = match &fill.fee {
                Some(t) if t.token == "USDC" => (Some(t.amount), None),
                Some(_) => (None, Some(SettlementHoldReason::UnsupportedToken)),
                None => (None, Some(SettlementHoldReason::UnknownFee)),
            };

            let queue = open_queues.entry(sym.clone()).or_default();
            let mut rem_fill_qty = fill.quantity;

            // Consume opposing open lots FIFO first.
            let mut matched_qty_total = Decimal::ZERO;
            let mut weighted_entry_notional = Decimal::ZERO;
            let mut weighted_gross_pnl = Decimal::ZERO;
            let mut entry_fee_accum = Some(Decimal::ZERO);
            let mut funding_accum = Decimal::ZERO;
            let mut holds = BTreeSet::new();
            let mut entry_ids = Vec::new();
            let mut fund_ids = BTreeSet::new();
            let mut earliest_open_ms = fill.timestamp_ms;
            let mut owner_strat = strat.to_string();
            let mut closed_side_label = String::new();

            while rem_fill_qty > Decimal::ZERO {
                let Some(front) = queue.front() else {
                    break;
                };
                if front.side == fill.side {
                    break;
                }
                let mut lot = queue.pop_front().unwrap();
                let take = rem_fill_qty.min(lot.qty_remaining);
                let ratio = (take / lot.qty_remaining).min(Decimal::ONE);
                earliest_open_ms = earliest_open_ms.min(lot.opened_at_ms);
                owner_strat = lot.owner_strategy.clone();
                closed_side_label = match lot.side {
                    FillSide::Buy => "LONG".into(),
                    FillSide::Sell => "SHORT".into(),
                };
                entry_ids.push(lot.fill_id.clone());
                for fid in &lot.funding_ids {
                    fund_ids.insert(fid.clone());
                }

                let leg_gross = match lot.side {
                    FillSide::Buy => (fill.price - lot.price) * take,
                    FillSide::Sell => (lot.price - fill.price) * take,
                };
                weighted_gross_pnl += leg_gross;
                weighted_entry_notional += lot.price * take;
                matched_qty_total += take;

                if let Some(h) = &lot.entry_fee_hold {
                    holds.insert(h.clone());
                    entry_fee_accum = None;
                } else if let (Some(acc), Some(rem_ef)) = (entry_fee_accum, lot.entry_fee_remaining)
                {
                    let part = if take == lot.qty_remaining {
                        rem_ef
                    } else {
                        (rem_ef * ratio).round_dp(8)
                    };
                    entry_fee_accum = Some(acc + part);
                    lot.entry_fee_remaining = Some(rem_ef - part);
                }

                let fund_part = if take == lot.qty_remaining {
                    lot.attributed_funding_remaining
                } else {
                    (lot.attributed_funding_remaining * ratio).round_dp(8)
                };
                funding_accum += fund_part;
                lot.attributed_funding_remaining -= fund_part;
                lot.qty_remaining -= take;
                rem_fill_qty -= take;

                if lot.qty_remaining > Decimal::ZERO {
                    queue.push_front(lot);
                }
            }

            if matched_qty_total > Decimal::ZERO {
                if let Some(h) = &fee_hold {
                    holds.insert(h.clone());
                }
                if !self.has_complete_funding_coverage(&sym, earliest_open_ms, fill.timestamp_ms)
                    || self.has_unresolved_pending_funding(
                        &sym,
                        earliest_open_ms,
                        fill.timestamp_ms,
                    )
                {
                    holds.insert(SettlementHoldReason::MissingFundingCoverage);
                }
                if fill.pnl_convention == PnlConvention::Unverified
                    && fill.closed_pnl_usdc.is_some()
                {
                    holds.insert(SettlementHoldReason::UnverifiedPnlConvention);
                }
                // Check for any funding records inside the trade span with unknown payment or non-USDC token
                for fp in &ordered_funding {
                    if fp.symbol.trim().to_ascii_uppercase() == sym
                        && fp.timestamp_ms >= earliest_open_ms
                        && fp.timestamp_ms <= fill.timestamp_ms
                    {
                        match &fp.payment {
                            None => {
                                holds.insert(SettlementHoldReason::MissingFundingCoverage);
                            }
                            Some(tok) if tok.token != "USDC" => {
                                holds.insert(SettlementHoldReason::UnsupportedToken);
                            }
                            _ => {}
                        }
                    }
                }

                let exit_fee_part = fee_val.map(|f| {
                    if matched_qty_total == fill.quantity {
                        f
                    } else {
                        ((f * matched_qty_total) / fill.quantity).round_dp(8)
                    }
                });

                let gross = weighted_gross_pnl.round_dp(8);
                let funding_opt = (!holds.contains(&SettlementHoldReason::MissingFundingCoverage)
                    && !holds.contains(&SettlementHoldReason::UnsupportedToken))
                .then_some(funding_accum.round_dp(8));

                let verified_net = if holds.is_empty() {
                    if let (Some(ef), Some(xf), Some(fn_usd)) =
                        (entry_fee_accum, exit_fee_part, funding_opt)
                    {
                        // Fees positive = cost, negative = rebate; funding positive = received.
                        Some((gross - ef - xf + fn_usd).round_dp(8))
                    } else {
                        holds.insert(SettlementHoldReason::UnknownFee);
                        None
                    }
                } else {
                    None
                };

                let status = if holds.is_empty() && verified_net.is_some() {
                    SettlementStatus::VerifiedSettled
                } else {
                    SettlementStatus::HeldIncompleteEvidence
                };
                let rt_id = format!("rt-{}-{}-{}", sym, fill.fill_id, self.round_trips.len() + 1);
                if self
                    .round_trip_keys
                    .insert(format!("{sym}:{}", fill.fill_id))
                {
                    self.round_trips.push(SettledRoundTrip {
                        round_trip_id: rt_id,
                        symbol: sym.clone(),
                        owner_strategy: owner_strat,
                        side_label: closed_side_label,
                        qty: matched_qty_total,
                        entry_vwap: (weighted_entry_notional / matched_qty_total).round_dp(8),
                        exit_vwap: fill.price,
                        opened_at_ms: earliest_open_ms,
                        closed_at_ms: fill.timestamp_ms,
                        gross_realized_pnl_usdc: gross,
                        entry_fee_usdc: entry_fee_accum,
                        exit_fee_usdc: exit_fee_part,
                        funding_usdc: funding_opt,
                        verified_net_pnl_usdc: verified_net,
                        status,
                        holds,
                        entry_fill_ids: entry_ids,
                        exit_fill_ids: vec![fill.fill_id.clone()],
                        funding_ids: fund_ids.into_iter().collect(),
                    });
                }
            }

            if rem_fill_qty > Decimal::ZERO {
                let open_fee = fee_val.map(|f| {
                    if rem_fill_qty == fill.quantity {
                        f
                    } else {
                        ((f * rem_fill_qty) / fill.quantity).round_dp(8)
                    }
                });
                queue.push_back(OpenLot {
                    side: fill.side,
                    owner_strategy: strat.to_string(),
                    qty_initial: rem_fill_qty,
                    qty_remaining: rem_fill_qty,
                    price: fill.price,
                    opened_at_ms: fill.timestamp_ms,
                    fill_id: fill.fill_id.clone(),
                    entry_fee_remaining: open_fee,
                    entry_fee_hold: fee_hold,
                    attributed_funding_remaining: Decimal::ZERO,
                    funding_ids: Vec::new(),
                });
            }
        }
        let _ = open_queues
            .values()
            .map(|q| q.iter().map(|l| l.qty_initial).sum::<Decimal>());
        Ok(&self.round_trips)
    }

    fn has_complete_funding_coverage(&self, symbol: &str, start_ms: i64, end_ms: i64) -> bool {
        let Some(windows) = self.funding_coverage.get(symbol) else {
            return false;
        };
        windows
            .iter()
            .any(|w| w.complete && w.start_ms <= start_ms && w.end_ms >= end_ms)
    }

    fn has_unresolved_pending_funding(&self, symbol: &str, start_ms: i64, end_ms: i64) -> bool {
        self.pending_funding.values().any(|ob| {
            !ob.resolved
                && ob.symbol.eq_ignore_ascii_case(symbol)
                && ob.interval_end_ms >= start_ms
                && ob.interval_start_ms <= end_ms
        })
    }

    pub fn reconcile_account_equity(
        &mut self,
        start_equity_usdc: Decimal,
        end_equity_usdc: Decimal,
        unrealized_pnl_delta_usdc: Decimal,
        tolerance_usdc: Decimal,
    ) -> Result<EquityReconciliationReport> {
        ensure!(
            tolerance_usdc >= Decimal::ZERO,
            "tolerance cannot be negative"
        );
        self.settle_all()?;
        let mut holds = BTreeSet::new();
        let mut net_pnl_sum = Decimal::ZERO;
        let mut all_rt_verified = true;

        for rt in &self.round_trips {
            if let Some(net) = rt.verified_net_pnl_usdc {
                net_pnl_sum += net;
            } else {
                all_rt_verified = false;
                for h in &rt.holds {
                    holds.insert(h.clone());
                }
            }
        }

        let mut ext_sum = Decimal::ZERO;
        for flow in self.cash_flows_by_id.values() {
            if flow.token != "USDC" {
                holds.insert(SettlementHoldReason::UnsupportedToken);
                all_rt_verified = false;
            } else {
                ext_sum += flow.signed_amount;
            }
        }

        let verified_pnl_opt = all_rt_verified.then_some(net_pnl_sum.round_dp(8));
        let expected_opt = verified_pnl_opt
            .map(|p| (start_equity_usdc + p + unrealized_pnl_delta_usdc + ext_sum).round_dp(8));
        let disc_opt = expected_opt.map(|exp| (end_equity_usdc - exp).round_dp(8));
        let verified = matches!(disc_opt, Some(d) if d.abs() <= tolerance_usdc) && holds.is_empty();
        if let Some(d) = disc_opt {
            if d.abs() > tolerance_usdc {
                holds.insert(SettlementHoldReason::ExternalCashFlowGap);
            }
        }

        Ok(EquityReconciliationReport {
            start_equity_usdc,
            end_equity_usdc,
            verified_realized_net_pnl_usdc: verified_pnl_opt,
            unrealized_pnl_delta_usdc,
            external_net_cash_flow_usdc: ext_sum.round_dp(8),
            expected_end_equity_usdc: expected_opt,
            discrepancy_usdc: disc_opt,
            verified,
            holds,
        })
    }

    /// Emits each `VerifiedSettled` round-trip **at most once** so reconnects/restarts never double-count.
    pub fn emit_eligible_learning_outcomes(&mut self) -> Vec<VerifiedLearningOutcome> {
        let mut out = Vec::new();
        for rt in &self.round_trips {
            if rt.status != SettlementStatus::VerifiedSettled {
                continue;
            }
            let (Some(net), Some(ef), Some(xf), Some(fn_usd)) = (
                rt.verified_net_pnl_usdc,
                rt.entry_fee_usdc,
                rt.exit_fee_usdc,
                rt.funding_usdc,
            ) else {
                continue;
            };
            if self.emitted_round_trips.insert(rt.round_trip_id.clone()) {
                self.next_learning_seq += 1;
                out.push(VerifiedLearningOutcome {
                    exactly_once_seq: self.next_learning_seq,
                    round_trip_id: rt.round_trip_id.clone(),
                    symbol: rt.symbol.clone(),
                    owner_strategy: rt.owner_strategy.clone(),
                    opened_at_ms: rt.opened_at_ms,
                    closed_at_ms: rt.closed_at_ms,
                    qty: rt.qty,
                    entry_fee_usdc: ef,
                    exit_fee_usdc: xf,
                    funding_usdc: fn_usd,
                    verified_net_pnl_usdc: net,
                });
            }
        }
        out
    }

    pub fn round_trips(&self) -> &[SettledRoundTrip] {
        &self.round_trips
    }
}

/// Durable SQLite-backed financial settlement repository ensuring atomic commit of fills,
/// funding payments, missed-interval pending funding obligations, external cash flows,
/// round-trip settlement holds, and at-most-once learning emissions across process crashes.
#[derive(Clone)]
pub struct SettlementStore {
    storage: Storage,
    account_scope: String,
}

impl SettlementStore {
    pub fn open(storage: Storage, account_scope: impl Into<String>) -> Result<Self> {
        let account_scope = account_scope.into().trim().to_ascii_lowercase();
        ensure!(
            !account_scope.is_empty(),
            "settlement account_scope required"
        );
        storage.with_conn(|conn| -> Result<()> {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS settlement_fills (
                    account_scope TEXT NOT NULL,
                    fill_id TEXT NOT NULL,
                    owner_strategy TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    PRIMARY KEY (account_scope, fill_id)
                );
                CREATE TABLE IF NOT EXISTS settlement_funding (
                    account_scope TEXT NOT NULL,
                    funding_id TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    PRIMARY KEY (account_scope, funding_id)
                );
                CREATE TABLE IF NOT EXISTS settlement_cash_flows (
                    account_scope TEXT NOT NULL,
                    cash_flow_id TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    PRIMARY KEY (account_scope, cash_flow_id)
                );
                CREATE TABLE IF NOT EXISTS settlement_funding_coverage (
                    account_scope TEXT NOT NULL,
                    symbol TEXT NOT NULL,
                    start_ms INTEGER NOT NULL,
                    end_ms INTEGER NOT NULL,
                    complete INTEGER NOT NULL,
                    PRIMARY KEY (account_scope, symbol, start_ms, end_ms)
                );
                CREATE TABLE IF NOT EXISTS settlement_pending_funding (
                    account_scope TEXT NOT NULL,
                    obligation_id TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    PRIMARY KEY (account_scope, obligation_id)
                );
                CREATE TABLE IF NOT EXISTS settlement_round_trips (
                    account_scope TEXT NOT NULL,
                    round_trip_id TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    PRIMARY KEY (account_scope, round_trip_id)
                );
                CREATE TABLE IF NOT EXISTS settlement_learning_emissions (
                    account_scope TEXT NOT NULL,
                    round_trip_id TEXT NOT NULL,
                    seq INTEGER NOT NULL,
                    payload_json TEXT NOT NULL,
                    PRIMARY KEY (account_scope, round_trip_id)
                );
                "#,
            )?;
            Ok(())
        })?;
        Ok(Self {
            storage,
            account_scope,
        })
    }

    pub fn record_fill(&self, fill: &ExchangeFill, owner_strategy: Option<&str>) -> Result<bool> {
        ensure!(
            fill.quantity > Decimal::ZERO && fill.price > Decimal::ZERO && fill.timestamp_ms > 0,
            "fill quantity, price, and timestamp must be positive"
        );
        let strat = owner_strategy.unwrap_or("unattributed").trim();
        ensure!(!strat.is_empty(), "owner_strategy cannot be empty");
        let payload = serde_json::to_string(fill)?;
        self.storage.with_conn(|conn| -> Result<bool> {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let prev: Option<(String, String)> = tx
                .query_row(
                    "SELECT owner_strategy, payload_json FROM settlement_fills WHERE account_scope=?1 AND fill_id=?2",
                    params![self.account_scope, fill.fill_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if let Some((prev_strat, prev_json)) = prev {
                let prev_fill: ExchangeFill = serde_json::from_str(&prev_json)?;
                ensure!(
                    prev_fill == *fill,
                    "conflicting duplicate durable fill '{}'",
                    fill.fill_id
                );
                if owner_strategy.is_some() {
                    ensure!(
                        prev_strat == strat,
                        "conflicting strategy ownership for fill '{}'",
                        fill.fill_id
                    );
                }
                tx.commit()?;
                return Ok(false);
            }
            tx.execute(
                "INSERT INTO settlement_fills (account_scope, fill_id, owner_strategy, payload_json) VALUES (?1,?2,?3,?4)",
                params![self.account_scope, fill.fill_id, strat, payload],
            )?;
            tx.commit()?;
            Ok(true)
        })
    }

    pub fn record_funding(&self, payment: &FundingPayment) -> Result<bool> {
        ensure!(
            payment.timestamp_ms > 0,
            "funding timestamp must be positive"
        );
        let payload = serde_json::to_string(payment)?;
        self.storage.with_conn(|conn| -> Result<bool> {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let prev: Option<String> = tx
                .query_row(
                    "SELECT payload_json FROM settlement_funding WHERE account_scope=?1 AND funding_id=?2",
                    params![self.account_scope, payment.funding_id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(prev_json) = prev {
                let prev_fp: FundingPayment = serde_json::from_str(&prev_json)?;
                ensure!(
                    prev_fp == *payment,
                    "conflicting duplicate funding '{}'",
                    payment.funding_id
                );
                tx.commit()?;
                return Ok(false);
            }
            tx.execute(
                "INSERT INTO settlement_funding (account_scope, funding_id, payload_json) VALUES (?1,?2,?3)",
                params![self.account_scope, payment.funding_id, payload],
            )?;
            tx.commit()?;
            Ok(true)
        })
    }

    pub fn record_external_cash_flow(&self, flow: &ExternalCashFlow) -> Result<bool> {
        ensure!(
            !flow.cash_flow_id.trim().is_empty() && flow.timestamp_ms > 0,
            "invalid external cash flow"
        );
        let payload = serde_json::to_string(flow)?;
        self.storage.with_conn(|conn| -> Result<bool> {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let prev: Option<String> = tx
                .query_row(
                    "SELECT payload_json FROM settlement_cash_flows WHERE account_scope=?1 AND cash_flow_id=?2",
                    params![self.account_scope, flow.cash_flow_id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(prev_json) = prev {
                let prev_cf: ExternalCashFlow = serde_json::from_str(&prev_json)?;
                ensure!(
                    prev_cf == *flow,
                    "conflicting duplicate cash flow '{}'",
                    flow.cash_flow_id
                );
                tx.commit()?;
                return Ok(false);
            }
            tx.execute(
                "INSERT INTO settlement_cash_flows (account_scope, cash_flow_id, payload_json) VALUES (?1,?2,?3)",
                params![self.account_scope, flow.cash_flow_id, payload],
            )?;
            tx.commit()?;
            Ok(true)
        })
    }

    pub fn record_funding_coverage(&self, window: &FundingCoverageWindow) -> Result<()> {
        ensure!(
            window.end_ms >= window.start_ms && !window.symbol.trim().is_empty(),
            "invalid funding coverage window"
        );
        let sym = window.symbol.trim().to_ascii_uppercase();
        self.storage.with_conn(|conn| -> Result<()> {
            conn.execute(
                "INSERT INTO settlement_funding_coverage (account_scope, symbol, start_ms, end_ms, complete)
                 VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT(account_scope, symbol, start_ms, end_ms) DO UPDATE SET complete=excluded.complete",
                params![
                    self.account_scope,
                    sym,
                    window.start_ms,
                    window.end_ms,
                    if window.complete { 1 } else { 0 }
                ],
            )?;
            Ok(())
        })
    }

    pub fn record_pending_funding(&self, ob: &PendingFundingObligation) -> Result<()> {
        ensure!(
            !ob.obligation_id.trim().is_empty()
                && !ob.symbol.trim().is_empty()
                && ob.interval_end_ms >= ob.interval_start_ms
                && ob.position_qty > Decimal::ZERO,
            "invalid pending funding obligation"
        );
        let mut norm = ob.clone();
        norm.symbol = norm.symbol.trim().to_ascii_uppercase();
        let payload = serde_json::to_string(&norm)?;
        self.storage.with_conn(|conn| -> Result<()> {
            conn.execute(
                "INSERT INTO settlement_pending_funding (account_scope, obligation_id, payload_json)
                 VALUES (?1,?2,?3)
                 ON CONFLICT(account_scope, obligation_id) DO UPDATE SET payload_json=excluded.payload_json",
                params![self.account_scope, norm.obligation_id, payload],
            )?;
            Ok(())
        })
    }

    pub fn resolve_pending_funding_and_ingest(
        &self,
        obligation_id: &str,
        payment: &FundingPayment,
    ) -> Result<()> {
        ensure!(
            payment.timestamp_ms > 0,
            "funding timestamp must be positive"
        );
        let fp_json = serde_json::to_string(payment)?;
        self.storage.with_conn(|conn| -> Result<()> {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let ob_json: String = tx
                .query_row(
                    "SELECT payload_json FROM settlement_pending_funding WHERE account_scope=?1 AND obligation_id=?2",
                    params![self.account_scope, obligation_id],
                    |r| r.get(0),
                )
                .with_context(|| format!("missing durable pending funding obligation '{obligation_id}'"))?;
            let mut ob: PendingFundingObligation = serde_json::from_str(&ob_json)?;
            ob.resolved = true;
            ob.funding_id = Some(payment.funding_id.clone());
            let new_ob_json = serde_json::to_string(&ob)?;
            tx.execute(
                "UPDATE settlement_pending_funding SET payload_json=?1 WHERE account_scope=?2 AND obligation_id=?3",
                params![new_ob_json, self.account_scope, obligation_id],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO settlement_funding (account_scope, funding_id, payload_json) VALUES (?1,?2,?3)",
                params![self.account_scope, payment.funding_id, fp_json],
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn rebuild_engine(&self) -> Result<SettlementEngine> {
        let mut eng = SettlementEngine::new();
        self.storage.with_conn(|conn| -> Result<()> {
            {
                let mut stmt = conn.prepare(
                    "SELECT symbol, start_ms, end_ms, complete FROM settlement_funding_coverage WHERE account_scope=?1",
                )?;
                let rows = stmt.query_map(params![self.account_scope], |r| {
                    Ok(FundingCoverageWindow {
                        symbol: r.get(0)?,
                        start_ms: r.get(1)?,
                        end_ms: r.get(2)?,
                        complete: r.get::<_, i64>(3)? != 0,
                    })
                })?;
                for w in rows {
                    eng.record_funding_coverage(w?)?;
                }
            }
            {
                let mut stmt = conn.prepare(
                    "SELECT payload_json FROM settlement_pending_funding WHERE account_scope=?1",
                )?;
                let rows = stmt.query_map(params![self.account_scope], |r| r.get::<_, String>(0))?;
                for raw in rows {
                    let ob: PendingFundingObligation = serde_json::from_str(&raw?)?;
                    eng.record_pending_funding_obligation(ob)?;
                }
            }
            {
                let mut stmt = conn.prepare(
                    "SELECT payload_json FROM settlement_cash_flows WHERE account_scope=?1",
                )?;
                let rows = stmt.query_map(params![self.account_scope], |r| r.get::<_, String>(0))?;
                for raw in rows {
                    let cf: ExternalCashFlow = serde_json::from_str(&raw?)?;
                    eng.ingest_external_cash_flow(cf)?;
                }
            }
            {
                let mut stmt = conn.prepare(
                    "SELECT payload_json FROM settlement_funding WHERE account_scope=?1",
                )?;
                let rows = stmt.query_map(params![self.account_scope], |r| r.get::<_, String>(0))?;
                for raw in rows {
                    let fp: FundingPayment = serde_json::from_str(&raw?)?;
                    eng.ingest_funding(fp)?;
                }
            }
            {
                let mut stmt = conn.prepare(
                    "SELECT owner_strategy, payload_json FROM settlement_fills WHERE account_scope=?1",
                )?;
                let rows = stmt.query_map(params![self.account_scope], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?;
                for row in rows {
                    let (strat, raw) = row?;
                    let f: ExchangeFill = serde_json::from_str(&raw)?;
                    eng.ingest_fill(f, Some(&strat))?;
                }
            }
            {
                let mut stmt = conn.prepare(
                    "SELECT round_trip_id, seq FROM settlement_learning_emissions WHERE account_scope=?1",
                )?;
                let rows = stmt.query_map(params![self.account_scope], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
                })?;
                for row in rows {
                    let (rt_id, seq) = row?;
                    eng.restore_emitted_learning(rt_id, seq);
                }
            }
            Ok(())
        })?;
        Ok(eng)
    }

    pub fn recompute_and_commit(
        &self,
    ) -> Result<(Vec<SettledRoundTrip>, Vec<VerifiedLearningOutcome>)> {
        let mut eng = self.rebuild_engine()?;
        let rts = eng.settle_all()?.to_vec();
        let newly_emitted = eng.emit_eligible_learning_outcomes();
        self.storage.with_conn(|conn| -> Result<()> {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            for rt in &rts {
                let raw = serde_json::to_string(rt)?;
                tx.execute(
                    "INSERT INTO settlement_round_trips (account_scope, round_trip_id, payload_json)
                     VALUES (?1,?2,?3)
                     ON CONFLICT(account_scope, round_trip_id) DO UPDATE SET payload_json=excluded.payload_json",
                    params![self.account_scope, rt.round_trip_id, raw],
                )?;
            }
            for lo in &newly_emitted {
                let raw = serde_json::to_string(lo)?;
                tx.execute(
                    "INSERT OR IGNORE INTO settlement_learning_emissions (account_scope, round_trip_id, seq, payload_json)
                     VALUES (?1,?2,?3,?4)",
                    params![self.account_scope, lo.round_trip_id, lo.exactly_once_seq as i64, raw],
                )?;
            }
            tx.commit()?;
            Ok(())
        })?;
        Ok((rts, newly_emitted))
    }

    pub fn emitted_learning_outcomes(&self) -> Result<Vec<VerifiedLearningOutcome>> {
        self.storage.with_conn(|conn| -> Result<Vec<VerifiedLearningOutcome>> {
            let mut stmt = conn.prepare(
                "SELECT payload_json FROM settlement_learning_emissions WHERE account_scope=?1 ORDER BY seq ASC",
            )?;
            let rows = stmt.query_map(params![self.account_scope], |r| r.get::<_, String>(0))?;
            let mut out = Vec::new();
            for raw in rows {
                out.push(serde_json::from_str(&raw?)?);
            }
            Ok(out)
        })
    }
}

/// Evaluates an offline settlement fixture (list of fills, funding, external cash flows & coverage windows).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementFixture {
    pub fills: Vec<ExchangeFill>,
    pub funding: Vec<FundingPayment>,
    pub external_cash_flows: Vec<ExternalCashFlow>,
    pub funding_coverage: Vec<FundingCoverageWindow>,
    pub start_equity_usdc: Decimal,
    pub end_equity_usdc: Decimal,
    pub unrealized_pnl_delta_usdc: Decimal,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettlementAuditReport {
    pub round_trips: Vec<SettledRoundTrip>,
    pub equity_reconciliation: EquityReconciliationReport,
    pub eligible_learning_outcomes: Vec<VerifiedLearningOutcome>,
}

pub fn audit_settlement_fixture(f: SettlementFixture) -> Result<SettlementAuditReport> {
    let mut eng = SettlementEngine::new();
    for w in f.funding_coverage {
        eng.record_funding_coverage(w)?;
    }
    for cf in f.external_cash_flows {
        eng.ingest_external_cash_flow(cf)?;
    }
    for fp in f.funding {
        eng.ingest_funding(fp)?;
    }
    for fill in f.fills {
        eng.ingest_fill(fill, Some("fixture"))?;
    }
    let eq = eng
        .reconcile_account_equity(
            f.start_equity_usdc,
            f.end_equity_usdc,
            f.unrealized_pnl_delta_usdc,
            Decimal::new(1, 4),
        )
        .context("equity reconciliation failed")?;
    let learning = eng.emit_eligible_learning_outcomes();
    let round_trips = eng.round_trips().to_vec();
    if eq
        .holds
        .contains(&SettlementHoldReason::UnmatchedExitQuantity)
    {
        bail!("unmatched exit hold");
    }
    Ok(SettlementAuditReport {
        round_trips,
        equity_reconciliation: eq,
        eligible_learning_outcomes: learning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::TokenAmount;

    fn mk_fill(
        id: &str,
        oid: u64,
        side: FillSide,
        qty: i64,
        px: i64,
        fee: Option<Decimal>,
        ts: i64,
    ) -> ExchangeFill {
        ExchangeFill {
            fill_id: id.into(),
            order_id: oid,
            client_order_id: Some(format!("cloid-{id}")),
            symbol: "BTC".into(),
            side,
            quantity: Decimal::from(qty),
            price: Decimal::from(px),
            closed_pnl_usdc: (side == FillSide::Sell).then_some(Decimal::from(100)),
            pnl_convention: PnlConvention::BeforeFees,
            fee: fee.map(|a| TokenAmount {
                token: "USDC".into(),
                amount: a,
            }),
            timestamp_ms: ts,
        }
    }

    #[test]
    fn partial_fill_round_trip_attributes_entry_fee_maker_rebate_and_funding_exactly_once() {
        let mut eng = SettlementEngine::new();
        eng.record_funding_coverage(FundingCoverageWindow {
            symbol: "BTC".into(),
            start_ms: 1_000,
            end_ms: 10_000,
            complete: true,
        })
        .unwrap();
        // Entry Long 2 @ 50000, taker fee $40.00
        assert!(eng
            .ingest_fill(
                mk_fill(
                    "f-in",
                    1,
                    FillSide::Buy,
                    2,
                    50_000,
                    Some(Decimal::from(40)),
                    2_000
                ),
                Some("adaptive_ema_anchor")
            )
            .unwrap());
        // Replay exact fill after reconnect -> ignored cleanly (idempotent)
        assert!(!eng
            .ingest_fill(
                mk_fill(
                    "f-in",
                    1,
                    FillSide::Buy,
                    2,
                    50_000,
                    Some(Decimal::from(40)),
                    2_000
                ),
                Some("adaptive_ema_anchor")
            )
            .unwrap());

        // Funding received +$6.00 across 2 BTC position at t=4_000
        eng.ingest_funding(FundingPayment {
            funding_id: "fund-1".into(),
            symbol: "BTC".into(),
            payment: Some(TokenAmount {
                token: "USDC".into(),
                amount: Decimal::from(6),
            }),
            timestamp_ms: 4_000,
        })
        .unwrap();

        // Exit #1 partial close 1 @ 51000 with maker rebate -$5.00 at t=5_000
        eng.ingest_fill(
            mk_fill(
                "f-out-1",
                2,
                FillSide::Sell,
                1,
                51_000,
                Some(Decimal::from(-5)),
                5_000,
            ),
            None,
        )
        .unwrap();
        // Exit #2 remainder 1 @ 52000 with taker fee +$20.00 at t=7_000
        eng.ingest_fill(
            mk_fill(
                "f-out-2",
                3,
                FillSide::Sell,
                1,
                52_000,
                Some(Decimal::from(20)),
                7_000,
            ),
            None,
        )
        .unwrap();

        let rts = eng.settle_all().unwrap();
        assert_eq!(rts.len(), 2);
        // RT1: gross +1000, entry_fee 20, exit_fee -5 (rebate), funding +3 => net = 1000 - 20 - (-5) + 3 = 988
        assert_eq!(rts[0].status, SettlementStatus::VerifiedSettled);
        assert_eq!(rts[0].verified_net_pnl_usdc, Some(Decimal::from(988)));
        // RT2: gross +2000, entry_fee 20, exit_fee 20, funding +3 => net = 2000 - 20 - 20 + 3 = 1963
        assert_eq!(rts[1].status, SettlementStatus::VerifiedSettled);
        assert_eq!(rts[1].verified_net_pnl_usdc, Some(Decimal::from(1963)));

        // Exactly-once learning outcome emission: second call must return empty!
        let learn1 = eng.emit_eligible_learning_outcomes();
        let learn2 = eng.emit_eligible_learning_outcomes();
        assert_eq!(learn1.len(), 2);
        assert!(learn2.is_empty());
    }

    #[test]
    fn missing_fee_or_coverage_retains_hold_and_never_emits_zero_or_learning() {
        let mut eng = SettlementEngine::new();
        eng.ingest_fill(
            mk_fill("f1", 1, FillSide::Buy, 1, 100, None, 1_000),
            Some("mom"),
        )
        .unwrap();
        eng.ingest_fill(
            mk_fill("f2", 2, FillSide::Sell, 1, 120, Some(Decimal::ONE), 2_000),
            None,
        )
        .unwrap();
        let rts = eng.settle_all().unwrap();
        assert_eq!(rts.len(), 1);
        assert_eq!(rts[0].status, SettlementStatus::HeldIncompleteEvidence);
        assert!(rts[0].verified_net_pnl_usdc.is_none());
        assert!(rts[0].holds.contains(&SettlementHoldReason::UnknownFee));
        assert!(rts[0]
            .holds
            .contains(&SettlementHoldReason::MissingFundingCoverage));
        assert!(eng.emit_eligible_learning_outcomes().is_empty());
    }

    #[test]
    fn equity_reconciliation_separates_deposits_and_flags_discrepancies() {
        let mut eng = SettlementEngine::new();
        eng.record_funding_coverage(FundingCoverageWindow {
            symbol: "BTC".into(),
            start_ms: 1_000,
            end_ms: 10_000,
            complete: true,
        })
        .unwrap();
        eng.ingest_fill(
            mk_fill(
                "f1",
                1,
                FillSide::Buy,
                1,
                1_000,
                Some(Decimal::from(2)),
                2_000,
            ),
            Some("mom"),
        )
        .unwrap();
        eng.ingest_fill(
            mk_fill(
                "f2",
                2,
                FillSide::Sell,
                1,
                1_100,
                Some(Decimal::from(3)),
                3_000,
            ),
            None,
        )
        .unwrap();
        eng.ingest_external_cash_flow(ExternalCashFlow {
            cash_flow_id: "dep-1".into(),
            kind: ExternalCashFlowKind::Deposit,
            token: "USDC".into(),
            signed_amount: Decimal::from(500),
            timestamp_ms: 2_500,
        })
        .unwrap();

        // Net pnl = 100 - 2 - 3 = 95. Start 10000 + 95 + 500 = 10595
        let ok_rep = eng
            .reconcile_account_equity(
                Decimal::from(10_000),
                Decimal::from(10_595),
                Decimal::ZERO,
                Decimal::new(1, 2),
            )
            .unwrap();
        assert!(ok_rep.verified);
        assert_eq!(ok_rep.discrepancy_usdc, Some(Decimal::ZERO));

        let bad_rep = eng
            .reconcile_account_equity(
                Decimal::from(10_000),
                Decimal::from(10_400),
                Decimal::ZERO,
                Decimal::new(1, 2),
            )
            .unwrap();
        assert!(!bad_rep.verified);
        assert!(bad_rep
            .holds
            .contains(&SettlementHoldReason::ExternalCashFlowGap));
    }

    #[test]
    fn durable_settlement_store_survives_restart_holds_pending_funding_and_emits_learning_once() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("settle.db");
        let st1 = Storage::open(&db).unwrap();
        let ss1 = SettlementStore::open(st1, "0x1111111111111111111111111111111111111111").unwrap();
        ss1.record_funding_coverage(&FundingCoverageWindow {
            symbol: "BTC".into(),
            start_ms: 1_000,
            end_ms: 10_000,
            complete: true,
        })
        .unwrap();
        ss1.record_fill(
            &mk_fill(
                "d1",
                10,
                FillSide::Buy,
                1,
                50_000,
                Some(Decimal::from(10)),
                2_000,
            ),
            Some("adaptive_ema_anchor"),
        )
        .unwrap();
        ss1.record_fill(
            &mk_fill(
                "d2",
                11,
                FillSide::Sell,
                1,
                51_000,
                Some(Decimal::from(10)),
                6_000,
            ),
            None,
        )
        .unwrap();
        // Record missed-hour pending funding obligation independently of forward cursor
        ss1.record_pending_funding(&PendingFundingObligation {
            obligation_id: "pend-btc-4000".into(),
            symbol: "BTC".into(),
            interval_start_ms: 3_000,
            interval_end_ms: 4_000,
            position_qty: Decimal::ONE,
            side_label: "LONG".into(),
            resolved: false,
            funding_id: None,
        })
        .unwrap();

        let (rts1, learn1) = ss1.recompute_and_commit().unwrap();
        assert_eq!(rts1.len(), 1);
        assert_eq!(rts1[0].status, SettlementStatus::HeldIncompleteEvidence);
        assert!(rts1[0]
            .holds
            .contains(&SettlementHoldReason::MissingFundingCoverage));
        assert!(learn1.is_empty());

        // Simulate process restart on fresh Storage + SettlementStore handle
        drop(ss1);
        let st2 = Storage::open(&db).unwrap();
        let ss2 = SettlementStore::open(st2, "0x1111111111111111111111111111111111111111").unwrap();
        ss2.resolve_pending_funding_and_ingest(
            "pend-btc-4000",
            &FundingPayment {
                funding_id: "fund-backfill-4000".into(),
                symbol: "BTC".into(),
                payment: Some(TokenAmount {
                    token: "USDC".into(),
                    amount: Decimal::from(-3),
                }),
                timestamp_ms: 4_000,
            },
        )
        .unwrap();

        let (rts2, learn2) = ss2.recompute_and_commit().unwrap();
        assert_eq!(rts2[0].status, SettlementStatus::VerifiedSettled);
        // Gross 1000 - 10 - 10 - 3 = 977
        assert_eq!(rts2[0].verified_net_pnl_usdc, Some(Decimal::from(977)));
        assert_eq!(learn2.len(), 1);
        assert_eq!(learn2[0].verified_net_pnl_usdc, Decimal::from(977));

        // Second recompute after another restart MUST NOT re-emit the learning outcome
        drop(ss2);
        let st3 = Storage::open(&db).unwrap();
        let ss3 = SettlementStore::open(st3, "0x1111111111111111111111111111111111111111").unwrap();
        let (_rts3, learn3) = ss3.recompute_and_commit().unwrap();
        assert!(learn3.is_empty());
        assert_eq!(ss3.emitted_learning_outcomes().unwrap().len(), 1);
    }
}
