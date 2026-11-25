# HYDRA — Project Leviathan engineering handoff

## Recovery identity

- Version and current recovery tag: **v1.7.0-alpha.16** on `main`.
- Exact release commit: `git rev-parse v1.7.0-alpha.16^{commit}`.
- Previous recovery: v1.7.0-alpha.15 (`bdbae818f385d852684617acddc63a77733c2f54`).
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
- Alpha.16 source: 330 offline desktop-feature tests pass; alpha.15 contains 323.
  Release artifacts and tagged CI evidence are tracked separately.

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
7. The alpha.16 lockfile audit reports zero vulnerability entries and one Chrono
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

## Authoritative repository

Project Leviathan is Hydra. Main is the only active development branch. The retired archive/dead-code-hydra-1.6.2 branch is for historical preservation only. Resume at the post-alpha.8 exact residual-proof bridge; complete financial settlement remains open.

## Exact residual bridge preserved during consolidation

The new main source retains the native single-clearinghouse position snapshot
and transactional Decimal residual bridge. Exact current/close facts must agree
with post-receipt scoped quantity and entry price. Complete zero snapshots prove
full closure; legacy or previously unknown precision is never reconstructed.
Archived evidence, current precision, position reduction and journal application
commit together. Net settlement remains unknown and independently blocks entry.
The full offline desktop-feature suite passes 267 tests. Next: runtime-specific
bridge regressions, further exact arithmetic and complete financial settlement.

## Alpha.9 runtime residual checkpoint

All 271 offline desktop-feature tests pass, including direct runtime partial/full
confirmation, unavailable and contradictory post-receipt proof, no resend,
short-side exact quantities and hidden exact over-reduction. Native protective
proof uses one bounded position snapshot with no equity/mark/open-order dependency.
Financial settlement and reduced-position reservation release remain independent
open work. Main is the sole active branch; do not create additional branches.

## Alpha.10 native receipt identity checkpoint

All 277 offline desktop-feature tests pass. Each scoped native aggregate order
identity belongs to one intent; competing or repeated claims retain a submitted
hold rather than a second filled fact. Existing facts must agree with bounded
durable intent and result JSON before they can seed or authorize exact position
precision. Historical duplicate identity migration refuses without deleting or
rewriting any records. Individual ledger fills remain separately deduplicated.

Continue directly on main with authoritative order-status/replay recovery and
complete fee/funding coverage/conventions. These local consistency guards do not
establish cryptographic database attestation or complete net settlement.

## Alpha.11 lifecycle provenance checkpoint

All 287 offline desktop-feature tests pass. The Rust API acquires bounded native
order-status observations and retains selected identity/lifecycle evidence in an
append-only application path. Existing exact replays preserve timestamps; newly
stale evidence and conflicting identity reuse are refused. No observation changes
an intent, creates a guessed fill or releases risk/financial holds.

Next: operator-facing bounded lifecycle inspection and authoritative recovery
proofs, persistent exact signed-wire request provenance and complete fee/funding
coverage. The new observation API is not an automatic recovery loop or a new CLI.
Continue directly on main; do not create additional branches.

## Alpha.12 offline inspector checkpoint

All 295 offline desktop-feature tests pass. The CLI/Rust `status-history` inspector
reads an existing bound history in one read-only transaction before trading
configuration or engine initialization. It performs no migration or network call,
retains original times, validates selected provenance and labels incomplete
windows explicitly. Missing or corrupt evidence never becomes financial proof.

Continue directly on main with operator-facing live observation, exact signed-wire
request provenance and authoritative recovery/settlement. Native GUI lifecycle
inspection remains planned; the existing interface is unchanged in this milestone.

## Alpha.13 acknowledgment/ledger coverage checkpoint

All 304 offline desktop-feature tests pass. A known exact native acknowledgment
can anchor complete quantity coverage in the scoped ledger, read in one consistent
SQLite transaction. Partial fills remain explicitly partial; excess/mismatched
facts fail. Native reduction verification preserves exact quantity rather than
reconstructing it from a legacy float. No round-trip settlement or risk/financial
hold is cleared. Unknown costs, foreign-token fees and funding gaps remain explicit.

