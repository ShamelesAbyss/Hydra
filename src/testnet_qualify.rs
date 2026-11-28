//! Isolated Testnet functional qualification scenario (`v2.0.0 | THE AWAKENING`, A07).
//!
//! Uses ONLY non-production Testnet/controlled transports (`!venue.is_production_venue()`),
//! shared precision rounding, shared `ExecutionEngine` + `Aegis` reservations, native
//! `reduce_only` protective stops with read-back + safe replace, simulated restart/reconciliation,
//! reduce-only close, and final zero-residual-exposure verification.
//! Failure or residual exposure emits an honest `scenario_passed=false` artifact and returns `Err`.

use crate::adapters::VenueAdapter;
use crate::aegis::{Aegis, RiskLimits};
use crate::config::AppConfig;
use crate::core::engine::Engine;
use crate::core::{OrderIntent, Side};
use crate::execution::ExecutionEngine;
use crate::portfolio::PortfolioManager;
use crate::protective_orders::ProtectiveOrderSpec;
use crate::settlement::SettlementStore;
use crate::storage::Storage;
use crate::trusted_evidence::{TrustStore, TrustedEvidenceKind};
use anyhow::{ensure, Context, Result};
use chrono::Utc;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestnetScenarioParams {
    pub account: String,
    pub signer: String,
    pub symbol: String,
    pub requested_max_notional_usd: f64,
    pub trust_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestnetQualifyReport {
    pub environment: String,
    pub executed_mutations: bool,
    pub wallet: String,
    pub signer: String,
    pub symbol: String,
    pub equity_usd: f64,
    pub requested_notional_usd: f64,
    pub min_notional_usd: String,
    pub entry_fill_verified: bool,
    pub stop_readback_verified: bool,
    pub stop_replace_verified: bool,
    pub restart_reconciliation_verified: bool,
    pub close_verified: bool,
    pub stop_cancel_verified: bool,
    pub zero_residual_exposure: bool,
    pub scenario_passed: bool,
    pub failure_reason: Option<String>,
}

pub async fn run_testnet_scenario(
    params: &TestnetScenarioParams,
    venue: Arc<dyn VenueAdapter>,
    work_dir: &Path,
    trust_store: &TrustStore,
    out_artifact: Option<&Path>,
) -> Result<TestnetQualifyReport> {
    ensure!(
        !venue.is_production_venue(),
        "Testnet qualification refused: venue is ProductionMainnet"
    );
    let constraints = venue.instrument_constraints().await?;
    let c = constraints
        .get(&params.symbol)
        .with_context(|| format!("missing Testnet constraints for {}", params.symbol))?;
    let req_ntl = Decimal::try_from(params.requested_max_notional_usd)
        .context("invalid requested_max_notional_usd")?;
    ensure!(
        req_ntl >= c.min_notional_usd && req_ntl <= Decimal::from(50u32),
        "requested max_notional_usd {} rejected (must be in [{}, 50]; never clamped upward)",
        req_ntl,
        c.min_notional_usd
    );

    let baseline_pos = venue.open_positions().await?;
    ensure!(
        !baseline_pos
            .iter()
            .any(|p| p.symbol.eq_ignore_ascii_case(&params.symbol) && p.size.abs() > 1e-12),
        "disposable Testnet wallet already has an open {} position; clean baseline required",
        params.symbol
    );
    let baseline_stops = venue.list_protective_stops().await?;
    let baseline_stop_oids: std::collections::BTreeSet<u64> = baseline_stops
        .iter()
        .filter(|s| s.symbol.eq_ignore_ascii_case(&params.symbol))
        .filter_map(|s| s.exchange_oid)
        .collect();

    std::fs::create_dir_all(work_dir)?;
    let db_path = work_dir.join("testnet_qualify.db");
    let storage = Storage::open(&db_path)?;
    storage.migrate()?;
    storage.bind_trading_account(&params.account, false)?;

    let equity = venue.account_equity_usd().await?;
    let eq_dec = Decimal::try_from(equity).unwrap_or(Decimal::ZERO);
    ensure!(
        eq_dec >= req_ntl,
        "Testnet account equity {} insufficient for bounded test notional {}",
        eq_dec,
        req_ntl
    );

    let snap = venue.snapshot(&params.symbol, 4).await?;
    let px_raw = Decimal::try_from(snap.mid * 1.005).context("invalid mid")?;
    let qty_raw = (req_ntl / px_raw).round_dp(8);
    let (limit_px, order_qty) = c.validate_and_round(px_raw, qty_raw, Decimal::ONE)?;

    let limits = RiskLimits {
        max_total_exposure_ratio: Decimal::new(90, 2),
        max_market_notional: Decimal::from(100u32),
        max_leverage: Decimal::from(5u32),
        max_drawdown_ratio: Decimal::new(50, 2),
        max_snapshot_age_ms: 30_000,
    };
    let aegis = Aegis::new(storage.clone(), &params.account, limits)?;
    let _settlement = SettlementStore::open(storage.clone(), &params.account)?;
    let mut exec = ExecutionEngine::new(storage.clone(), true).with_authority(aegis, 100.0)?;
    let mut portfolio = PortfolioManager::new(storage.clone());

    let started_ms = Utc::now().timestamp_millis();
    let buy_intent = OrderIntent {
        id: uuid::Uuid::new_v4().to_string(),
        venue: venue.kind().as_str().to_string(),
        symbol: params.symbol.clone(),
        side: Side::Long,
        size: order_qty.to_f64().unwrap_or(0.0),
        limit_price: Some(limit_px.to_f64().unwrap_or(snap.mid * 1.005)),
        reduce_only: false,
        stop_price: Some(snap.mid * 0.96),
        take_profit: Some(snap.mid * 1.05),
        source_signal: Some("testnet_qualify".into()),
        created_at: Utc::now(),
    };

    let mut report = TestnetQualifyReport {
        environment: "HyperliquidTestnet".into(),
        executed_mutations: true,
        wallet: params.account.clone(),
        signer: params.signer.clone(),
        symbol: params.symbol.clone(),
        equity_usd: equity,
        requested_notional_usd: params.requested_max_notional_usd,
        min_notional_usd: c.min_notional_usd.to_string(),
        entry_fill_verified: false,
        stop_readback_verified: false,
        stop_replace_verified: false,
        restart_reconciliation_verified: false,
        close_verified: false,
        stop_cancel_verified: false,
        zero_residual_exposure: false,
        scenario_passed: false,
        failure_reason: None,
    };

    let scenario_res: Result<()> = (async {
        let buy_res = exec.submit(&buy_intent, venue.as_ref()).await?;
        ensure!(
            buy_res.accepted && buy_res.filled_size > 0.0,
            "Testnet IOC entry did not fill: {}",
            buy_res.message
        );
        // Read-back fill evidence from exchange order status / user fills.
        let now_ms = Utc::now().timestamp_millis();
        let obs = venue.order_status_observation(&buy_intent).await?;
        let fills = venue
            .fetch_account_fills(started_ms.saturating_sub(10_000), now_ms + 10_000)
            .await?;
        let obs_filled = matches!(
            &obs.outcome,
            crate::order_status::Outcome::Order(ord)
                if ord.status == "filled" && ord.remaining_size < ord.original_size
        );
        let fill_seen = obs_filled
            || fills.iter().any(|f| {
                f.symbol.eq_ignore_ascii_case(&params.symbol)
                    && (f.client_order_id.as_deref() == Some(buy_intent.id.as_str())
                        || buy_res
                            .venue_order_id
                            .as_deref()
                            .and_then(|s| s.parse::<u64>().ok())
                            == Some(f.order_id))
            });
        ensure!(
            fill_seen,
            "Testnet entry fill not confirmed in order status/fills read-back"
        );
        report.entry_fill_verified = true;
        portfolio.open_from_fill(&buy_intent, &buy_res, Some("testnet_qualify"), None)?;
        storage.finish_intent(&buy_intent.id, "applied", Some(&buy_res))?;

        // Place native reduce_only stop & verify via read-back.
        let filled_dec = Decimal::try_from(buy_res.filled_size).context("invalid filled size")?;
        let trig_1 = Decimal::try_from((snap.mid * 0.96).max(0.1))?.round_dp(2);
        let stop_1 = ProtectiveOrderSpec {
            cloid: uuid::Uuid::new_v4().to_string(),
            exchange_oid: None,
            symbol: params.symbol.clone(),
            owner_strategy: "testnet_qualify".into(),
            position_side: Side::Long,
            protective_side: Side::Short,
            qty: filled_dec,
            trigger_price: trig_1,
            reduce_only: true,
        };
        let placed_1 = venue.place_protective_stop(&stop_1).await?;
        let stops_after_1 = venue.list_protective_stops().await?;
        ensure!(
            stops_after_1.iter().any(|s| {
                s.symbol.eq_ignore_ascii_case(&params.symbol)
                    && s.reduce_only
                    && (placed_1.exchange_oid.is_some() && s.exchange_oid == placed_1.exchange_oid
                        || s.cloid == placed_1.cloid)
            }),
            "placed native reduce_only stop absent in exchange read-back"
        );
        report.stop_readback_verified = true;

        // Safe stop replace: place stop_2 BEFORE canceling stop_1, verify both, then cancel stop_1.
        let trig_2 = Decimal::try_from((snap.mid * 0.955).max(0.1))?.round_dp(2);
        let stop_2 = ProtectiveOrderSpec {
            cloid: uuid::Uuid::new_v4().to_string(),
            exchange_oid: None,
            symbol: params.symbol.clone(),
            owner_strategy: "testnet_qualify".into(),
            position_side: Side::Long,
            protective_side: Side::Short,
            qty: filled_dec,
            trigger_price: trig_2,
            reduce_only: true,
        };
        let placed_2 = venue.place_protective_stop(&stop_2).await?;
        venue
            .cancel_protective_stop(&params.symbol, placed_1.exchange_oid, &placed_1.cloid)
            .await?;
        let stops_after_2 = venue.list_protective_stops().await?;
        let has_2 = stops_after_2.iter().any(|s| {
            s.symbol.eq_ignore_ascii_case(&params.symbol)
                && s.reduce_only
                && (placed_2.exchange_oid.is_some() && s.exchange_oid == placed_2.exchange_oid
                    || s.cloid == placed_2.cloid)
        });
        let still_1 = stops_after_2
            .iter()
            .any(|s| placed_1.exchange_oid.is_some() && s.exchange_oid == placed_1.exchange_oid);
        ensure!(has_2 && !still_1, "safe stop replace read-back failed");
        report.stop_replace_verified = true;

        // Simulated restart/reconciliation over the same Testnet SQLite DB.
        let mut rec_cfg = AppConfig::from_embedded_defaults(work_dir)?;
        rec_cfg.live_trading = false;
        rec_cfg.data_dir = work_dir.to_path_buf();
        let mut restart_eng = Engine::new_with_venues(
            rec_cfg,
            storage.clone(),
            vec![Box::new(VenueBox(venue.clone()))],
            vec![params.symbol.clone()],
            false,
            None,
            None,
            None,
            None,
            Some(SettlementStore::open(storage.clone(), &params.account)?),
            constraints.clone(),
            false,
        )
        .await?;
        let rec_rep = restart_eng.reconcile_startup_and_recovery().await?;
        let prot_rep = restart_eng.reconcile_protective_orders().await?;
        ensure!(
            rec_rep.unresolved_holds == 0 && prot_rep.protection_verified,
            "restart reconciliation failed: holds={} protected={}",
            rec_rep.unresolved_holds,
            prot_rep.protection_verified
        );
        report.restart_reconciliation_verified = true;

        // Reduce-only close of ONLY the test position quantity.
        let close_intent = OrderIntent {
            id: uuid::Uuid::new_v4().to_string(),
            venue: venue.kind().as_str().to_string(),
            symbol: params.symbol.clone(),
            side: Side::Short,
            size: buy_res.filled_size,
            limit_price: None,
            reduce_only: true,
            stop_price: None,
            take_profit: None,
            source_signal: Some("testnet_qualify_close".into()),
            created_at: Utc::now(),
        };
        let close_res = exec.submit(&close_intent, venue.as_ref()).await?;
        ensure!(
            close_res.accepted && close_res.filled_size >= buy_res.filled_size - 1e-9,
            "Testnet reduce-only close failed: {}",
            close_res.message
        );
        report.close_verified = true;

        // Cancel remaining test protective stops (stop_2 and/or any replacement placed during restart recovery).
        let active_stops = venue.list_protective_stops().await?;
        let mut cancel_targets: Vec<ProtectiveOrderSpec> = active_stops
            .into_iter()
            .filter(|s| {
                s.symbol.eq_ignore_ascii_case(&params.symbol)
                    && !s
                        .exchange_oid
                        .is_some_and(|oid| baseline_stop_oids.contains(&oid))
            })
            .collect();
        if cancel_targets.is_empty() {
            cancel_targets.push(placed_2.clone());
        }
        for target in cancel_targets {
            venue
                .cancel_protective_stop(&params.symbol, target.exchange_oid, &target.cloid)
                .await?;
        }
        let stops_final = venue.list_protective_stops().await?;
        let residual_stops = stops_final
            .iter()
            .filter(|s| {
                s.symbol.eq_ignore_ascii_case(&params.symbol)
                    && !s
                        .exchange_oid
                        .is_some_and(|oid| baseline_stop_oids.contains(&oid))
            })
            .count();
        ensure!(residual_stops == 0, "residual Testnet stops remain");
        report.stop_cancel_verified = true;

        // Verify zero residual exposure on the symbol.
        let final_pos = venue.open_positions().await?;
        let residual_pos = final_pos
            .iter()
            .any(|p| p.symbol.eq_ignore_ascii_case(&params.symbol) && p.size.abs() > 1e-12);
        ensure!(!residual_pos, "residual Testnet position exposure remains");
        report.zero_residual_exposure = true;
        Ok(())
    })
    .await;

    if let Err(ref e) = scenario_res {
        report.failure_reason = Some(format!("{e:#}"));
    }
    report.scenario_passed = scenario_res.is_ok()
        && report.entry_fill_verified
        && report.stop_readback_verified
        && report.stop_replace_verified
        && report.restart_reconciliation_verified
        && report.close_verified
        && report.stop_cancel_verified
        && report.zero_residual_exposure;

    let bytes = serde_json::to_vec_pretty(&report)?;
    if let Some(p) = out_artifact {
        std::fs::write(p, &bytes)?;
    }
    let summary = if report.scenario_passed {
        format!(
            "Testnet functional proof PASSED on {} (fill+stop+replace+restart+close+zero residual)",
            params.symbol
        )
    } else {
        format!(
            "Testnet functional proof FAILED/PARTIAL on {}: {:?}",
            params.symbol, report.failure_reason
        )
    };
    if scenario_res.is_ok() {
        trust_store.record_checked(
            TrustedEvidenceKind::TestnetExecutionProof,
            &bytes,
            "n/a",
            &params.trust_fingerprint,
            report.scenario_passed,
            &summary,
            Utc::now().timestamp_millis(),
        )?;
    } else {
        let _ = trust_store.record_checked(
            TrustedEvidenceKind::TestnetExecutionProof,
            &bytes,
            "n/a",
            &params.trust_fingerprint,
            false,
            &summary,
            Utc::now().timestamp_millis(),
        );
    }
    scenario_res?;
    Ok(report)
}

/// Thin forwarding wrapper so `Engine::new_with_venues` can share `Arc<dyn VenueAdapter>`.
struct VenueBox(Arc<dyn VenueAdapter>);

#[async_trait::async_trait]
impl VenueAdapter for VenueBox {
    fn kind(&self) -> crate::core::VenueKind {
        self.0.kind()
    }
    fn is_production_venue(&self) -> bool {
        self.0.is_production_venue()
    }
    fn attach_pilot_authority(&self, cell: crate::live_gate::PilotAuthorityCell) {
        self.0.attach_pilot_authority(cell);
    }
    fn account_identity(&self) -> Option<(String, String)> {
        self.0.account_identity()
    }
    async fn fetch_account_fills(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<crate::ledger::ExchangeFill>> {
        self.0.fetch_account_fills(start_ms, end_ms).await
    }
    async fn fetch_account_funding(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<crate::ledger::FundingPayment>> {
        self.0.fetch_account_funding(start_ms, end_ms).await
    }
    async fn fetch_account_cash_flows(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<crate::settlement::ExternalCashFlow>> {
        self.0.fetch_account_cash_flows(start_ms, end_ms).await
    }
    async fn ping(&self) -> Result<()> {
        self.0.ping().await
    }
    async fn snapshot(&self, symbol: &str, lookback: usize) -> Result<crate::core::MarketSnapshot> {
        self.0.snapshot(symbol, lookback).await
    }
    async fn open_positions(&self) -> Result<Vec<crate::adapters::VenuePosition>> {
        self.0.open_positions().await
    }
    async fn submit_order(&self, intent: &OrderIntent) -> Result<crate::core::OrderResult> {
        self.0.submit_order(intent).await
    }
    async fn close_position(
        &self,
        symbol: &str,
        size: f64,
        side_is_buy_to_close: bool,
    ) -> Result<crate::core::OrderResult> {
        self.0
            .close_position(symbol, size, side_is_buy_to_close)
            .await
    }
    async fn account_equity_usd(&self) -> Result<f64> {
        self.0.account_equity_usd().await
    }
    async fn account_exposure_evidence(
        &self,
    ) -> Result<crate::adapters::account_evidence::AccountExposureEvidence> {
        self.0.account_exposure_evidence().await
    }
    async fn order_status_observation(
        &self,
        intent: &OrderIntent,
    ) -> Result<crate::order_status::Observation> {
        self.0.order_status_observation(intent).await
    }
    async fn open_exchange_orders(
        &self,
    ) -> Result<Vec<crate::execution_recovery::ExchangeOrderSnapshot>> {
        self.0.open_exchange_orders().await
    }
    async fn list_protective_stops(&self) -> Result<Vec<ProtectiveOrderSpec>> {
        self.0.list_protective_stops().await
    }
    async fn place_protective_stop(
        &self,
        spec: &ProtectiveOrderSpec,
    ) -> Result<ProtectiveOrderSpec> {
        self.0.place_protective_stop(spec).await
    }
    async fn cancel_protective_stop(
        &self,
        symbol: &str,
        exchange_oid: Option<u64>,
        cloid: &str,
    ) -> Result<()> {
        self.0
            .cancel_protective_stop(symbol, exchange_oid, cloid)
            .await
    }
    async fn instrument_constraints(
        &self,
    ) -> Result<BTreeMap<String, crate::execution_recovery::InstrumentOrderConstraints>> {
        self.0.instrument_constraints().await
    }
}
