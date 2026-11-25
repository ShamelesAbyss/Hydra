# HYDRA — Project Leviathan 1.7.0-alpha.6

Scoped risk-reservation reconciliation and durable evidence. This engineering
prerelease retains all financial and live-readiness uncertainty gates.

## Implemented

- Reconcile definite zero-fill rejected entries and known untouched applied entries
  only when bound account, canonical intent/receipt, position and current exposure agree.
- Obtain exact default-perp standard-account quantities/notionals, equity and server
  time from one clearinghouse payload, with bounded separate open-order reads.
- Require evidence newer than the journal outcome and prior reservation/snapshot.
- Atomically update risk state, archive complete batch evidence, append events and
  preserve or latch drawdown breakers. Replays do not duplicate releases.
- Reuse the validated bundle for admission; maintenance reads cannot block protective
  reduce-only submission. Unknown net settlement remains an independent hold.
- Bound evidence acquisition to 15 seconds, decoded info responses to 4 MiB and
  diagnostic previews to redacted Unicode-safe character limits.
- Reject unsupported contingent entry orders and inconsistent zero-size notionals.
- Include repository artwork in CI packages; preserve the redesigned GitHub presentation.

## Verification and limits

228 offline tests pass on the release-version implementation. Both Windows release executables compile and pass version checks. Archive
checks are repeated during packaging. Dependency copies remain at zero reported
vulnerability entries and one Chrono maintenance warning, with no suppressions.
Signing continues to pass the eight official cross-language conformance fixtures.
Windows executables remain unsigned. No real account probes, signed requests,
exchange orders or transfers are used during development verification.

REST reads are not an atomic exchange snapshot. Clock regression retains holds.
Submitted, ambiguous, resting, unapplied, orphan, legacy-metadata, reduced and
closed reservations are not automatically released. The reserve-before-journal
crash window remains the next milestone. Complete net fee/funding settlement,
broader recovery and stable-live certification remain open.
