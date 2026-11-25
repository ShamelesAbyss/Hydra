# Project Leviathan implementation plan

Development preserves the repository history, MIT license, Rust engine, terminal
interface and Hyperliquid mainnet integration. Shipping defaults remain disarmed.
No development milestone may submit live exchange orders.

## Delivery status at alpha.5

| Area | Delivered foundation | Remaining acceptance boundary |
|---|---|---|
| Execution | Durable intents, typed receipts, residual proof, ownership and narrow entry recovery | Broader cloid/fill replay and complete settlement |
| Ledger/risk | Decimal fill/funding facts, account binding, persistent Aegis | Scoped terminal-entry reconciliation landed; orphan atomicity and net outcomes remain |
| Native desktop | eframe/glow cockpit, read-only SQLite telemetry and explicit local setup | Richer authenticated IPC and operational UI verification |
| Cognition | Existing adaptive components and durable original entry context | Authoritative outcome provenance and shared planner |
| Research | Oracle fixed-cost replay; Genesis bounded seeded CPU Pareto search | Full execution models, out-of-sample work and resumability |
| Presentation | Original SVG identity/diagrams and actual native captures, visible on main | Continued release/documentation synchronization |
| Dependencies | Native narrow signer; audit 0 vulnerability entries / 1 warning | Chrono maintenance migration and ongoing review |

The milestone descriptions below retain the broader target scope. A delivered
foundation is not full acceptance or stable-live certification. New engine admission now closes the reservation/journal insertion gap atomically.
Scoped terminal-entry reconciliation uses same-payload position/notional evidence
and definite receipt/position proof. The immediate next increments are fixed-point
core-position facts and authoritative settlement; historical ambiguous/orphan holds
are never guessed away.

## Milestone 1 — durable execution foundation (prerelease)

Implement a common submission boundary, configuration authorization, persistent
intent IDs, typed responses, metadata/slippage validation, confirmed residuals,
conservative startup checks and fault-injection tests. Keep unknown net PnL out
of learning. Publish verified Windows CLI artifacts and a recovery handoff.

## Milestone 2 — authoritative account ledger (stable-live prerequisite)

1. Model signer/account identity, margin/account modes and approved instruments.
2. Introduce fixed-point quantities/price validation using authoritative metadata.
3. Fetch order status by cloid and deduplicate fills by exchange identity; persist
   fees and funding in their native denomination and document conversion policy.
4. Reconcile outstanding intents, open orders and signed net positions at startup
   and periodically. Handle snapshot/stream race boundaries explicitly.
5. Apply position, learning context and net outcome updates atomically and exactly
   once; preserve original risk distance through restart.
6. Remove guessed/stale equity sizing; validate balance composition and freshness.
7. Add account-wide process exclusion, secure signing storage and durable breakers.
8. Verify crash windows, disconnections, partial IOC, unknown positions, manual
   exchange intervention, fee corrections and database failures with recorded fixtures.

Publish a stable milestone only when these contracts pass and supported execution
modes are accurately documented. Existing historical estimates require provenance;
do not retroactively relabel them as authoritative.

## Milestone 3 — independent engine and native desktop

The optional native client uses eframe/glow; Windows rendering and local
read-only telemetry are implemented. Add authenticated local IPC beyond the
versioned CLI/SQLite snapshot boundary, and validate graphics fallback behavior. GUI reconnects without owning signing or engine lifetime. Build the
command center from real snapshots and persistent events; unavailable values
are visibly unavailable. Preserve Ratatui and headless builds. Verify resize,
DPI, minimize/reconnect and keyboard navigation before adding visual effects.

## Milestone 4 — shared planning and risk authority

Extract deterministic planning shared by live/paper/replay. Preserve existing
strategies. Add explicit net-position ownership/arbitration before combining
strategies per instrument. Port selected Passivbot math with provenance and
fixtures; add EMA Anchor, trailing/dynamic grids and bounded scaling incrementally.
AEGIS evaluates pending exposure and has sole authority over hard limits.

