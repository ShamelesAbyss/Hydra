# Dependency audit and remediation

## Current checkout assessment

The post-alpha.4 dependency cleanup removes the legacy Hyperliquid Rust SDK and
ethers dependency graph, replaces its active order-signing path with a narrow
native client, upgrades rust_decimal to 1.43.0, and upgrades Ratatui to 0.30.2.

`cargo audit --json` now exits **0**, reporting **0 vulnerability entries** and
**1 unmaintained warning**. No advisories are ignored. No patch override or
manually pruned lockfile entry is used. Cargo resolves the graph from the changed
manifests. This is a checkout result, not a deployed or stable-live certification.

- Assessed lockfile SHA-256: `b53ae3532704d2e53ebbd1d8ed9169d2aea976e0d9ddc573868fe2905a6765d4`.
- Advisory database commit: `10db6998b5d74ec438bcf0763125bd425a068a8f`.
- Advisory database updated: `2026-11-24T22:47:44+01:00`.
- Scan target restrictions: none; Cargo.lock includes optional/platform copies.
- The warning remains visible in CI; vulnerability auditing remains a required job.

The baseline alpha.1 inventory contained 39 vulnerability entries and 20 warnings.
Compatible alpha.2 upgrades reduced that to 19 entries and 17 warnings. The
post-alpha.4 changes close those 19 reported entries by removing their obsolete
parents or upgrading the parent that selected them.

## Resolved dependency copies

| Previous copy | Advisory IDs | Result in the current graph |
|---|---|---|
| crypto-bigint 0.5.5 | RUSTSEC-2026-0387 | Removed with ethers/k256 signing graph; ECDSA is supplied by secp256k1 0.33.1 / libsecp256k1 |
| h2 0.3.27 | RUSTSEC-2026-0258 | Removed with reqwest 0.11 / hyper 0.14 parents |
| ring 0.16.20 | RUSTSEC-2025-0009 | Removed with ethers-providers / jsonwebtoken 8 |
| rustls 0.21.12 | RUSTSEC-2026-0346, RUSTSEC-2026-0349 | Removed; active HTTP client uses reqwest 0.12.28 and rustls 0.23.46 |
| rustls-webpki 0.101.7 | RUSTSEC-2026-0098, RUSTSEC-2026-0099, RUSTSEC-2026-0104, RUSTSEC-2026-0382 | Removed with the old TLS stack; current copy is 0.103.17 |
| webpki-roots 0.25.4 | RUSTSEC-2026-0381 | Removed; current copy is 1.0.12 |
| rkyv 0.7.46 | RUSTSEC-2026-0235 | rust_decimal 1.43.0 removed this normal optional dependency; Cargo removes its unused graph |
| zip 0.6.6 | RUSTSEC-2026-0367, RUSTSEC-2026-0368, RUSTSEC-2026-0369, RUSTSEC-2026-0429, RUSTSEC-2026-0545, RUSTSEC-2026-0548 | Removed with the unused ethers-solc optional graph |
| zstd-safe 5 / zstd-sys 2.0.16 | RUSTSEC-2026-0403, RUSTSEC-2026-0410 | Removed with old zip/zstd parents |
| lru 0.12.5 | RUSTSEC-2026-0002, RUSTSEC-2026-0253 (unsound warnings) | Ratatui 0.30.2 resolves patched lru 0.18.5 |
| paste 1.0.15 | RUSTSEC-2024-0436 (unmaintained warning) | Removed by Ratatui parent upgrade |

Earlier bundled SQLite, modern TLS and URL-origin upgrades remain in place.
Removing a dependency copy closes that copy's scanner finding; it is not a claim
that every former vulnerable function was reachable or exploited in Hydra.

## Native order-signing boundary

The production adapter supports a single default-validator perpetual IOC order
with a canonical cloid and an explicit reduce-only flag. It has no transfer,
withdrawal, vault, spot or account-management mutation API. Mainnet, standard
account valuation, approved-agent checks and the live authorization gates remain.
Metadata supplies the actual asset index and size precision. IDs outside the
default perpetual namespace and unsupported decimal precision fail closed.

`hyperliquid_wire` uses named MessagePack serialization in protocol field order,
Keccak-256, the EIP-712 Exchange/Agent domain, and the maintained secp256k1 library's
RFC6979 recoverable ECDSA implementation. Hydra does not implement EC arithmetic
or a signature nonce algorithm. The signer derives addresses locally without
account probes. Key parse errors omit input and low-level parse details.

