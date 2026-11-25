# HYDRA — Project Leviathan 1.7.0-alpha.13

Exact acknowledged-fill coverage. Engineering prerelease on main; this does not
certify full order history, round-trip settlement or stable-live readiness.
No real account probes, signed network requests or exchange orders were used.

## Implemented

- Read journal, exact native acknowledgment and scoped ledger fills in one
  SQLite transaction, preventing mixed snapshots across connections.
- Add `FinancialLedger::acknowledged_order_evidence` to require exact coverage
  of a known native acknowledged quantity. Scope and account binding must agree;
  legacy float receipts cannot be relabelled as exact native acknowledgments.
- Retain useful partial evidence while refusing excess acknowledged quantity.
  Native reduction verification uses original exact quantity with an explicit
  legacy compatibility check, rather than reconstructing precision from floats.
- Use bounded indexed fill selection without duplicate cloid/order-ID matches,
  validate payload/index provenance, and enforce record and byte budgets.
- Preserve missing/foreign-currency fee uncertainty, signed rebates and unverified
  PnL conventions. Complete quantity coverage never marks round-trip settlement
  verified or changes execution/risk/financial holds.

All 304 release-version offline tests pass. The fresh audit reports zero
vulnerability entries and one Chrono maintenance warning. Clippy completes with
warnings; both Windows release binaries compile. Extracted portable smoke is repeated
during packaging. This is a Rust
ledger API milestone; there is no new financial-repair CLI or GUI. Native images
show the unchanged alpha.9 layout. Executables are unsigned; verify checksums.

## Remaining limits

An acknowledged fill set does not establish other entry costs, funding coverage,
strategy attribution, exact signed-wire provenance or complete account history.
Automatic recovery, full fixed-point core migration and net settlement remain
open. Local consistency checks are not cryptographic database attestation.
