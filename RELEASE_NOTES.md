# HYDRA — Project Leviathan 1.7.0-alpha.11

Read-only native lifecycle provenance. Engineering prerelease on main; this does
not establish automatic order recovery, complete settlement or stable-live readiness.
No live account probes, signed network requests or exchange orders were used in
development verification; transport fixtures use loopback only.

## Implemented

- Add a Rust API for a single bounded native order-status read by canonical client
  order identity, with a 15-second acquisition deadline and 64 KiB decoded cap.
- Preserve selected exact native order fields, identity, lifecycle and acquisition
  timestamps. Require account/environment/intent scope and reject unsupported
  conditional orders, malformed values and contradictory missing-order responses.
- Append immutable selected observations transactionally. Exact replays remain
  idempotent across restart without refreshing their original acquisition times;
  conflicting reuse cannot overwrite history. New stale observations are refused.
- Never turn missing or terminal-looking status into a guessed fill, execution
  price, zero cost, resolved intent or released risk/financial hold.
- Cross-check client identity encoding against production wire serialization;
  verify both endpoint-specific and existing Info response bounds with loopback.

This is a read-only Rust API and durable evidence foundation, not a new CLI command
or an automatic repair loop. Existing signing, order submission, ledger fills and
financial safeguards are preserved. Native screenshots show the unchanged alpha.9
interface. All 287 release-version offline tests pass. The fresh audit reports zero
vulnerability entries and one Chrono maintenance warning; Clippy completes with
warnings. Both Windows release binaries compile. Extracted portable smoke is repeated
during packaging.
Executables are unsigned.

## Remaining limits

Lifecycle labels and order quantities do not establish average execution price,
complete fees/funding or net settlement. Exact signed-wire request provenance,
authoritative automatic replay/recovery and full fixed-point core migration
remain open. Local consistency is not cryptographic database attestation.