Signed transport owns its HTTP policy: fixed mainnet exchange endpoint, no
redirects, no automatic request retries, a 15-second total timeout and a 1 MiB
response limit. It never logs signatures or request bodies. Transport/HTTP/JSON
failures remain unresolved errors for the durable execution gate. The monotonic
process nonce allocator refuses an excessive lead over the local clock.

The prior SDK logged signed payloads at DEBUG. The native replacement has no such
logging call. Existing defensive log-filter regressions are retained.

## Independent conformance evidence

The offline signing suite compares hashes and signatures with published official
Python SDK constants at commit
[`2fdb18f9517675ea03695a0962bd19eece9c83f0`](https://github.com/hyperliquid-dex/hyperliquid-python-sdk/tree/2fdb18f9517675ea03695a0962bd19eece9c83f0).
Sources: [signing.py](https://github.com/hyperliquid-dex/hyperliquid-python-sdk/blob/2fdb18f9517675ea03695a0962bd19eece9c83f0/hyperliquid/utils/signing.py)
and [signing_test.py](https://github.com/hyperliquid-dex/hyperliquid-python-sdk/blob/2fdb18f9517675ea03695a0962bd19eece9c83f0/tests/signing_test.py).

Eight tests cover the published ordered-action hash and both-network pure
signatures for dummy, limit, cloid, vault and trigger fixture actions, plus
nonce/vault/expiry/network/order boundaries. The production IOC serializer's full
MessagePack bytes and hash are compared with an independent fixture encoder.
Tests use public upstream fixture material only, with no environment keys,
account probes, orders or other network calls. Testnet/vault/trigger cases are
pure conformance fixtures, not exposed production capabilities.

The upstream Rust SDK HEAD observed during assessment was
`aac75585daf12d0a3761126cc7da7a5e035b5853`. It uses Alloy, but its signer graph
still selects the affected older crypto-bigint family; pinning that revision
alone would not close the signing dependency alert.

## Remaining maintenance and readiness work

**RUSTSEC-2026-0543: chrono 0.4.44 is unmaintained.** No patched version is listed.
Chrono remains in persisted timestamps, public data types, SQLite conversions and
reporting. A deliberate timestamp API/storage compatibility migration is needed;
the warning has not been ignored or dismissed.

A passing dependency audit does not establish a complete security review,
profitability or safe unattended trading. Broad account recovery, complete
fee/funding settlement, unknown acknowledgments and legacy financial provenance
remain separate engineering work. No mainnet calls, credential probes or signed
orders were used for this cleanup. See [test results](TEST_RESULTS.md) for final
integrated suite counts and [project status](PROJECT_STATUS.md) for readiness.

Alpha.5 package-version lockfile SHA-256: `d3249be92cc26dfb5bacb66d04fe1ba812e307432adf5dddf2611cfe85b123a8`. The dependency copies are unchanged from the inspected remediation graph.

Alpha.6 package-version lock SHA-256: `967459d4b27fc7a09c5fa9dcb3e242ed4d75d2ef295a64a62728538bfeb4aa36`; dependency copies remain unchanged.

Alpha.7 package-version lock SHA-256: `17877afdacea02e645a05831c3704f7c29d02dd638e129f7aae7b2b2a80a5ae8`; dependency copies remain unchanged.

Alpha.8 package-version lock SHA-256: `f3bf59fd2cf33ba25cd34d66d1eea760cc19b94faa49f35ddea129d149dc53a7`; dependency copies remain unchanged. Fresh release audit exits 0: zero vulnerability entries, one Chrono maintenance warning; no advisory ignores.

Alpha.9 main release lock SHA-256: `68b225510aaaa16c4ad0be8fe2b845004571b4680c17eaf37da3886d59339789`. Fresh audit exits 0: zero vulnerability entries and one Chrono maintenance warning, without advisory ignores.

Alpha.10 main release lock SHA-256: `35411a8c744eb8baa1930927184039a735b13b2234c83f8e0e3b41d2d911cf72`. Fresh audit exits 0: zero vulnerability entries and one Chrono maintenance warning; no advisory ignores.

Alpha.11 release lock SHA-256: `6dcbcc4fd0a3b4c8bc1b5a2fd698edc72cbe51ff5b45e40ab6d0411691886c7f`. Fresh audit exits 0 with zero vulnerability entries and one Chrono maintenance warning; no advisory ignores.

Alpha.12 release lock SHA-256: `40f58679c4f3d62b78a343f08309e953d47b17d6019fd76d0c8e08b654ce72f6`. Fresh audit exits 0: zero vulnerability entries and one Chrono maintenance warning; no advisory ignores.
