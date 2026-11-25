# HYDRA — Project Leviathan 1.7.0-alpha.20

One-snapshot local intent evidence and bounded native history reads.
Engineering prerelease on main; live-trading readiness remains incomplete.

## Implemented

- Add `intent-evidence --db PATH --intent UUID --limit N`, an inspection-only
  protocol-1 CLI/Rust report combining lifecycle, unsigned action, signing preparation
  and optional exact native acknowledgment quantity in one read-only transaction.
- Reuse scoped readers without changing status-history protocol 1 or wire-history
  protocol 2. Require matching identity/account/journal state; preserve original
  times, history caps and explicit table availability. Missing precision is null.
- Bound native fact fields in SQL before materialization, refuse unsupported
  views/primary keys and reject shadowed lifecycle row identifiers that could
  undermine the bounded scan. Corrupt evidence cannot silently become legacy facts.
- Keep live-source/completeness, automatic repair, risk release and financial
  settlement flags false. No engine migration, wallet, coordinator or venue opens.
  Independent execution/risk/financial holds and the permanent memorial remain.

Eight added regressions cover byte preservation, exact quantity without repair
permission, a deterministic shared WAL snapshot across another connection's journal
update, absent optional history, invalid budgets/files/identity, oversized native
metadata, unsupported table/rowid shapes, and CLI configuration/wallet bypass.
Full tests, audit, Windows builds and extracted smoke are checked before publication.
No real account probes, signed network requests, orders or transfers were used.
Native UI remains unchanged from the actual alpha.9 screenshots.

## Remaining limits

This report is local evidence, not a recovery classifier or authorization. One local
snapshot does not prove atomic venue state, complete history, signing/submission,
acceptance or net settlement. Automatic recovery, full envelope attestation, complete
entry/fee/funding attribution and protected credential storage remain incomplete.
Executables are unsigned; historical artifacts remain immutable. The memorial and
its maintenance rule are included in source/portable packages.
