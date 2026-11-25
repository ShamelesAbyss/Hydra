# Project Leviathan implementation plan

Development preserves the repository history, MIT license, Rust engine, terminal
interface and Hyperliquid mainnet integration. Shipping defaults remain disarmed.
No development milestone may submit live exchange orders.

## Current balanced delivery authority

[MILESTONE_MATRIX.md](MILESTONE_MATRIX.md) is the current acceptance matrix and
prioritized backlog. It separates live-certification gates from independent work.
Alpha.11–20 concentrated on execution evidence; resume HUNTER/cockpit, then
ORACLE/GENESIS validation and FANG/COGNITION/AEGIS independent capabilities.
The chronological records below are history, not an instruction to keep refining
NEXUS before other heads can advance. No major milestone is fully certified.

## Historical delivery status at alpha.5

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

## Next request-provenance boundary after alpha.13

Persist the actual native unsigned wire action before signing/network write in
the engine submission path. Retain canonical cloid, asset, exact wire price/size,
side/reduce-only/type and canonical encoded action bytes or digest, bound to the
submitted journal/account. Database failure must prevent submission. Do not store
private keys or signatures, or equate unsigned action provenance with complete
signed nonce evidence. Preserve current submission semantics and independent holds.

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

Alpha.15 completes the bounded read-only action inspector. Next: link acknowledged fill bounds to retained unsigned action evidence, then nonce/signing-boundary evidence and conservative recovery integration.

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

## Acceptance increment delivered in alpha.16 — action / acknowledgment consistency

Use the same SQLite transaction to read any retained action alongside native
acknowledgment facts. Require exact quantity, side-aware limit, precision and
account/client identity agreement. Reuse the bounded decoder rather than opening a
second database snapshot. Existing action records that are corrupt must fail;
pre-alpha.14 receipts with no action record remain explicitly historical, without
invented pre-write provenance. Validate receipt persistence, restart reads and
ledger/reduction paths against overfill, limit, precision, scope and corruption
fixtures. No automatic repair, hold release or complete settlement is implied.

Signing nonce/envelope evidence remains a later increment. Continue offline; no
real account requests, signed mutations or transfers are part of verification.

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

## Signing-preparation acceptance defined after alpha.16

At alpha.16 the signer used a process-local counter. The acceptance criteria below
led to the coordinator in alpha.17, engine integration in alpha.18 and bounded
inspection in alpha.19. Full signed-envelope proof remains outside those deliveries.

The signed-path acceptance boundaries were:

1. Coordinate nonce identity by canonical public signer, which can differ from the
   bound trading account for approved agents. Define durable coordination across
   processes, restarts and different account-database paths; account locking alone
   must not be assumed to provide signer-wide nonce uniqueness.
2. Retain account, public signer, selected nonce, exact action digest and supported
   mainnet domain/vault/expiry scope before signing or network write. Preparation
   must refer to the unchanged retained action through the opaque journal capability.
   Never store keys or emit signature/payload logs. Do not add automatic replay.
3. Preserve bounded clock-drift/overflow refusal, fixed mainnet IOC-only transport,
   no redirects/retries and existing unresolved-order gates. A committed preparation
   is explicitly not proof of signing, sending or venue acceptance. Full envelope
   attestation remains separate unless independently designed and verified.
4. Test multiple connections/processes/database paths, shared and distinct signers,
   restart, clock rollback, overflow and persistence faults. Use pure preparation
   fixtures and simulated senders; verify committed nonce/action consistency before
   any simulated mutation. Keep the eight official cross-language fixtures passing.
5. Preserve historical absence and all execution/risk/financial holds. Add truthful
   read-only inspection only after the underlying provenance contract is implemented.
   Do not activate live verification, signed account probes or real orders.

The coordinator, ordered preparation and local inspection are now implemented and
verified as described in the alpha.17–19 sections below. They do not establish full
signed-envelope attestation, complete financial recovery or power-loss certification.

## Durable signer-nonce coordinator foundation — alpha.17

`signer_nonce::NonceCoordinator` reserves a nonce per canonical public signer in a
separate operator-owned SQLite database. It uses a five-second busy timeout,
IMMEDIATE transactions, FULL synchronization, bounded canonical u64 history and
an identified/versioned ordinary schema. Time is sampled after acquiring the
write transaction. Returned reservations remain consumed if later work fails;
clock rollback advances from retained history within the existing 1,000 ms future
window. Overflow, corruption, incompatible schema/journal mode and file/commit
failure refuse allocation without an in-memory fallback or sequence reset.

The default path is `Hydra/signer-nonces/nonces.sqlite` beside account-locks under
LOCALAPPDATA on Windows or the existing XDG/user state root on other platforms.
Cooperating processes must share and preserve this root; distinct roots, users,
hosts, deletion/replacement and storage tampering are outside the coordination
contract. Unix directory/file modes are 0700/0600; Windows inherits operator-directory
access controls. It stores public signer identities and nonces, never credentials.
The API is synchronous; future async integration must account for blocking waits.

This milestone delivers the Rust coordinator foundation, not live integration.
The native sender still uses its original process-local counter. Signing nonce
preparation, envelope provenance, automatic recovery and net settlement remain
incomplete. Existing live defaults, signed transport and independent holds are
unchanged. Verification uses offline fixtures and separate local child processes.

## Durable nonce / engine signing-preparation integration — alpha.18

