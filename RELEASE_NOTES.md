# HYDRA — Project Leviathan 1.7.0-alpha.16

Exact native acknowledgment / retained unsigned-action consistency.
Engineering prerelease on main; live readiness remains incomplete.

## Implemented

- Reuse the bounded action decoder inside the caller's SQLite transaction.
  Native receipt persistence validates any retained action before writing exact
  facts or advancing the submitted journal. Later native fact reads repeat checks.
- Require exact fill quantity no greater than the actual rounded request, supported
  size precision and a side-aware average-price limit. Retained account/client,
  side/reduce-only, scope and encoded-byte consistency checks also apply.
- Refuse corrupt retained records and incompatible action tables without a legacy
  fallback. Preserve historical receipts that have no action record without
  inventing pre-write history. Independent execution/risk/financial holds remain.

Seven additional tests cover accepted receipt restart, atomic refusal of overfill,
sub-float limit breach and precision mismatch, sell-price direction, corrupted
retained evidence on later reads, historical absence, incompatible table refusal,
and one caller snapshot across an external atomic WAL update. Synthetic SQL updates
model corruption/concurrency; ordinary production ingestion does not rewrite facts.
Full release tests, audit, Windows builds and portable smoke are checked before
publication. No real account probes, signed network requests or orders were used.
The native interface remains unchanged from the actual alpha.9 screenshots.

## Remaining limits

Unsigned records are not proof of sending or acceptance, and local consistency
is not database attestation. Full signed-envelope/nonce provenance, automatic
ambiguous-order recovery, complete entry/fee/funding attribution and net settlement
remain incomplete. Executables are unsigned. Older releases remain immutable.
