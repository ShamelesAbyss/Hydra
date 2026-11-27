//! Bounded offline HUNTER research. Scores are selection heuristics, never forecasts.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HunterRequest {
    pub as_of_ms: i64,
    pub source: String,
    pub approved: BTreeSet<String>,
    pub excluded: BTreeSet<String>,
    pub current_watchlist: Vec<String>,
    pub policy: HunterPolicy,
    pub markets: Vec<MarketObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketObservation {
    pub symbol: String,
    pub observed_at_ms: i64,
    pub volume_24h_usd: Option<f64>,
    pub depth_usd: Option<f64>,
    pub spread_bps: Option<f64>,
    pub volatility_pct: Option<f64>,
    pub directionality: Option<f64>,
    pub funding_bps: Option<f64>,
    pub open_interest_usd: Option<f64>,
    pub coverage_fraction: Option<f64>,
    pub age_days: Option<f64>,
    pub ema_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HunterPolicy {
    pub max_age_ms: i64,
    pub min_volume: f64,
    pub min_depth: f64,
    pub max_spread: f64,
    pub max_abs_funding: f64,
    pub min_coverage: f64,
    pub min_age: f64,
    pub slots: usize,
    pub switch_margin: f64,
    pub min_open_interest: f64,
}
impl Default for HunterPolicy {
    fn default() -> Self {
        Self {
            max_age_ms: 60_000,
            min_volume: 1_000_000.0,
            min_depth: 100_000.0,
            max_spread: 10.0,
            max_abs_funding: 10.0,
            min_coverage: 0.95,
            min_age: 30.0,
            slots: 5,
            switch_margin: 5.0,
            min_open_interest: 500_000.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketAssessment {
    pub observation: MarketObservation,
    pub score: Option<f64>,
    pub eligible: bool,
    pub reasons: Vec<String>,
    pub selected: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HunterReport {
    pub read_only: bool,
    pub source_is_live: bool,
    pub trading_authorized: bool,
    pub source: String,
    pub as_of_ms: i64,
    pub ranked: Vec<MarketAssessment>,
    pub watchlist: Vec<String>,
    pub limitations: Vec<String>,
}
fn safe_symbol(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
}

/// Score [0,100]: volume 20%, depth 25%, spread cost 20%, absolute funding
/// cost 10%, directionality 15%, OI 10%. Liquidity terms saturate at 10x
/// eligibility thresholds; volatility is a completeness check, not alpha.
/// Strictly greater hysteresis applies in score points. This only selects a
/// research watchlist and never liquidates positions or allocates capital.
pub fn evaluate(request: &HunterRequest) -> Result<HunterReport> {
    let p = &request.policy;
    ensure!(request.as_of_ms >= 0, "as_of_ms must be nonnegative");
    ensure!(
        !request.source.trim().is_empty() && request.source.len() <= 256,
        "source must contain 1..256 bytes"
    );
    ensure!(
        request.markets.len() <= 1024
            && request.approved.len() <= 1024
            && request.excluded.len() <= 1024
            && request.current_watchlist.len() <= 32,
        "input resource budget exceeded"
    );
    ensure!(
        (1..=32).contains(&p.slots) && p.max_age_ms > 0,
        "invalid slots or max_age_ms"
    );
    ensure!(
        [
            p.min_volume,
            p.min_depth,
            p.min_open_interest,
            p.max_spread,
            p.max_abs_funding,
            p.min_age
        ]
        .iter()
        .all(|v| v.is_finite() && *v > 0.0),
        "thresholds must be finite and positive"
    );
    ensure!(
        p.min_coverage.is_finite()
            && (0.0..=1.0).contains(&p.min_coverage)
            && p.switch_margin.is_finite()
            && (0.0..=100.0).contains(&p.switch_margin),
        "invalid coverage or switch margin"
    );
    let mut seen = BTreeSet::new();
    for m in &request.markets {
        ensure!(safe_symbol(&m.symbol), "invalid market symbol");
        ensure!(
            seen.insert(m.symbol.as_str()),
            "duplicate market symbol: {}",
            m.symbol
        );
    }
    ensure!(
        request
            .approved
            .iter()
            .chain(&request.excluded)
            .chain(&request.current_watchlist)
            .all(|s| safe_symbol(s)),
        "invalid policy/watchlist symbol"
    );
    ensure!(
        request
            .current_watchlist
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            == request.current_watchlist.len(),
        "duplicate watchlist symbol"
    );
    let mut ranked = Vec::with_capacity(request.markets.len());
    for m in &request.markets {
        let mut reasons = Vec::new();
        if !request.approved.contains(&m.symbol) {
            reasons.push("not approved".into());
        }
        if request.excluded.contains(&m.symbol) {
            reasons.push("explicitly excluded".into());
        }
        if m.observed_at_ms < 0 || m.observed_at_ms > request.as_of_ms {
            reasons.push("invalid or future observation".into());
        } else if request.as_of_ms - m.observed_at_ms > p.max_age_ms {
            reasons.push("stale observation".into());
        }
        let metrics = [
            ("volume_24h_usd", m.volume_24h_usd, p.min_volume, f64::MAX),
            ("depth_usd", m.depth_usd, p.min_depth, f64::MAX),
            ("spread_bps", m.spread_bps, 0.0, p.max_spread),
            ("volatility_pct", m.volatility_pct, 0.0, f64::MAX),
            ("directionality", m.directionality, 0.0, 1.0),
            (
                "funding_bps",
                m.funding_bps,
                -p.max_abs_funding,
                p.max_abs_funding,
            ),
            (
                "open_interest_usd",
                m.open_interest_usd,
                p.min_open_interest,
                f64::MAX,
            ),
            (
                "coverage_fraction",
                m.coverage_fraction,
                p.min_coverage,
                1.0,
            ),
            ("age_days", m.age_days, p.min_age, f64::MAX),
        ];
        for (name, value, min, max) in metrics {
            match value {
                None => reasons.push(format!("missing {name}")),
                Some(v) if !v.is_finite() || v < min || v > max => {
                    reasons.push(format!("invalid or below eligibility threshold: {name}"))
                }
                _ => (),
            }
        }
        if !m.ema_ready {
            reasons.push("EMA not ready".into());
        }
        let eligible = reasons.is_empty();
        let score = if eligible {
            // Divide before saturation to avoid overflow for finite extreme inputs.
            let liquidity = |v: f64, floor: f64| (v / floor).min(10.0) / 10.0;
            Some(
                20.0 * liquidity(m.volume_24h_usd.unwrap(), p.min_volume)
                    + 25.0 * liquidity(m.depth_usd.unwrap(), p.min_depth)
                    + 20.0 * (1.0 - m.spread_bps.unwrap() / p.max_spread)
                    + 10.0 * (1.0 - m.funding_bps.unwrap().abs() / p.max_abs_funding)
                    + 15.0 * m.directionality.unwrap()
                    + 10.0 * liquidity(m.open_interest_usd.unwrap(), p.min_open_interest),
            )
        } else {
            None
        };
        // Never echo nonfinite observations into a JSON report.
        let mut observation = m.clone();
        for value in [
            &mut observation.volume_24h_usd,
            &mut observation.depth_usd,
            &mut observation.spread_bps,
            &mut observation.volatility_pct,
            &mut observation.directionality,
            &mut observation.funding_bps,
            &mut observation.open_interest_usd,
            &mut observation.coverage_fraction,
            &mut observation.age_days,
        ] {
            if value.is_some_and(|v| !v.is_finite()) {
                *value = None;
            }
        }
        ranked.push(MarketAssessment {
            observation,
            score,
            eligible,
            reasons,
            selected: false,
        });
    }
    ranked.sort_by(|a, b| {
        b.score
            .unwrap_or(-1.0)
            .total_cmp(&a.score.unwrap_or(-1.0))
            .then_with(|| a.observation.symbol.cmp(&b.observation.symbol))
    });
    let mut selected: Vec<usize> = ranked
        .iter()
        .enumerate()
        .filter(|(_, a)| a.eligible && request.current_watchlist.contains(&a.observation.symbol))
        .map(|(i, _)| i)
        .take(p.slots)
        .collect();
    for candidate in 0..ranked.len() {
        if !ranked[candidate].eligible || selected.contains(&candidate) {
            continue;
        }
        if selected.len() < p.slots {
            selected.push(candidate);
        } else {
            let weakest = *selected.iter().max().unwrap();
            if ranked[candidate].score.unwrap() > ranked[weakest].score.unwrap() + p.switch_margin {
                selected.retain(|i| *i != weakest);
                selected.push(candidate);
            }
        }
    }
    selected.sort_unstable();
    let watchlist = selected
        .iter()
        .map(|i| ranked[*i].observation.symbol.clone())
        .collect();
    for i in selected {
        ranked[i].selected = true;
    }
    Ok(HunterReport { read_only:true,source_is_live:false,trading_authorized:false,source:request.source.clone(),as_of_ms:request.as_of_ms,ranked,watchlist,limitations:vec![
        "Offline caller-supplied snapshot; provenance and venue availability are not independently verified.".into(),
        "Heuristic scores are not expected returns, profitability evidence, or trading recommendations.".into(),
        "No correlation, execution reliability, portfolio exposure budget, capital allocation, or autonomous acquisition is modeled.".into(),
        "Watchlist removal does not close positions; no network, credential, signing, or execution authority exists here.".into(),
    ] })
}

// ============================================================================
// HUNTER Public Hyperliquid Market Discovery & Portfolio-Aware Ranking
// ============================================================================

pub const HYPERLIQUID_PUBLIC_INFO_URL: &str = "https://api.hyperliquid.xyz/info";
const MAX_PUBLIC_INFO_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicMarketObservation {
    pub symbol: String,
    pub mark_px: f64,
    pub oracle_px: Option<f64>,
    pub prev_day_px: Option<f64>,
    pub change_24h_pct: Option<f64>,
    pub volume_24h_usd: f64,
    pub open_interest_usd: f64,
    pub funding_bps_1h: f64,
    pub impact_spread_bps: Option<f64>,
    pub max_leverage: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicHunterPolicy {
    pub min_volume_24h_usd: f64,
    pub min_open_interest_usd: f64,
    pub max_impact_spread_bps: f64,
    pub max_abs_funding_bps_1h: f64,
    pub slots: usize,
    pub switch_margin: f64,
    /// Score discount (0..=40 points) applied to markets already held in the
    /// current portfolio so new discovery favors uncorrelated capacity.
    pub held_symbol_penalty: f64,
}

impl Default for PublicHunterPolicy {
    fn default() -> Self {
        Self {
            min_volume_24h_usd: 5_000_000.0,
            min_open_interest_usd: 2_000_000.0,
            max_impact_spread_bps: 25.0,
            max_abs_funding_bps_1h: 5.0,
            slots: 8,
            switch_margin: 4.0,
            held_symbol_penalty: 8.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicMarketAssessment {
    pub observation: PublicMarketObservation,
    pub already_held: bool,
    pub eligible: bool,
    pub score: Option<f64>,
    pub selected: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicHunterReport {
    pub read_only: bool,
    pub trading_authorized: bool,
    pub source: String,
    pub acquired_at_rfc3339: String,
    pub total_markets: usize,
    pub eligible_markets: usize,
    pub ranked: Vec<PublicMarketAssessment>,
    pub watchlist: Vec<String>,
    pub limitations: Vec<String>,
}

/// Parses Hyperliquid `metaAndAssetCtxs` JSON response (`[meta, [ctx...]]`)
/// strictly from real observed venue fields without fabricating depth or age.
pub fn parse_hyperliquid_meta_and_ctxs(payload: &[u8]) -> Result<Vec<PublicMarketObservation>> {
    ensure!(
        payload.len() <= MAX_PUBLIC_INFO_BYTES,
        "Hyperliquid metaAndAssetCtxs response exceeded 4 MiB bound"
    );
    let root: serde_json::Value = serde_json::from_slice(payload)
        .map_err(|e| anyhow::anyhow!("decode metaAndAssetCtxs JSON: {e}"))?;
    let arr = root
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("expected JSON array [meta, assetCtxs]"))?;
    ensure!(arr.len() >= 2, "metaAndAssetCtxs array too short");
    let universe = arr[0]
        .get("universe")
        .and_then(|u| u.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing meta.universe array"))?;
    let ctxs = arr[1]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing assetCtxs array"))?;
    let count = universe.len().min(ctxs.len()).min(1024);

    let parse_f64 = |v: &serde_json::Value, key: &str| -> Option<f64> {
        v.get(key)
            .and_then(|x| {
                x.as_str()
                    .and_then(|s| s.parse::<f64>().ok())
                    .or_else(|| x.as_f64())
            })
            .filter(|f| f.is_finite())
    };

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let meta = &universe[i];
        let ctx = &ctxs[i];
        if meta
            .get("isDelisted")
            .and_then(|d| d.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let Some(name) = meta.get("name").and_then(|n| n.as_str()) else {
            continue;
        };
        if !safe_symbol(name) {
            continue;
        }
        let Some(mark_px) = parse_f64(ctx, "markPx").filter(|p| *p > 0.0) else {
            continue;
        };
        let oracle_px = parse_f64(ctx, "oraclePx").filter(|p| *p > 0.0);
        let prev_day_px = parse_f64(ctx, "prevDayPx").filter(|p| *p > 0.0);
        let change_24h_pct = prev_day_px.map(|prev| ((mark_px - prev) / prev) * 100.0);
        let volume_24h_usd = parse_f64(ctx, "dayNtlVlm").unwrap_or(0.0).max(0.0);
        let oi_coins = parse_f64(ctx, "openInterest").unwrap_or(0.0).max(0.0);
        let open_interest_usd = (oi_coins * mark_px).max(0.0);
        let funding_bps_1h = parse_f64(ctx, "funding").unwrap_or(0.0) * 10_000.0;
        let max_leverage = meta
            .get("maxLeverage")
            .and_then(|l| l.as_u64())
            .unwrap_or(1)
            .min(u32::MAX as u64) as u32;

        let impact_spread_bps = ctx
            .get("impactPxs")
            .and_then(|imp| imp.as_array())
            .and_then(|pair| {
                if pair.len() < 2 {
                    return None;
                }
                let bid = pair[0].as_str().and_then(|s| s.parse::<f64>().ok())?;
                let ask = pair[1].as_str().and_then(|s| s.parse::<f64>().ok())?;
                if bid.is_finite() && ask.is_finite() && bid > 0.0 && ask >= bid {
                    let mid = 0.5 * (bid + ask);
                    Some(((ask - bid) / mid) * 10_000.0)
                } else {
                    None
                }
            });

        out.push(PublicMarketObservation {
            symbol: name.to_string(),
            mark_px,
            oracle_px,
            prev_day_px,
            change_24h_pct,
            volume_24h_usd,
            open_interest_usd,
            funding_bps_1h,
            impact_spread_bps,
            max_leverage,
        });
    }
    Ok(out)
}

/// Evaluates public Hyperliquid markets with liquidity/cost filters, portfolio-aware
/// diversification weighting for held symbols, and deterministic watchlist hysteresis.
pub fn evaluate_public_markets(
    source: &str,
    markets: &[PublicMarketObservation],
    policy: &PublicHunterPolicy,
    current_watchlist: &[String],
    held_symbols: &BTreeSet<String>,
) -> Result<PublicHunterReport> {
    ensure!(
        (1..=32).contains(&policy.slots),
        "public hunter slots must be 1..=32"
    );
    ensure!(
        policy.min_volume_24h_usd.is_finite()
            && policy.min_volume_24h_usd > 0.0
            && policy.min_open_interest_usd.is_finite()
            && policy.min_open_interest_usd > 0.0
            && policy.max_impact_spread_bps.is_finite()
            && policy.max_impact_spread_bps > 0.0
            && policy.max_abs_funding_bps_1h.is_finite()
            && policy.max_abs_funding_bps_1h > 0.0
            && policy.switch_margin.is_finite()
            && (0.0..=100.0).contains(&policy.switch_margin)
            && policy.held_symbol_penalty.is_finite()
            && (0.0..=40.0).contains(&policy.held_symbol_penalty),
        "invalid public hunter policy bounds"
    );

    let mut ranked = Vec::with_capacity(markets.len());
    for m in markets {
        let mut reasons = Vec::new();
        if m.volume_24h_usd < policy.min_volume_24h_usd {
            reasons.push(format!(
                "24h volume ${:.0} below ${:.0} floor",
                m.volume_24h_usd, policy.min_volume_24h_usd
            ));
        }
        if m.open_interest_usd < policy.min_open_interest_usd {
            reasons.push(format!(
                "open interest ${:.0} below ${:.0} floor",
                m.open_interest_usd, policy.min_open_interest_usd
            ));
        }
        if m.funding_bps_1h.abs() > policy.max_abs_funding_bps_1h {
            reasons.push(format!(
                "|funding| {:.2} bps/h exceeds {:.2} bps/h cap",
                m.funding_bps_1h.abs(),
                policy.max_abs_funding_bps_1h
            ));
        }
        match m.impact_spread_bps {
            None => reasons.push("impact spread unavailable".into()),
            Some(sp) if sp > policy.max_impact_spread_bps => reasons.push(format!(
                "impact spread {:.2} bps exceeds {:.2} bps cap",
                sp, policy.max_impact_spread_bps
            )),
            _ => {}
        }

        let already_held = held_symbols.contains(&m.symbol);
        let eligible = reasons.is_empty();
        let score = if eligible {
            let vol_term = (m.volume_24h_usd / policy.min_volume_24h_usd).min(20.0) / 20.0;
            let oi_term = (m.open_interest_usd / policy.min_open_interest_usd).min(20.0) / 20.0;
            let spread_term =
                1.0 - (m.impact_spread_bps.unwrap() / policy.max_impact_spread_bps).clamp(0.0, 1.0);
            let funding_term =
                1.0 - (m.funding_bps_1h.abs() / policy.max_abs_funding_bps_1h).clamp(0.0, 1.0);
            let raw = 35.0 * vol_term + 25.0 * oi_term + 25.0 * spread_term + 15.0 * funding_term;
            let adjusted = if already_held {
                (raw - policy.held_symbol_penalty).max(0.0)
            } else {
                raw
            };
            Some(adjusted.clamp(0.0, 100.0))
        } else {
            None
        };

        ranked.push(PublicMarketAssessment {
            observation: m.clone(),
            already_held,
            eligible,
            score,
            selected: false,
            reasons,
        });
    }

    ranked.sort_by(|a, b| {
        b.score
            .unwrap_or(-1.0)
            .total_cmp(&a.score.unwrap_or(-1.0))
            .then_with(|| a.observation.symbol.cmp(&b.observation.symbol))
    });

    let mut selected: Vec<usize> = ranked
        .iter()
        .enumerate()
        .filter(|(_, a)| a.eligible && current_watchlist.contains(&a.observation.symbol))
        .map(|(i, _)| i)
        .take(policy.slots)
        .collect();

    for candidate in 0..ranked.len() {
        if !ranked[candidate].eligible || selected.contains(&candidate) {
            continue;
        }
        if selected.len() < policy.slots {
            selected.push(candidate);
        } else if let Some(&weakest) = selected.iter().max() {
            if ranked[candidate].score.unwrap()
                > ranked[weakest].score.unwrap() + policy.switch_margin
            {
                selected.retain(|i| *i != weakest);
                selected.push(candidate);
            }
        }
    }
    selected.sort_unstable();
    let watchlist: Vec<String> = selected
        .iter()
        .map(|i| ranked[*i].observation.symbol.clone())
        .collect();
    for i in selected {
        ranked[i].selected = true;
    }

    let eligible_markets = ranked.iter().filter(|m| m.eligible).count();
    Ok(PublicHunterReport {
        read_only: true,
        trading_authorized: false,
        source: source.to_string(),
        acquired_at_rfc3339: chrono::Utc::now().to_rfc3339(),
        total_markets: ranked.len(),
        eligible_markets,
        ranked,
        watchlist,
        limitations: vec![
            "Read-only public Hyperliquid metaAndAssetCtxs observation; no wallet credentials or signing authority used.".into(),
            "Selection scores rank liquidity, open interest, impact spread, and funding cost — never profit forecasts.".into(),
            "Portfolio-aware penalty discounts already-held symbols for diversification research; watchlist changes never close or open positions.".into(),
        ],
    })
}

/// Fetches a live read-only public perpetual market snapshot from Hyperliquid `/info`
/// (or a loopback test endpoint) and ranks opportunities with portfolio awareness.
pub fn fetch_hyperliquid_public_snapshot(
    info_url: &str,
    policy: &PublicHunterPolicy,
    current_watchlist: &[String],
    held_symbols: &BTreeSet<String>,
) -> Result<PublicHunterReport> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| anyhow::anyhow!("build hunter runtime: {e}"))?;

    let payload = rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent(format!(
                "HYDRA-Leviathan-Hunter/{}",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|e| anyhow::anyhow!("build HTTP client: {e}"))?;
        let resp = client
            .post(info_url)
            .json(&serde_json::json!({ "type": "metaAndAssetCtxs" }))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("request Hyperliquid public metaAndAssetCtxs: {e}"))?;
        if !resp.status().is_success() {
            anyhow::bail!("Hyperliquid public /info returned HTTP {}", resp.status());
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("read Hyperliquid response: {e}"))?;
        ensure!(
            bytes.len() <= MAX_PUBLIC_INFO_BYTES,
            "Hyperliquid public /info response exceeded 4 MiB bound"
        );
        Ok::<Vec<u8>, anyhow::Error>(bytes.to_vec())
    })?;

    let markets = parse_hyperliquid_meta_and_ctxs(&payload)?;
    evaluate_public_markets(info_url, &markets, policy, current_watchlist, held_symbols)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> HunterRequest {
        HunterRequest {
            as_of_ms: 100_000,
            source: "offline fixture".into(),
            approved: ["AAA".into(), "BBB".into()].into(),
            excluded: BTreeSet::new(),
            current_watchlist: vec![],
            policy: HunterPolicy {
                slots: 1,
                ..Default::default()
            },
            markets: vec![MarketObservation {
                symbol: "AAA".into(),
                observed_at_ms: 99_000,
                volume_24h_usd: Some(2e6),
                depth_usd: Some(2e5),
                spread_bps: Some(1.0),
                volatility_pct: Some(3.0),
                directionality: Some(0.5),
                funding_bps: Some(0.0),
                open_interest_usd: Some(1e6),
                coverage_fraction: Some(1.0),
                age_days: Some(100.0),
                ema_ready: true,
            }],
        }
    }
    #[test]
    fn missing_invalid_and_stale_fail_closed() {
        let mut r = fixture();
        r.markets[0].depth_usd = None;
        r.markets[0].funding_bps = Some(f64::NAN);
        r.markets[0].observed_at_ms = 0;
        let out = evaluate(&r).unwrap();
        assert!(!out.ranked[0].eligible);
        assert_eq!(out.ranked[0].reasons.len(), 3);
        assert!(out.watchlist.is_empty());
        assert!(serde_json::to_string(&out).is_ok());
        r.markets[0].observed_at_ms = 100_001;
        assert!(evaluate(&r).unwrap().ranked[0]
            .reasons
            .iter()
            .any(|s| s.contains("future")));
    }
    #[test]
    fn approvals_and_exclusions_are_independent() {
        let mut r = fixture();
        r.approved.clear();
        r.excluded.insert("AAA".into());
        let out = evaluate(&r).unwrap();
        assert_eq!(out.ranked[0].reasons.len(), 2);
        assert!(!out.trading_authorized);
        assert!(!out.source_is_live);
        assert!(out.read_only);
    }
    #[test]
    fn ties_are_lexical_and_input_order_independent() {
        let mut r = fixture();
        let mut b = r.markets[0].clone();
        b.symbol = "BBB".into();
        r.markets.insert(0, b);
        let out = evaluate(&r).unwrap();
        assert_eq!(out.watchlist, vec!["AAA"]);
        let before = serde_json::to_string(&r).unwrap();
        r.markets.reverse();
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            serde_json::to_value(evaluate(&r).unwrap()).unwrap()
        );
        r.markets.reverse();
        assert_eq!(before, serde_json::to_string(&r).unwrap());
    }
    #[test]
    fn hysteresis_and_invalid_retention() {
        let mut r = fixture();
        let mut b = r.markets[0].clone();
        b.symbol = "BBB".into();
        b.directionality = Some(0.6);
        r.markets.push(b);
        r.current_watchlist = vec!["AAA".into()];
        assert_eq!(evaluate(&r).unwrap().watchlist, vec!["AAA"]);
        r.markets[1].directionality = Some(1.0);
        assert_eq!(evaluate(&r).unwrap().watchlist, vec!["BBB"]);
        r.markets[1].directionality = Some(0.5);
        r.markets[0].ema_ready = false;
        assert_eq!(evaluate(&r).unwrap().watchlist, vec!["BBB"]);
    }
    #[test]
    fn resource_and_schema_bounds() {
        let mut r = fixture();
        r.markets.push(r.markets[0].clone());
        assert!(evaluate(&r).is_err());
        r.markets = vec![r.markets[0].clone(); 1025];
        assert!(evaluate(&r).is_err());
        r = fixture();
        r.policy.slots = 33;
        assert!(evaluate(&r).is_err());
        r = fixture();
        r.markets[0].symbol = "bad symbol".into();
        assert!(evaluate(&r).is_err());
        let mut json = serde_json::to_value(fixture()).unwrap();
        json["surprise"] = true.into();
        assert!(serde_json::from_value::<HunterRequest>(json).is_err());
    }
    #[test]
    fn volatility_is_not_opportunity_and_extremes_are_bounded() {
        let mut r = fixture();
        let a = evaluate(&r).unwrap().ranked[0].score;
        r.markets[0].volatility_pct = Some(f64::MAX);
        assert_eq!(a, evaluate(&r).unwrap().ranked[0].score);
        r.markets[0].volume_24h_usd = Some(f64::MAX);
        let score = evaluate(&r).unwrap().ranked[0].score.unwrap();
        assert!((0.0..=100.0).contains(&score));
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn invalid_policy_values_fail_before_scoring() {
        for bad in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
            let request = HunterRequest {
                as_of_ms: 0,
                source: "boundary".into(),
                approved: BTreeSet::new(),
                excluded: BTreeSet::new(),
                current_watchlist: vec![],
                policy: HunterPolicy {
                    min_depth: bad,
                    ..Default::default()
                },
                markets: vec![],
            };
            assert!(evaluate(&request).is_err());
        }
    }
    #[test]
    fn empty_snapshot_has_no_fabricated_markets() {
        let request = HunterRequest {
            as_of_ms: 0,
            source: "empty snapshot".into(),
            approved: BTreeSet::new(),
            excluded: BTreeSet::new(),
            current_watchlist: vec!["OLD".into()],
            policy: HunterPolicy::default(),
            markets: vec![],
        };
        let report = evaluate(&request).unwrap();
        assert!(report.ranked.is_empty());
        assert!(report.watchlist.is_empty());
        assert_eq!(request.current_watchlist, vec!["OLD"]);
    }

    #[test]
    fn public_hyperliquid_loopback_acquisition_and_portfolio_aware_ranking() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let body = serde_json::json!([
            {
                "universe": [
                    { "name": "BTC", "maxLeverage": 50 },
                    { "name": "ETH", "maxLeverage": 50 },
                    { "name": "ILLIQ", "maxLeverage": 10 },
                    { "name": "DEAD", "maxLeverage": 5, "isDelisted": true }
                ]
            },
            [
                {
                    "markPx": "100000.0",
                    "oraclePx": "100005.0",
                    "prevDayPx": "98000.0",
                    "dayNtlVlm": "500000000.0",
                    "openInterest": "2500.0",
                    "funding": "0.00005",
                    "impactPxs": ["99995.0", "100005.0"]
                },
                {
                    "markPx": "3500.0",
                    "oraclePx": "3501.0",
                    "prevDayPx": "3400.0",
                    "dayNtlVlm": "450000000.0",
                    "openInterest": "60000.0",
                    "funding": "0.00004",
                    "impactPxs": ["3499.5", "3500.5"]
                },
                {
                    "markPx": "1.2",
                    "dayNtlVlm": "10000.0",
                    "openInterest": "5000.0",
                    "funding": "0.0001",
                    "impactPxs": ["1.18", "1.22"]
                },
                {
                    "markPx": "10.0",
                    "dayNtlVlm": "99999999.0",
                    "openInterest": "999999.0",
                    "funding": "0.0",
                    "impactPxs": ["9.99", "10.01"]
                }
            ]
        ])
        .to_string();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body_clone = body.clone();
        let srv = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body_clone.len(),
                body_clone
            );
            s.write_all(resp.as_bytes()).unwrap();
        });

        let held: BTreeSet<String> = ["BTC".to_string()].into();
        let policy = PublicHunterPolicy {
            slots: 2,
            held_symbol_penalty: 12.0,
            ..Default::default()
        };
        let report =
            fetch_hyperliquid_public_snapshot(&format!("http://{addr}/info"), &policy, &[], &held)
                .unwrap();
        srv.join().unwrap();

        assert!(report.read_only && !report.trading_authorized);
        // Delisted DEAD excluded; ILLIQ ineligible due to low volume/OI/wide spread.
        assert_eq!(report.total_markets, 3);
        assert_eq!(report.eligible_markets, 2);
        // Portfolio-aware penalty promotes unheld ETH above already-held BTC.
        assert_eq!(report.ranked[0].observation.symbol, "ETH");
        assert!(!report.ranked[0].already_held);
        assert_eq!(report.ranked[1].observation.symbol, "BTC");
        assert!(report.ranked[1].already_held);
        assert_eq!(report.watchlist, vec!["ETH", "BTC"]);
    }
}
