//! State manager: restart recovery, reconciliation, emergency flatten.

use anyhow::Result;
use chrono::Utc;
use owo_colors::OwoColorize;

use crate::adapters::{make_venue, VenueAdapter};
use crate::config::AppConfig;
use crate::core::VenueKind;
use crate::storage::Storage;

pub struct StateManager {
    storage: Storage,
}

impl StateManager {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    /// Compare local DB positions against venue-reported positions on startup.
    pub async fn reconcile_with_venues(
        &self,
        cfg: &AppConfig,
        venues: &[Box<dyn VenueAdapter>],
    ) -> Result<()> {
        let local = self.storage.list_open_positions()?;
        // No-identity dry-run is market-data-only. Never pretend to reconcile an account.
        if !cfg.live_trading
            && cfg.hl_account_address.is_none()
            && cfg.hl_private_key.is_none()
            && local.is_empty()
        {
            self.storage.ensure_execution_resolved()?;
            tracing::info!("market-data-only dry-run: account reconciliation unavailable without an account identity");
            return Ok(());
        }
        for v in venues {
            let remote = v.open_positions().await?;
            crate::account_positions::validate(&local, &remote, v.kind().as_str())?;
            if cfg.live_trading {
                let account = crate::ownership::configured_account(cfg)?
                    .ok_or_else(|| anyhow::anyhow!("recovery account identity missing"))?;
                let recovered = crate::position_context::recover_applied_entries(
                    &self.storage,
                    &account,
                    v.kind().as_str(),
                    &remote,
                )?;
                if !recovered.is_empty() {
                    tracing::info!(count=recovered.len(), "recovered durable entry applications; financial settlement remains separate");
                }
            }
            let eq = v.account_equity_usd().await?;
            anyhow::ensure!(eq.is_finite() && eq >= 0.0, "invalid account equity");
            self.storage.record_equity(v.kind().as_str(), eq)?;
        }
        Ok(())
    }

    pub async fn force_reconcile(&self, cfg: &AppConfig) -> Result<()> {
        let _ownership = if cfg.live_trading {
            let account = crate::ownership::configured_account(cfg)?
                .ok_or_else(|| anyhow::anyhow!("recovery account identity missing"))?;
            self.storage.bind_trading_account(&account, false)?;
            Some(crate::ownership::AccountOwnership::acquire(
                &crate::ownership::AccountOwnership::default_root()?,
                &account,
            )?)
        } else {
            None
        };
        let mut venues: Vec<Box<dyn VenueAdapter>> = Vec::new();
        for k in [VenueKind::Hyperliquid] {
            venues.push(make_venue(k, cfg).await?);
        }
        self.reconcile_with_venues(cfg, &venues).await?;
        println!("{}", "reconcile complete".green().bold());
        Ok(())
    }

    pub async fn emergency_flatten(&self, cfg: &AppConfig) -> Result<()> {
        anyhow::ensure!(
            cfg.live_trading,
            "ENABLE_LIVE_TRADING=false; no orders or local position changes"
        );
        // Explicit owner command (`emergency-flatten --confirm`): reduce-only risk reduction only.
        crate::live_gate::require_risk_reduction_authority(
            crate::live_gate::ExecutionChannel::StateFlatten,
            cfg.live_trading,
        )?;
        let account = crate::ownership::configured_account(cfg)?
            .ok_or_else(|| anyhow::anyhow!("missing trading account"))?;
        self.storage.bind_trading_account(&account, false)?;
        let _ownership = crate::ownership::AccountOwnership::acquire(
            &crate::ownership::AccountOwnership::default_root()?,
            &account,
        )?;
        let venue = make_venue(VenueKind::Hyperliquid, cfg).await?;
        let mut execution =
            crate::execution::ExecutionEngine::new(self.storage.clone(), cfg.live_trading);
        let mut portfolio = crate::portfolio::PortfolioManager::new(self.storage.clone());
        portfolio.flatten_all(cfg, &mut execution, &[venue]).await
    }

    pub async fn print_snapshot(&self, cfg: &AppConfig) -> Result<()> {
        let sessions = self.storage.list_sessions(1)?;
        if let Some(s) = sessions.first() {
            println!(
                " last session #{} started {} (reason: {:?})",
                s.id,
                s.started_at.to_rfc3339(),
                s.shutdown_reason
            );
        }
        let pos = self.storage.list_open_positions()?;
        println!(" open positions in local state: {}", pos.len());
        let compound = self.storage.get_compound_state()?;
        println!(
            " compound.realized_profit   : ${:.2}",
            compound.realized_profit
        );
        println!(
            " compound.current_risk_mult : {:.3}x",
            compound.current_risk_mult
        );
        println!(" last reconciliation sample (live fetch):");
        for k in [VenueKind::Hyperliquid] {
            match make_venue(k, cfg).await {
                Ok(v) => match v.account_equity_usd().await {
                    Ok(eq) => println!("   {:<12} equity ≈ ${:.2}", k.as_str(), eq),
                    Err(e) => println!("   {:<12} equity unavailable ({e})", k.as_str()),
                },
                Err(e) => println!("   {:<12} adapter unavailable ({e})", k.as_str()),
            }
        }
        let _ = Utc::now();
        Ok(())
    }
}
