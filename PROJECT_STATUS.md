# HYDRA — Project Leviathan engineering handoff

## Recovery identity

- Version and current recovery tag: **v1.7.0-alpha.4** on `leviathan/execution-foundation`.
- Exact release commit: `git rev-parse v1.7.0-alpha.4^{commit}`.
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

## Alpha.3 cockpit checkpoint

Implemented optional eframe 0.35 native Windows cockpit, bounded read-only telemetry schema version 1, independent background refresh, seven subsystem selectors, recorded portfolio/risk/ledger views and diagnostic filtering. The CLI telemetry command bypasses trading configuration and migrations. Explicit local history initialization preserves existing files and requires no exchange access.

The executable was launched and observed responsive. Operator screenshot feedback confirmed rendering. Automated visual interaction remains blocked by a computer-use kernel-assets error. Eight independent signing-reference tests passed against published official Python SDK constants. Full live recovery, financial settlement and the 19 remaining dependency vulnerability entries still need work. Oracle research implementation follows this release.

## Alpha.4 research and correctness checkpoint

Oracle and Genesis are wired through offline CLI commands and native workspaces. One bounded background research job runs at a time while telemetry remains independently refreshed. Actual reports drive equity and Pareto displays. Research configuration loading excludes wallet fields and engine storage setup.

Normal protective exits now continue after another instrument close fails. Live engine history is persistently account-bound, with explicit audited adoption required for unbound legacy history. Raw exchange decimals reject excess precision. Invalid UTF-8 response previews preserve character boundaries. These changes have focused regression coverage.

The focused financial code review identified and drove these fixes. The formal security plugin could not resolve the local Windows Git worktree, so a canonical plugin scan remains outstanding. Position entry-context persistence is the next active integration work.

## Entry recovery checkpoint after alpha.4

New positions now store immutable original risk distance and learning context in
one transaction with their durable entry-intent identity. Restarted positions
retain original risk even after their stop has ratcheted. Legacy positions keep
unknown original risk explicitly; their persisted protective stops remain active.

Startup can finish the narrow crash window between position persistence and
journal application only when a known fill receipt, account binding and fresh
remote position agree. Ambiguous acknowledgments remain unresolved. Pre-entry
validation also rejects unowned or inconsistent exchange positions before sending
an order. Recovery does not certify fees, funding or net PnL, and does not release
risk reservations automatically.

## Native dependency modernization after alpha.4

Checkpoint `a941c68489325afa076f5d00bee0b43e2ea97312` removes the active legacy
SDK/ethers graph. A narrow mainnet IOC-only client retains typed receipts,
canonical cloid, ordered MessagePack and independently tested EIP-712 signing.
Maintained libsecp256k1 bindings provide cryptographic signing. Signed transport
has a fixed endpoint, no redirect/retry, a 15-second timeout and bounded replies.

Targeted rust_decimal/Ratatui upgrades remove affected optional and TUI copies.
Fresh audit: 0 vulnerability entries, 1 Chrono maintenance warning, exit 0.
All 209 offline desktop-feature tests pass. This changes source; alpha.4 binaries
remain older until a new release is built and verified. Remaining development
priorities are financial settlement, evidence-driven reservation reconciliation,
broad recovery and the Chrono migration. No stable-live claim is made.

## Cross-platform ownership lifetime

Linux CI exposed an intermittent lock lifetime when a parallel child spawn
briefly inherits a descriptor. Guard disposal now explicitly unlocks before File
closes. A deterministic duplicate-handle regression preserves exclusivity while
owned and proves the next owner cannot be unlocked by a stale duplicate close.
The full offline desktop-feature suite now passes 210 tests. No lock file is
unlinked and no acquisition retry hides contention.
