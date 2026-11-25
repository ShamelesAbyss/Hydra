# Release verification — 1.7.0-alpha.19

All **360 offline desktop-feature all-target release-version tests pass**.
Nine added regressions cover byte-preserving protocol-2 preparation inspection,
missing table/record distinction without migration, invalid scope/domain/phase and
unknown fields, hash and nonce contradictions, indexed metadata/oversize refusal,
orphan records, views/unindexed schemas, native acknowledgment refusal on corrupt
preparation, and the real CLI's configuration/wallet bypass with retained metadata.
Original times and all no-live/no-signature/no-submission-proof flags are preserved.

Formatting and Clippy complete with existing warnings. Fresh lockfile audit exits
0 with zero vulnerability entries and one Chrono maintenance warning, no ignores.
Windows CLI/cockpit compilation and extracted smoke are checked before publication.
No real account probes, signed network requests, orders or transfers were used.
The permanent memorial dedication is verified as the exact final README section.
Alpha.18 passed all four tagged CI jobs and all five release asset digests match:
[Verified alpha.18 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45702690919).

# Release verification — 1.7.0-alpha.18

All **351 offline desktop-feature all-target release-version tests pass**.
New preparation tests cover supported domain/hash/public-signer metadata, absent
secret/signature fields, scope/action/intent conflicts, invalid identity/nonce,
immutable duplicate intent/signer-nonce refusal, restart and nonce consumption on
journal failure. Ignored/rewritten JSON or indexed metadata rolls back. Native
simulated senders verify preparation before their send counter and refuse nonce
history or preparation persistence faults. The old process-local allocator test
is replaced with a durable async-reservation test; coordinator coverage remains.

Formatting and Clippy complete with existing warnings. Fresh lockfile audit exits
0 with zero vulnerability entries and one Chrono maintenance warning, no ignores.
Windows CLI/cockpit compilation and extracted smoke are checked before publication.
No real account probes, signed network requests, orders or transfers were used.
Native fixtures retain invalid keys and simulated transport as independent guards.
Alpha.17 passed all four tagged CI jobs and all five release asset digests match:
[Verified alpha.17 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45700688152).

# Release verification — 1.7.0-alpha.17

All **341 offline desktop-feature all-target release-version tests pass**.
The new coordinator tests cover restart, rollback and bounded drift/overflow,
canonical signer aliases and independent signers, concurrent SQLite connections,
foreign/future/view/invalid-column/corrupt databases, file/clock/write failures and
ignored or rewritten updates. A process fixture plus parent regression exercises
four concurrent child processes with separate bound account databases, sharing
one signer coordinator, then reopens it and reserves a later value.

Formatting and Clippy complete with warnings. Fresh lockfile audit exits 0 with
zero vulnerability entries and one Chrono maintenance warning, without ignores.
Windows CLI/cockpit release compilation and extracted smoke are checked before
publication. The live nonce sender is unchanged in this foundation milestone.
No real account probes, signed network requests, orders or transfers were used.
Alpha.16 passed all four tagged CI jobs; its five asset digests and download links
were verified:
[Verified alpha.16 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45650103078).

# Release verification — 1.7.0-alpha.16

All **330 offline desktop-feature all-target release-version tests pass**.
Seven added regressions cover retained-action acknowledgment restart, rollback on
exact overfill/sub-float limit/precision failures, sell-side limit direction,
corrupt retained evidence on subsequent reads without fallback, historical absent
record/table compatibility, incompatible table refusal, and the caller's single
WAL snapshot across an external atomic action update. Synthetic SQL mutations model
corruption/concurrency; production ingestion does not rewrite retained evidence.

Formatting and Clippy complete with warnings. Fresh lockfile audit exits 0 with
zero vulnerability entries and one Chrono maintenance warning, without ignores.
Windows CLI/cockpit release compilation and extracted smoke are checked before
publication. No real accounts, signed network requests or orders were used.
Alpha.16 passed all four tagged CI jobs for commit
`0a86095bc147e3c10bf2841de2b72ac2635f1b4b`. All five published asset digests
match and all five download URLs pass HTTP checks. Extracted Windows CLI/cockpit
packages pass seven smoke checks; Linux bytes come from the verified tagged job.
[Verified alpha.16 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45650103078).

# Release verification — 1.7.0-alpha.15

All **323 offline desktop-feature all-target release-version tests pass**.
Nine added regressions cover byte-preserving action inspection and original times,
missing files/tables/records without creation or migration, incompatible schema,
views and missing primary key, corrupt scope/precision/bytes/fields, oversized
selected data and unknown intent, internally consistent intent contradictions,
and CLI bypass of missing configuration and invalid wallet environment.

