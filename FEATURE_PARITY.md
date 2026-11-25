# Passivbot / Hydra capability ledger

Status is relative to the Project Leviathan main prerelease. An existing Hydra
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
| Total wallet exposure | Standard-mode account snapshot plus pending-order exposure and reservations | Narrow terminal-entry reconciliation implemented; broader account/lifecycle models remain |
| Exposure enforcement | Persistent Aegis authority, reservations, unresolved gate and position ownership | Orphan, reduced/closed and ambiguous lifecycle cases remain held |
| Auto-Unstuck | Not integrated | Budget and realized-loss accounting first |
| Equity hard stop-loss | Persistent drawdown entry breaker and explicit audited rearm | Complete protective flattening is not certified |
| Market Forager | Configured symbols only | Approved instrument ranking/rotation pending |
| Dynamic asset selection | Not integrated | Hunter eligibility and hysteresis pending |
| Historical data management | Polling candle snapshots only | Cache, integrity manifests, funding history pending |
| Backtesting | Oracle deterministic single-instrument replay | Explicit costs/limits; richer execution models remain |
| Multi-scenario simulation | Not implemented | Shared planner plus cost/latency scenarios |
| Evolutionary optimization | Genesis seeded bounded CPU search through Oracle | Resumability and out-of-sample validation remain |
| Multi-objective optimization | Return/drawdown/fee Pareto filtering implemented | Constraint-aware NSGA-II remains planned |
| Pareto front analysis | Actual Genesis report and native visualization | Out-of-sample verification remains |
| Execution recovery | Durable intents, original entry context, narrow known-fill application recovery | Broad cloid/fill replay and net settlement remain |
| Monitoring | Ratatui and native eframe read-only cockpit; versioned telemetry CLI | Authenticated richer IPC remains planned |
| GPU screening | Not implemented | Optional only after measured CPU parity |
| Regression testing | Existing suite plus execution fault injection | Broaden to accounting/streams/replay later |

Justified exclusions: Python runtime, automatic deployment of optimizer candidates,
unbounded Martingale, mandatory cloud services, withdrawal/transfer operations, and
obsolete active Ethereum integration. Broader requested features are deferred,
not silently excluded.

Read-only lifecycle provenance is implemented through the Rust API in alpha.11. Alpha.12 adds the offline `status-history` CLI/Rust inspector. A live-observation CLI, dedicated GUI, automatic order-state repair and complete net settlement remain planned.

Alpha.13 adds exact native acknowledged-quantity coverage and consistent ledger snapshots through the Rust API. Complete order history, funding/entry attribution and round-trip settlement remain unverified.
