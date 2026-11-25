# HYDRA — Project Leviathan 1.7.0-alpha.15

Bounded offline unsigned-action inspection. Engineering prerelease on main;
this does not certify live trading, submission or settlement.

## Implemented

- Add `hydra wire-history --db PATH --intent UUID`, dispatched before trading
  configuration, wallet or engine storage setup. Opens existing SQLite read-only,
  never creates files or migrates history, and reads one consistent transaction.
- Check supported ordinary tables and primary key, bound account, intent/client
  identity, side/reduce-only, decimal quantity/precision and retained metadata.
  Reproduce canonical MessagePack from displayed fields and refuse inconsistencies.
- Bound selected JSON/metadata before decoding. Preserve original recorded times.
  Distinguish unavailable history table from no retained action; neither establishes
  that no order was sent. Explicit JSON flags leave sending/acceptance unproven and
  full signed-envelope provenance unavailable.

Nine added tests cover byte-preserving inspection, unavailable history without
migration, corrupt fields/bytes/scope/precision, oversized data, incompatible table
shape/version, intent contradictions and CLI configuration/wallet bypass.
Full release-version tests, audit, Windows builds and portable smoke are checked
before publication. No real account probes, signed requests or orders were used.
Native UI remains unchanged from the actual alpha.9 screenshots.

## Remaining limits

Local consistency checks are not database attestation or live venue evidence.
Nonce/full signed-envelope provenance, automatic order recovery, complete fee and
funding attribution, financial settlement and protected credential storage remain
incomplete. Execution/risk/accounting holds are unchanged. Executables are unsigned.