Formatting and Clippy complete with warnings. Fresh audit exits 0 with zero
vulnerability entries and one Chrono maintenance warning, no ignores. Windows CLI
and cockpit release compilation and extracted portable smoke are checked before
publication. No real account probes, signed requests, orders or transfers were used.
Alpha.15 passed all four CI jobs for tagged source `bdbae818f385d852684617acddc63a77733c2f54`.
All five published release asset digests match the local archives/manifests.
[Verified alpha.15 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45648387540).

# Release verification — 1.7.0-alpha.14

All **314 offline desktop-feature all-target release-version tests pass**.
Ten additional regressions cover durable rounded action bytes and restart,
account/intent/side/client/quantity/precision refusal, changed or terminal journal,
insert failures and immutable duplicate protection, competing SQLite connections,
native simulated sender ordering and refusal, and controller journal dispatch.
The native sender tests use an invalid signing key and a test-only simulated
sender; they send no signed requests and do not certify live transport behavior.

Formatting and Clippy complete with warnings. Fresh lockfile audit exits 0 with
zero vulnerability entries and one Chrono maintenance warning, no ignores.
Windows CLI/native release compilation and extracted portable smoke are checked
before publication. No real account probes, orders or transfers were used.
Alpha.13 passed all four tagged CI jobs and all five published asset digests match:
[verified alpha.13 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45641564303).

# Release verification — 1.7.0-alpha.13

All **304 offline desktop-feature all-target release-version tests pass**.
Nine additional tests cover exact acknowledged quantity, cloid/OID overlap without
double counting, partial/excess fills, legacy precision distinction, native scope,
reduction projection, unknown/foreign/rebate/zero fees, corrupt provenance,
unchanged holds and restart. A deterministic two-connection WAL test proves one
snapshot across an external atomic journal/fill commit.

Formatting and Clippy complete with warnings. Fresh audit exits 0 with zero
vulnerability entries and one Chrono maintenance warning, no advisory ignores.
Both Windows CLI and native cockpit release binaries compile. Extracted portable
smoke is repeated during packaging.
No real account probes, signed network requests or exchange orders were used.
Synthetic REST-decoded test data preserves the existing prohibition against
fixture-source evidence entering a mainnet-scoped ledger.
Alpha.12 passed all four tagged CI jobs:
[verified alpha.12 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45636922784).

# Release verification — 1.7.0-alpha.12

All **295 offline desktop-feature all-target release-version tests pass**.
Eight new inspector regressions cover byte-preserving read-only snapshots,
missing files/tables without creation or migration, unknown binding/identity,
unsupported versions and views, corrupt selected provenance, explicit bounded
windows and records, and the CLI configuration/wallet bypass. The partial-binding
fixture preserves existing audit metadata; no repair bypass was introduced.

Fresh lockfile audit: zero vulnerability entries and one Chrono maintenance
warning, exit 0, without advisory ignores. Formatting and Clippy complete with
warnings. Both Windows CLI and native cockpit release binaries compile. Extracted portable
smoke is repeated during packaging.
No real accounts, signed network requests or exchange orders were used.
Alpha.11 passed all four tagged CI jobs:
[verified alpha.11 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45632847453).

# Release verification — 1.7.0-alpha.11

The complete release-version desktop-feature all-target suite passes **287
offline tests**. Ten new observation tests cover native client identity, selected
exact fields, opaque/missing lifecycle states, account/intent scope, conditional
order refusal, byte/time/decimal budgets, unchanged execution/financial holds,
restart-safe replay without timestamp refresh and persistence/conflict failures.
Existing Info transport tests also verify 64 KiB and 4 MiB declared/chunked caps;
production wire serialization agrees with the observation client-ID encoding.

Formatting and Clippy complete with warnings; the new enum-size warning was
addressed before this complete verified run. Fresh audit: zero vulnerability
entries and one Chrono maintenance warning, exit 0, no advisory ignores.
Both Windows CLI and native cockpit release binaries compile; extracted
portable smoke is repeated during packaging.
No real accounts, signed network requests or exchange orders were used. HTTP
fixtures are loopback only. Alpha.10 passed all four tagged CI jobs:
[verified alpha.10 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45587952292).

# Release verification — 1.7.0-alpha.10

The complete release-version desktop-feature all-target suite passes **277
offline tests**. Six new regressions cover native aggregate order identity reuse,
competing connections, lossless refusal of historical duplicate migration,
contradictory result payloads, changed durable intent scope and missing/oversized
journal evidence. The fresh lockfile audit exits 0 with zero vulnerability entries
and one Chrono maintenance warning, without advisory ignores.

No real account probes, signed network requests or exchange orders were used.
Both Windows CLI and native cockpit release binaries compile. Clippy completes
with warnings. Extracted portable smoke and tagged CI evidence are checked
separately before publication. Alpha.9 previously passed all four tagged jobs:
[verified alpha.9 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45585708718).

# Release verification — 1.7.0-alpha.9

