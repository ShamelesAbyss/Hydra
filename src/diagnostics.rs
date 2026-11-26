//! Typed, offline diagnostics shared by CLI and native research workspaces.
use crate::{
    cognition_shadow,
    core::{Candle, Side},
    fang_planner, portfolio_research, research,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum DiagnosticKind {
    Fang,
    EmaAnchor,
    Cognition,
    Aegis,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanningRequest {
    pub candidates: Vec<fang_planner::PlanCandidate>,
    pub budget: fang_planner::PlanningBudget,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnchorRequest {
    pub candles: Vec<Candle>,
    pub side: Side,
    pub options: fang_planner::EmaAnchorOptions,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "report", rename_all = "snake_case")]
pub enum DiagnosticReport {
    Fang(fang_planner::PlanningReport),
    EmaAnchor(fang_planner::EmaAnchorReport),
    Cognition(cognition_shadow::ShadowReport),
    Aegis(portfolio_research::PortfolioReport),
}
pub fn evaluate(kind: DiagnosticKind, path: &Path) -> Result<DiagnosticReport> {
    Ok(match kind {
        DiagnosticKind::Fang => {
            let r: PlanningRequest = research::read_json(path)?;
            DiagnosticReport::Fang(fang_planner::plan(&r.candidates, &r.budget)?)
        }
        DiagnosticKind::EmaAnchor => {
            let r: AnchorRequest = research::read_json(path)?;
            DiagnosticReport::EmaAnchor(fang_planner::ema_anchor(&r.candles, r.side, &r.options)?)
        }
        DiagnosticKind::Cognition => {
            DiagnosticReport::Cognition(cognition_shadow::evaluate(&research::read_json(path)?)?)
        }
        DiagnosticKind::Aegis => {
            DiagnosticReport::Aegis(portfolio_research::evaluate(&research::read_json(path)?)?)
        }
    })
}
/// Empty templates carry no fictional market or account evidence.
pub fn template(kind: DiagnosticKind) -> Result<String> {
    Ok(match kind {
        DiagnosticKind::Fang => serde_json::to_string_pretty(&PlanningRequest {
            candidates: vec![],
            budget: fang_planner::PlanningBudget {
                approved_symbols: Default::default(),
                excluded_symbols: Default::default(),
                total_remaining_notional: rust_decimal::Decimal::ZERO,
                per_symbol_remaining_notional: Default::default(),
                occupied_symbols: Default::default(),
                confidence_floor: 0.5,
            },
        })?,
        DiagnosticKind::EmaAnchor => serde_json::to_string_pretty(&AnchorRequest {
            candles: vec![],
            side: Side::Long,
            options: Default::default(),
        })?,
        DiagnosticKind::Cognition => {
            serde_json::to_string_pretty(&cognition_shadow::ShadowRequest {
                as_of_ms: 0,
                policy: Default::default(),
                samples: vec![],
            })?
        }
        DiagnosticKind::Aegis => {
            serde_json::to_string_pretty(&portfolio_research::PortfolioRequest {
                as_of_ms: 0,
                approved: Default::default(),
                account: portfolio_research::AccountSnapshot {
                    account_id: "operator-supplied".into(),
                    observed_at_ms: 0,
                    equity_usd: None,
                    cash_usd: None,
                    unrealized_pnl_usd: None,
                    other_equity_adjustments_usd: None,
                    exposures_complete: false,
                },
                budgets: Default::default(),
                positions: vec![],
                returns: vec![],
            })?
        }
    })
}
