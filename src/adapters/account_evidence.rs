//! Bounded, scoped evidence used to replace reservations with observed exposure.
use anyhow::{ensure, Context, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_POSITIONS: usize = 1024;
pub const MAX_ORDERS: usize = 4096;
const MAX_MARKS: usize = 10_000;
const MAX_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionEvidence {
    pub symbol: String,
    pub signed_size: Decimal,
    pub entry_price: Decimal,
    pub notional: Decimal,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenOrderEvidence {
    pub oid: u64,
    pub symbol: String,
    pub size: Decimal,
    pub limit_price: Decimal,
    pub mark_price: Decimal,
    pub reduce_only: bool,
    pub order_type: String,
    pub is_trigger: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountExposureEvidence {
    pub account: String,
    pub venue: String,
    pub environment: String,
    pub perp_dex: String,
    pub account_mode: String,
    pub acquired_started_ms: i64,
    pub acquired_finished_ms: i64,
    pub exchange_time_ms: i64,
    pub snapshot: crate::aegis::AccountSnapshot,
    pub positions: Vec<PositionEvidence>,
    pub open_orders: Vec<OpenOrderEvidence>,
}

impl AccountExposureEvidence {
    /// Independently recompute exposure, preventing caller-supplied aggregates
    /// from omitting a position or an outstanding entry order.
    pub fn validate(&self, now_ms: i64, max_age_ms: i64) -> Result<()> {
        ensure!(
            self.account == crate::ownership::canonical_account(&self.account)?,
            "noncanonical evidence account"
        );
        ensure!(
            self.venue == "hyperliquid"
                && self.environment == "mainnet"
                && self.perp_dex.is_empty()
                && self.account_mode == "disabled",
            "unsupported account evidence scope"
        );
        ensure!(
            max_age_ms > 0
                && self.acquired_started_ms > 0
                && self.acquired_finished_ms >= self.acquired_started_ms
                && self
                    .acquired_finished_ms
                    .checked_sub(self.acquired_started_ms)
                    .is_some_and(|n| n <= 15_000)
                && now_ms
                    .checked_sub(self.acquired_started_ms)
                    .is_some_and(|n| n >= 0 && n <= max_age_ms)
                && now_ms >= self.acquired_finished_ms
                && self.exchange_time_ms > 0
                && now_ms
                    .checked_sub(self.exchange_time_ms)
                    .is_some_and(|n| n >= 0 && n <= max_age_ms),
            "account evidence is stale, future-dated or acquired too slowly"
        );
        ensure!(
            self.snapshot.observed_at_ms == self.acquired_started_ms
                && self.snapshot.revision == self.acquired_started_ms
                && self.snapshot.equity >= Decimal::ZERO,
            "account snapshot identity does not match acquisition"
        );
        ensure!(
            self.positions.len() <= MAX_POSITIONS && self.open_orders.len() <= MAX_ORDERS,
            "account evidence exceeds record budget"
        );
        let mut symbols = BTreeSet::new();
        let mut orders = BTreeSet::new();
        let mut exposure = BTreeMap::<String, Decimal>::new();
        for p in &self.positions {
            validate_symbol(&p.symbol)?;
            ensure!(symbols.insert(&p.symbol), "duplicate account position");
            ensure!(
                p.signed_size != Decimal::ZERO
                    && p.entry_price > Decimal::ZERO
                    && p.notional > Decimal::ZERO,
                "invalid account position"
            );
            accumulate(&mut exposure, &p.symbol, p.notional)?;
        }
        for order in &self.open_orders {
            validate_symbol(&order.symbol)?;
            ensure!(
                !order.order_type.is_empty()
                    && order.order_type.len() <= 64
                    && !order.order_type.chars().any(char::is_control),
                "invalid open-order type"
            );
            ensure!(
                order.oid > 0 && orders.insert(order.oid),
                "duplicate or invalid open-order identity"
            );
            ensure!(
                order.size > Decimal::ZERO
                    && order.limit_price > Decimal::ZERO
                    && order.mark_price > Decimal::ZERO,
                "invalid open order"
            );
            if !order.reduce_only {
                ensure!(
                    order.order_type == "Limit" && !order.is_trigger,
                    "unsupported contingent or unbounded entry order"
                );
                accumulate(
                    &mut exposure,
                    &order.symbol,
                    order
                        .size
                        .checked_mul(order.limit_price.max(order.mark_price))
                        .context("open-order exposure overflow")?,
                )?;
            }
        }
        ensure!(
            exposure == self.snapshot.exposure_by_market,
            "account snapshot omits or changes observed exposure"
        );
        Ok(())
    }
}
fn accumulate(values: &mut BTreeMap<String, Decimal>, symbol: &str, value: Decimal) -> Result<()> {
    let prior = values.entry(symbol.into()).or_default();
    *prior = prior
        .checked_add(value)
        .context("account exposure overflow")?;
    Ok(())
}
fn validate_symbol(symbol: &str) -> Result<()> {
    ensure!(
        !symbol.is_empty()
            && symbol.len() <= 64
            && symbol
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)),
        "unsupported default-perpetual symbol"
    );
    Ok(())
}
fn decimal(value: &str) -> Result<Decimal> {
    ensure!(value.len() <= 128, "account decimal exceeds budget");
    Decimal::from_str_exact(value).context("account decimal exceeds exact precision")
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawState {
    asset_positions: Vec<RawAssetPosition>,
    margin_summary: RawMargin,
    time: i64,
}
#[derive(Deserialize)]
struct RawAssetPosition {
    position: RawPosition,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPosition {
    coin: String,
    szi: String,
    entry_px: Option<String>,
    position_value: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMargin {
    account_value: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawOrder {
    oid: u64,
    coin: String,
    sz: String,
    limit_px: String,
    reduce_only: bool,
    order_type: String,
    is_trigger: bool,
    #[serde(default)]
    children: Vec<serde_json::Value>,
}

/// Positions, equity and exchange time are decoded from one clearinghouse body.
/// The caller records acquisition bounds covering every read used in the bundle.
pub fn parse_hyperliquid(
    account: &str,
    mode: &str,
    state_json: &str,
    orders_json: &str,
    mids_json: &str,
    started_ms: i64,
    finished_ms: i64,
) -> Result<AccountExposureEvidence> {
    for body in [state_json, orders_json, mids_json] {
        ensure!(
            body.len() <= MAX_PAYLOAD_BYTES,
            "account response exceeds byte budget"
        );
    }
    ensure!(
        mode == "disabled",
        "unsupported collateral model for account evidence"
    );
    let state: RawState = serde_json::from_str(state_json)?;
    let orders: Vec<RawOrder> = serde_json::from_str(orders_json)?;
    let mids: BTreeMap<String, String> = serde_json::from_str(mids_json)?;
    ensure!(
        state.asset_positions.len() <= MAX_POSITIONS
            && orders.len() <= MAX_ORDERS
            && mids.len() <= MAX_MARKS,
        "account response exceeds record budget"
    );
    let mut positions = Vec::new();
    let mut open_orders = Vec::new();
    let mut exposure = BTreeMap::new();
    let mut position_symbols = BTreeSet::new();
    for raw in state.asset_positions {
        let raw = raw.position;
        validate_symbol(&raw.coin)?;
        ensure!(
            position_symbols.insert(raw.coin.clone()),
            "duplicate account position"
        );
        let size = decimal(&raw.szi)?;
        let notional = decimal(
            raw.position_value
                .as_deref()
                .context("account position value missing")?,
        )?;
        if size == Decimal::ZERO {
            ensure!(
                notional == Decimal::ZERO,
                "zero-size account position has nonzero exposure"
            );
            continue;
        }
        let entry = decimal(
            raw.entry_px
                .as_deref()
                .context("account position lacks entry price")?,
        )?;
        accumulate(&mut exposure, &raw.coin, notional)?;
        positions.push(PositionEvidence {
            symbol: raw.coin,
            signed_size: size,
            entry_price: entry,
            notional,
        });
    }
    for raw in orders {
        ensure!(
            raw.children.is_empty(),
            "nested open orders require explicit reconciliation"
        );
        validate_symbol(&raw.coin)?;
        let size = decimal(&raw.sz)?;
        let limit = decimal(&raw.limit_px)?;
        let mark = decimal(mids.get(&raw.coin).context("open-order mark missing")?)?;
        if !raw.reduce_only {
            accumulate(
                &mut exposure,
                &raw.coin,
                size.checked_mul(limit.max(mark))
                    .context("open-order exposure overflow")?,
            )?;
        }
        open_orders.push(OpenOrderEvidence {
            oid: raw.oid,
            symbol: raw.coin,
            size,
            limit_price: limit,
            mark_price: mark,
            reduce_only: raw.reduce_only,
            order_type: raw.order_type,
            is_trigger: raw.is_trigger,
        });
    }
    let evidence = AccountExposureEvidence {
        account: crate::ownership::canonical_account(account)?,
        venue: "hyperliquid".into(),
        environment: "mainnet".into(),
        perp_dex: String::new(),
        account_mode: mode.into(),
        acquired_started_ms: started_ms,
        acquired_finished_ms: finished_ms,
        exchange_time_ms: state.time,
        snapshot: crate::aegis::AccountSnapshot {
            revision: started_ms,
            observed_at_ms: started_ms,
            equity: decimal(&state.margin_summary.account_value)?,
            exposure_by_market: exposure,
        },
        positions,
        open_orders,
    };
    evidence.validate(finished_ms, 30_000)?;
    Ok(evidence)
}
