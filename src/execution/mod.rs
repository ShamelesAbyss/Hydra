//! Execution engine: the ONLY path that may contact a live venue.
//! All orders pass through `submit`, which enforces:
//! 1. live-trading gate (env or --live)
//! 2. basic parameter sanity
//! 3. structured logging of every lifecycle event

use anyhow::{bail, Result};
use tracing::{info, warn};

use crate::adapters::VenueAdapter;
use crate::core::{OrderIntent, OrderResult};
use crate::storage::Storage;

pub struct ExecutionEngine {
    storage: Storage,
    live: bool,
}

impl ExecutionEngine {
    pub fn new(storage: Storage, live: bool) -> Self {
        if live { info!("execution engine: LIVE"); } else { warn!("execution engine: DRY-RUN (no orders will be sent)"); }
        Self { storage, live }
    }

    pub fn is_live(&self) -> bool { self.live }

    pub async fn submit(&mut self, intent: &OrderIntent, venue: &dyn VenueAdapter) -> Result<OrderResult> {
        // ---- Parameter sanity ----
        if intent.size <= 0.0 || !intent.size.is_finite() {
            bail!("invalid size {}", intent.size);
        }
        if let Some(lp) = intent.limit_price {
            if lp <= 0.0 || !lp.is_finite() {
                bail!("invalid limit_price {}", lp);
            }
        }

        // ---- Dry-run gate ----
        if !self.live {
            info!(
                intent_id=%intent.id, venue=%intent.venue, symbol=%intent.symbol,
                side=intent.side.as_str(), size=intent.size,
                "DRY-RUN would submit"
            );
            return Ok(OrderResult {
                intent_id: intent.id.clone(),
                accepted: false,
                venue_order_id: None,
                filled_size: 0.0,
                avg_fill_price: None,
                message: "dry-run: ENABLE_LIVE_TRADING=false".into(),
            });
        }

        info!(
            intent_id=%intent.id, venue=%intent.venue, symbol=%intent.symbol,
            side=intent.side.as_str(), size=intent.size, reduce_only=intent.reduce_only,
            "submitting live order"
        );
        let res = venue.submit_order(intent).await?;
        info!(
            intent_id=%intent.id, accepted=res.accepted, filled=res.filled_size,
            venue_order_id=?res.venue_order_id, msg=%res.message,
            "order result"
        );
        let _ = &self.storage; // storage is recorded by caller with session_id
        Ok(res)
    }

    pub async fn close(
        &mut self,
        venue: &dyn VenueAdapter,
        symbol: &str,
        size: f64,
        side_is_buy_to_close: bool,
    ) -> Result<OrderResult> {
        if !self.live {
            info!(symbol, size, "DRY-RUN would close");
            return Ok(OrderResult {
                intent_id: uuid::Uuid::new_v4().to_string(),
                accepted: false,
                venue_order_id: None,
                filled_size: 0.0,
                avg_fill_price: None,
                message: "dry-run close".into(),
            });
        }
        venue.close_position(symbol, size, side_is_buy_to_close).await
    }
}
