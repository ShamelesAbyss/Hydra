# Passivbot / Hydra capability ledger

Status is relative to the Project Leviathan main prerelease. An existing Hydra
feature is not proof of full parity with upstream. No upstream Rust code is ported yet.

| Requested upstream capability | Hydra equivalent/status | Validation or next work |
|---|---|---|
| Rust strategy engine | `FangUnifiedPlanner` (`fang-registry-v1`: 7 strategies) + divergence/momentum/mean reversion | `fang_engine`, `fang_parity`, existing strategy/indicator tests pass |
| Deterministic orchestration | Authoritative shared planner unified across Engine, Synapse Paper & ORACLE | Verified identical selection & sizing in `tests/fang_parity.rs` |
| Hyperliquid execution | Existing mainnet adapter, journal/typed response improvements | Offline faults; comprehensive reconciliation pending |
| EMA Anchor | Adaptive `ema_anchor` shared kernel integrated into `FangUnifiedPlanner` | Verified across `fang_parity`, ORACLE & Synapse Paper |
| Trailing Martingale | Permanently excluded (anti-Martingale `size_multiplier <= 1.5` & hard USD cap enforced) | Verified rejection of unbounded multipliers in `fang_planner` |
| Trailing grids | `TrailingEntryState` & `TrailingExitState` implemented in `fang_engine.rs` | Unit-tested in `fang_engine`; live exchange order persistence pending |
| Dynamic entry grids | Bounded geometric/arithmetic grids (`1..=20` levels, cumulative USD cap) | Unit-tested & wired into `FangUnifiedPlanner` |
| Dynamic take-profit grids | Volatility-scaled `compute_adaptive_take_profit` (`0.6x–1.8x` ATR scale) | Verified in `fang_engine`; partial-fill live scaling pending |
| Wallet exposure limits | Existing risk sizing | Configuration validation and accounting gaps remain |
| Total wallet exposure | Standard-mode account snapshot plus pending-order exposure and reservations | Narrow terminal-entry reconciliation implemented; broader account/lifecycle models remain |
| Exposure enforcement | Persistent Aegis authority, reservations, unresolved gate and position ownership | Orphan, reduced/closed and ambiguous lifecycle cases remain held |
| Auto-Unstuck | Not integrated | Budget and realized-loss accounting first |
| Equity hard stop-loss | Persistent drawdown entry breaker and explicit audited rearm | Complete protective flattening is not certified |
| Market Forager | Offline approved-market eligibility and liquidity/cost ranking | Autonomous public-data acquisition remains |
| Dynamic asset selection | Bounded research watchlist with deterministic hysteresis | Live rotation, correlation and capital budgets remain |
| Historical data management | Polling candle snapshots only | Cache, integrity manifests, funding history pending |
| Backtesting | Oracle deterministic single-instrument replay | Explicit costs/limits; richer execution models remain |
| Multi-scenario simulation | Fixed-configuration disjoint held-out windows and explicit cost scenarios | Train/select/test fitting, shared planner and latency models remain |
| Evolutionary optimization | Seeded CPU search, immutable checkpoint/resume and frozen-frontier later held-out assessment | Full train/select/test fitting, dataset authenticity and NSGA-II remain |
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

Alpha.14 retains native engine unsigned IOC action bytes before submission; nonce/signed-envelope provenance and automatic recovery remain incomplete.

Alpha.15 adds bounded offline `wire-history` inspection of retained unsigned native actions. It does not prove submission, acceptance or complete signed-envelope provenance.

Alpha.16 checks exact native acknowledgment bounds against any retained unsigned action on persistence and later reads, using one transaction. Historical absence remains explicit; full signed provenance and settlement remain incomplete.

Alpha.17 adds a durable signer-nonce coordinator Rust foundation. It is not wired into the live sender yet; account ownership, signing preparation and recovery integration remain distinct acceptance work.

Alpha.18 integrates the durable coordinator into all native IOC sender calls and adds engine pre-signing metadata journaling. Full signed-envelope proof, preparation inspection, automatic recovery and net settlement remain incomplete.

Alpha.19 adds protocol-2 read-only signing-preparation inspection and shared native-fact validation. Preparation remains local metadata without signature, submission, acceptance or financial-settlement proof.

Alpha.20 adds one-snapshot inspection-only `intent-evidence` and bounds native fact/schema reads. Recovery classification, automatic repair, complete signed-envelope proof and net settlement remain open.

Alpha.23 adds native FANG planning/EMA, COGNITION strict historical shadow analytics
and AEGIS offline portfolio/correlation diagnostics. These do not change live
strategy orchestration, risk authority or financial settlement. See ANALYTICS.md.
