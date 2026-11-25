# HUNTER / offline market radar

HUNTER evaluates operator-supplied observations; it does not fetch live markets,
allocate capital, submit orders, close positions or authorize execution.

```powershell
.\hydra.exe hunter --snapshot market-snapshot.json
```

Open HUNTER in the native cockpit (Alt+1), copy the empty JSON template, populate
it with your own observations, enter its local path and select **Evaluate snapshot**.
Evaluation runs off the rendering thread. Reports expose the original source label
and as-of timestamp; neither establishes current exchange state. JSON input is
limited to 32 MiB, 1,024 markets and a watchlist of at most 32 symbols.

## Input contract

The top-level request has `as_of_ms` (nonnegative Unix milliseconds), `source`,
`approved` and `excluded` symbol arrays, `current_watchlist`, `policy` and `markets`.
Every observation contains `symbol`, `observed_at_ms`, `volume_24h_usd`, `depth_usd`,
`spread_bps`, `volatility_pct`, `directionality` (0–1), `funding_bps`,
`open_interest_usd`, `coverage_fraction` (0–1), `age_days` and `ema_ready`.
Missing metrics are represented by null and block eligibility. Unknown fields,
duplicate symbols and invalid policy/resource limits are rejected.

Policy defaults: `max_age_ms=60000`, `min_volume=1000000`, `min_depth=100000`,
`max_spread=10`, `max_abs_funding=10`, `min_coverage=0.95`, `min_age=30`,
`min_open_interest=500000`, `slots=5`, `switch_margin=5` score points.
Approval, exclusion, freshness, completeness, numeric ranges and EMA readiness
are independent gates. Snapshot freshness is relative to the supplied as-of,
not the wall clock. The caller owns data provenance and the meaning of depth.

## Ranking and watchlist hysteresis

Eligible observations receive a 0–100 heuristic score: volume 20%, depth 25%,
spread cost 20%, absolute funding cost 10%, directionality 15%, open interest 10%.
Liquidity terms saturate at ten times their eligibility floor. Volatility is a
completeness/range check and never raises the score by itself. Scores do not
predict returns or establish statistical confidence. Ties use lexical symbols.
Eligible current members are retained up to the slot budget; a challenger must
strictly exceed the weakest retained score by the configured switch margin.
An ineligible incumbent is removed from this research watchlist only.

The native workspace shows eligibility reasons, searchable/selected-only results,
score bars and a depth-versus-spread map with observation tooltips. It reports
`read_only=true`, `source_is_live=false` and `trading_authorized=false`.

## Outstanding capabilities

Public-data acquisition, verified historical caching, cross-asset correlation,
portfolio budgets and autonomous execution-connected rotation remain planned.
The research watchlist cannot bypass AEGIS or NEXUS and is not live selection.
