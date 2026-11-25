# HYDRA — Project Leviathan 1.7.0-alpha.14

Native unsigned-action pre-write provenance. Engineering prerelease on main;
this does not certify complete signed request provenance or stable-live readiness.
No real account probes, signed network requests or exchange orders were used in
verification. Native adapter unit tests use a simulated sender and invalid key.

## Implemented

- Route engine submission through an opaque journal-aware capability after the
  submitted intent is durable. Native IOC action provenance commits before the
  signing/network-write boundary.
- Retain actual rounded price/size, native asset, side/reduce-only, canonical client
  identity, IOC/grouping and exact canonical MessagePack bytes, bound to the account
  and unchanged submitted intent. No private keys or signatures are retained.
- Prevent submission on persistence/scope/precision failure. Reusing a recorded
  intent cannot overwrite action history or make a second native submission through
  that journal. Competing connections can record only one boundary.
- Preserve production IOC encoding, signing/transport policy, receipt handling and
  independent execution/risk/financial holds. Older intents receive no invented
  wire history. Legacy/custom adapters keep their existing submission behavior.

Focused provenance, native sender and controller routing regressions pass. Full
release-version tests, fresh audit, Windows/native compilation and portable smoke
are verified before publication. The native interface is unchanged from the real
alpha.9 screenshots. Executables remain unsigned; verify published checksums.

## Remaining limits

The record is an unsigned action before submission, not proof it was sent or
accepted. Signing nonce and full signed-envelope provenance remain open. Low-level
direct adapter calls retain caller-owned journaling; the new guarantee applies
to the platform engine path. This does not refresh risk snapshots, settle fees or
funding, repair ambiguous orders, clear holds or establish database attestation.
