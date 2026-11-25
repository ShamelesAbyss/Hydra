# NEXUS account evidence and identity

The post-alpha.4 source checkpoint replaces the legacy SDK/ethers path with a
narrow native mainnet IOC order client. No live requests were used to test it.
Eight offline conformance tests exercise the native signer against published
official Python SDK hashes/signatures. Existing alpha.4 release binaries retain
the older SDK until a separately verified replacement release is published.

## Identity and account modes

Local address parsing is canonical and rejects zero/path-like identities. Before
submission, userRole must identify the master as a user; a separate signer must be
an agent whose owner is that exact account. Vault/subaccount variants are currently
refused. Identity evidence is refreshed after at most 30 seconds.

Account abstraction is queried explicitly. Only `disabled` (standard) is currently
supported for sizing. Standard perp accountValue is used alone; spot stablecoins
are not added. Unified, portfolioMargin, default and discontinued abstraction modes
require distinct collateral models and currently produce an explicit refusal.

Sources: [official account modes](https://hyperliquid.gitbook.io/hyperliquid-docs/trading/account-abstraction-modes),
[official info endpoint](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint).

## FinancialLedger

An account/environment scope separates mainnet, paper and replay records. Fill tid
and funding hash/time/coin identify economic events. Decimal values retain actual
precision. Duplicate REST/WebSocket observations do not apply cash flow twice.
Conflicting known facts abort the entire batch; previously unavailable fields can
be enriched without replacing known values. A fixture cannot enter a mainnet scope.

`realized_cash_flow` streams account facts in a consistent read transaction. Missing
fees/funding/conversions or unverified closedPnl convention leave net unavailable.
USDT is not silently treated as USDC. Builder fees are not subtracted twice.
`order_evidence` matches actual oid/cloid against a separately bound intent and
rejects excess fills, conflicting IDs and instrument/side mismatches.

## Operator read-only synchronization

```text
hydra sync-ledger --since-ms <inclusive-start> --end-ms <inclusive-end>
```

Requires the explicit HYPERLIQUID_ACCOUNT_ADDRESS. Retrieves fill/funding info only;
no order submission, cancellation, signature or transfer is performed. Pagination
overlaps timestamp boundaries, rejects saturated same-time or unsorted pages and
has fixed page/record limits. Evidence committed before interruption is replayable.
Exchange retention may omit older data; retrieval is not a full-history certificate.

This command reports account evidence, not a settled strategy trade. It does not
clear unknown acknowledgments, net-PnL holds or Aegis reservations. Complete entry
and exit fee attribution, funding coverage and snapshot/order reconciliation are
required before those restrictions can safely be removed.

## Ownership and security

OS file locks coordinate each trading account independently of its database path.
Live engine startup and emergency flatten compete for the same user-level lock;
stop the existing controller before using another. Lock files are never unlinked
while owned. Locks do not constrain other machines or exchange clients.

The native signed transport sends only one IOC order at a time to the fixed
mainnet exchange endpoint. It owns a dedicated HTTP client with redirects and
automatic retries disabled, a 15-second timeout, and a 1 MiB response bound.
Transport, HTTP and decoding failures remain errors for the durable unresolved
execution gate. No transfer, withdrawal, vault, spot or account-management mutation
API is exposed. Signing uses maintained secp256k1/libsecp256k1 primitives and
Keccak-256; the ordered action and EIP-712 encoding are checked against independent
fixtures. See [dependency assessment](DEPENDENCY_AUDIT.md).

Signed payloads and signatures are never logged by the replacement. The defensive
legacy SDK debug/trace metadata filter remains regression-tested. Private key
fields are redacted from AppConfig Debug output. Secure OS credential storage
remains open work. A passing dependency audit is not live-trading certification.

## Database account ownership

Live startup binds an empty engine database to one trading account. A bound database rejects another account. Existing unbound history requires an explicit operator assertion through bind-history --db <path> --account <address> --adopt-existing-history. Adoption records its origin in metadata and preserves existing records. It does not settle uncertain financial outcomes.

## Scoped account evidence and reservation lifecycle

The default-perp standard-account evidence reader retains canonical account scope,
clearinghouse server time, exact signed quantities/entry prices and `positionValue`
notionals from the same clearinghouse response. Outstanding order IDs and
conservative entry-order valuations are retained separately. Acquisition has a
15-second total timeout; decoded info responses are stream-capped at 4 MiB before
JSON parsing. Diagnostic previews are redacted and bounded on character boundaries.

New entries may reconcile only definitely rejected zero-fill entries or known,
untouched applied positions. Aegis rechecks the local journal/position proof under
one SQLite transaction and archives the complete validated evidence once per
batch. It never clears unknown net accounting or ambiguous acknowledgments.
REST reads are not an atomic exchange snapshot, and wall-clock revision regression
retains holds. See RISK_MANAGEMENT.md for the accepted and held categories.

Only ordinary non-trigger limit entry orders are valued automatically; unsupported
contingent entry types are refused. The default-perp endpoint may include spot
orders; unsupported symbols are refused rather than silently omitted. The
[official info schema](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint)
is the protocol reference for the retained order-type/trigger fields.

## Exact execution facts — alpha.8

Native filled receipts preserve original decimal quantity, average price and
order identity in `execution_fill_facts`, scoped to the bound mainnet account.
The native production parser validates these against actual signed wire bounds.
Facts and receipt journal status commit together; failed persistence leaves a
submitted hold. `position_precision_state` captures new entry provenance in the
same transaction as position metadata. Aegis ownership uses exact values when
present, while legacy positions retain conservative compatibility checks.

The strategy/core position view still uses floating point. Existing reductions
atomically set current exact quantity to NULL rather than invent an exact
residual; original receipt facts remain available. A recorded unknown or invalid
precision state never falls back to legacy inference. Exact residual proofs and
complete net fee/funding settlement remain separate development work.

## Exact native residual proof — alpha.9

Native protective confirmation uses one bounded default-perpetual clearinghouse
read with account/environment identity, exact signed quantity/entry price and
exchange/acquisition times. It does not require unrelated equity, mark-price or
open-order endpoints. Duplicate, malformed, oversized, stale or wrong-scope
snapshots cannot prove closure.

When exact current and native close facts exist, checked Decimal subtraction
must agree with the post-receipt remote residual and original entry price.
A complete snapshot with no matching position proves exact zero. The retained
`execution_residual_evidence`, current precision, confirmed reduction, position
view and journal application commit together. Failure preserves the known fill
as unresolved and never retries the exchange write. Legacy or already unknown
precision remains unknown. Unknown net settlement still independently holds new
exposure; reduced-position reservation release remains future work.

## Native receipt identity and consistency — alpha.10

A unique account/venue/environment/order-ID index prevents two intents from
claiming one native aggregate order acknowledgment. Individual ledger fills
remain separate records and may legitimately share an order ID. Receipt/facts
persistence stays atomic; duplicate or competing claims leave unresolved
submitted intent evidence instead of a second filled fact.

Exact position facts are revalidated against bounded durable intent and result
JSON, including scope, accepted quantity, average price and order identity.
Missing, malformed, oversized or contradictory journal evidence cannot become
legacy precision fallback. These are local consistency checks, not cryptographic
attestation of arbitrary database contents or proof of complete settlement.
Migration refuses historical duplicate identities without deleting or rewriting
any records; authoritative audited reconciliation is required before continuing.

## Read-only native lifecycle observations — alpha.11

The Rust API can query order status by canonical client order identity through
`VenueAdapter::order_status_observation`. One read has a 15-second deadline and
64 KiB decoded response cap, enforced before JSON allocation. Parsed selected
fields retain exact quantities/limit price and explicit acquisition/lifecycle
timestamps. Conditional orders, inconsistent scope, malformed values and
contradictory missing-order responses cannot form native observations.

`order_status::record_observation` appends selected provenance transactionally
in `execution_status_observations`. A local observation identity cannot be reused
with different content. Exact stored replays are idempotent without refreshing
their original timestamps; new stale observations are refused. Lifecycle labels
and remaining quantity never invent execution price, fees/funding, filled quantity,
settled net PnL or a rejected intent. This API does not change execution state,
release reservations or bypass financial holds. There is no new CLI command,
automatic polling or repair loop in this milestone.

## Offline lifecycle inspection — alpha.12

`hydra status-history --db PATH --intent UUID --limit N` reads retained local
observations without venue access. Dispatch precedes trading configuration,
logging and engine storage initialization. The read-only SQLite transaction never
creates a missing database, runs migrations, changes account binding or resolves
an order. It validates ordinary table shape, bound identity, scoped journal and
selected observation provenance; incompatible or corrupt evidence is refused.

The protocol-1 JSON explicitly sets `read_only=true`, `source_is_live=false` and
`exchange_history_complete=false`. Inspection covers at most the newest 10,000
global observation row IDs and returns at most 64 matching records. Window and
record caps are explicit; no returned rows does not prove no venue order exists.
Original acquisition/record times are preserved, never refreshed into live proof.
Missing observation tables are marked unavailable. This is a CLI/Rust inspector;
a dedicated GUI panel, live-observation CLI and automatic recovery remain future work.

## Acknowledged-fill quantity coverage — alpha.13

`FinancialLedger::acknowledged_order_evidence` requires exact quantity coverage
of a known native acknowledgment, matching account/environment scope and current
binding. Journal, native facts and selected ledger fills share one SQLite read
transaction. Native reduction verification uses the original exact quantity plus
a checked legacy projection; legacy-only records keep conservative compatibility
checks and are never relabelled as native acknowledgments.

Partial sets remain available through `order_evidence`, but excess acknowledged
quantity or mismatched payload/index provenance fails. Indexed cloid/order-ID
selection avoids duplicate matches and unbounded sorting; selection is capped at
10,000 fills, 32 KiB per payload and 16 MiB combined payload bytes.

This is a Rust ledger API, not a new financial-repair command. Returned coverage
explicitly leaves round-trip settlement unverified. Missing or foreign-token fees
remain unknown in USDC; rebates remain signed and explicit zero fees remain known.
Unverified PnL conventions and funding/entry-cost gaps do not become invented net
profit. No API here changes execution state or releases risk/financial holds.

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
