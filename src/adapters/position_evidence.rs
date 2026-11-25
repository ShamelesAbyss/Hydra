//! One bounded default-perpetual clearinghouse read for protective residual proof.
use anyhow::{ensure, Context, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactPosition {
    pub symbol: String,
    pub signed_quantity: Decimal,
    pub entry_price: Decimal,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionSnapshot {
    pub account: String,
    pub venue: String,
    pub environment: String,
    pub perp_dex: String,
    pub acquired_started_ms: i64,
    pub acquired_finished_ms: i64,
    pub exchange_time_ms: i64,
    pub positions: Vec<ExactPosition>,
}
impl PositionSnapshot {
    pub fn validate(&self, now_ms: i64) -> Result<()> {
        ensure!(
            self.account == crate::ownership::canonical_account(&self.account)?
                && self.venue == "hyperliquid"
                && self.environment == "mainnet"
                && self.perp_dex.is_empty(),
            "unsupported position snapshot scope"
        );
        ensure!(
            self.acquired_started_ms > 0
                && self.acquired_finished_ms >= self.acquired_started_ms
                && self
                    .acquired_finished_ms
                    .checked_sub(self.acquired_started_ms)
                    .is_some_and(|n| n <= 15_000)
                && now_ms
                    .checked_sub(self.acquired_started_ms)
                    .is_some_and(|n| (0..=30_000).contains(&n))
                && now_ms >= self.acquired_finished_ms
                && self.exchange_time_ms > 0
                && now_ms
                    .checked_sub(self.exchange_time_ms)
                    .is_some_and(|n| (0..=30_000).contains(&n)),
            "position snapshot is stale, future-dated or acquired too slowly"
        );
        ensure!(
            self.positions.len() <= 1024,
            "position snapshot exceeds record budget"
        );
        let mut symbols = BTreeSet::new();
        for p in &self.positions {
            symbol(&p.symbol)?;
            ensure!(
                symbols.insert(&p.symbol)
                    && p.signed_quantity != Decimal::ZERO
                    && p.entry_price > Decimal::ZERO,
                "invalid or duplicate position evidence"
            );
        }
        Ok(())
    }
}
fn symbol(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)),
        "unsupported position symbol"
    );
    Ok(())
}
fn decimal(value: &str) -> Result<Decimal> {
    ensure!(value.len() <= 128, "position decimal exceeds budget");
    Decimal::from_str_exact(value).context("unsupported exact position precision")
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct State {
    asset_positions: Vec<Asset>,
    time: i64,
}
#[derive(Deserialize)]
struct Asset {
    position: RawPosition,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPosition {
    coin: String,
    szi: String,
    entry_px: Option<String>,
}
/// No order-book, mark-price, equity or account-mode endpoint is needed to prove
/// default-perpetual residual identity. Unknown/malformed rows invalidate all proof.
pub fn parse_hyperliquid(
    account: &str,
    body: &str,
    started: i64,
    finished: i64,
) -> Result<PositionSnapshot> {
    ensure!(
        body.len() <= 4 * 1024 * 1024,
        "position payload exceeds byte budget"
    );
    let state: State = serde_json::from_str(body)?;
    ensure!(
        state.asset_positions.len() <= 1024,
        "position payload exceeds record budget"
    );
    let mut seen = BTreeSet::new();
    let mut positions = Vec::new();
    for asset in state.asset_positions {
        let p = asset.position;
        symbol(&p.coin)?;
        ensure!(seen.insert(p.coin.clone()), "duplicate position row");
        let qty = decimal(&p.szi)?;
        if qty == Decimal::ZERO {
            continue;
        }
        let price = decimal(
            p.entry_px
                .as_deref()
                .context("nonzero position lacks entry price")?,
        )?;
        positions.push(ExactPosition {
            symbol: p.coin,
            signed_quantity: qty,
            entry_price: price,
        });
    }
    let result = PositionSnapshot {
        account: crate::ownership::canonical_account(account)?,
        venue: "hyperliquid".into(),
        environment: "mainnet".into(),
        perp_dex: String::new(),
        acquired_started_ms: started,
        acquired_finished_ms: finished,
        exchange_time_ms: state.time,
        positions,
    };
    result.validate(finished)?;
    Ok(result)
}
