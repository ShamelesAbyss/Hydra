//! Pure parsers for Hyperliquid `/info` account-history responses.
//!
//! Schema references (no network requests are made by this module):
//! https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint
//! https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint/perpetuals
//!
//! Request individual fills with aggregateByTime=false. A returned page is not
//! a certificate of complete history: endpoint retention and same-time boundary
//! truncation must be handled by the synchronizer. Raw closedPnl is retained with
//! Unverified fee convention; importing JSON alone never certifies net trade PnL.
use anyhow::{ensure, Context, Result};
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::ledger::{ExchangeFill, FillSide, FundingPayment, PnlConvention, TokenAmount};

/// Resource bounds, not claims that a particular API history is complete.
pub const MAX_PAGE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PAGE_RECORDS: usize = 2_000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFill {
    tid: u64,
    oid: u64,
    coin: String,
    side: String,
    px: String,
    sz: String,
    time: i64,
    #[serde(default)]
    cloid: Option<String>,
    #[serde(default)]
    closed_pnl: Option<String>,
    #[serde(default)]
    fee: Option<String>,
    #[serde(default)]
    fee_token: Option<String>,
    #[serde(default)]
    builder_fee: Option<String>,
}

#[derive(Deserialize)]
struct RawFunding {
    time: i64,
    hash: String,
    delta: RawFundingDelta,
}

#[derive(Deserialize)]
struct RawFundingDelta {
    #[serde(rename = "type")]
    kind: String,
    coin: String,
    #[serde(default)]
    usdc: Option<String>,
}

/// Parse userFills or userFillsByTime JSON. Decimal financial inputs must be JSON
/// strings; accepting binary floats would corrupt exact accounting. Builder fees
/// are already included in `fee` and are never subtracted a second time.
pub fn parse_fills(json: &str) -> Result<Vec<ExchangeFill>> {
    ensure!(
        json.len() <= MAX_PAGE_BYTES,
        "fill history page exceeds byte budget"
    );
    let rows: Vec<RawFill> =
        serde_json::from_str(json).context("invalid userFills response schema")?;
    ensure!(
        rows.len() <= MAX_PAGE_RECORDS,
        "fill history page exceeds record budget"
    );
    rows.into_iter()
        .map(|row| {
            supported_perp(&row.coin)?;
            ensure!(
                row.tid > 0 && row.oid > 0 && row.time > 0,
                "fill missing durable exchange identity or time"
            );
            let quantity = decimal(&row.sz)?;
            let price = decimal(&row.px)?;
            ensure!(
                quantity > Decimal::ZERO && price > Decimal::ZERO,
                "nonpositive fill quantity or price"
            );
            quantity
                .checked_mul(price)
                .context("fill notional exceeds decimal range")?;
            let side = match row.side.as_str() {
                "B" => FillSide::Buy,
                "A" => FillSide::Sell,
                _ => anyhow::bail!("unknown Hyperliquid fill side"),
            };
            let client_order_id = row
                .cloid
                .map(|id| {
                    Ok::<_, anyhow::Error>(
                        uuid::Uuid::parse_str(id.strip_prefix("0x").unwrap_or(&id))
                            .context("invalid exchange client order ID")?
                            .to_string(),
                    )
                })
                .transpose()?;
            let fee = match (row.fee, row.fee_token) {
                (Some(amount), Some(token)) => {
                    token_name(&token)?;
                    Some(TokenAmount {
                        token,
                        amount: decimal(&amount)?,
                    })
                }
                // Missing denomination is unknown, not automatically USDC.
                (Some(amount), None) => {
                    decimal(&amount)?;
                    None
                }
                (None, Some(token)) => {
                    token_name(&token)?;
                    None
                }
                (None, None) => None,
            };
            if let Some(builder_fee) = row.builder_fee {
                decimal(&builder_fee)?;
            }
            Ok(ExchangeFill {
                fill_id: row.tid.to_string(),
                order_id: row.oid,
                client_order_id,
                symbol: row.coin,
                side,
                quantity,
                price,
                closed_pnl_usdc: row.closed_pnl.map(|value| decimal(&value)).transpose()?,
                pnl_convention: PnlConvention::Unverified,
                fee,
                timestamp_ms: row.time,
            })
        })
        .collect()
}

/// Funding hashes may cover multiple instruments. The durable key includes the
/// transaction hash, timestamp AND instrument; account/environment scope is added
/// by FinancialLedger. Non-funding ledger updates are deliberately rejected.
pub fn parse_funding(json: &str) -> Result<Vec<FundingPayment>> {
    ensure!(
        json.len() <= MAX_PAGE_BYTES,
        "funding history page exceeds byte budget"
    );
    let rows: Vec<RawFunding> =
        serde_json::from_str(json).context("invalid userFunding response schema")?;
    ensure!(
        rows.len() <= MAX_PAGE_RECORDS,
        "funding history page exceeds record budget"
    );
    rows.into_iter()
        .map(|row| {
            ensure!(
                row.delta.kind == "funding",
                "non-funding update supplied to funding parser"
            );
            supported_perp(&row.delta.coin)?;
            ensure!(row.time > 0, "invalid funding timestamp");
            ensure!(
                row.hash.len() == 66
                    && row.hash.starts_with("0x")
                    && row.hash[2..].bytes().all(|b| b.is_ascii_hexdigit())
                    && row.hash[2..].bytes().any(|b| b != b'0'),
                "funding update lacks a valid transaction hash"
            );
            let funding_id = format!(
                "{}:{}:{}",
                row.hash.to_ascii_lowercase(),
                row.time,
                row.delta.coin
            );
            let payment = row
                .delta
                .usdc
                .map(|value| -> Result<_> {
                    Ok(TokenAmount {
                        token: "USDC".into(),
                        amount: decimal(&value)?,
                    })
                })
                .transpose()?;
            Ok(FundingPayment {
                funding_id,
                symbol: row.delta.coin,
                payment,
                timestamp_ms: row.time,
            })
        })
        .collect()
}

fn decimal(value: &str) -> Result<Decimal> {
    let value = value.trim();
    ensure!(
        !value.is_empty() && value.len() <= 96,
        "invalid financial decimal"
    );
    Decimal::from_str_exact(value).context("invalid or out-of-range financial decimal")
}
fn token_name(token: &str) -> Result<()> {
    ensure!(
        !token.is_empty() && token.len() <= 16 && token.bytes().all(|b| b.is_ascii_alphanumeric()),
        "invalid fee token"
    );
    Ok(())
}
fn supported_perp(symbol: &str) -> Result<()> {
    // Spot and HIP-3 require instrument/collateral metadata before closed-PnL or
    // funding can be labeled USDC. Reject visibly instead of misdenominating it.
    ensure!(
        !symbol.is_empty()
            && symbol.len() <= 64
            && symbol
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "unsupported spot/HIP-3 instrument or invalid symbol; collateral mapping required"
    );
    Ok(())
}
