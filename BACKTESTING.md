# Oracle candle replay

Oracle runs deterministic single-instrument candle research with Hydra's existing
strategy factory. Supply a JSON array of Candle records containing RFC3339 ts,
open, high, low, close and volume fields in strictly increasing timestamp order.
Use observed historical input with sufficient indicator warmup.

```text
hydra --config config/strategies.toml backtest --candles historical-candles.json
hydra --config config/strategies.toml backtest --candles historical-candles.json --options replay-options.json
```

Options contain symbol, initial_equity, risk_fraction, max_exposure_fraction,
fee_bps, slippage_bps, warmup_bars, max_bars and max_trades. The default warmup
is 240 bars. JSON inputs are bounded to 32 MiB and evaluation work is bounded.
The report embeds options, dataset range, simulated trades, equity, costs and
drawdown. Research configuration loading omits wallet settings and storage setup.

Signals see completed candles. Entries execute at the next candle open. Stops
take precedence when a candle touches both stop and target, and adverse stop
gaps use the candle open. Fees and slippage apply to both legs. Entry notional
plus its fee stays within the configured unleveraged allocation.

The model assumes full fills and fixed costs. Funding, order-book queues,
partial fills, liquidation, adaptive learning, trailing/time stops and portfolio
margin remain future work. Configured strategy order selects the first matching
signal. Bar-close evaluation differs from intrabar live processing. Use the model
limitations included in every report when interpreting results.

## Rolling held-out validation and cost scenarios (alpha.22)

```powershell
.\hydra.exe --config config/strategies.toml validate-research --candles candles.json --options validation.json
```

In ORACLE, select **Rolling held-out / cost scenarios**, copy the default options
JSON, adjust the local file and run validation. Individual scenario/fold panels
show actual simulated equity curves, trades and original model assumptions.
No engine database, key, venue request or execution authority is needed.

Defaults: 240 prior history bars, 240 test bars, step 240, at most 8 folds;
scenarios baseline (4.5 bps fee / 5 bps slippage) and higher_cost (9 / 10).
`history_bars` replaces replay warmup, and scenario costs replace replay costs.
Test windows never overlap (`step_bars >= test_bars`); earlier history may overlap
prior test windows and supplies indicators only. Each replay receives only its
own prior history and test interval, resets strategy state and capital, and closes
positions at that fold's final bar. A signal at its first test bar can fill no
earlier than the next bar. No parameters are fitted or selected by this evaluator.

Reports retain half-open dataset indices, timestamps, full replay reports, unused
tail count and per-scenario summaries. Mean fold return is an arithmetic mean;
pooled net is the sum of independently reset fold outcomes, **not a compounded
portfolio return**. Data used to choose the fixed configuration may contaminate
an apparently held-out sample; this tool cannot establish an unseen-data claim.
Preserve the exact strategy TOML and candle dataset to reproduce a report; their
contents and hashes are not embedded in it.

Limits: 32 folds, 8 scenarios, 64 combined replays, 100 million aggregate history
work units and one million retained equity/trade entries. Full input chronology
is checked, including gaps. All original Oracle model limits remain: no funding,
queue/partial-fill/latency or portfolio simulation. This is a validation foundation,
not full train/select/test walk-forward optimization or live readiness evidence.
