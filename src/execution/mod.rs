//! Durable execution boundary. Every entry and reduction is journaled before submission.
use anyhow::{bail, ensure, Result};
use chrono::Utc;

use crate::adapters::VenueAdapter;
use crate::core::{OrderIntent, OrderResult, Side};
use crate::storage::Storage;

/// The CLI flag is an assertion, never an override of the configuration gate.
pub fn authorized_live(configured: bool, requested: bool) -> Result<bool> {
    ensure!(
        !requested || configured,
        "--live requires ENABLE_LIVE_TRADING=true"
    );
    Ok(configured)
}

pub struct ExecutionEngine {
    storage: Storage,
    live: bool,
}

impl ExecutionEngine {
    pub fn new(storage: Storage, live: bool) -> Self {
        Self { storage, live }
    }

    pub fn is_live(&self) -> bool {
        self.live
    }

    pub async fn submit(
        &mut self,
        intent: &OrderIntent,
        venue: &dyn VenueAdapter,
    ) -> Result<OrderResult> {
        ensure!(
            intent.size.is_finite() && intent.size > 0.0,
            "invalid order size"
        );
        ensure!(
            intent.venue == venue.kind().as_str(),
            "order/venue mismatch"
        );
        ensure!(
            uuid::Uuid::parse_str(&intent.id)?.to_string() == intent.id,
            "client order UUID must use canonical lowercase form"
        );
        if let Some(px) = intent.limit_price {
            ensure!(px.is_finite() && px > 0.0, "invalid limit price");
        }
        if !self.live {
            return Ok(OrderResult {
                intent_id: intent.id.clone(),
                accepted: false,
                venue_order_id: None,
                filled_size: 0.0,
                avg_fill_price: None,
                message: "dry-run: ENABLE_LIVE_TRADING=false; no exchange write".into(),
            });
        }
        if !intent.reduce_only {
            self.storage.ensure_execution_resolved()?;
            ensure!(!self.storage.list_open_positions()?.iter().any(|p| p.venue == intent.venue && p.symbol == intent.symbol),
                "instrument already owned by an active position; strategy scaling/arbitration required");
        } else {
            self.storage
                .ensure_symbol_execution_resolved(&intent.venue, &intent.symbol)?;
        }
        // Cancellation after this point leaves 'submitted', blocking new exposure.
        self.storage.reserve_intent(intent)?;
        let res = match venue.submit_order(intent).await {
            Ok(res) => res,
            Err(e) => {
                self.storage
                    .finish_intent(&intent.id, "acknowledgment_unknown", None)?;
                return Err(e.context("order outcome unknown; do not resubmit, reconcile first"));
            }
        };
        let valid = res.intent_id == intent.id
            && res.filled_size.is_finite()
            && res.filled_size >= 0.0
            && res.filled_size <= intent.size
            && (res.filled_size == 0.0
                || (res.accepted && res.avg_fill_price.is_some_and(|p| p.is_finite() && p > 0.0)));
        if !valid {
            self.storage
                .finish_intent(&intent.id, "acknowledgment_unknown", None)?;
            bail!("invalid exchange acknowledgment; reconciliation required");
        }
        let state = if !res.accepted {
            "rejected"
        } else if res.filled_size > 0.0 {
            "filled"
        } else {
            "resting"
        };
        self.storage.finish_intent(&intent.id, state, Some(&res))?;
        Ok(res)
    }

    pub async fn close(
        &mut self,
        venue: &dyn VenueAdapter,
        symbol: &str,
        size: f64,
        buy: bool,
    ) -> Result<OrderResult> {
        let intent = OrderIntent {
            id: uuid::Uuid::new_v4().to_string(),
            venue: venue.kind().as_str().into(),
            symbol: symbol.into(),
            side: if buy { Side::Long } else { Side::Short },
            size,
            limit_price: None,
            reduce_only: true,
            stop_price: None,
            take_profit: None,
            source_signal: None,
            created_at: Utc::now(),
        };
        self.submit(&intent, venue).await
    }
}
