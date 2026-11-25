//! Venue adapter trait + registry.
//!
//! v1.4.3: Ethereum/Uniswap path removed. Hyperliquid covers every perp the
//! user actually trades (180+ assets including ETH, BTC, HYPE, SOL), with
//! sub-second fills, no gas, and no MEV exposure. Single-venue keeps the
//! codebase lean.

use anyhow::Result;
use async_trait::async_trait;

use crate::config::AppConfig;
use crate::core::{MarketSnapshot, OrderIntent, OrderResult, VenueKind};

pub mod account_evidence;
pub mod hyperliquid;
pub mod position_evidence;

#[derive(Clone, Debug)]
pub struct OrderReceipt {
    pub result: OrderResult,
    pub precise_fill: Option<crate::execution_facts::PreciseFill>,
}

#[async_trait]
pub trait VenueAdapter: Send + Sync {
    fn kind(&self) -> VenueKind;

    /// Live/best-effort health ping.
    async fn ping(&self) -> Result<()>;

    /// Fetch a market snapshot (candles + mid + book-derived spread if possible).
    async fn snapshot(&self, symbol: &str, lookback: usize) -> Result<MarketSnapshot>;

    /// Pull the venue-reported open positions for the configured account.
    async fn open_positions(&self) -> Result<Vec<VenuePosition>>;

    /// Optional exact post-order proof. Legacy adapters retain unknown precision.
    async fn position_snapshot(&self) -> Result<Option<position_evidence::PositionSnapshot>> {
        Ok(None)
    }

    /// Submit an order. When live_trading is disabled upstream, this is never called.
    async fn submit_order(&self, intent: &OrderIntent) -> Result<OrderResult>;

    /// Legacy adapters may omit precision; native filled receipts preserve it.
    async fn submit_order_with_evidence(&self, intent: &OrderIntent) -> Result<OrderReceipt> {
        Ok(OrderReceipt {
            result: self.submit_order(intent).await?,
            precise_fill: None,
        })
    }

    /// Close / reduce a position on the venue. Implementations may noop if N/A.
    async fn close_position(
        &self,
        symbol: &str,
        size: f64,
        side_is_buy_to_close: bool,
    ) -> Result<OrderResult>;

    /// USD-denominated equity for compounding/risk sizing.
    async fn account_equity_usd(&self) -> Result<f64>;

    /// Scoped, identity-preserving evidence for conservative reservation release.
    /// Unsupported adapters cannot authorize automatic reconciliation.
    async fn account_exposure_evidence(&self) -> Result<account_evidence::AccountExposureEvidence> {
        anyhow::bail!("venue does not supply scoped account exposure evidence")
    }

    /// Read-only lifecycle evidence; never resubmit or resolve an intent here.
    async fn order_status_observation(
        &self,
        _intent: &OrderIntent,
    ) -> Result<crate::order_status::Observation> {
        anyhow::bail!("venue does not supply scoped order status observations")
    }

    /// Fresh positions plus pending entry exposure. No fallback to local estimates.
    async fn risk_snapshot(&self) -> Result<crate::aegis::AccountSnapshot> {
        anyhow::bail!("venue does not supply an authoritative risk snapshot")
    }
}

#[derive(Debug, Clone)]
pub struct VenuePosition {
    pub symbol: String,
    pub size: f64, // signed: + long, - short
    pub entry_price: f64,
    pub unrealized_pnl: f64,
    pub leverage: f64,
}

pub async fn make_venue(kind: VenueKind, cfg: &AppConfig) -> Result<Box<dyn VenueAdapter>> {
    match kind {
        VenueKind::Hyperliquid => {
            let v = hyperliquid::HyperliquidAdapter::new(cfg).await?;
            Ok(Box::new(v))
        }
    }
}
