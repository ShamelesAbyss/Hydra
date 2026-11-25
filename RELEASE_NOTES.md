# HYDRA — Project Leviathan 1.7.0-alpha.7

Atomic live-entry admission. This is an engineering prerelease, not stable-live
certification. No real account probes, signed requests or orders were used.

## Implemented

- Commit risk reservation and submitted execution intent in the same IMMEDIATE
  SQLite transaction before any exchange write. Either insert failure rolls back
  both records and tentative account snapshot changes.
- Require the execution controller and authority to share one protected storage
  handle, avoiding ambiguous memory/path database identities.
- Use scoped exact account evidence for every authority-backed entry; recheck
  binding, local/remote ownership, same-market positions/orders and global
  execution/financial holds inside the admission transaction.
- Validate bounded notional against size/limit price; preserve drawdown latches
  on denied risk without creating a submitted order.
- Retain the separate protective reduce-only journal path. Historical orphan
  reservations and unknown accounting state are never guessed away.

The release-version suite passes 240 offline tests. Both Windows release binaries
compile; portable smoke checks are repeated during packaging. Dependency copies remain unchanged
from the zero-vulnerability/one-Chrono-warning graph; the fresh release audit
confirms zero vulnerability entries and one maintenance warning. Signing retains the eight official cross-language fixtures.
Windows executables remain unsigned; verify the published archive checksums.

## Remaining limits

Legacy core positions retain floating-point storage; exact scoped proof can
conservatively refuse values that cannot match the stored representation.
Fixed-point core facts, complete fee/funding settlement, broader replay/recovery,
persistent snapshot sequencing and stable-live certification remain open.
The standalone risk-only reserve API remains available to explicit offline/risk
callers; the atomic journal guarantee applies to new engine live-entry admission.