The full release-version desktop-feature all-target suite passes **271 offline
tests**. The fresh lockfile audit exits 0 with zero vulnerability entries and one
Chrono maintenance warning. Sixteen new regressions beyond alpha.8 cover exact
position snapshot boundaries, residual proof, complete-zero closure, transaction
rollback, restart, runtime partial/full confirmation, failed proof/no resend,
short-side quantities and over-reduction below floating-point resolution.
No real account, signed network request or exchange order was used.

All four main consolidation CI jobs passed for source
`fbb52619a1328533062aa7fe0a8b3bf5b4312223`:
[verified main run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45581670769).
Release compilation encountered local disk exhaustion; generated build caches
were reclaimed and the release retry succeeds for both Windows CLI and native
cockpit binaries. Clippy completes with warnings. Main is the only active
development branch. Extracted portable smoke and tagged CI evidence are separate.

# Main consolidation and residual-proof verification

The complete desktop-feature all-target suite passes **267 offline tests**.
Twelve new parser/storage tests cover bounded exact position snapshots, complete
zero proofs, signed quantities, scope/time/precision contradictions, restart,
unknown legacy precision and atomic rollback at each persistence boundary.
No live account, signed request or exchange order was used.

Main promotion preserves all Leviathan and presentation commits. Main-specific
Windows/Linux/native desktop CI is tracked separately from the already verified
alpha.8 implementation and published binaries. Promotion is not stable-live
certification. Runtime-specific residual regression coverage remains next work.

# Release verification — 1.7.0-alpha.8

The release-version desktop-feature all-target suite passes all **255 offline
tests**. The fresh lockfile audit exits 0 with zero vulnerability entries and one
Chrono maintenance warning. All four implementation CI jobs passed for
`2b1560c16b8027e326bb687f43560eaaae4b699e`:
[GitHub run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45579083275).
Tagged release checks and artifact smoke results are tracked separately.

# Exact native facts checkpoint after alpha.7

All **255 offline desktop-feature all-target tests pass**. The new native parser
and fourteen integration regressions verify exact quantity/price/order facts,
wire bounds below floating-point resolution, atomic journal/facts persistence,
restart ownership, account/provenance checks, position rollback, explicit unknown
residuals and no retry after a failed facts write. Nonfinite optional prices are
rejected before JSON serialization. Formatting and Clippy complete with warnings.
No live exchange account, signed network request or order was used. This source
checkpoint does not change the already published alpha.7 binaries.

# Verification — 1.7.0-alpha.7

The atomic-admission source checkpoint passes 240 offline desktop-feature tests.
The release-version suite also passes all 240 tests; the fresh release audit
exits 0 with zero vulnerability entries and one Chrono maintenance warning. Both
Windows release binaries compile; archive smoke checks are repeated in packaging.
Formatting and Clippy pass with warnings. New coverage includes both insert
failure boundaries, restart, concurrency, duplicate IDs, scoped ownership,
conservative notional, financial holds and pre-network rollback.

# Atomic admission checkpoint after alpha.6

The full desktop-feature all-target suite passes **240 offline tests**. Twelve
additional tests verify atomic reservation/journal failure rollback, durable
restart state, duplicate IDs, competing connections, authority/connection binding,
scoped inputs, bounded notional, pending orders, retained financial holds and no
network send after journal failure. Formatting and Clippy complete with warnings.
No real account access, signed requests or exchange orders were used.

# Verification — 1.7.0-alpha.6

The scoped reconciliation implementation passes 228 offline desktop-feature
all-target tests, including archive rollback, exact scoped evidence, causal proof,
runtime reuse and independent settlement holds. The release-version suite also passes all 228 tests, and the fresh release audit
exits 0 with zero vulnerability entries and one Chrono maintenance warning.
Both Windows release executables compile and pass version checks; extracted
archive smoke verification is repeated during packaging. Formatting
and Clippy pass with existing warnings. No real accounts, signed requests or
orders are used; HTTP response-limit fixtures use loopback only.

# Scoped reconciliation checkpoint after alpha.5

The full desktop-feature all-target suite passes **228 offline tests**. Eighteen
additional tests cover scoped/exact account evidence, rejected/applied release
proofs, retained ambiguous/orphan/reduced/closed holds, causality/freshness, durable
batch evidence, restart replay, injected transaction failures, independent financial
settlement holds, runtime snapshot reuse/reduction bypass and HTTP byte/diagnostic
limits. Transport fixtures use loopback only; no account probes or signed requests.

# Verification — 1.7.0-alpha.5

The release-version full desktop-feature all-target suite passes **210 offline
tests**. The fresh release lockfile audit exits 0 with **0 vulnerability entries**
and **1 Chrono maintenance warning**, without advisory ignores. Formatting and
Clippy completed on the same code with existing warnings. Both Windows release executables compile and pass version/help smoke checks.
Portable-archive isolation checks are repeated during packaging. No signed network
requests, real accounts or orders were used.