Next: actual unsigned wire-action provenance before native submission, then
broader authoritative recovery and complete funding/entry attribution. Exact
signed-wire/nonce provenance, secure credentials and stable-live certification
remain separate open requirements. Continue directly on main.

## Native unsigned-action pre-write provenance — alpha.14

Engine submission now supplies an opaque journal capability to the native adapter.
After validating the unchanged durable submitted intent and account binding, the
adapter commits the actual rounded IOC action and canonical MessagePack bytes to
`execution_wire_requests` before the signing/network-write boundary. Stored facts
include asset, side, price, quantity, reduce-only, client ID, IOC/grouping and size
precision. No keys or signatures are stored. Persistence or scope failure prevents
this native submission; a recorded intent cannot overwrite history or pass that
boundary a second time. Competing connections can record only one boundary.

This is unsigned pre-submission evidence, not proof of sending or acceptance.
Nonce/full signed-envelope provenance, automatic ambiguous-order recovery and
complete net settlement remain open. Direct low-level adapter callers retain
responsibility for journaling; custom adapters keep their compatibility path.
Older intents receive no fabricated wire history. Risk snapshots are not refreshed
by this record, and independent execution, reservation and financial holds remain.

## Offline unsigned-action inspection — alpha.15

`hydra wire-history --db PATH --intent UUID` prints protocol-1 JSON from an existing
read-only SQLite transaction. It bypasses trading configuration, wallet, logging
and engine migrations. Reads use a 500 ms busy timeout, ordinary-table/schema
checks, a required single-column action primary key, and bounded selected fields
(4 KiB action JSON, 2 KiB encoded bytes, 32 KiB journal). Account, client identity,
side, reduce-only, quantity and metadata precision must agree; displayed fields
must reproduce the exact canonical MessagePack bytes. Unknown fields are refused.

Original timestamps are preserved. Missing table and absent record are distinct;
neither proves no submission occurred. Output explicitly marks the source offline,
signed-envelope unavailable and submission/acceptance unproven. This validates local
consistency, not database attestation or current venue state. No hold is released,
no accounting result is inferred, and older history is never fabricated.

## Published alpha.15 verification and continuation

Commit `bdbae818f385d852684617acddc63a77733c2f54` is on authoritative main
and tagged v1.7.0-alpha.15. All four exact-source CI jobs pass in run 45648387540.
Windows CLI/cockpit and Linux CLI, source and both checksum manifests are published;
all five GitHub asset digests match. The local suite passes 323 offline tests.
GitHub intermittently returned 5xx after writes; remote state/digests were checked
before retries and no published assets or tags were replaced. The live default
README renders all 20 images. No branch was created.

Continue with retained-action/native-acknowledgment consistency in one transaction,
then nonce/envelope provenance and conservative recovery. Preserve every unresolved
execution, risk and financial hold and all historical release checkpoints.

## Retained action / native acknowledgment consistency — alpha.16

Native exact receipt persistence now validates any retained unsigned action in the
same transaction before writing facts or changing the submitted journal. Later
native fact reads repeat these checks in their caller's transaction, covering
position precision, ledger acknowledgment coverage and exact residual paths.
The shared bounded decoder checks account/client/side/reduce-only and canonical
MessagePack consistency; fill validation requires exact quantity no greater than
the rounded request, permitted size precision and a side-aware average-price limit.

Failure leaves pre-existing intent/facts and independent holds unresolved. A corrupt
retained record or incompatible action table cannot silently become legacy evidence.
Older acknowledgments with no retained action remain readable as historical facts;
missing action history is not fabricated or relabelled as pre-write provenance.
No signed-envelope/nonce proof, automatic repair, risk release, complete settlement
or live readiness is established. Verification is offline.

## Published alpha.16 verification and continuation

Commit `0a86095bc147e3c10bf2841de2b72ac2635f1b4b` is on main and tagged
v1.7.0-alpha.16. All four tagged CI jobs pass in run 45650103078. Windows CLI/cockpit,
Linux CLI, source and both checksum manifests are published; all five remote asset
digests and download URLs verify. All 330 offline tests pass locally. The live
README renders 20 images without broken assets. No new branch was created.

Continue with the planned durable signing-preparation boundary below. Signing
nonce/full envelope provenance remains unimplemented; do not infer it from the
unsigned action or acknowledgment consistency checks. Keep independent holds.
