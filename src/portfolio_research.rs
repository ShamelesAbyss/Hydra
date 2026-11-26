//! Caller-supplied offline AEGIS diagnostics, isolated from live risk authority.
use anyhow::{anyhow, ensure, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountSnapshot {
    pub account_id: String,
    pub observed_at_ms: i64,
    pub equity_usd: Option<Decimal>,
    pub cash_usd: Option<Decimal>,
    pub unrealized_pnl_usd: Option<Decimal>,
    pub other_equity_adjustments_usd: Option<Decimal>,
    /// An unverified caller assertion; false means portfolio totals are unknown.
    pub exposures_complete: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionExposure {
    pub symbol: String,
    pub observed_at_ms: i64,
    pub long_notional_usd: Option<Decimal>,
    pub short_notional_usd: Option<Decimal>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnObservation {
    pub at_ms: i64,
    pub return_fraction: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnSeries {
    pub symbol: String,
    pub observations: Vec<ReturnObservation>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortfolioBudgets {
    pub max_snapshot_age_ms: i64,
    pub max_gross_leverage: Decimal,
    pub max_abs_net_leverage: Decimal,
    pub max_long_leverage: Decimal,
    pub max_short_leverage: Decimal,
    pub max_asset_equity_fraction: Decimal,
    pub max_asset_gross_fraction: Decimal,
    pub min_cash_equity_fraction: Decimal,
    pub return_window_ms: i64,
    pub min_correlation_samples: usize,
    pub min_return_variance: f64,
}
impl Default for PortfolioBudgets {
    fn default() -> Self {
        Self {
            max_snapshot_age_ms: 60_000,
            max_gross_leverage: Decimal::ONE,
            max_abs_net_leverage: Decimal::ONE,
            max_long_leverage: Decimal::ONE,
            max_short_leverage: Decimal::ONE,
            max_asset_equity_fraction: Decimal::new(25, 2),
            max_asset_gross_fraction: Decimal::new(50, 2),
            min_cash_equity_fraction: Decimal::ZERO,
            return_window_ms: 30 * 86_400_000,
            min_correlation_samples: 30,
            min_return_variance: 1e-16,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortfolioRequest {
    pub as_of_ms: i64,
    pub approved: BTreeSet<String>,
    pub account: AccountSnapshot,
    pub budgets: PortfolioBudgets,
    pub positions: Vec<PositionExposure>,
    pub returns: Vec<ReturnSeries>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExposureTotals {
    pub long_usd: Decimal,
    pub short_usd: Decimal,
    pub gross_usd: Decimal,
    pub net_usd: Decimal,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetRisk {
    pub symbol: String,
    pub approved: bool,
    pub fresh: bool,
    pub long_notional_usd: Option<Decimal>,
    pub short_notional_usd: Option<Decimal>,
    pub gross_usd: Option<Decimal>,
    pub net_usd: Option<Decimal>,
    pub equity_fraction: Option<Decimal>,
    pub gross_fraction: Option<Decimal>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairCorrelation {
    pub first: String,
    pub second: String,
    pub aligned_samples: usize,
    pub pearson: Option<f64>,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortfolioReport {
    pub research_only: bool,
    pub risk_release_authorized: bool,
    pub execution_authorized: bool,
    pub caller_data_verified: bool,
    pub account_id: String,
    pub as_of_ms: i64,
    pub return_window_start_ms: i64,
    pub budgets: PortfolioBudgets,
    pub account_snapshot: AccountSnapshot,
    pub equity_identity_matches: Option<bool>,
    /// None if any exposure is unknown, stale, or the caller reports incompleteness.
    pub totals: Option<ExposureTotals>,
    pub gross_leverage: Option<Decimal>,
    pub net_leverage: Option<Decimal>,
    pub long_leverage: Option<Decimal>,
    pub short_leverage: Option<Decimal>,
    pub cash_equity_fraction: Option<Decimal>,
    /// This is only a calculation against unverified supplied budgets, never live compliance.
    pub within_supplied_budgets: Option<bool>,
    pub assets: Vec<AssetRisk>,
    pub correlations: Vec<PairCorrelation>,
    pub breaches: Vec<String>,
    pub unknowns: Vec<String>,
    pub limitations: Vec<String>,
}
fn add(a: Decimal, b: Decimal) -> Result<Decimal> {
    let result = a
        .checked_add(b)
        .ok_or_else(|| anyhow!("Decimal exposure/identity sum overflow"))?;
    ensure!(
        result.checked_sub(a) == Some(b) && result.checked_sub(b) == Some(a),
        "Decimal exposure/identity sum loses precision"
    );
    Ok(result)
}
fn sub(a: Decimal, b: Decimal) -> Result<Decimal> {
    let result = a
        .checked_sub(b)
        .ok_or_else(|| anyhow!("Decimal exposure/identity subtraction overflow"))?;
    ensure!(
        a.checked_sub(result) == Some(b) && result.checked_add(b) == Some(a),
        "Decimal exposure/identity subtraction loses precision"
    );
    Ok(result)
}
fn divide(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_div(b)
        .ok_or_else(|| anyhow!("Decimal ratio overflow or invalid denominator"))
}
fn product(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_mul(b)
        .ok_or_else(|| anyhow!("Decimal budget multiplication overflow"))
}
fn label_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
}
fn timestamp(at: i64, as_of: i64) -> Result<()> {
    ensure!(at >= 0 && at <= as_of, "negative or future timestamp");
    Ok(())
}
fn correlation(
    first: &str,
    second: &str,
    series: &BTreeMap<&str, &ReturnSeries>,
    cutoff: i64,
    b: &PortfolioBudgets,
) -> PairCorrelation {
    let mut out = PairCorrelation {
        first: first.into(),
        second: second.into(),
        aligned_samples: 0,
        pearson: None,
        unavailable_reason: None,
    };
    let (Some(a), Some(c)) = (series.get(first), series.get(second)) else {
        out.unavailable_reason = Some("return series missing".into());
        return out;
    };
    let mut pairs = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.observations.len() && j < c.observations.len() {
        let (x, y) = (&a.observations[i], &c.observations[j]);
        match x.at_ms.cmp(&y.at_ms) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                if x.at_ms >= cutoff {
                    pairs.push((x.return_fraction, y.return_fraction));
                }
                i += 1;
                j += 1;
            }
        }
    }
    out.aligned_samples = pairs.len();
    if pairs.len() < b.min_correlation_samples {
        out.unavailable_reason =
            Some("insufficient exactly aligned historical observations".into());
        return out;
    }
    let n = pairs.len() as f64;
    let mx = pairs.iter().map(|p| p.0).sum::<f64>() / n;
    let my = pairs.iter().map(|p| p.1).sum::<f64>() / n;
    let xx = pairs.iter().map(|p| (p.0 - mx).powi(2)).sum::<f64>();
    let yy = pairs.iter().map(|p| (p.1 - my).powi(2)).sum::<f64>();
    if xx / (n - 1.0) <= b.min_return_variance || yy / (n - 1.0) <= b.min_return_variance {
        out.unavailable_reason = Some("zero or insufficient return variance".into());
        return out;
    }
    let xy = pairs.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>();
    let rho = xy / xx.sqrt() / yy.sqrt();
    if rho.is_finite() {
        out.pearson = Some(rho.clamp(-1.0, 1.0));
    } else {
        out.unavailable_reason = Some("correlation numerically unavailable".into());
    }
    out
}

/// Analyze only explicit caller evidence. Decimal exposure sums precede ratios;
/// no market acquisition, liquidation, allocation, order, or risk-hold mutation.
pub fn evaluate(request: &PortfolioRequest) -> Result<PortfolioReport> {
    let b = &request.budgets;
    let account = &request.account;
    ensure!(request.as_of_ms >= 0, "as_of_ms must be nonnegative");
    let universe: BTreeSet<_> = request
        .approved
        .iter()
        .map(String::as_str)
        .chain(request.positions.iter().map(|p| p.symbol.as_str()))
        .chain(request.returns.iter().map(|s| s.symbol.as_str()))
        .collect();
    ensure!(universe.len() <= 32, "combined asset universe exceeds 32");
    ensure!(
        request.positions.len() <= 32
            && request.approved.len() <= 32
            && request.returns.len() <= 32,
        "asset budget exceeds 32"
    );
    ensure!(
        request
            .returns
            .iter()
            .map(|s| s.observations.len())
            .try_fold(0usize, usize::checked_add)
            .is_some_and(|n| n <= 2048),
        "return observation budget exceeds 2048"
    );
    ensure!(
        label_ok(&account.account_id) && request.approved.iter().all(|s| label_ok(s)),
        "invalid account/approved identifier"
    );
    ensure!(
        b.max_snapshot_age_ms > 0 && b.return_window_ms > 0,
        "positive snapshot age and return window required"
    );
    ensure!(
        [
            b.max_gross_leverage,
            b.max_abs_net_leverage,
            b.max_long_leverage,
            b.max_short_leverage,
            b.max_asset_equity_fraction,
            b.max_asset_gross_fraction,
            b.min_cash_equity_fraction
        ]
        .iter()
        .all(|d| *d >= Decimal::ZERO),
        "negative budget"
    );
    ensure!(
        b.max_asset_gross_fraction <= Decimal::ONE,
        "gross concentration fraction must be <=1"
    );
    ensure!(
        (3..=2048).contains(&b.min_correlation_samples)
            && b.min_return_variance.is_finite()
            && b.min_return_variance >= 0.0,
        "invalid correlation sample/variance threshold"
    );
    timestamp(account.observed_at_ms, request.as_of_ms)?;
    let account_fresh = request.as_of_ms - account.observed_at_ms <= b.max_snapshot_age_ms;
    let mut breaches = Vec::new();
    let mut unknowns = Vec::new();
    let identity = match (
        account.equity_usd,
        account.cash_usd,
        account.unrealized_pnl_usd,
        account.other_equity_adjustments_usd,
    ) {
        (Some(e), Some(c), Some(u), Some(o)) => {
            let matches = add(add(c, u)?, o)? == e;
            if !matches {
                breaches.push(
                    "equity identity differs from cash + unrealized PnL + other adjustments".into(),
                );
            }
            Some(matches)
        }
        _ => {
            unknowns.push("equity/cash identity components unavailable".into());
            None
        }
    };
    if !account_fresh {
        unknowns.push("account snapshot stale".into());
    }
    if !account.exposures_complete {
        unknowns.push("caller reports incomplete portfolio exposures".into());
    }
    if account.equity_usd.is_some_and(|e| e <= Decimal::ZERO) {
        breaches.push("equity is nonpositive; leverage denominator unavailable".into());
    }
    let equity = account
        .equity_usd
        .filter(|e| *e > Decimal::ZERO && account_fresh && identity == Some(true));
    if equity.is_none() {
        unknowns.push("positive fresh identity-consistent equity unavailable".into());
    }
    let mut seen = BTreeSet::new();
    let mut assets = Vec::new();
    let mut long = Decimal::ZERO;
    let mut short = Decimal::ZERO;
    let mut exposure_known = account.exposures_complete;
    for p in &request.positions {
        ensure!(
            label_ok(&p.symbol) && seen.insert(p.symbol.clone()),
            "invalid or duplicate position asset"
        );
        timestamp(p.observed_at_ms, request.as_of_ms)?;
        ensure!(
            [p.long_notional_usd, p.short_notional_usd]
                .iter()
                .flatten()
                .all(|v| *v >= Decimal::ZERO),
            "negative directional notional"
        );
        let approved = request.approved.contains(&p.symbol);
        if !approved {
            breaches.push(format!("{} is not approved", p.symbol));
        }
        let fresh = request.as_of_ms - p.observed_at_ms <= b.max_snapshot_age_ms;
        if !fresh {
            exposure_known = false;
            unknowns.push(format!("{} exposure stale", p.symbol));
        }
        let (gross, net) = match (p.long_notional_usd, p.short_notional_usd) {
            (Some(l), Some(s)) => {
                long = add(long, l)?;
                short = add(short, s)?;
                (Some(add(l, s)?), Some(sub(l, s)?))
            }
            _ => {
                exposure_known = false;
                unknowns.push(format!("{} directional exposure unknown", p.symbol));
                (None, None)
            }
        };
        assets.push(AssetRisk {
            symbol: p.symbol.clone(),
            approved,
            fresh,
            long_notional_usd: p.long_notional_usd,
            short_notional_usd: p.short_notional_usd,
            gross_usd: gross,
            net_usd: net,
            equity_fraction: None,
            gross_fraction: None,
        });
    }
    assets.sort_by(|a, c| a.symbol.cmp(&c.symbol));
    let totals = if exposure_known {
        Some(ExposureTotals {
            long_usd: long,
            short_usd: short,
            gross_usd: add(long, short)?,
            net_usd: sub(long, short)?,
        })
    } else {
        None
    };
    let (
        mut gross_leverage,
        mut net_leverage,
        mut long_leverage,
        mut short_leverage,
        mut cash_fraction,
    ) = (None, None, None, None, None);
    if let Some(e) = equity {
        cash_fraction = account.cash_usd.map(|c| divide(c, e)).transpose()?;
        if account.cash_usd.is_some_and(|c| c < Decimal::ZERO) {
            breaches.push("cash balance is negative".into());
        }
        if let Some(c) = account.cash_usd {
            if c < product(e, b.min_cash_equity_fraction)? {
                breaches.push("cash/equity fraction below supplied minimum".into());
            }
        }
        if let Some(t) = &totals {
            gross_leverage = Some(divide(t.gross_usd, e)?);
            net_leverage = Some(divide(t.net_usd, e)?);
            long_leverage = Some(divide(t.long_usd, e)?);
            short_leverage = Some(divide(t.short_usd, e)?);
            for (name, value, limit) in [
                ("gross leverage", t.gross_usd, b.max_gross_leverage),
                (
                    "absolute net leverage",
                    t.net_usd.abs(),
                    b.max_abs_net_leverage,
                ),
                ("long leverage", t.long_usd, b.max_long_leverage),
                ("short leverage", t.short_usd, b.max_short_leverage),
            ] {
                if value > product(e, limit)? {
                    breaches.push(format!("{name} exceeds supplied budget"));
                }
            }
        }
        for a in &mut assets {
            if a.fresh {
                if let Some(g) = a.gross_usd {
                    a.equity_fraction = Some(divide(g, e)?);
                    if g > product(e, b.max_asset_equity_fraction)? {
                        breaches.push(format!(
                            "{} equity concentration exceeds supplied budget",
                            a.symbol
                        ));
                    }
                }
            }
        }
    }
    if let Some(t) = &totals {
        if t.gross_usd > Decimal::ZERO {
            for a in &mut assets {
                if let Some(g) = a.gross_usd {
                    a.gross_fraction = Some(divide(g, t.gross_usd)?);
                    if g > product(t.gross_usd, b.max_asset_gross_fraction)? {
                        breaches.push(format!(
                            "{} gross concentration exceeds supplied budget",
                            a.symbol
                        ));
                    }
                }
            }
        }
    }
    let mut series = BTreeMap::new();
    for s in &request.returns {
        ensure!(
            label_ok(&s.symbol) && series.insert(s.symbol.as_str(), s).is_none(),
            "invalid or duplicate return series"
        );
        ensure!(
            request.approved.contains(&s.symbol),
            "return series asset not approved"
        );
        let mut previous = None;
        for point in &s.observations {
            timestamp(point.at_ms, request.as_of_ms)?;
            ensure!(
                previous.is_none_or(|at| point.at_ms > at),
                "return observations must be strictly chronological and unique"
            );
            ensure!(
                point.return_fraction.is_finite() && point.return_fraction.abs() <= 1e6,
                "return must be finite and within +/-1000000"
            );
            previous = Some(point.at_ms);
        }
    }
    let return_cutoff = request.as_of_ms.saturating_sub(b.return_window_ms);
    let mut correlations = Vec::new();
    for i in 0..assets.len() {
        for j in i + 1..assets.len() {
            correlations.push(correlation(
                &assets[i].symbol,
                &assets[j].symbol,
                &series,
                return_cutoff,
                b,
            ));
        }
    }
    let within = if unknowns.is_empty() {
        Some(breaches.is_empty())
    } else {
        None
    };
    Ok(PortfolioReport {research_only:true,risk_release_authorized:false,execution_authorized:false,caller_data_verified:false,account_id:account.account_id.clone(),as_of_ms:request.as_of_ms,return_window_start_ms:return_cutoff,budgets:b.clone(),account_snapshot:account.clone(),equity_identity_matches:identity,totals,gross_leverage,net_leverage,long_leverage,short_leverage,cash_equity_fraction:cash_fraction,within_supplied_budgets:within,assets,correlations,breaches,unknowns,limitations:vec![
        "Account identity, valuations, equity/cash components, exposure completeness, approved assets and returns are caller-supplied and unverified. This is not authoritative account reconciliation or live-risk compliance.".into(),
        "Equity identity is cash + unrealized PnL + explicit other adjustments; position notionals are not treated as cash assets. Fees, liabilities, collateral haircuts and funding must be included by the caller.".into(),
        "Decimal sums are checked before ratios; ratios and budget products use Decimal precision and may round at representational limits. No floating-point notional aggregation is used.".into(),
        "Correlations use exactly matching timestamps within the inclusive historical window ending at as_of_ms, with no forward-fill. Caller-return interval consistency and survivorship/completeness are not independently verified.".into(),
        "Missing or constant/low-variance returns produce unknown correlations, never zero substitutes. Pearson correlation is historical, does not imply causality, and omits tail dependence and stress scenarios.".into(),
        "Within-supplied-budgets is only a descriptive calculation and excludes unavailable correlation/stress limits; it never releases a hold, overrides AEGIS, authorizes orders, or allocates capital.".into(),
    ]})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }
    fn request() -> PortfolioRequest {
        PortfolioRequest {
            as_of_ms: 100,
            approved: ["AAA".into(), "BBB".into()].into(),
            account: AccountSnapshot {
                account_id: "offline".into(),
                observed_at_ms: 99,
                equity_usd: Some(d("100")),
                cash_usd: Some(d("100")),
                unrealized_pnl_usd: Some(Decimal::ZERO),
                other_equity_adjustments_usd: Some(Decimal::ZERO),
                exposures_complete: true,
            },
            budgets: PortfolioBudgets {
                max_asset_equity_fraction: Decimal::ONE,
                max_asset_gross_fraction: Decimal::ONE,
                min_correlation_samples: 3,
                ..Default::default()
            },
            positions: vec![PositionExposure {
                symbol: "AAA".into(),
                observed_at_ms: 99,
                long_notional_usd: Some(d("60")),
                short_notional_usd: Some(d("40")),
            }],
            returns: vec![],
        }
    }
    fn series(symbol: &str, values: &[f64]) -> ReturnSeries {
        ReturnSeries {
            symbol: symbol.into(),
            observations: values
                .iter()
                .enumerate()
                .map(|(i, v)| ReturnObservation {
                    at_ms: 10 + i as i64,
                    return_fraction: *v,
                })
                .collect(),
        }
    }
    fn with_pair() -> PortfolioRequest {
        let mut r = request();
        let mut p = r.positions[0].clone();
        p.symbol = "BBB".into();
        p.long_notional_usd = Some(Decimal::ZERO);
        p.short_notional_usd = Some(Decimal::ZERO);
        r.positions.push(p);
        r
    }
    #[test]
    fn exact_decimal_sums_budget_equality_and_hedge_gross() {
        let mut r = request();
        let out = evaluate(&r).unwrap();
        let t = out.totals.unwrap();
        assert_eq!(t.gross_usd, d("100"));
        assert_eq!(t.net_usd, d("20"));
        assert_eq!(out.gross_leverage, Some(Decimal::ONE));
        assert_eq!(out.within_supplied_budgets, Some(true));
        r.positions[0].long_notional_usd = Some(d("60.00000000000000000000000001"));
        let out = evaluate(&r).unwrap();
        assert_eq!(out.within_supplied_budgets, Some(false));
        assert!(out.breaches.iter().any(|s| s.contains("gross leverage")));
        r.positions[0].long_notional_usd = Some(d("0.1"));
        r.positions[0].short_notional_usd = Some(d("0.2"));
        assert_eq!(evaluate(&r).unwrap().totals.unwrap().gross_usd, d("0.3"));
    }
    #[test]
    fn precision_loss_is_rejected_instead_of_rounding_exposure() {
        assert!(add(Decimal::MAX, d("0.1")).is_err());
        assert!(sub(Decimal::MAX, d("0.1")).is_err());
        assert_eq!(add(d("0.1"), d("0.2")).unwrap(), d("0.3"));
    }
    #[test]
    fn unknown_identity_equity_and_exposure_never_compliant() {
        let mut r = request();
        r.account.equity_usd = None;
        let out = evaluate(&r).unwrap();
        assert_eq!(out.within_supplied_budgets, None);
        assert_eq!(out.gross_leverage, None);
        r = request();
        r.positions[0].short_notional_usd = None;
        assert!(evaluate(&r).unwrap().totals.is_none());
        r = request();
        r.account.exposures_complete = false;
        assert_eq!(evaluate(&r).unwrap().within_supplied_budgets, None);
        r = request();
        r.account.cash_usd = Some(d("99"));
        let out = evaluate(&r).unwrap();
        assert_eq!(out.equity_identity_matches, Some(false));
        assert_ne!(out.within_supplied_budgets, Some(true));
    }
    #[test]
    fn rejects_duplicates_overflow_bad_chronology_and_future() {
        let mut r = request();
        r.positions.push(r.positions[0].clone());
        assert!(evaluate(&r).is_err());
        r = request();
        r.positions[0].long_notional_usd = Some(Decimal::MAX);
        r.positions[0].short_notional_usd = Some(Decimal::MAX);
        assert!(evaluate(&r).is_err());
        r = request();
        r.positions[0].observed_at_ms = 101;
        assert!(evaluate(&r).is_err());
        r = request();
        r.returns = vec![series("AAA", &[0.1, 0.2, 0.3])];
        r.returns[0].observations[1].at_ms = 10;
        assert!(evaluate(&r).is_err());
        r.returns[0].observations[1].at_ms = 101;
        assert!(evaluate(&r).is_err());
    }
    #[test]
    fn positive_negative_and_zero_variance_correlations() {
        let mut r = with_pair();
        r.returns = vec![
            series("AAA", &[-0.1, 0.0, 0.1]),
            series("BBB", &[-0.2, 0.0, 0.2]),
        ];
        assert!((evaluate(&r).unwrap().correlations[0].pearson.unwrap() - 1.0).abs() < 1e-12);
        r.returns[1] = series("BBB", &[0.2, 0.0, -0.2]);
        assert!((evaluate(&r).unwrap().correlations[0].pearson.unwrap() + 1.0).abs() < 1e-12);
        r.returns[1] = series("BBB", &[0.1, 0.1, 0.1]);
        assert_eq!(evaluate(&r).unwrap().correlations[0].pearson, None);
    }
    #[test]
    fn missing_unaligned_and_expired_returns_remain_unknown() {
        let mut r = with_pair();
        assert_eq!(evaluate(&r).unwrap().correlations[0].pearson, None);
        r.returns = vec![
            series("AAA", &[0.1, 0.2, 0.3]),
            series("BBB", &[0.1, 0.2, 0.3]),
        ];
        for p in &mut r.returns[1].observations {
            p.at_ms += 5;
        }
        assert_eq!(evaluate(&r).unwrap().correlations[0].aligned_samples, 0);
        r.returns[1] = series("BBB", &[0.1, 0.2, 0.3]);
        r.budgets.return_window_ms = 10;
        assert_eq!(evaluate(&r).unwrap().correlations[0].aligned_samples, 0);
    }
    #[test]
    fn no_authority_and_deterministic_input_preservation() {
        let mut r = with_pair();
        let before = serde_json::to_value(&r).unwrap();
        let out = evaluate(&r).unwrap();
        assert!(out.research_only);
        assert!(
            !out.risk_release_authorized && !out.execution_authorized && !out.caller_data_verified
        );
        assert_eq!(before, serde_json::to_value(&r).unwrap());
        r.positions.reverse();
        assert_eq!(
            serde_json::to_value(out).unwrap(),
            serde_json::to_value(evaluate(&r).unwrap()).unwrap()
        );
    }
    #[test]
    fn stale_unknown_and_nonfinite_or_budget_overflow_fail_closed() {
        let mut r = request();
        r.budgets.max_snapshot_age_ms = 1;
        r.positions[0].observed_at_ms = 0;
        assert_eq!(evaluate(&r).unwrap().within_supplied_budgets, None);
        r = request();
        r.returns = vec![series("AAA", &[f64::NAN])];
        assert!(evaluate(&r).is_err());
        r = request();
        r.budgets.max_gross_leverage = Decimal::MAX;
        assert!(evaluate(&r).is_err());
        r = request();
        r.returns = vec![series("AAA", &vec![0.1; 2049])];
        assert!(evaluate(&r).is_err());
    }
}
