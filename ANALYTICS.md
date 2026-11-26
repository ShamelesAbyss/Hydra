# FANG / COGNITION / AEGIS offline analytics

These tools consume local, caller-supplied JSON and have no execution, financial
settlement, live-learning, deployment or risk-release authority. They do not fetch
an account or mutate engine storage. Inputs are bounded to 32 MiB. Open a head
with Alt+2/3/4, copy its empty request template, populate it with your own data and
run the background evaluation. Copy the complete typed report as JSON.

```powershell
.\hydra.exe diagnose --kind fang --snapshot planning.json
.\hydra.exe diagnose --kind ema-anchor --snapshot completed-candles.json
.\hydra.exe diagnose --kind cognition --snapshot normalized-outcomes.json
.\hydra.exe diagnose --kind aegis --snapshot portfolio.json
```

## FANG / deterministic planning foundation

Input `candidates` and `budget`. Candidate: bounded unique `id`, existing Hydra
`signal`, positive `reference_price`, exact Decimal `requested_notional` (quote
units). Budget: `approved_symbols`, `excluded_symbols`, exact Decimal
`total_remaining_notional`, `per_symbol_remaining_notional`, `occupied_symbols`
and `confidence_floor`. Prefer decimal strings to preserve input precision.
Candidates require native Hyperliquid venue, finite confidence/edge and directionally
valid stop/take prices. Priority is descending confidence/edge, oldest signal,
then lexical strategy/symbol/ID. Any occupied symbol is blocked; only one owner
can be selected per symbol. Every rejection has an explicit reason.

Capacity is externally supplied, not an authoritative account valuation. Checked
Decimal subtraction refuses precision loss. This pure kernel is available to
future live/replay callers, but those callers do **not** yet use it. It constructs
no orders; independent AEGIS, freshness, margin, lot/tick/minimums, fees and native
protective-order checks remain required before any future execution integration.

Basic EMA Anchor mode accepts `candles`, `side` and `options`. Defaults fast/slow
12/48, offset 10 bps, maximum quote distance 100 bps. Completed chronological OHLCV
must satisfy warmup/range constraints. The report shows both seeded EMAs, directional
anchor deviation and an optional bounded **price-only** suggestion. This is a
Hydra-owned basic kernel, not a Passivbot port/parity claim or adaptive grid.
No sizing, scaling, inventory shift or live integration is included.

## COGNITION / strict prior-outcome shadow evaluation

Input `as_of_ms`, `policy`, `samples`. Every sample has unique `id`, `strategy`,
`symbol`, caller-assigned `regime`, `decision_at_ms`, `outcome_at_ms`, finite
`normalized_r` and `source` (`simulated` or `caller_supplied_normalized_r`).
`verified_net` is refused because this module cannot authenticate settlement.

At each historical decision, the fixed shadow gate sees only outcomes available
**strictly before** that decision. Same-timestamp and future outcomes are excluded.
Sources remain separate. Defaults: 30-day lookback, seven-day half-life, 30 minimum
samples, 20 minimum Kish effective samples, lower-bound threshold zero and width
multiplier 1.96. Groups report recency-weighted mean, unbiased weighted variance,
effective sample count and an explicitly approximate normal interval. Native plots
show the actual interval and zero reference; unavailable intervals remain unknown.

These are descriptive statistics, not calibrated probabilities or guaranteed
coverage. Serial dependence, heavy tails, selected opportunities, survivorship and
policy tuning invalidate naive coverage claims. Supplied regimes are not inferred
market regimes. Baseline and shadow sums use identical supplied opportunities;
skipped outcomes contribute zero to the per-opportunity shadow comparison, which
is **not portfolio return**. At most 10,000 samples; worst-case work is quadratic.
No new live learning policy is installed or fed unverified outcomes.

## AEGIS / portfolio concentration and historical correlation

Input `as_of_ms`, `approved`, `account`, `budgets`, `positions`, `returns`.
Account identity is explicitly equity = cash + unrealized PnL + other adjustments;
position notionals are never treated as cash assets. Components may be null;
`exposures_complete=false`, missing values, stale snapshots or inconsistent equity
prevent a positive known budget result. Account IDs, completeness and valuations
are caller assertions, never authoritative reconciliation.

Separate long/short Decimal notionals preserve hedge gross exposure. Checked sums
precede ratios. Reports show gross/net/long/short leverage, per-asset concentration,
unknowns and budget breaches. Defaults are templates, not recommended risk limits.
Ratios/products may round at Decimal representation limits; this report is not the
live risk gate. The existing persistent AEGIS enforcement remains independent.

Optional return series require unique strictly chronological timestamps and exactly
aligned historical samples in the supplied lookback window. No forward-fill occurs.
Missing, sparse, constant or low-variance series yield unavailable correlation,
never an invented zero. Pearson estimates do not capture tail stress or establish
causality; return-interval consistency and data completeness are caller obligations.
The native pair visualization labels actual coefficients, sample counts and unknowns.
Limits: 32 combined assets, 2,048 total observations. No allocation or hold changes.
