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
