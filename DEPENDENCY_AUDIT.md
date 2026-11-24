# Dependency audit — execution foundation

`cargo audit` 0.23.0 was run against the committed dependency versions using the
RustSec advisory database. It returned nonzero. This checkpoint has **not** passed
dependency security verification and must remain prerelease.

This inventory is a lockfile scan, not proof that every affected API executes in
Hydra on every target. Target/feature reachability and exploitability have not yet
been established. No advisory is suppressed or claimed fixed.

Scan result: 39 vulnerable package/advisory entries and 20 warning entries. Multiple versions of a package may repeat an advisory.

| Advisory | Locked package/version | Reported issue | Patched range |
|---|---|---|---|
| [RUSTSEC-2026-0204](https://rustsec.org/advisories/RUSTSEC-2026-0204.html) | crossbeam-epoch 0.9.18 | Invalid pointer dereference in `fmt::Pointer` impl for `Atomic` and `Shared` when the underlying pointer is invalid | >=0.9.20 |
| [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html) | crypto-bigint 0.5.5 | Multiple timing variability issues in operations intended to be constant-time | >=0.7.6 |
| [RUSTSEC-2026-0258](https://rustsec.org/advisories/RUSTSEC-2026-0258.html) | h2 0.3.27 | h2 unbounded empty DATA frames | >=0.4.16 |
| [RUSTSEC-2026-0459](https://rustsec.org/advisories/RUSTSEC-2026-0459.html) | libsqlite3-sys 0.30.1 | `libsqlite3-sys` via C SQLite CVE-2026-119375 | >=0.39.0 |
| [RUSTSEC-2026-0474](https://rustsec.org/advisories/RUSTSEC-2026-0474.html) | openssl 0.10.78 | `X509Ref::ocsp_responders` can produce a `&str` that is not valid UTF-8 | >=0.10.79 |
| [RUSTSEC-2026-0476](https://rustsec.org/advisories/RUSTSEC-2026-0476.html) | openssl 0.10.78 | Buffer overflow with AES key wrap with padding in `cipher_update` | >=0.10.79 |
| [RUSTSEC-2026-0477](https://rustsec.org/advisories/RUSTSEC-2026-0477.html) | openssl 0.10.78 | Buffer overflow with AES key wrap with padding in `cipher_update_inplace` | >=0.10.80 |
| [RUSTSEC-2026-0185](https://rustsec.org/advisories/RUSTSEC-2026-0185.html) | quinn-proto 0.11.14 | Remote memory exhaustion in quinn-proto from unbounded out-of-order stream reassembly | >=0.11.15 |
| [RUSTSEC-2026-0338](https://rustsec.org/advisories/RUSTSEC-2026-0338.html) | quinn-proto 0.11.14 | Remote peer can panic quinn-proto with zero-length STREAM frames during unordered reads | >=0.11.18 |
| [RUSTSEC-2026-0339](https://rustsec.org/advisories/RUSTSEC-2026-0339.html) | quinn-proto 0.11.14 | Unbounded growth of pending connection ID retirements in quinn-proto | >=0.11.17 |
| [RUSTSEC-2026-0340](https://rustsec.org/advisories/RUSTSEC-2026-0340.html) | quinn-proto 0.11.14 | Peer minimum ACK delay can panic quinn-proto when ACK_FREQUENCY transmission is enabled | >=0.11.18 |
| [RUSTSEC-2026-0342](https://rustsec.org/advisories/RUSTSEC-2026-0342.html) | quinn-proto 0.11.14 | Zero-length DATAGRAM frames bypass the receive buffer limit in quinn-proto | >=0.11.17 |
| [RUSTSEC-2026-0523](https://rustsec.org/advisories/RUSTSEC-2026-0523.html) | quinn-proto 0.11.14 | MAX_STREAM_DATA frame can open streams beyond the stream limit | >=0.11.19 |
| [RUSTSEC-2026-0524](https://rustsec.org/advisories/RUSTSEC-2026-0524.html) | quinn-proto 0.11.14 | Long Retry token can panic or exhaust memory in a QUIC client | >=0.11.19 |
| [RUSTSEC-2026-0538](https://rustsec.org/advisories/RUSTSEC-2026-0538.html) | quinn-udp 0.5.14 | Receive loop can spin after a truncated datagram | >=0.5.16, <0.6.0, >=0.6.3 |
| [RUSTSEC-2025-0009](https://rustsec.org/advisories/RUSTSEC-2025-0009.html) | ring 0.16.20 | Some AES functions may panic when overflow checking is enabled. | >=0.17.12 |
| [RUSTSEC-2026-0235](https://rustsec.org/advisories/RUSTSEC-2026-0235.html) | rkyv 0.7.46 | Insufficient archive validation can cause out-of-bounds reads in archives containing Rc/Arc | >=0.8.17 |
| [RUSTSEC-2026-0346](https://rustsec.org/advisories/RUSTSEC-2026-0346.html) | rustls 0.21.12 | `Debug` impls for client session values expose TLS secrets | >=0.23.46 |
| [RUSTSEC-2026-0349](https://rustsec.org/advisories/RUSTSEC-2026-0349.html) | rustls 0.21.12 | Unauthenticated ServerHello can downgrade later connections to classical key exchange | >=0.23.46 |
| [RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285.html) | rustls 0.23.39 | TLS 1.3 handshake messages incorrectly accepted across encryption level boundaries | >=0.23.45 |
| [RUSTSEC-2026-0346](https://rustsec.org/advisories/RUSTSEC-2026-0346.html) | rustls 0.23.39 | `Debug` impls for client session values expose TLS secrets | >=0.23.46 |
| [RUSTSEC-2026-0348](https://rustsec.org/advisories/RUSTSEC-2026-0348.html) | rustls 0.23.39 | Brotli certificate decompression can allocate up to 1 GiB before authentication | >=0.23.46 |
| [RUSTSEC-2026-0349](https://rustsec.org/advisories/RUSTSEC-2026-0349.html) | rustls 0.23.39 | Unauthenticated ServerHello can downgrade later connections to classical key exchange | >=0.23.46 |
| [RUSTSEC-2026-0098](https://rustsec.org/advisories/RUSTSEC-2026-0098.html) | rustls-webpki 0.101.7 | Name constraints for URI names were incorrectly accepted | >=0.103.12, <0.104.0-alpha.1, >=0.104.0-alpha.6 |
| [RUSTSEC-2026-0099](https://rustsec.org/advisories/RUSTSEC-2026-0099.html) | rustls-webpki 0.101.7 | Name constraints were accepted for certificates asserting a wildcard name | >=0.103.12, <0.104.0-alpha.1, >=0.104.0-alpha.6 |
| [RUSTSEC-2026-0104](https://rustsec.org/advisories/RUSTSEC-2026-0104.html) | rustls-webpki 0.101.7 | Reachable panic in certificate revocation list parsing | >=0.103.13, <0.104.0-alpha.1, >=0.104.0-alpha.7 |
| [RUSTSEC-2026-0382](https://rustsec.org/advisories/RUSTSEC-2026-0382.html) | rustls-webpki 0.101.7 | Certificate verification may accept distrusted intermediates | >=0.103.16, <0.104.0-alpha.1, >=0.104.0-alpha.8 |
| [RUSTSEC-2026-0382](https://rustsec.org/advisories/RUSTSEC-2026-0382.html) | rustls-webpki 0.103.13 | Certificate verification may accept distrusted intermediates | >=0.103.16, <0.104.0-alpha.1, >=0.104.0-alpha.8 |
| [RUSTSEC-2026-0592](https://rustsec.org/advisories/RUSTSEC-2026-0592.html) | url 2.5.8 | Stack overflow in `Url::origin()` on nested `blob:` URLs | >=2.5.9 |
| [RUSTSEC-2026-0381](https://rustsec.org/advisories/RUSTSEC-2026-0381.html) | webpki-roots 0.25.4 | Bundled root certificates allow trust in distrusted emSign intermediates | >=1.0.10 |
| [RUSTSEC-2026-0381](https://rustsec.org/advisories/RUSTSEC-2026-0381.html) | webpki-roots 1.0.7 | Bundled root certificates allow trust in distrusted emSign intermediates | >=1.0.10 |
| [RUSTSEC-2026-0367](https://rustsec.org/advisories/RUSTSEC-2026-0367.html) | zip 0.6.6 | Insecure Preservation of SUID/SGID Bits during ZIP Extraction Leading to Potential Privilege Escalation | >=9.0.0 |
| [RUSTSEC-2026-0368](https://rustsec.org/advisories/RUSTSEC-2026-0368.html) | zip 0.6.6 | zip: `ZipArchive::extract` creates symbolic links whose attacker-controlled targets are outside the destination directory | >=9.0.0 |
| [RUSTSEC-2026-0369](https://rustsec.org/advisories/RUSTSEC-2026-0369.html) | zip 0.6.6 | ZipStreamReader::extract() and ZipArchive::extract() apply Unix permissions through symlinks without confining the target | >=9.0.0 |
| [RUSTSEC-2026-0429](https://rustsec.org/advisories/RUSTSEC-2026-0429.html) | zip 0.6.6 | zip: `ZipArchive::extract` aborts the process on a symlink entry with a large declared size | >=9.0.0 |
| [RUSTSEC-2026-0545](https://rustsec.org/advisories/RUSTSEC-2026-0545.html) | zip 0.6.6 | AES entries returned without checking their authentication code when the data ends early or is empty | >=9.0.0 |
| [RUSTSEC-2026-0548](https://rustsec.org/advisories/RUSTSEC-2026-0548.html) | zip 0.6.6 | `ZipStreamReader` extracts entries under local-header names that can differ from the central directory | >=9.0.5 |
| [RUSTSEC-2026-0403](https://rustsec.org/advisories/RUSTSEC-2026-0403.html) | zstd-safe 5.0.2+zstd.1.5.2 | Uninitialized memory exposed when streaming decompression of a legacy frame fails | ^7.4.0, >=8.2.0 |
| [RUSTSEC-2026-0410](https://rustsec.org/advisories/RUSTSEC-2026-0410.html) | zstd-sys 2.0.16+zstd.1.5.7 | WebAssembly allocator shim corrupts memory on allocation failure | >=2.1.2 |

## Warnings

| Kind | Advisory | Package |
|---|---|---|
| unmaintained | RUSTSEC-2026-0543 | chrono |
| unmaintained | RUSTSEC-2025-0057 | fxhash |
| unmaintained | RUSTSEC-2026-0542 | humantime |
| unmaintained | RUSTSEC-2024-0384 | instant |
| unmaintained | RUSTSEC-2024-0436 | paste |
| unmaintained | RUSTSEC-2025-0010 | ring |
| unmaintained | RUSTSEC-2025-0134 | rustls-pemfile |
| unsound | RUSTSEC-2026-0190 | anyhow |
| unsound | RUSTSEC-2026-0002 | lru |
| unsound | RUSTSEC-2026-0253 | lru |
| unsound | RUSTSEC-2026-0486 | rustix |
| unsound | RUSTSEC-2026-0386 | tokio |
| unsound | RUSTSEC-2026-0395 | zstd-safe |
| unsound | RUSTSEC-2026-0397 | zstd-safe |
| unsound | RUSTSEC-2026-0399 | zstd-safe |
| unsound | RUSTSEC-2026-0400 | zstd-safe |
| unsound | RUSTSEC-2026-0401 | zstd-safe |
| unsound | RUSTSEC-2026-0402 | zstd-safe |
| unsound | RUSTSEC-2026-0404 | zstd-safe |
| unsound | RUSTSEC-2026-0406 | zstd-safe |

## Remediation sequence

1. Use `cargo tree --target all -i <package>@<version>` and actual call sites to
   distinguish SDK/runtime dependencies, target-specific code and inactive optional
   packages. Preserve this distinction in each finding.
2. Upgrade compatible leaf dependencies under tests; avoid a broad unverified
   lockfile refresh of the signing/exchange stack.
3. Modernize the pinned Hyperliquid SDK/ethers/HTTP stack where old major versions
   prevent safe upgrades. Revalidate signing, nonce, typed response and account
   behavior using offline recorded fixtures.
4. Review SQLite, TLS, compression, cryptographic arithmetic and runtime advisories
   first for direct runtime impact. Unmaintained warnings also need a replacement
   plan, not an assertion that lack of an exploit means safety.
5. Repeat tests/build/audit on Windows and Linux and update this file. A stable
   release requires an explicit, evidence-backed disposition for remaining alerts.
