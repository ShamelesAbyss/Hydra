# HYDRA — Project Leviathan 1.7.0-alpha.17

Durable signer-nonce coordinator foundation. Engineering prerelease on main;
this does not yet change the live sender's process-local nonce allocation.

## Implemented

- Reserve nonces per canonical public signer through a separate operator-owned
  SQLite coordinator, shared across cooperating processes and account databases.
- Use IMMEDIATE transactions, FULL synchronization, bounded canonical u64 history,
  schema/application identity checks and a five-second busy timeout. Sample time
  after obtaining the transaction; burn returned reservations even if later work fails.
- Refuse corruption, unsupported schema/journal mode, excessive clock drift,
  overflow and file/commit failures without falling back to memory or resetting
  history. Store public identities/nonces only; no keys, signatures or requests.

Tests cover restarts and rollback, shared aliases and independent signers, concurrent
connections, four child processes with separate bound account histories, injected
write failures, corrupt/foreign/future/view/schema refusal and invalid input.
Full release tests, audit, Windows builds and extracted smoke are checked before
publication. No real account probes, signed requests or exchange orders were used.
Native UI remains unchanged from the actual alpha.9 screenshots.

## Remaining limits

This is a synchronous Rust foundation. Native engine integration and signing
preparation provenance are the next milestone; the existing live sender is unchanged.
Cooperating clients must preserve one shared operator state root. Separate roots,
users/hosts, deletion or tampering cannot be coordinated or attested by this API.
No crash/power-loss certification, complete signed-envelope evidence, automatic
recovery or financial settlement is claimed. Independent holds remain intact.
Executables are unsigned; previous artifacts remain immutable.
