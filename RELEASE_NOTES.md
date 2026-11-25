# HYDRA — Project Leviathan 1.7.0-alpha.18

Durable signer nonce integration and engine signing-preparation provenance.
Engineering prerelease on main; live-trading readiness remains incomplete.

## Implemented

- Replace the native process-local nonce counter with the shared durable signer
  coordinator. Reserve on a blocking worker before signing; failures stop submission.
- On the engine path, atomically retain account, public signer, selected nonce,
  action hash, mainnet Agent digest/domain, absent vault/expiry and original time
  before signing. Scope and exact bytes must match the unchanged submitted intent
  and retained rounded IOC action. Phase is explicitly `prepared_not_sent`.
- Refuse duplicates, conflicts and failed/ignored/rewritten journal writes. Nonces
  remain consumed if preparation fails; neither database is reset or rolled back
  as a substitute for recovery. Independent execution/risk/financial holds remain.
- Preserve fixed mainnet IOC transport, no redirects/retries and upstream identity
  checks. No key, signature or signed authorization payload is retained or logged.

Native sender tests use invalid keys and simulated responses. Additional tests cover
metadata/domain agreement, changed scope/action/intent, duplicate nonce/intent,
rollback and ignored/rewritten rows, restart, nonce consumption on journal failure,
and sender refusal on nonce/preparation faults. Existing signing fixtures remain.
Full tests, audit, Windows builds and extracted smoke are checked before publication.
No real account probes, signed network requests, orders or transfers were used.
Native interface remains unchanged from the actual alpha.9 screenshots.

## Operation and remaining limits

Native calls now require writable shared operator nonce state. Direct low-level
callers use the durable coordinator but still own their action/order journals;
the engine path supplies the opaque preparation capability. Preserve one operator
root across cooperating clients. Separate users/hosts/roots, deleted or tampered
history and legacy clients are outside the coordination guarantee.

Preparation is not proof of signing, sending, venue acceptance or net settlement.
Full signed-envelope attestation, bounded preparation inspection, automatic recovery,
complete fee/funding attribution and protected credential storage remain incomplete.
The two database commits are ordered, not a distributed atomic transaction.
Executables are unsigned; historical artifacts remain immutable.
