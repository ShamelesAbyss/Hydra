# Dependency audit and remediation

## Scope and result

The baseline `v1.7.0-alpha.1` checkout (`141f67381eb95a2d1084ea44717cdff24fd043c6`)
had 39 vulnerable package/advisory entries and 20 warnings. Its Cargo.lock SHA-256
was `33b4306137257feac2961acc51d59db82158404d277e3443bc1b6167a140c7b7`.

The authorized dependency modernization reduces this to **19 vulnerability entries
and 17 warnings** (7 unmaintained, 10 unsound). `cargo audit --json` still exits 1.
There are no advisory ignores, Cargo patch overrides, manually removed lockfile
packages, or claims of a clean audit. This is a checkout change, not a deployed fix.

The updated lockfile SHA-256 at this assessment is
`dbf6181b9b56f518e96f793e57d2777387ee88bcf1989a70336f75d758f3bfd6`.
Future feature/dependency changes require regenerating the inventory and digest.

## Changes applied

| Dependency | Before | After | Purpose |
|---|---|---|---|
| rusqlite | 0.32.1 | 0.43.0 | Upgrade its bundled SQLite dependency |
| libsqlite3-sys | 0.30.1 | 0.40.0 | Patched bundled SQLite, including crafted DB/WAL memory exposure |
| anyhow | 1.0.102 | 1.0.105 | Remove reported downcast unsoundness warning |
| tokio | 1.52.1 | 1.54.1 | Remove reported AsyncFd warning |
| rustix | 1.1.4 | 1.1.6 | Remove reported xattr initialization warning |
| OpenSSL / openssl-sys | 0.10.78 / 0.9.114 | 0.10.82 / 0.9.118 | Patched Linux native-TLS dependency |
| rustls | 0.23.39 | 0.23.46 | Patch Hydra's modern HTTP-client TLS stack |
| rustls-webpki | 0.103.13 | 0.103.17 | Patched verification dependency |
| webpki-roots | 1.0.7 | 1.0.12 | Updated trust roots |
| quinn-proto / quinn-udp | 0.11.14 / 0.5.14 | 0.11.19 / 0.5.16 | Patch retained optional QUIC dependency copies |
| crossbeam-epoch | 0.9.18 | 0.9.21 | Patch retained dependency copy |
| url / form_urlencoded | 2.5.8 / 1.2.2 | 2.5.9 / 1.2.3 | Patch URL-origin recursion advisory |

The SQLite change retains the existing bundled, chrono and serde_json capabilities;
unneeded default wasm/cache features are disabled. Source uses no prepared-cache
API. The new rusqlite MSRV is Rust 1.89, within the verified Rust 1.94 toolchain.
No schema conversion or database rewrite is part of this dependency upgrade.

