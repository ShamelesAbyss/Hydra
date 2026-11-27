//! AEGIS Independent Portfolio Risk Sovereignty (`v1.14.0 | PROJECT IMMORTALITY`).
//!
//! Safety invariants:
//! - Fail-closed: unavailable risk inputs (equity, collateral, liquidation distance, depth,
//!   instrument max leverage, daily/weekly PnL) ALWAYS deny exposure expansion.
//! - Final rejection authority regardless of strategy confidence, ORACLE metrics, or GENESIS score.
//! - Validates post-rounding & post-fee exposure, correlated clusters, funding drag, and
//!   audited circuit-breaker / daily/weekly loss limits.

use crate::aegis::RearmAuthorization;
use crate::core::Side;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SovereignRiskPolicy {
    pub max_total_exposure_ratio: Decimal,
    pub max_market_notional_usd: Decimal,
    pub max_strategy_allocation_ratio: Decimal,
    pub max_single_asset_concentration: Decimal,
    pub correlation_threshold: f64,
    pub max_correlated_cluster_ratio: Decimal,
    pub min_liquidation_distance_pct: Decimal,
    pub max_funding_drag_bps_1h: Decimal,
    pub max_liquidity_participation_ratio: Decimal,
    pub max_volatility_pct: Decimal,
    pub max_daily_loss_ratio: Decimal,
    pub max_weekly_loss_ratio: Decimal,
    pub max_drawdown_ratio: Decimal,
}

