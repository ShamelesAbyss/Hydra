# HYDRA — Project Leviathan engineering handoff

## Recovery identity

- Current version: **1.7.0-alpha.1**.
- Recovery tag: **v1.7.0-alpha.1** on `leviathan/execution-foundation`.
- Current release commit: resolve with `git rev-parse v1.7.0-alpha.1^{commit}`;
  the tag identifies the exact source without embedding a self-referential hash.
- Original baseline: `5720ecde7565583e85dde3ebf48acafa71af35e0` (1.6.2).
- Last verified stable GitHub release: **none**. This checkpoint is a prerelease.
- Current phase: execution foundations, before authoritative account ledger and GUI.
- Mainnet was not activated; no exchange orders were used in development/tests.

## Completed functionality

1. Authoritative config live gate, SDK-side defense, CLI cannot override false.
2. Durable unique UUID intent journal before network submission, persistent unknown
   outcomes, duplicate-send prevention and unresolved new-exposure gate.
3. Typed SDK response parsing; no requested-quantity fill fallback.
4. Validated instrument metadata, correct perp decimal cap and bounded IOC slippage.
5. Unified close path for position manager, shutdown reductions/flatten and emergency.
6. Exchange-confirmed residual quantities and atomic reduction evidence updates.
7. Conservative startup mismatch handling; no invented zero-PnL closures.
8. Critical position read failures propagate; confirmed-but-unaccounted net outcomes
   cannot contaminate learning/compounding.
9. Original strategy/learning/TUI functionality retained with existing tests.
10. Offline failure/restart regression suite, audits, roadmap and CI packaging.

## Known defects / limitations

**This alpha is not ready for unattended live use.**

- New reductions record gross evidence but have NULL net PnL. New exposure remains
  blocked until fees/funding are reconciled. No automatic reconcile/clear utility
  exists; never erase journal rows to force trading.
- Historical fills/open orders/funding are not ingested. `recover` only compares
  positions, and startup/account checks do not resolve orders or stream races.
- Equity aggregation, stale risk equity, signer/account identity and unified
  margin support need correction and authoritative tests.
- Account-wide process locking is absent; use one engine per account. Different
  databases/processes are not coordinated by this journal.
- Windows-protected credential storage and complete security/history audit remain.
- Dependency audit has unresolved findings; details are in TEST_RESULTS.md and
  DEPENDENCY_AUDIT.md. CI audit should report failure until remediated.
- Several legacy configuration fields do not imply implemented features. Partial
  TP, independent drawdown authority and native exchange protective stops need work.
- Entry context/original risk distance are not fully restart-persistent.
- No native GUI, IPC daemon, backtester, optimizer, GPU engine or installer yet.

## Architectural decisions

- Preserve Rust, Hyperliquid mainnet support, local-first SQLite, headless and Ratatui.
- Keep live disabled by default; never silently substitute simulation.
- Preserve exchange uncertainty rather than guess or repeat writes.
- Reconciliation discrepancies do not create synthetic realized outcomes.
- Quarantine gross reductions until verified net fees/funding are available.
- Reject another entry on an owned instrument until explicit net-position arbitration.
- Use additive schema changes and FULL SQLite synchronous writes.
- No Passivbot source code/runtime is imported in this checkpoint.
- Incomplete verified work is prerelease, never mislabeled stable.

## Validation

See TEST_RESULTS.md for exact completed commands and limitations. The baseline had
95 passing tests; this checkpoint adds 20 focused offline regression tests (115 total).
No profit/performance improvement, security certification or GUI functionality is claimed.

## Exact next steps

1. Read HYDRA_AUDIT.md, ARCHITECTURE.md and IMPLEMENTATION_PLAN.md.
2. Reproduce `cargo test --locked --all-targets`, `cargo fmt --check`,
   `cargo clippy --locked --all-targets`, and `cargo audit`.
3. Resolve dependency alerts with reachability analysis and supported SDK/runtime
   upgrades; do not blanket-ignore or falsely mark them fixed.
4. Design the authoritative account ledger: cloid order queries, fill IDs, fees,
   funding, account-mode-aware equity and cursor/snapshot race boundaries.
5. Build fake/recorded-fixture accounting and crash-replay tests before live code.
6. Make entry/exit local application transactional and exactly once, including
   strategy context and original R; implement safe journal reconciliation.
7. Add account-wide process ownership and credential storage; complete independent
   risk readiness/limits. Only then certify a stable execution release.
8. Add native eframe/WGPU GUI over authenticated local IPC and genuine telemetry.
9. Follow the remaining milestone sequence; publish durable verified checkpoints.

Do not start trading, transfer funds, use real credentials in tests, force-push,
rewrite history, delete releases or overwrite stable artifacts.
