# HYDRA — Project Leviathan 1.7.0-alpha.19

Bounded offline signing-preparation inspection and consistent native evidence.
Engineering prerelease on main; live-trading readiness remains incomplete.

## Implemented

- Extend `wire-history` to protocol 2 with explicit preparation-table availability
  and optional retained metadata. Core SQLite schema 2 and status-history protocol 1
  remain unchanged. Inspect one existing read-only transaction without migrations,
  wallet, configuration, global nonce coordinator or network access.
- Bound JSON/metadata before decoding; check ordinary table/primary-key shape,
  account/signer/nonce/domain scope and action/Agent hashes from the exact retained
  unsigned bytes. Preserve original times and immutable capture-event phase.
- Reuse preparation validation on exact native receipt persistence and subsequent
  fact reads. Corrupt/orphan/index-contradictory evidence cannot silently fall back
  to historical absence. Independent execution/risk/financial holds remain intact.
- Preserve the owner's permanent memorial dedication verbatim at the end of the
  README and include its maintenance rule in portable packages/publication checks.

Added tests cover byte-preserving protocol-2 inspection, absent history distinction,
scope/domain/phase/hash/nonce/index corruption, oversize input, orphan records,
views/unindexed tables, native acknowledgment refusal and CLI wallet/config bypass.
Full tests, audit, Windows builds and extracted smoke are checked before publication.
No real account probes, signed network requests, orders or transfers were used.
Native UI remains unchanged from the actual alpha.9 screenshots.

## Remaining limits

Preparation does not prove signing, sending or venue acceptance. Its phase records
an earlier capture point, not current order state. Full signed envelopes are not
stored or certified; historical metadata is not fresh venue evidence or database
attestation. Automatic recovery and complete entry/fee/funding/net settlement
remain incomplete. Executables are unsigned; previous artifacts stay immutable.
