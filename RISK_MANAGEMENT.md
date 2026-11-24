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
The runtime does not yet automate this proof, so reservations may conservatively
block further exposure. Do not delete them to make trading resume.

Protective reductions bypass ordinary entry exposure/breaker checks, but still
require live authorization, reduce-only semantics and confirmed position/fill
evidence. Unknown prior writes on the same instrument still block duplicate closes.
Account-wide emergency flatten is limited to supported locally known positions;
unknown positions and open-order cancellation need further reconciliation work.

Effective portfolio exposure is controlled; this is not a complete liquidation or
instrument margin model. Instrument leverage settings, liquidation-distance checks,
native protective stops, full loss attribution and streaming race recovery remain
required before stable-live certification. Risk controls are not loss guarantees.
