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
            let here: Vec<_> = local
                .iter()
                .filter(|l| l.venue == v.kind().as_str())
                .collect();
            for r in &remote {
                anyhow::ensure!(
                    r.size.is_finite() && r.entry_price.is_finite(),
                    "invalid account position"
                );
                let matches: Vec<_> = here.iter().filter(|l| l.symbol == r.symbol).collect();
                anyhow::ensure!(
                    matches.len() == 1,
                    "unknown or duplicate position {}; reconciliation unresolved",
                    r.symbol
                );
                let l = matches[0];
                let signed = l.size * if l.side == "LONG" { 1.0 } else { -1.0 };
                anyhow::ensure!(
                    (signed - r.size).abs() <= 1e-10
                        && (l.entry_price - r.entry_price).abs() <= 1e-8,
                    "position mismatch {}; reconciliation unresolved",
                    r.symbol
                );
            }
            for l in here {
                anyhow::ensure!(
                    remote.iter().any(|r| r.symbol == l.symbol),
                    "local position {} missing remotely; fill history required, PnL unknown",
                    l.symbol
                );
            }
            let eq = v.account_equity_usd().await?;
            anyhow::ensure!(eq.is_finite() && eq >= 0.0, "invalid account equity");
            self.storage.record_equity(v.kind().as_str(), eq)?;
        }
        Ok(())
    }

    pub async fn force_reconcile(&self, cfg: &AppConfig) -> Result<()> {
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
        let account = crate::ownership::configured_account(cfg)?
            .ok_or_else(|| anyhow::anyhow!("missing trading account"))?;
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