## Milestone 5 — discovery and cognition

Use approved markets, authoritative metadata, spread/depth/cost filters,
correlations and budgets for Hunter. Persist normalized entry context, confidence,
recency/sample-aware statistics and strategy attribution for Cognition. Shadow
candidate policies; risk constraints remain independent.

## Milestone 6 — Oracle and Genesis

Expand existing deterministic fixed-cost Oracle replay with dataset integrity,
funding, partial fills, latency and walk-forward scenarios. Establish shared
live/planner parity. Extend the existing seeded Genesis mutation/crossover and
Pareto filtering with resumable checkpoints, out-of-sample acceptance and NSGA-II.
GPU acceleration is optional and requires CPU parity and reproducible benchmarks.

## Milestone 7 — visual workspaces and distribution

Build genuine workspace functionality on verified telemetry/research services.
Use emerald/cyan/ultraviolet selectively over graphite, clear mode labels and
readable numerical hierarchy. Analytical geometry has documented mappings;
animations correspond to actual events. Create original branding, icons,
screenshots, onboarding and a portable distribution before an installer.

Every coherent verified checkpoint updates status/tests/changelog, commits,
pushes, tags and publishes artifacts. Experimental incomplete work stays marked
prerelease. Never overwrite releases or rewrite history.

## Next proof boundary after alpha.8: exact reduction residuals

1. Preserve a bounded account/mainnet/default-perp position snapshot from a single
   clearinghouse response, including exact signed quantity, entry price and
   exchange/acquisition times. Protective reduction verification must not depend
   on unrelated open-order or mark-price endpoints.
2. For positions with exact entry/current provenance and native close facts,
   compute expected residual with checked Decimal subtraction. Require matching
   post-receipt scoped remote evidence; exact zero requires a complete bounded
   snapshot, not an omitted/failed response.
3. Commit current exact quantity, compatibility position state, reduction facts,
   journal application and retained evidence atomically. A mismatch or persistence
   failure leaves the known receipt unresolved; never retry a signed write.
4. Keep legacy unknown precision explicit. No residual proof establishes complete
   fees/funding or net PnL; existing independent financial holds remain active.
5. Verify partial/full reductions, account/time/side/entry mismatches, duplicate
   symbols, sub-float quantity differences, restart and each transaction boundary
   with offline fixtures before adding the runtime bridge.

## Alpha.9 continuation on main

The exact residual proof boundary above is implemented and covered by parser,
storage and runtime fixtures. Continue directly on main. Next work: stronger
receipt identity/replay guards, authoritative order-status recovery and complete
fee/funding coverage/conventions before net settlement. Do not infer unknown
costs or release independent holds merely because a current residual is known.

## Next authoritative recovery boundary after alpha.10

The [official order-status schema](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint#query-order-status-by-oid-or-cloid)
accepts a numeric venue ID or a client order ID for the account. Hydra must match
returned identity and intent scope before using any observation. A missing-order
response is uncertainty, not proof of rejection. Lifecycle status alone cannot
supply average execution price, complete fees/funding or net settlement.

Next: bounded read-only native order-status observations, durable provenance and
offline schema/identity tests, without resubmitting ambiguous orders. Automatic
resolution remains held until the required financial and causal proofs exist.

## Next financial proof boundary after alpha.12

Reconcile scoped ledger fills against known exact native acknowledgments in one
consistent SQLite snapshot. Require exact acknowledged quantity coverage before
calling an order's recorded fill set complete; keep partial sets useful but
explicitly incomplete. Preserve missing/unsupported fee components and unverified
PnL conventions. No order-local evidence alone settles a round-trip trade or
proves complete funding coverage. Keep all independent financial holds active.

Persisting exact signed-wire request provenance and authoritative unknown-order
recovery remain separate required work. Continue directly on main.
