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
    pub fn new(storage: Storage) -> Self { Self { storage } }

    /// Compare local DB positions against venue-reported positions on startup.
    pub async fn reconcile_with_venues(
        &self,
        _cfg: &AppConfig,
        venues: &[Box<dyn VenueAdapter>],
    ) -> Result<()> {
        let local = self.storage.list_open_positions()?;
        for v in venues {
            let remote = match v.open_positions().await {
                Ok(p) => p,
                Err(e) => { tracing::warn!(venue=v.kind().as_str(), error=%e, "reconcile: could not fetch remote positions"); continue; }
            };
            // Log mismatches
            for r in &remote {
                let matched = local.iter().any(|l| l.venue == v.kind().as_str() && l.symbol == r.symbol);
                if !matched {
                    tracing::warn!(venue=v.kind().as_str(), symbol=%r.symbol, size=r.size,
                        "venue has position unknown to local state - leaving untouched, logging only");
                }
            }
            for l in &local {
                if l.venue != v.kind().as_str() { continue; }
                let has = remote.iter().any(|r| r.symbol == l.symbol);
                if !has {
                    tracing::warn!(position_id=l.id, symbol=%l.symbol,
                        "local position missing on venue - marking closed at entry price");
                    let session = self.storage.list_sessions(1)?.first().map(|s| s.id).unwrap_or(0);
                    self.storage.close_position(l.id, l.entry_price, 0.0, session, "reconcile_missing")?;
                }
            }
            // equity snapshot
            if let Ok(eq) = v.account_equity_usd().await {
                if eq > 0.0 {
                    self.storage.record_equity(v.kind().as_str(), eq)?;
                }
            }
        }
        Ok(())
    }

    pub async fn force_reconcile(&self, cfg: &AppConfig) -> Result<()> {
        let mut venues: Vec<Box<dyn VenueAdapter>> = Vec::new();
        for k in [VenueKind::Hyperliquid] {
            if let Ok(v) = make_venue(k, cfg).await { venues.push(v); }
        }
        self.reconcile_with_venues(cfg, &venues).await?;
        println!("{}", "reconcile complete".green().bold());
        Ok(())
    }

    pub async fn emergency_flatten(&self, cfg: &AppConfig) -> Result<()> {
        let mut venues: Vec<Box<dyn VenueAdapter>> = Vec::new();
        for k in [VenueKind::Hyperliquid] {
            if let Ok(v) = make_venue(k, cfg).await { venues.push(v); }
        }
        if !cfg.live_trading {
            println!("{}", "ENABLE_LIVE_TRADING=false — not sending close orders".yellow().bold());
        }
        for p in self.storage.list_open_positions()? {
            if let Some(v) = venues.iter().find(|v| v.kind().as_str() == p.venue) {
                if cfg.live_trading {
                    let _ = v.close_position(&p.symbol, p.size, p.side == "SHORT").await;
                }
                let session = self.storage.list_sessions(1)?.first().map(|s| s.id).unwrap_or(0);
                self.storage.close_position(p.id, p.entry_price, 0.0, session, "emergency_flatten")?;
                println!("flattened {}/{} size={}", p.venue, p.symbol, p.size);
            }
        }
        Ok(())
    }

    pub async fn print_snapshot(&self, cfg: &AppConfig) -> Result<()> {
        let sessions = self.storage.list_sessions(1)?;
        if let Some(s) = sessions.first() {
            println!(" last session #{} started {} (reason: {:?})",
                s.id, s.started_at.to_rfc3339(), s.shutdown_reason);
        }
        let pos = self.storage.list_open_positions()?;
        println!(" open positions in local state: {}", pos.len());
        let compound = self.storage.get_compound_state()?;
        println!(" compound.realized_profit   : ${:.2}", compound.realized_profit);
        println!(" compound.current_risk_mult : {:.3}x", compound.current_risk_mult);
        println!(" last reconciliation sample (live fetch):");
        for k in [VenueKind::Hyperliquid] {
            match make_venue(k, cfg).await {
                Ok(v) => {
                    match v.account_equity_usd().await {
                        Ok(eq) => println!("   {:<12} equity ≈ ${:.2}", k.as_str(), eq),
                        Err(e) => println!("   {:<12} equity unavailable ({e})", k.as_str()),
                    }
                }
                Err(e) => println!("   {:<12} adapter unavailable ({e})", k.as_str()),
            }
        }
        let _ = Utc::now();
        Ok(())
    }
}
