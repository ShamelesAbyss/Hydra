# Passivbot / Hydra capability ledger

Status is relative to this execution-foundation prerelease. An existing Hydra
feature is not proof of full parity with upstream. No upstream Rust code is ported yet.

| Requested upstream capability | Hydra equivalent/status | Validation or next work |
|---|---|---|
| Rust strategy engine | Existing divergence/momentum/mean reversion | Existing indicator/learning tests preserved |
| Deterministic orchestration | Intent boundary added; planner not extracted | Shared live/simulation planner pending |
| Hyperliquid execution | Existing mainnet adapter, journal/typed response improvements | Offline faults; comprehensive reconciliation pending |
| EMA Anchor | Not integrated | Port kernel with stable/development fixture comparison |
| Trailing Martingale | Not integrated | Hard exposure budget required; no unbounded doubling |
| Trailing grids | Not integrated | Planner and trailing-state persistence first |
| Dynamic entry grids | Not integrated | Metadata and exposure-aware order planning |
| Dynamic take-profit grids | Not integrated | Partial-fill ownership/net-position semantics |
| Wallet exposure limits | Existing risk sizing | Configuration validation and accounting gaps remain |
| Total wallet exposure | Existing approximate open notional | Include resting/pending orders and authoritative equity |
| Exposure enforcement | Journal unresolved gate and single-position ownership | Independent Aegis hard authority pending |
| Auto-Unstuck | Not integrated | Budget and realized-loss accounting first |
| Equity hard stop-loss | Not integrated as authoritative persistent action | Durable circuit breaker and verified equity required |
| Market Forager | Configured symbols only | Approved instrument ranking/rotation pending |
| Dynamic asset selection | Not integrated | Hunter eligibility and hysteresis pending |
| Historical data management | Polling candle snapshots only | Cache, integrity manifests, funding history pending |
| Backtesting | Not implemented | Oracle execution model and dataset provenance |
| Multi-scenario simulation | Not implemented | Shared planner plus cost/latency scenarios |
| Evolutionary optimization | Not implemented | Oracle then seeded Genesis population evaluation |
| Multi-objective optimization | Not implemented | Constraint-aware NSGA-II |
| Pareto front analysis | Not implemented | Explicit objectives and out-of-sample verification |
| Execution recovery | Durable intent evidence and conservative mismatch refusal | Automated cloid/fill replay pending |
| Monitoring | Existing Ratatui; bounded buffers | Native GUI and independent IPC pending |
| GPU screening | Not implemented | Optional only after measured CPU parity |
| Regression testing | Existing suite plus execution fault injection | Broaden to accounting/streams/replay later |

Justified exclusions: Python runtime, automatic deployment of optimizer candidates,
unbounded Martingale, mandatory cloud services, withdrawal/transfer operations, and
obsolete active Ethereum integration. Broader requested features are deferred,
not silently excluded.