All native IOC sender calls now derive the public signer locally and reserve a
nonce through the shared operator coordinator on a Tokio blocking worker. The old
process-local counter is removed. The platform engine additionally commits an
immutable `execution_signing_preparations` row before signing or HTTP construction:
account, public signer, canonical nonce, exact action hash, mainnet Agent digest,
chain 1337/source a, absent vault/expiry, original preparation time and phase
`prepared_not_sent`. No key, signature or signed authorization payload is stored.

The capability requires the unchanged submitted intent, current account binding
and an exact match to the retained unsigned action. Duplicate intent or signer/nonce
rows, scope/byte conflicts and failed/ignored/rewritten persistence stop submission.
The shared nonce transaction commits first; failed local preparation burns that
nonce rather than rolling it back or making it reusable. This is ordered durable
preparation, not a distributed transaction across the two databases.

Native sends require writable shared operator state at the alpha.17 coordinator
path, including direct low-level callers. Only the engine capability guarantees
local action/preparation journaling; direct callers still own their order journal.
Preserve one shared root across cooperating clients and account database paths.
Existing upstream account/agent checks and fixed mainnet IOC/no-redirect/no-retry
transport remain. No risk snapshot is refreshed by this record.

Preparation is not proof of signing, sending, acceptance or financial settlement.
Historical absence, unresolved execution/risk/financial holds and unsigned release
classification remain. The CLI still inspects unsigned action history; bounded
preparation inspection is the next increment. Verification uses invalid-key native
fixtures, pure metadata checks and simulated senders, never signed network requests.

## Bounded signing-preparation inspection — alpha.19

`wire-history` now emits protocol 2, adding explicit preparation-table availability
and optional retained metadata. Status-history protocol 1 and core SQLite schema 2
are unchanged. One read-only transaction validates indexed account/signer/nonce,
canonical values, supported mainnet domain and exact hashes recomputed from the
retained unsigned MessagePack. Selected preparation JSON is capped at 2 KiB; fields
and table/primary-key shape are bounded/checked before decoding. No global nonce
coordinator, wallet, trading configuration, engine migration or venue is opened.

Original preparation times and the immutable `prepared_not_sent` capture-event
phase are preserved. This phase is not an assertion that the current order remains
unsent. Output keeps live-source, submission/acceptance-proof and signed-envelope
flags false. Missing tables and absent records are distinct historical limits;
orphan, corrupt or contradictory retained preparation fails rather than becoming
unknown legacy evidence. Local consistency is not database attestation.

Native exact-receipt persistence and later fact reads reuse this validator in the
caller's transaction. Corrupt retained preparation therefore also refuses native
acknowledgment/position/ledger/residual evidence paths without clearing any hold.
Older records with no preparation remain explicitly historical. No automatic
repair, full signed-envelope proof or complete financial settlement is introduced.

The permanent memorial dedication remains verbatim at the end of README.md.
Publication checks and portable packages include its project maintenance rule.

## Current next acceptance work after alpha.20

Prioritize conservative recovery admission from consistent local execution evidence:
classify missing, ambiguous, rejected, resting and filled evidence without guessing
net settlement or clearing independent holds. Require original scope/identity,
exact acknowledged quantity, retained action/preparation consistency where available,
and explicit historical coverage limits. Keep venue observations separate from local
preparation; neither unsigned bytes nor a prepared nonce proves sending/acceptance.

Full signed-envelope attestation, verified fee/funding/entry attribution, broader
startup/stream recovery and protected credential storage remain open. Implement
bounded pure classification and fault fixtures before any automatic state mutation;
continue development verification without real account probes or signed requests.

## One-snapshot local intent evidence — alpha.20

`hydra intent-evidence --db PATH --intent UUID --limit N` emits an inspection-only
protocol-1 report. A single existing read-only SQLite transaction supplies lifecycle
history, unsigned action, signing preparation and optional exact native acknowledged
quantity. Shared readers preserve status-history protocol 1 and wire-history
protocol 2. Account, identity and journal state must agree. Original times remain;
no wallet, engine migration, coordinator or venue is initialized.

Lifecycle/action/preparation/native-fact table availability is explicit. Native
quantity is null when exact receipt facts are absent, never guessed as zero or
reconstructed from floating-point legacy values. History/record caps remain visible.
The report always leaves exchange completeness, automatic repair authorization,
risk release authorization and financial settlement verification false. One local
snapshot is not an atomic exchange snapshot or current account evidence.

Native fact metadata is now bounded in SQL before materialization; unsupported
views or missing native primary keys fail rather than becoming legacy precision.
Lifecycle scanning also rejects shadowed rowid aliases and requires SQLite's
physical indexed row identifier for its bounded window. Corruption remains an
error and never clears execution, risk or financial holds. The report provides
local review evidence, not a recovery classifier or automatic state mutation.

The permanent memorial remains verbatim as the final README section and is retained
with its maintenance rule in future portable/source packages.

## Recovery-classification design notes (planned after alpha.20)

Public API documentation was checked without making account queries. It distinguishes
trading-account addresses from agent addresses, documents a missing-order result,
and limits retained time-based fill history to the most recent 10,000 fills with
up to 2,000 per response. The fill example also describes the reported fee as already
including builder fees. These limits do not establish complete lifetime coverage.
[Primary API reference](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint).

Conservative classifier policy must keep missing/unrecognized observations unresolved,
never equate cancellation with proof of zero executions, and never double count
builder fees. Match scoped client/order identities and exact known quantities;
retain partial/contradictory/stale histories as review requirements. This is planned
policy, not a new mutation path or certification. No archival report may independently
authorize replay, financial settlement or release of execution/risk/financial holds.
