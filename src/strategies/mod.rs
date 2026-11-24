//! Strategy trait + factory.

use anyhow::Result;

use crate::config::{AppConfig, StrategyConfig};
use crate::core::{MarketSnapshot, Regime, Signal};

pub mod divergence;
pub mod mean_reversion;
pub mod momentum;
pub mod regime;

pub trait Strategy: Send + Sync {
    fn name(&self) -> &'static str;
    fn evaluate(&self, snap: &MarketSnapshot, regime: Regime, cfg: &AppConfig) -> Option<Signal>;
}

pub fn make_strategies(cfg: &StrategyConfig) -> Result<Vec<Box<dyn Strategy>>> {
    let mut out: Vec<Box<dyn Strategy>> = Vec::new();
    for name in &cfg.global.enabled_strategies {
        match name.as_str() {
            "divergence" if cfg.divergence.enabled => {
                out.push(Box::new(divergence::DivergenceStrategy::new(
                    cfg.divergence.clone(),
                )));
            }
            "momentum" if cfg.momentum.enabled => {
                out.push(Box::new(momentum::MomentumStrategy::new(
                    cfg.momentum.clone(),
                )));
            }
            "mean_reversion" if cfg.mean_reversion.enabled => {
                out.push(Box::new(mean_reversion::MeanReversionStrategy::new(
                    cfg.mean_reversion.clone(),
                )));
            }
            _ => tracing::warn!(strategy=%name, "unknown or disabled strategy ignored"),
        }
    }
    if out.is_empty() {
        anyhow::bail!("no strategies initialised");
    }
    tracing::info!(count = out.len(), "strategies initialised");
    Ok(out)
}