# Cross-platform lock-lifetime regression

The full desktop-feature all-target suite passes **210 offline tests** after
adding deterministic duplicate-handle ownership coverage. Linux CI exposed an
intermittent close-only lock lifetime during concurrent child-process startup.
AccountOwnership now explicitly unlocks at its RAII guard boundary. The new test
keeps an old duplicate alive, verifies immediate reacquisition after guard drop,
and verifies that closing the old duplicate cannot release the new owner's lock.
A WSL/Linux flock reproduction independently confirms the operating-system
semantics. The actual Rust Linux CI result is checked separately.

# Native dependency modernization checkpoint

At `a941c68489325afa076f5d00bee0b43e2ea97312`, the full desktop-feature,
all-target suite passes **209 offline tests**. Formatting and Clippy pass with
existing style warnings. Eight official cross-language signing fixtures remain
passing on the maintained secp256k1 signer, including a comparison of the full
production order MessagePack bytes with an independent fixture encoder.

The fresh dependency audit exits 0 with **0 vulnerability entries** and **1
unmaintained Chrono warning**. No advisory is suppressed. The legacy SDK/ethers
paths are removed from the active graph; Ratatui and rust_decimal parent upgrades
remove the remaining affected transitive copies. See DEPENDENCY_AUDIT.md for
advisory IDs, lock digest, graph changes and verification scope.

These are checkout checks. Existing alpha.4 executable downloads precede this
fix. No mainnet account probes, signed requests or orders were used. Financial
settlement, broad recovery and live-readiness certification remain outstanding.

# Entry recovery checkpoint after 1.7.0-alpha.4

The full desktop-feature suite passes 205 offline tests. Nine additional tests
cover atomic entry metadata, original risk across restart and stop ratcheting,
legacy unknown metadata, rollback on write failure, intent identity checks,
known-receipt recovery exactly once, mismatched account/position rejection,
ambiguous acknowledgment retention, and pre-entry unowned-position rejection.
No exchange access or signed actions were used in these checks.

# Verification — 1.7.0-alpha.4

The full desktop-feature suite passes 196 offline tests. New coverage includes
seven Oracle tests, five Genesis tests, research CLI storage/credential isolation,
two research-worker tests, per-instrument exit failure isolation, database account
binding, and financial decimal underflow rejection. Formatting and Clippy pass
with documented pre-existing style warnings.

The focused financial review was static. Its formal scan plugin could not resolve
the valid Windows Git worktree, so no canonical plugin scan is claimed.
Review findings drove the account binding, exit isolation and decimal fixes.
The UTF-8 diagnostic preview correction addresses a pre-existing supporting-path
panic. Complete security and financial certification remain open.

The dependency inventory still contains 19 vulnerability entries and 17 warnings.
No real exchange orders, credential probes or transfers were used in verification.

# Verification — 1.7.0-alpha.3

Full desktop-feature suite passes 178 offline tests. Both binaries compile with the optional desktop feature. The cockpit was launched and remained responsive, and operator screenshot feedback confirmed rendering. Automated computer-use interaction failed with a kernel-assets error. The telemetry CLI and explicit history initialization tests pass without a trading configuration.

# Verification — 1.7.0-alpha.2

Windows MSVC, Rust/Cargo 1.94.0. Exact source: tag v1.7.0-alpha.2.
No exchange orders, real credential probes, transfers or mainnet development tests.

- Full offline source suite: **162 passed, zero failures**.
- Includes 17 decimal ledger integration tests, 7 wire parser tests, 12 Aegis tests,
  16 execution tests, 5 ownership/equity tests and preserved strategy/learning/TUI tests.
- Formatting, Clippy and optimized build are required before publication; Clippy
  is allowed to report existing style warnings, not represented as warning-free.
- Portable Windows executable is extracted and launch checked offline before upload.
- Dependency audit: **19 vulnerability entries and 17 warnings remain**, no suppressions.
  Baseline alpha.1 had 39 and 20. See DEPENDENCY_AUDIT.md for active graph distinctions.
- Linux and Windows remote CI results are recorded on the tagged-source run/release;
  no local Linux execution claim is made.
- Original alpha.1 suite: 115 passing tests; 1.6.2 baseline: 95.

New coverage includes OS cross-process locks, agent/account binding, unknown
collateral modes, stale equity invalidation, payload-log filtering, decimal replay,
conflicting identities, fee conventions, provenance, snapshot/WebSocket duplicate
observations, transaction rollback, pending reservations, concurrency, persistent
drawdown/rearm and timestamp-boundary pagination failures.

No profitability, performance improvement, complete financial recovery, GUI or
clean-security-audit claim is made for this tag. Unknown costs/conventions and
incomplete trade/funding coverage remain explicitly unresolved.
