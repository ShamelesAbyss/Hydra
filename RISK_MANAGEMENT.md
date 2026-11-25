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