The [SQLite advisory](https://rustsec.org/advisories/RUSTSEC-2026-0459.html)
identifies crafted sibling DB/WAL files as its local input boundary. Hydra opens
local SQLite databases and enables WAL; upgrading the bundled implementation closes
that affected dependency copy. Normal migrations, transactions, persistence and
learning behavior must remain covered by the Rust regression suite.

## Reachability and remaining blockers

`cargo tree --locked --target all -i <name>@<version>` was used to distinguish the
enabled graph from packages merely retained in Cargo.lock. Graph inclusion alone
does not prove that an affected function is called. These are assessment notes,
not dismissals or risk acceptance.

| Remaining copy | Alert IDs | Existing parent / scope | Disposition |
|---|---|---|---|
| crypto-bigint 0.5.5 | RUSTSEC-2026-0387 | elliptic-curve 0.13 / ecdsa / k256 / ethers-signers; actual exchange signing path | Open, high priority; patched 0.7.6 cannot replace the 0.5 API by a lock refresh |
| h2 0.3.27 | RUSTSEC-2026-0258 | reqwest 0.11 / hyper 0.14 in SDK/ethers HTTP graph | Open; upgrade parent HTTP stack, not an incompatible h2 substitution |
| ring 0.16.20 | RUSTSEC-2025-0009 | jsonwebtoken 8.3 / ethers-providers | Open; Hydra's active adapter does not call provider JWT APIs, but dependency remains enabled |
| rustls 0.21.12 | RUSTSEC-2026-0346, RUSTSEC-2026-0349 | reqwest 0.11 and tokio-tungstenite 0.20 / SDK/ethers | Open; inspect actual TLS backend selection per target and replace legacy stack |
| rustls-webpki 0.101.7 | RUSTSEC-2026-0098, RUSTSEC-2026-0099, RUSTSEC-2026-0104, RUSTSEC-2026-0382 | Legacy rustls 0.21 graph | Open; not repaired by upgrading the separate 0.103 copy |
| webpki-roots 0.25.4 | RUSTSEC-2026-0381 | Legacy TLS graph | Open; requires parent stack modernization |
| rkyv 0.7.46 | RUSTSEC-2026-0235 | Lockfile only: no enabled all-target inverse dependency edges | Retained and open; no archive-deserialization use found in Hydra |
| zip 0.6.6 | RUSTSEC-2026-0367, RUSTSEC-2026-0368, RUSTSEC-2026-0369, RUSTSEC-2026-0429, RUSTSEC-2026-0545, RUSTSEC-2026-0548 | Lockfile only: no enabled all-target inverse dependency edges | Retained and open; do not remove lock entries merely to hide scanner findings |
| zstd-safe 5.0.2 / zstd-sys 2.0.16 | RUSTSEC-2026-0403, RUSTSEC-2026-0410 | Lockfile only: no enabled all-target inverse dependency edges | Retained and open; feature changes must trigger another reachability review |

The [crypto-bigint advisory](https://rustsec.org/advisories/RUSTSEC-2026-0387.html)
concerns timing-dependent arithmetic. Its presence in the signer graph is material;
exploitability depends on the operations, compiler, CPU and observable side channels.
No claim that ordinary network access alone exploits it is made. No signing
algorithm, nonce algorithm or signed serialization was changed by these upgrades.

Remaining warnings: chrono, fxhash, humantime, instant, paste, ring, rustls-pemfile
(unmaintained); two lru warnings; eight zstd-safe unsoundness warnings. Ratatui 0.29
uses lru 0.12 for its layout cache. The zstd warnings share the inactive graph status
above. Chrono is pervasive in public data types and SQLite conversions; replacing
it needs explicit compatibility work. Maintenance warnings are not proof of an
exploit, and they are not dismissed here.

## SDK modernization investigation

The official published Rust SDK remains 0.6.0. Reference HEAD observed:
`aac75585daf12d0a3761126cc7da7a5e035b5853` in
[hyperliquid-dex/hyperliquid-rust-sdk](https://github.com/hyperliquid-dex/hyperliquid-rust-sdk).
Its dependency graph requests ethers with abigen/default features, reqwest 0.11,
and tokio-tungstenite 0.20/native TLS. Disabling defaults only on Hydra's direct
ethers declaration does not remove features enabled by the SDK.

An additional concrete concern is SDK `ExchangeClient::post`: it logs the serialized
exchange payload, including signature and nonce, at debug level. Hydra must prevent
that SDK log target from reaching diagnostic sinks, even when operators request
verbose logs. Replacing the SDK should remove this behavior entirely.

A narrow native replacement should initially support only the allowed trading
mutations, typed responses and read-only information; omit transfers/withdrawals.
Before use, validate ordered MessagePack action encoding, nonce/vault/expiry bytes,
Keccak action hash, EIP-712 Agent domain (Exchange, version 1, chain 1337), network
source, recoverable signature and cloid formatting against independent fixtures.
Do not replace cryptographic primitives or serialization by guesswork.

Independent reference: official Python SDK commit
`2fdb18f9517675ea03695a0962bd19eece9c83f0`, specifically
[signing.py](https://github.com/hyperliquid-dex/hyperliquid-python-sdk/blob/2fdb18f9517675ea03695a0962bd19eece9c83f0/hyperliquid/utils/signing.py)
and [signing_test.py](https://github.com/hyperliquid-dex/hyperliquid-python-sdk/blob/2fdb18f9517675ea03695a0962bd19eece9c83f0/tests/signing_test.py).
These are research/fixture references, not a Python runtime dependency.

## Verification status

Audit: FAIL with 19 vulnerability entries / 17 warnings, reduced from 39 / 20.
Windows `cargo test --locked --all-targets` passed for the concurrently integrated checkout snapshot;
the final integrated test count and source revision belong in TEST_RESULTS.md after integration completes.
No mainnet calls, account probes, real credentials or exchange orders were used.
This work does not certify stable-live readiness, erase existing risk gates, or
close unresolved financial-accounting state.

Current release lock SHA-256: `896b389c3bc84d883253b1c525a82e47af85b0cdde9df7f3761e8c50e497187a` (alpha.2 version metadata included).
