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
    authority: Option<(crate::aegis::Aegis, f64)>,
    ledger: Option<crate::ledger::FinancialLedger>,
}

impl ExecutionEngine {
    pub fn new(storage: Storage, live: bool) -> Self {
        Self {
            storage,
            live,
            authority: None,
            ledger: None,
        }
    }

    pub fn with_authority(
        mut self,
        authority: crate::aegis::Aegis,
        slippage_bps: f64,
    ) -> Result<Self> {
        ensure!(
            slippage_bps.is_finite() && slippage_bps > 0.0 && slippage_bps <= 1000.0,
            "invalid execution slippage"
        );
        ensure!(
            authority.uses_storage(&self.storage),
            "risk authority and execution must share one storage handle"
        );
        self.authority = Some((authority, slippage_bps));
        Ok(self)
    }

    pub fn with_ledger(mut self, ledger: crate::ledger::FinancialLedger) -> Self {
        self.ledger = Some(ledger);
        self
    }

    pub fn is_live(&self) -> bool {
        self.live
    }

    /// Conservative maintenance path. It is never invoked for protective
    /// reductions, so an unavailable account read cannot block their submission.
    pub async fn reconcile_reservations(&self, venue: &dyn VenueAdapter) -> Result<usize> {
        Ok(self
            .reconciliation_evidence(venue)
            .await?
            .map_or(0, |value| value.0))
    }
    async fn reconciliation_evidence(
        &self,
        venue: &dyn VenueAdapter,
    ) -> Result<
        Option<(
            usize,
            crate::adapters::account_evidence::AccountExposureEvidence,
        )>,
    > {
        if !self.live {
            return Ok(None);
        }
        let Some((authority, _)) = &self.authority else {
            return Ok(None);
        };
        if !authority.has_terminal_reservations()? {
            return Ok(None);
        }
        let evidence = venue.account_exposure_evidence().await?;
        ensure!(
            evidence.venue == venue.kind().as_str(),
            "account evidence venue mismatch"
        );
        let count = authority
            .reconcile_terminal_entries(&evidence, Utc::now().timestamp_millis())?
            .len();
        Ok(Some((count, evidence)))
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
        let reconciled = if !intent.reduce_only {
            self.reconciliation_evidence(venue).await?
        } else {
            None
        };
        if !intent.reduce_only {
            self.storage.ensure_execution_resolved()?;
            ensure!(!self.storage.list_open_positions()?.iter().any(|p| p.venue == intent.venue && p.symbol == intent.symbol),
                "instrument already owned by an active position; strategy scaling/arbitration required");
        } else {
            self.storage
                .ensure_symbol_execution_resolved(&intent.venue, &intent.symbol)?;
        }
        let mut journaled = false;
        let mut bounded_intent = intent.clone();
        if !intent.reduce_only {
            if let Some((authority, slippage)) = &self.authority {
                use rust_decimal::Decimal;
                let evidence = if let Some((_, evidence)) = reconciled {
                    evidence
                } else {
                    venue.account_exposure_evidence().await?
                };
                ensure!(
                    evidence.venue == venue.kind().as_str(),
                    "account evidence venue mismatch"
                );
                let account = &evidence.snapshot;
                ensure!(
                    !account
                        .exposure_by_market
                        .get(&intent.symbol)
                        .is_some_and(|value| *value > Decimal::ZERO),
                    "instrument already has exchange exposure; reconcile ownership before entering"
                );
                let market = venue.snapshot(&intent.symbol, 2).await?;
                ensure!(
                    market.mid.is_finite() && market.mid > 0.0 && market.is_fresh(5),
                    "invalid risk price reference"
                );
                let bound = intent.limit_price.unwrap_or(
                    market.mid
                        * if intent.side.is_buy() {
                            1.0 + slippage / 10_000.0
                        } else {
                            1.0 - slippage / 10_000.0
                        },
                );
                ensure!(bound.is_finite() && bound > 0.0, "invalid price bound");
                bounded_intent.limit_price = Some(bound);
                let notional = Decimal::from_str_exact(&intent.size.to_string())?
                    .checked_mul(Decimal::from_str_exact(&bound.max(market.mid).to_string())?)
                    .ok_or_else(|| anyhow::anyhow!("risk notional overflow"))?;
                match authority.reserve_journaled_entry(
                    &bounded_intent,
                    notional,
                    Decimal::ONE,
                    &evidence,
                    Utc::now().timestamp_millis(),
                )? {
                    crate::aegis::RiskDecision::Approved(_) => {
                        journaled = true;
                    }
                    crate::aegis::RiskDecision::Blocked { reason, .. } => bail!("Aegis: {reason}"),
                }
            }
        }
        let intent = &bounded_intent;
        // Cancellation after this point leaves 'submitted', blocking new exposure.
        if !journaled {
            self.storage.reserve_intent(intent)?;
        }
        if let Some(ledger) = &self.ledger {
            ledger.bind_intent(&intent.id)?;
        }
        let receipt = match venue.submit_order_with_evidence(intent).await {
            Ok(res) => res,
            Err(e) => {
                self.storage
                    .finish_intent(&intent.id, "acknowledgment_unknown", None)?;
                return Err(e.context("order outcome unknown; do not resubmit, reconcile first"));
            }
        };
        let res = receipt.result;
        let valid = res.intent_id == intent.id
            && res.filled_size.is_finite()
            && res.filled_size >= 0.0
            && res.filled_size <= intent.size
            && res
                .avg_fill_price
                .is_none_or(|price| price.is_finite() && price > 0.0)
            && (res.filled_size == 0.0
                || (res.accepted && res.avg_fill_price.is_some_and(|p| p.is_finite() && p > 0.0)));
        let precise_valid = receipt
            .precise_fill
            .as_ref()
            .is_none_or(|facts| facts.validate(intent, &res).is_ok());
        if !valid || !precise_valid {
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
        crate::execution_facts::record_receipt(
            &self.storage,
            state,
            &res,
            receipt.precise_fill.as_ref(),
        )?;
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
