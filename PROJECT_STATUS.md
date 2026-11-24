# HYDRA — Project Leviathan engineering handoff

## Recovery identity

- Version and current recovery tag: **v1.7.0-alpha.2** on `leviathan/execution-foundation`.
- Exact release commit: `git rev-parse v1.7.0-alpha.2^{commit}`.
- Previous recovery: v1.7.0-alpha.1 (`141f67381eb95a2d1084ea44717cdff24fd043c6`).
- Last verified stable release: none; alpha work is not certified for unattended trading.
- Current phase: authoritative financial evidence and risk foundations, followed immediately by native cockpit/research implementation.
- Development verification remains offline; no real exchange orders, credential probes or transfers.

## Implemented since alpha.1

- Account/environment scoped decimal fill and funding tables, atomic deduplication,
  conflict rejection, enrichment of previously unknown facts, bounded provenance,
  streaming cash-flow reporting and restart-safe intent/account bindings.
- Pure bounded Hyperliquid fill/funding parsers and read-only `sync-ledger` retrieval.
- Persistent Aegis equity peaks/drawdown breakers, explicit audited rearm API,
  immutable risk policies and atomic exposure reservations across connections.
- Live engine entries use Aegis against fresh standard-mode positions/open orders.
  Reservations are not automatically expired or blindly released.
- Same-user/host account ownership using OS locks across different database paths.
- Agent-owner validation and explicit unsupported identity/margin-mode refusal.
- Standard perp equity excludes spot balances; stale/error equity cannot size entries.
- Independent log filter prevents SDK debug/trace signed-payload events; config
  Debug output redacts key material.
- Compatible dependency modernization: 39 to 19 vulnerability entries, 20 to 17 warnings.
- Transactional core schema version 2; ALTER failures and critical pragmas propagate.
- 162 offline tests pass, including cross-process locks, ledger replay, concurrency,
  logging, unknown costs, account identity and risk reservation tests.

## Important remaining limits

This is still an engineering prerelease, not a complete Leviathan or stable-live build.

1. Raw REST closedPnl fee convention is deliberately unverified. Entry fees, funding
   history coverage and round-trip attribution must be proven before settling trades.
   The alpha.1 NULL-net-PnL reduction restriction remains; no bypass/reset was added.
2. Aegis reservations remain active until explicit authoritative reconciliation;
   runtime automatic reservation release and policy migration are not complete.
   Config changes to an existing persistent policy fail visibly.
3. Effective exposure is bounded; instrument leverage setting, liquidation distance,
   native protective orders and full margin modeling remain incomplete.
4. Sequential REST snapshots are not atomic exchange snapshots. Full WebSocket
   resubscription/cursor/race integration and order-status recovery remain necessary.
5. Only explicit standard abstraction/master-user or approved-agent identity is
   supported for entry sizing. Unified/portfolio/default modes and vault/subaccounts
   fail closed. Do not switch account modes merely to bypass a readiness block.
6. Locks coordinate cooperating Hydra processes on one host/user, not other OS
   users, machines, manual exchange activity or third-party bots.
7. Nineteen dependency vulnerability entries remain, including signing/TLS SDK
   blockers. Protected Windows credential storage is still pending.
8. Native GUI, versioned read-only telemetry and deterministic Oracle research are
   the next active implementation milestones; source under development is not
   claimed as functionality in this recovery tag.

## Architecture and next actions

Keep Rust, headless, Ratatui and Hyperliquid mainnet integration. Never activate
trading during development. Keep signing isolated from graphical/research clients.
Read HYPERLIQUID_INTEGRATION.md, RISK_MANAGEMENT.md and DEPENDENCY_AUDIT.md.

Continue immediately after publication: finish optional native eframe cockpit with
read-only telemetry, GUI launch/visual checks and packaging; publish another verified
prerelease. Then integrate deterministic historical research and independent signing
fixtures before replacing the legacy signing stack. Preserve pending financial
uncertainty until genuine reconciliation proves settlement. Maintain release
artifacts/checksums and do not force-push, overwrite releases or fabricate results.
