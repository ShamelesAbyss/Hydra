# HYDRA — Project Leviathan engineering handoff

## Recovery identity

- Version and current recovery tag: **v1.7.0-alpha.8** on `leviathan/execution-foundation`.
- Exact release commit: `git rev-parse v1.7.0-alpha.8^{commit}`.
- Previous recovery: v1.7.0-alpha.7 (`dfb2fe6ef0c297c5ec12263707657b01e2bdf55a`).
- Last verified stable release: none; alpha work is not certified for unattended trading.
- Current phase: exact native execution facts, complete financial settlement and broader recovery; native cockpit, bounded research, scoped reconciliation and atomic entry admission are implemented.
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
- Dependency modernization: baseline 39 vulnerability entries / 20 warnings, then 19 / 17, now 0 vulnerability entries / 1 Chrono maintenance warning after native signing/parent upgrades.
- Transactional core schema version 2; ALTER failures and critical pragmas propagate.
- Alpha.8 source: 255 offline desktop-feature tests pass; alpha.7 contains 240.
  Release artifact and tagged CI evidence are tracked separately.

## Important remaining limits

This is still an engineering prerelease, not a complete Leviathan or stable-live build.

1. Raw REST closedPnl fee convention is deliberately unverified. Entry fees, funding
   history coverage and round-trip attribution must be proven before settling trades.
   The alpha.1 NULL-net-PnL reduction restriction remains; no bypass/reset was added.
2. Aegis reservations remain active until explicit authoritative reconciliation;
   automatic release is limited to the scoped proofs described below; policy migration remains incomplete.
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
7. The alpha.7 lockfile audit reports zero vulnerability entries and one Chrono
   maintenance warning. This is not a complete security audit. Protected Windows
   credential storage is still pending.
8. Native GUI, read-only telemetry, deterministic Oracle research and bounded
   Genesis search are implemented. Complete fixed-point position arithmetic,
   authoritative residual recovery and financial settlement remain incomplete.

## Architecture and next actions

Keep Rust, headless, Ratatui and Hyperliquid mainnet integration. Never activate
trading during development. Keep signing isolated from graphical/research clients.
Read HYPERLIQUID_INTEGRATION.md, RISK_MANAGEMENT.md and DEPENDENCY_AUDIT.md.

Continue immediately after publication: prove exact post-reduction residuals,
then complete financial coverage/conventions and broader replay. Native cockpit,
bounded research and native signing foundations are implemented. Preserve
financial uncertainty until genuine reconciliation proves settlement. Maintain
release artifacts/checksums; never force-push or fabricate results.

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

## Scoped reconciliation constraints — implemented after alpha.5

Design review identified two proof gaps before automatic release:
AccountSnapshot currently omits account/venue/environment identity and individual
position/order identities, and remote positions may come from a different
clearinghouse read than aggregate risk exposure. Reservation insertion also
precedes journal insertion, so orphan reservations must remain held.

The implemented scoped evidence bundle retains positions and exposure
from one clearinghouse payload. Releases are limited to for definite
zero-fill rejected entries, or known applied entries whose receipt, unique entry
metadata and exact owned remote position agree. Ambiguous/resting/missing-journal,
reduced and closed cases remain held. Keep financial-settlement gates separate.
No timeout, missing row or guessed zero may release exposure.

## Scoped reservation reconciliation checkpoint after alpha.5

Implemented the narrow bridge described above. Fresh account/mainnet/default-perp
bundles preserve exact clearinghouse position notionals and server time, with
separate bounded open-order reads. Definite zero-fill rejections and unchanged
applied entry positions can replace reservations with current observed exposure.
Journal/position proof is re-read within the same immediate transaction that
archives batch evidence, updates holds and snapshots, records events and latches
any drawdown. Protective reductions bypass maintenance reads; new entries reuse
the validated bundle. Ambiguous and net-settlement holds remain independent.

The full desktop-feature all-target suite passes 228 offline tests. Added coverage
includes the causal/account/ownership proof boundaries, restart replay, immutable
batch evidence, rollback injection, original financial holds, bounded HTTP bodies
and runtime reuse/reduce-only bypass. No real exchange account or order access
was used. Orphan reservation insertion gaps, persistent ingestion sequence numbers,
complete settlement and broader recovery remain next work.

## Atomic admission checkpoint after alpha.6

New engine entries now commit the Aegis reservation and submitted intent together.
The authority/controller connection identity is checked during binding; every
entry uses a scoped account bundle. Account ownership, pending same-market orders,
unknown execution/financial state and conservative bounded notional are rechecked
inside the same admission transaction. Failed journal/reservation writes leave no
partial pair; risk denials preserve breaker state without creating an intent.

All 240 offline desktop-feature tests pass, including twelve new admission/runtime
regressions, competing connections, restart persistence, duplicate terminal IDs,
independent financial holds and database-failure injection. Formatting and Clippy
pass with warnings. Historical orphan holds remain untouched. Fixed-point core
position facts, persistent snapshot sequence numbers and complete settlement are
next priorities; no real account or signed request was used in verification.

## Exact native execution facts checkpoint after alpha.7

Native filled acknowledgments retain quantity, average price and order identity
as original decimal strings. The production parser checks exact values against
the actual signed order quantity, limit and instrument precision. Journal status
and scoped fill facts commit together; failed persistence leaves the submitted
intent unresolved. New position metadata captures these facts atomically and
Aegis uses them when checking current ownership.

Legacy positions receive no invented precision. A reduction through the existing
floating-point residual path explicitly invalidates current exact quantity in
the same transaction. Original receipt facts remain intact; incomplete residual
or net-settlement evidence continues to hold new exposure. This is an additive
facts layer, not a complete fixed-point core migration.

All 255 offline desktop-feature tests pass. Fifteen new regressions cover the
production parser, sub-float overfills/limits, persistence rollback, restart,
account scope, legacy fallback, contradictory provenance and reduction invalidation.
Formatting and Clippy complete with warnings. No real account, signed network
request or exchange order was used. Published binaries remain alpha.7 until the
next verified release.
