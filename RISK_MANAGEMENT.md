# AEGIS persistent risk authority

Aegis has no dependency on strategy callbacks, learning scores or optimizer output.
It accepts explicit account facts and checked decimal limits. Current live-engine
integration is for supported standard-mode account snapshots; tests use offline facts.

Entry admission runs in an IMMEDIATE SQLite transaction. It checks account equity,
existing positions, outstanding entry orders and every unreconciled reservation.
Total and per-market exposure limits remain binding across threads/connections.
Overflow, corrupt state, stale/future observations and altered reused revisions fail
closed. The actual order intent is price-bounded before reservation/submission.

Equity peaks and drawdown breakers survive restart. A strategy cannot reset them.
`RearmAuthorization::explicit` requires operator identity and reason, with a separate
explicit peak-reset choice if the original drawdown still exists. Rearm is an API
contract; a complete secure operator UI is not yet shipped. Policy changes cannot
silently replace a persistent policy.

Reservations have no timeout expiry. `apply_reconciled_snapshot` requires a newer
account state and an explicit reconciliation identity before releasing named intents.
The runtime now automates a narrow terminal-entry proof before new entries:

- Definite rejected entries require zero fill, no order/fill price, matching
  canonical journal/reservation identity and no persisted position metadata.
- Applied entries require their unique original entry metadata, a matching known
  receipt, an untouched open position matching the same account evidence, and no
  outstanding order with that receipt identity.

The complete scoped account bundle is retained once per reconciliation batch in
`aegis_reconciliations`. Reservation changes, evidence archive, events, replacement
snapshot and any drawdown latch commit in one IMMEDIATE transaction. Newer
snapshots do not erase historical release evidence. Replay never duplicates a
release or overwrites an older proof.

Account bundles require canonical account/mainnet/default-perp/standard-mode scope,
exact decimals, bounded rows/bytes and a bounded acquisition window. Positions,
position notionals, equity and server time come from one clearinghouse response;
open orders and their conservative mark bounds require separate REST reads.
Non-reduce orders must be ordinary non-trigger limit orders. Contingent/unknown
entry types, unsupported spot/DEX symbols and inconsistent zero-quantity notionals
fail closed rather than understating exposure.
These reads are not an atomic exchange snapshot. Evidence must postdate the
journal outcome locally and at the exchange. The entire acquisition has a
15-second deadline. The validated bundle is reused for entry reservation, avoiding
an immediate second account read.

Submitted, unknown, resting, unapplied, orphan, legacy-metadata, reduced and closed
entries remain held. Conflicts and missing evidence do not authorize release.
Unknown net settlement still blocks new exposure independently. The
reserve-before-journal crash gap remains open; orphan reservations cannot be
removed automatically. Do not delete holds to make trading resume.

Protective reductions bypass ordinary entry exposure/breaker checks, but still
require live authorization, reduce-only semantics and confirmed position/fill
evidence. Unknown prior writes on the same instrument still block duplicate closes.
Account-wide emergency flatten is limited to supported locally known positions;
unknown positions and open-order cancellation need further reconciliation work.

Effective portfolio exposure is controlled; this is not a complete liquidation or
instrument margin model. Instrument leverage settings, liquidation-distance checks,
native protective stops, full loss attribution and streaming race recovery remain
required before stable-live certification. Risk controls are not loss guarantees.

## Atomic live-entry admission after alpha.6

The live execution controller now uses `reserve_journaled_entry`: risk admission
and the submitted execution intent commit together in one IMMEDIATE transaction.
Journal or reservation write failure rolls back both, including tentative snapshot
updates. A denied entry still commits its drawdown latch without creating an order.
The execution controller and authority must share the same protected Storage
handle; path equality alone is not used to assert database identity.

Every new live entry uses scoped exact account evidence. Ownership, account
binding, existing positions/pending orders and independent execution/financial
holds are checked again within the admission transaction. A concurrent caller
cannot pass an earlier precheck and commit a second unresolved order. Bounded
notional must cover the intent's size/limit price. Protective reductions retain
their separate journal path and bypass entry admission/maintenance.

This closes the reserve-before-journal orphan window for new engine entries.
Historical orphans stay held and are not deleted. The standalone `reserve_entry`
risk API remains available for explicit offline/risk-only callers; it does not
promise a journaled live order.

Legacy core positions still use floating-point storage. Scoped ownership proof
requires exact agreement with their decimal string representation. Values that
cannot be matched stay unresolved; fixed-point core-position migration remains
required. Unknown fee/funding settlement remains an independent entry gate.

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