impl Default for SovereignRiskPolicy {
    fn default() -> Self {
        Self {
            max_total_exposure_ratio: Decimal::new(50, 2), // 0.50x equity
            max_market_notional_usd: Decimal::from(5_000u32),
            max_strategy_allocation_ratio: Decimal::new(35, 2), // 0.35x equity
            max_single_asset_concentration: Decimal::new(30, 2), // 0.30x equity
            correlation_threshold: 0.70,
            max_correlated_cluster_ratio: Decimal::new(40, 2), // 0.40x equity
            min_liquidation_distance_pct: Decimal::from(12u32), // >= 12% buffer
            max_funding_drag_bps_1h: Decimal::from(10u32),     // <= 10 bps/h cost
            max_liquidity_participation_ratio: Decimal::new(10, 2), // <= 10% top depth
            max_volatility_pct: Decimal::from(15u32),          // <= 15%
            max_daily_loss_ratio: Decimal::new(3, 2),          // 3% daily loss
            max_weekly_loss_ratio: Decimal::new(8, 2),         // 8% weekly loss
            max_drawdown_ratio: Decimal::new(15, 2),           // 15% HWM drawdown
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SovereignPositionInput {
    pub symbol: String,
    pub owner_strategy: String,
    pub side: Side,
    pub notional_usd: Decimal,
    pub liquidation_distance_pct: Option<Decimal>,
    pub funding_bps_1h: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SovereignPortfolioState {
    pub account_equity_usd: Option<Decimal>,
    pub available_collateral_usd: Option<Decimal>,
    pub reserved_notional_usd: Decimal,
    pub open_positions: Vec<SovereignPositionInput>,
    pub daily_pnl_usd: Option<Decimal>,
    pub weekly_pnl_usd: Option<Decimal>,
    pub high_water_equity_usd: Option<Decimal>,
    pub pairwise_correlations: BTreeMap<(String, String), f64>,
    pub emergency_suspended: bool,
    pub breaker_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SovereignAdmissionProposal {
    pub symbol: String,
    pub owner_strategy: String,
    pub side: Side,
    pub requested_notional_usd: Decimal,
    pub estimated_fee_usd: Decimal,
    pub estimated_liquidation_distance_pct: Option<Decimal>,
    pub market_depth_usd: Option<Decimal>,
    pub market_volatility_pct: Option<Decimal>,
    pub funding_bps_1h: Option<Decimal>,
    pub instrument_max_leverage: Option<Decimal>,
    pub requested_leverage: Decimal,
    pub rearm_authorization: Option<RearmAuthorization>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum SovereignRiskDecision {
    Approved {
        post_fill_equity_usd: Decimal,
        post_fill_exposure_ratio: Decimal,
        remaining_collateral_usd: Decimal,
    },
    Rejected {
        reasons: Vec<String>,
        missing_inputs: Vec<String>,
        breaker_triggered: bool,
    },
}

pub fn evaluate_sovereign_admission(
    policy: &SovereignRiskPolicy,
    state: &SovereignPortfolioState,
    proposal: &SovereignAdmissionProposal,
) -> SovereignRiskDecision {
    let mut reasons = Vec::new();
    let mut missing = Vec::new();
    let mut breaker_triggered = false;

    if state.emergency_suspended {
        reasons.push("Emergency suspension is active".into());
        breaker_triggered = true;
    }
    if let Some(ref br) = state.breaker_reason {
        if proposal.rearm_authorization.is_none() {
            reasons.push(format!(
                "Persistent breaker latched without audited rearm: {br}"
            ));
            breaker_triggered = true;
        }
    }
    if proposal.requested_notional_usd <= Decimal::ZERO {
        reasons.push("Requested notional must be positive".into());
    }
    if proposal.estimated_fee_usd < Decimal::ZERO {
        reasons.push("Estimated execution fee cannot be negative".into());
    }
    if proposal.requested_leverage < Decimal::ONE {
        reasons.push("Requested leverage must be >= 1.0x".into());
    }

    let eq = match state.account_equity_usd {
        Some(v) if v > Decimal::ZERO => Some(v),
        Some(_) => {
            reasons.push("Account equity is non-positive".into());
            None
        }
        None => {
            missing.push("account_equity_usd".into());
            None
        }
    };
    let avail = match state.available_collateral_usd {
        Some(v) if v >= Decimal::ZERO => Some(v),
        _ => {
            missing.push("available_collateral_usd".into());
            None
        }
    };
    let daily_pnl = match state.daily_pnl_usd {
        Some(v) => Some(v),
        None => {
            missing.push("daily_pnl_usd".into());
            None
        }
    };
    let weekly_pnl = match state.weekly_pnl_usd {
        Some(v) => Some(v),
        None => {
            missing.push("weekly_pnl_usd".into());
            None
        }
    };
    let hwm = match state.high_water_equity_usd {
        Some(v) if v > Decimal::ZERO => Some(v),
        _ => {
            missing.push("high_water_equity_usd".into());
            None
        }
    };
    let liq_dist = match proposal.estimated_liquidation_distance_pct {
        Some(v) if v > Decimal::ZERO => Some(v),
        Some(_) => {
            reasons.push("Estimated liquidation distance must be positive".into());
            None
        }
        None => {
            missing.push("estimated_liquidation_distance_pct".into());
            None
        }
    };
    let depth = match proposal.market_depth_usd {
        Some(v) if v > Decimal::ZERO => Some(v),
        Some(_) => {
            reasons.push("Market liquidity depth must be positive".into());
            None
        }
        None => {
            missing.push("market_depth_usd".into());
            None
        }
    };
    let inst_max_lev = match proposal.instrument_max_leverage {
        Some(v) if v >= Decimal::ONE => Some(v),
        _ => {
            missing.push("instrument_max_leverage".into());
            None
        }
    };

    if !reasons.is_empty() || !missing.is_empty() {
        return SovereignRiskDecision::Rejected {
            reasons,
            missing_inputs: missing,
            breaker_triggered,
        };
    }

    let eq = eq.unwrap();
    let avail = avail.unwrap();
    let daily_pnl = daily_pnl.unwrap();
    let weekly_pnl = weekly_pnl.unwrap();
    let hwm = hwm.unwrap();
    let liq_dist = liq_dist.unwrap();
    let depth = depth.unwrap();
    let inst_max_lev = inst_max_lev.unwrap();

    // Daily & weekly loss & drawdown breakers
    if daily_pnl < Decimal::ZERO && (-daily_pnl / eq) >= policy.max_daily_loss_ratio {
        reasons.push(format!(
            "Daily loss {} reaches ceiling ratio {}",
            -daily_pnl, policy.max_daily_loss_ratio
        ));
        breaker_triggered = true;
    }
    if weekly_pnl < Decimal::ZERO && (-weekly_pnl / eq) >= policy.max_weekly_loss_ratio {
        reasons.push(format!(
            "Weekly loss {} reaches ceiling ratio {}",
            -weekly_pnl, policy.max_weekly_loss_ratio
        ));
        breaker_triggered = true;
    }
    if eq < hwm && ((hwm - eq) / hwm) >= policy.max_drawdown_ratio {
        reasons.push("Portfolio drawdown reached maximum drawdown ceiling".into());
        breaker_triggered = true;
    }

    // Instrument leverage & collateral
    if proposal.requested_leverage > inst_max_lev {
        reasons.push(format!(
            "Requested leverage {} exceeds instrument max {}",
            proposal.requested_leverage, inst_max_lev
        ));
    }
    let margin_required = (proposal.requested_notional_usd / proposal.requested_leverage)
        .round_dp(6)
        + proposal.estimated_fee_usd;
    if margin_required > avail {
        reasons.push(format!(
            "Required initial margin + fee ({margin_required}) exceeds available collateral ({avail})"
        ));
    }

    // Liquidation buffer & liquidity & volatility & funding drag
    if liq_dist < policy.min_liquidation_distance_pct {
        reasons.push(format!(
            "Liquidation buffer {liq_dist}% below policy floor {}%",
            policy.min_liquidation_distance_pct
        ));
    }
    if (proposal.requested_notional_usd / depth) > policy.max_liquidity_participation_ratio {
        reasons.push("Requested notional exceeds liquidity participation limit".into());
    }
    if let Some(vol) = proposal.market_volatility_pct {
        if vol > policy.max_volatility_pct {
            reasons.push(format!(
                "Market volatility {vol}% exceeds ceiling {}%",
                policy.max_volatility_pct
            ));
        }
    }
    if let Some(fund) = proposal.funding_bps_1h {
        let adverse_fund = match proposal.side {
            Side::Long => fund,
            Side::Short => -fund,
        };
        if adverse_fund > policy.max_funding_drag_bps_1h {
            reasons.push(format!(
                "Adverse hourly funding drag {adverse_fund} bps exceeds ceiling {} bps",
                policy.max_funding_drag_bps_1h
            ));
        }
    }

    // Post-fee exposure, per-market, per-strategy, concentration, and correlated cluster checks
    let post_eq = eq - proposal.estimated_fee_usd;
    if post_eq <= Decimal::ZERO {
        reasons.push("Post-fee equity non-positive".into());
        return SovereignRiskDecision::Rejected {
            reasons,
            missing_inputs: missing,
            breaker_triggered,
        };
    }

    let sym = proposal.symbol.trim().to_ascii_uppercase();
    let existing_total: Decimal = state.open_positions.iter().map(|p| p.notional_usd).sum();
    let post_total = existing_total + state.reserved_notional_usd + proposal.requested_notional_usd;
    let post_ratio = (post_total / post_eq).round_dp(6);
    if post_ratio > policy.max_total_exposure_ratio {
        reasons.push(format!(
            "Post-fill exposure ratio {post_ratio} exceeds ceiling {}",
            policy.max_total_exposure_ratio
        ));
    }

    let existing_sym: Decimal = state
        .open_positions
        .iter()
        .filter(|p| p.symbol.eq_ignore_ascii_case(&sym))
        .map(|p| p.notional_usd)
        .sum();
    let post_sym = existing_sym + proposal.requested_notional_usd;
    if post_sym > policy.max_market_notional_usd {
        reasons.push(format!(
            "Market notional {post_sym} exceeds cap {}",
            policy.max_market_notional_usd
        ));
    }
    if (post_sym / post_eq) > policy.max_single_asset_concentration {
        reasons.push("Single-asset concentration exceeds policy limit".into());
    }

    let existing_strat: Decimal = state
        .open_positions
        .iter()
        .filter(|p| p.owner_strategy == proposal.owner_strategy)
        .map(|p| p.notional_usd)
        .sum();
    if ((existing_strat + proposal.requested_notional_usd) / post_eq)
        > policy.max_strategy_allocation_ratio
    {
        reasons.push(format!(
            "Strategy '{}' allocation exceeds ceiling ratio {}",
            proposal.owner_strategy, policy.max_strategy_allocation_ratio
        ));
    }

    // Correlated exposure cluster: sum notionals of existing positions correlated >= threshold with `sym`
    let mut cluster_notional = post_sym;
    for pos in &state.open_positions {
        let ps = pos.symbol.trim().to_ascii_uppercase();
        if ps == sym {
            continue;
        }
        let key_a = (sym.clone(), ps.clone());
        let key_b = (ps, sym.clone());
        if let Some(&rho) = state
            .pairwise_correlations
            .get(&key_a)
            .or_else(|| state.pairwise_correlations.get(&key_b))
        {
            if rho.abs() >= policy.correlation_threshold {
                cluster_notional += pos.notional_usd;
            }
        }
    }
    if (cluster_notional / post_eq) > policy.max_correlated_cluster_ratio {
        reasons.push(format!(
            "Correlated exposure cluster ({cluster_notional} / {post_eq}) exceeds {}",
            policy.max_correlated_cluster_ratio
        ));
    }

    if reasons.is_empty() {
        SovereignRiskDecision::Approved {
            post_fill_equity_usd: post_eq.round_dp(6),
            post_fill_exposure_ratio: post_ratio,
            remaining_collateral_usd: (avail - margin_required).round_dp(6),
        }
    } else {
        SovereignRiskDecision::Rejected {
            reasons,
            missing_inputs: missing,
            breaker_triggered,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_state() -> SovereignPortfolioState {
        SovereignPortfolioState {
            account_equity_usd: Some(Decimal::from(10_000u32)),
            available_collateral_usd: Some(Decimal::from(8_000u32)),
            reserved_notional_usd: Decimal::ZERO,
            open_positions: vec![SovereignPositionInput {
                symbol: "ETH".into(),
                owner_strategy: "mom".into(),
                side: Side::Long,
                notional_usd: Decimal::from(2_500u32),
                liquidation_distance_pct: Some(Decimal::from(30u32)),
                funding_bps_1h: Some(Decimal::ONE),
            }],
            daily_pnl_usd: Some(Decimal::ZERO),
            weekly_pnl_usd: Some(Decimal::ZERO),
            high_water_equity_usd: Some(Decimal::from(10_000u32)),
            pairwise_correlations: BTreeMap::from([(("BTC".into(), "ETH".into()), 0.85)]),
            emergency_suspended: false,
            breaker_reason: None,
        }
    }

    fn base_proposal() -> SovereignAdmissionProposal {
        SovereignAdmissionProposal {
            symbol: "BTC".into(),
            owner_strategy: "adaptive_ema_anchor".into(),
            side: Side::Long,
            requested_notional_usd: Decimal::from(1_200u32),
            estimated_fee_usd: Decimal::from(2u32),
            estimated_liquidation_distance_pct: Some(Decimal::from(25u32)),
            market_depth_usd: Some(Decimal::from(100_000u32)),
            market_volatility_pct: Some(Decimal::from(3u32)),
            funding_bps_1h: Some(Decimal::ONE),
            instrument_max_leverage: Some(Decimal::from(20u32)),
            requested_leverage: Decimal::from(5u32),
            rearm_authorization: None,
        }
    }

    #[test]
    fn approves_valid_proposal_and_fails_closed_on_missing_inputs_correlation_daily_loss_or_liq_distance(
    ) {
        let pol = SovereignRiskPolicy::default();
        let st = base_state();
        let prop = base_proposal();

        // Clean valid proposal within cluster (2500 + 1200 = 3700 / 9998 <= 0.40) -> Approved
        assert!(matches!(
            evaluate_sovereign_admission(&pol, &st, &prop),
            SovereignRiskDecision::Approved { .. }
        ));

        // Correlated cluster violation (requesting 2000 BTC + 2500 ETH = 4500 > 0.40 * 10000)
        let mut big_prop = prop.clone();
        big_prop.requested_notional_usd = Decimal::from(2_000u32);
        assert!(matches!(
            evaluate_sovereign_admission(&pol, &st, &big_prop),
            SovereignRiskDecision::Rejected { .. }
        ));

        // Missing liquidation distance fails closed cleanly
        let mut no_liq = prop.clone();
        no_liq.estimated_liquidation_distance_pct = None;
        match evaluate_sovereign_admission(&pol, &st, &no_liq) {
            SovereignRiskDecision::Rejected { missing_inputs, .. } => {
                assert!(missing_inputs.contains(&"estimated_liquidation_distance_pct".to_string()));
            }
            _ => panic!("expected missing input rejection"),
        }

        // Daily loss ceiling breaches & triggers breaker
        let mut loss_st = st;
        loss_st.daily_pnl_usd = Some(Decimal::from(-350)); // -3.5% > 3%
        match evaluate_sovereign_admission(&pol, &loss_st, &prop) {
            SovereignRiskDecision::Rejected {
                breaker_triggered, ..
            } => {
                assert!(breaker_triggered);
            }
            _ => panic!("expected daily loss breaker rejection"),
        }
    }
}
