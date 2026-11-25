# HYDRA — Project Leviathan 1.7.0-alpha.8

Exact native execution facts. Engineering prerelease; stable-live readiness
remains incomplete. No real account probes, signed network requests or orders
were used in development verification.

## Implemented

- Preserve original native filled quantity, average price and order identity.
- Validate exact fills against actual signed order quantity, price limit and
  instrument precision, including differences below floating-point resolution.
- Persist scoped receipt facts and journal status atomically; failed persistence
  leaves the submitted intent unresolved and does not retry the exchange write.
- Capture new position precision with entry metadata in one transaction; Aegis
  ownership checks use preserved decimal facts when available.
- Explicitly mark residual precision unknown after existing floating-point
  reductions, preserving original native facts and financial holds.
- Reject nonfinite optional receipt prices before JSON serialization.

The source checkpoint passes 255 offline tests. Release-version tests, fresh
lockfile audit and portable binary smoke checks are repeated before publication.
The previous audit reports zero vulnerability entries and one Chrono maintenance
warning. Windows executables are unsigned; verify published archive checksums.

## Remaining limits

This is an additive exact-facts layer, not a full fixed-point core migration.
Legacy positions receive no invented precision. Exact residual proof, complete
fee/funding settlement, broader replay, persistent snapshot sequencing and
stable-live certification remain open. Unknown costs are never treated as zero.
