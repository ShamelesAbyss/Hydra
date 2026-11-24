# Verification — 1.7.0-alpha.1

## Scope and environment

Windows x86_64 MSVC; rustc/cargo 1.94.0. The final source is identified by
`v1.7.0-alpha.1`. No live orders, real credential use, authenticated exchange
account probes, transfers or funding actions were used in these checks.

| Check | Result |
|---|---|
| Untouched 1.6.2 baseline `cargo test --locked --all-targets` | PASS: 95 tests |
| Final `cargo test --locked --all-targets` | PASS: 115 tests, zero failures |
| Unit tests | 84 pass, including 5 added typed-response/metadata/tick tests |
| Execution fault-injection integration tests | 15 pass |
| Learning integration / indicator / TUI tests | 8 / 4 / 4 pass |
| `cargo fmt --check` | PASS |
| `cargo clippy --locked --all-targets` | Completes successfully with warnings; not warnings-as-errors clean |
| `cargo build --locked --release` | PASS on Windows |
| Packaged executable launch | `--version`, `--help`, and offline `config-check` smoke checks |
| `cargo audit --json` using cargo-audit 0.23.0 | FAIL: 39 vulnerability entries, 20 warnings; see DEPENDENCY_AUDIT.md |
| Git history key/token assignment pattern scan | No matches across all 3 baseline commits; limited patterns only |
| Linux build / test / launch | CI workflow provided; no local Linux execution claim |
| Native GUI / rendering / installer | Not implemented; not tested |
| Strategy profitability / trading latency / CPU/GPU improvement | Not established; no benchmark claim |

## New execution coverage

- Disabled config cannot be overridden by CLI assertion.
- Dry-run never calls exchange submission or creates execution journal entries.
- Intent reservation exists before fake network submission.
- Timeout leaves persistent unresolved state; fresh intents and repeat closes block.
- Reused intent UUID is not submitted again, even after rejection/restart.
- Wrong intent identity, missing price, invalid/oversized fill stay unresolved.
- Failed/rejected/dry-run close never archives the local position.
- Partial fill persists the exchange-confirmed residual.
- Full close requires matching remote zero position; net PnL is not invented.
- Stale/unavailable/opposite/non-finite remote position preserves local state.
- Shutdown reduction persists confirmed half-size remainder.
- Missing venue position does not become a synthetic zero-PnL trade.
- Disabled emergency flatten returns before network setup and preserves positions.
- Corrupt DB rows/table failure propagate through exposure reads.
- Migrations preserve existing position data and intent identity across reopen.
- DB failure rolls back reduction persistence and blocks a second close submission.
- Typed partial/resting/error/malformed/unknown/multi-status parser fixtures.
- Price rounding honors size-dependent decimal constraints without widening bounds.
- Adapter authorization and missing metadata fail without network access.

## Remaining validation before stable-live release

Authoritative historical fills/fees/funding, net accounting, open-order recovery,
account identity/modes/equity freshness, multi-process ownership, persistent risk
breakers, protective native stops, stream reconnection/races, full credential/history
audit and dependency remediation remain necessary. Existing historical learning data
may contain estimates and is not retroactively verified.

Clippy warnings include legacy too-many-arguments/style suggestions and a log-line
iterator warning. They are recorded, not misrepresented as a clean lint run.
The initial full baseline build took 4m50s including downloads, and the first
release build took 4m34s; different cache/profile conditions make them unsuitable
for a performance comparison.

## Reproduce

```text
cargo fmt --check
cargo clippy --locked --all-targets
cargo test --locked --all-targets
cargo build --locked --release
cargo audit
```

Run only offline CLI launch checks during development. Keep live configuration
disabled and use an isolated writable data directory. Never use mainnet trades as
automated test cases.
