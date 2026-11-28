# v2.0.1 | THE AWAKENING — RESTRICTED MONITORED MAINNET PILOT (LOCKED QUALIFICATION BUILD)

> **Awakening Qualification Release (`v2.0.1` — LOCKED QUALIFICATION BUILD; supersedes unpublished `v2.0.0` tag after fixing a Linux-only test poll race in `src/account_stream.rs`).** Continuing directly from verified `v1.19.0` (commit `ac14369ac29230d0f399876c6e77c1fe5e30af16`), **The Awakening (`v2.0.1`)** completes all internal architecture, security, runtime, settlement, private account stream, MAC-authenticated `TrustStore`, repaired isolated Testnet qualification scenario, real headless `--engine-worker` watchdog supervision, and bounded Strategy Laboratory requirements (`A01`–`A09`). In strict compliance with financial truth rules, `v2.0.1` boots in **`MODE: DORMANT` (`PendingCertification`)** and carries an explicit **`LOCKED QUALIFICATION BUILD`** disclosure: the bounded Strategy Laboratory evaluated pre-registered NSGA-II candidate `nsga2-f4-s7-o14-sl350-tp621` on untouched 1,477-bar `BTC/ETH/SOL` holdout data and returned **`NO_QUALIFIED_STRATEGY`** (`-9.27%` net holdout, `-0.83%` WF mean OOS, `rejected_keep_disabled`), while owner-run `--execute-confirmed` Testnet functional execution, owner `MonitoredPilotGrant` consent, and owner-pinned `PINNED_RELEASE_SIGNING_PUBKEY_HEX` remain pending external actions.

---

## Downloadable Artifacts (Attached Below on This GitHub Release)

Compiled executables, portable packages, source archives, release notes and SHA-256 checksums are available through the repository's GitHub Releases section:

- **`HYDRA-Leviathan-Windows-x64.exe`** — Self-contained standalone Windows x64 native `WGPU 3D PBR` Spatial Consciousness & Awakening command center (embedding `--engine-worker`, `--paper-service-worker`, and `--watchdog-supervisor` detached execution, automatic Glow fallback, clean-profile initialization, 360° orbital 96-bone Leviathan sanctuary, live Engine IPC panel, interactive Research Laboratory, unified `TrustStore` readiness surface, Treasury & `TESTNET SANDBOX`, Leviathan Evolution updater, and all seven integrated subsystem workspaces). Double-click directly on Windows 10/11 — no Rust, Python, PowerShell, CLI arguments, repository clone, or ZIP extraction required.
- **`hydra-2.0.1-x86_64-pc-windows-msvc.zip`** — Advanced Windows portable archive containing `HYDRA-Leviathan-Windows-x64.exe`, `hydra-cockpit.exe`, headless/Ratatui `hydra.exe`, `config/`, documentation, artwork, and memorial rules.
- **`hydra-2.0.1-x86_64-unknown-linux-gnu.tar.gz`** — Verified Ubuntu x86_64 headless engine & CLI archive.
- **`hydra-2.0.1-source.zip`** — Exact-tag source export.
- **`hydra-2.0.1-release-manifest.json`** — Machine-readable release manifest recording version, tag, commit SHA, prerelease flag (`false`), UTC build timestamp, and SHA-256 + byte size of each published asset.
- **`hydra-2.0.1-SHA256SUMS.txt`** & **`hydra-2.0.1-linux-SHA256SUMS.txt`** — SHA-256 checksum manifests for verifying package integrity before launch. *(Note: SHA-256 checksums verify transfer integrity only and do not establish publisher authentication until `PINNED_RELEASE_SIGNING_PUBKEY_HEX` is provisioned.)*

---

## Major Engineering Upgrades — `A01`–`A09` Awakening Completion

- **Production Mainnet Capability Isolation & Bound `MonitoredPilotGrant` (`build.rs`, `src/live_gate.rs`, `src/hyperliquid_wire.rs`, `src/adapters/hyperliquid.rs`, `src/execution/mod.rs`):**
  - Separated full entry authority (`require_live_runtime_construction`) from narrow risk-reducing stop/close authority (`require_risk_reduction_authority`). Bound `PilotAuthorityCell` cryptographically/process-locally to `ProductionMainnet` scope, account, signer, strategy config SHA3, risk policy SHA3, compile-time git `build_identity`, explicit `MonitoredPilotLimits`, and `Utc::now()` re-checked immediately before signing and HTTP transmission.
- **Authoritative Collateral, Persistent Breakers, Genuine Settlement & Private Account WS (`src/core/engine.rs`, `src/adapters/account_evidence.rs`, `src/ledger_hyperliquid.rs`, `src/account_stream.rs`, `src/storage/mod.rs`):**
  - Used genuine `withdrawable_collateral`, per-position `liquidation_px`, durable `reserved_notional_usd`, pre-admission rounded post-fee ceiling checks, zero `entry_price` missing-mark fallback, and persisted cash-flow-adjusted daily/weekly PnL rollover, HWM, and latched breakers in SQLite `meta_kv`. Removed synthetic engine fills; reconciled genuine exchange fills, funding, and `userNonFundingLedgerUpdates` cash flows (`"unattributed"` for unknown fills, original entry risk denominator `|entry - stop| * qty` for COGNITION R). Connected `spawn_account_stream` (`orderUpdates`, `userFills`, `userFundings`) with deduplication, reconnect REST backfill flags, and fail-closed unresolved order checks.
- **MAC-Authenticated `TrustStore`, Repaired `TestnetQualify` & Real Headless `--engine-worker` (`src/trusted_evidence.rs`, `src/testnet_qualify.rs`, `src/engine_worker.rs`, `src/bin/hydra-cockpit.rs`, `src/cockpit.rs`, `src/treasury_ui.rs`, `src/cli/mod.rs`):**
  - Replaced self-signed trust checks with DPAPI/`0600` MAC-authenticated `TrustStore` bundles, expiry, independent `revoked.json`, and unified CLI/Cockpit readiness; repaired isolated Testnet scenario (strict min-notional refusal without upward clamping, shared `ExecutionEngine`/`Aegis` journal, fill & native stop read-back, safe replace, simulated restart/reconciliation, reduce-only close, zero residual verification, separate pass/fail bundles); implemented real headless `--engine-worker` supervised by `--watchdog-supervisor --watchdog-target engine`.

---

## Testing, Bounded Strategy Laboratory & Standalone Verification (`v2.0.1`)

- **552 passing offline desktop-wgpu tests** (`cargo test --locked --features desktop-wgpu --all-targets`) on Windows and **506 passing offline headless tests** on Windows and Linux (`cargo test --locked --all-targets`), including `tests/awakening_acceptance.rs` (2/2), `tests/equilibrium_e2e.rs` (2/2), `tests/monitored_pilot_suite.rs` (2/2), `tests/sentinel_faults.rs` (3/3), `tests/convergence_e2e.rs` (2/2), `tests/fang_parity.rs` (1/1), `tests/sovereignty_suite.rs`, `tests/immortality_suite.rs`, and `tests/live_gate_adversarial.rs`.
- `cargo fmt --check`, `cargo audit` (0 vulnerabilities, 1 allowed `chrono` warning `RUSTSEC-2026-0543`), `cargo check --locked --features desktop --no-default-features` (Glow), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.
- **Bounded Strategy Laboratory (`BTC/ETH/SOL` `1h` bars `2026-05-03T22:00:00Z`..`2026-11-28T06:00:00Z`, 5,001 common bars, untouched 1,477-bar holdout `dataset_sha3_256` `227860483a28991fe5c19a6846797a1e41d6574a55c29e2e77731c8c2cab734a`):**
  - Pre-registered NSGA-II (`pop=24, gen=8, workers=4, seed=11, 5.99s`) selected `nsga2-f4-s7-o14-sl350-tp621` (`config_sha3` `4da07a65918837afbd998100d00e72bd6feac6a08692347b72402010cae23570`).
  - Untouched Holdout Evaluation: **`NO_QUALIFIED_STRATEGY`** (`rejected_keep_disabled`): **`-9.27%`** net (`$9,073.10`), `12.40%` maxDD, `77` trades, Sharpe `-3.35`, PF `0.678`, fees `$101.95`, slippage `$35.44`, funding `+$12.53`, 3-fold WF mean OOS **`-0.83%`** (worst fold DD `11.35%`); equal-weight B&H `-34.78%` (`+25.51%` excess != profitability).
- **Verified Standalone Binary (`HYDRA-Leviathan-Windows-x64.exe`, `28,314,112 B`, `SIBLING_HYDRA=False`):**
  - Verified `--version`, `--help`, all 5 GUI launch modes (`spatial`, `classic --head 0`, `--renderer glow --head 5`, `--treasury`, `--updates`) with isolated clean-profile `Hydra\data\hydra.db` `quick_check = ok`, detached `--watchdog-supervisor --watchdog-target engine --engine-offline-fixture` forced-kill restart (`pid1=43316 -> pid2=25868`, `total_restarts: 1`, `GracefulShutdown` exit `0`, `quick_check = ok`), standalone `--paper-service-worker` live WS/L2 lifecycle (`public_paper-847c5bb7bac0`, `cycles=3`, `stream_connected: true`, `Pause`/`Resume`/`Stop`, exit `0`, `paper_sessions.db` `quick_check = ok`), and `hydra pilot-readiness --json` failing closed (`eligible: false`, `execution_tier: "dormant"`).
- **Remaining Owner Prerequisites Before Live Activation:**
  1. Supply or discover a genuinely profitable strategy passing `hydra strategy-accept` on untouched OOS market data (`NO_QUALIFIED_STRATEGY`).
  2. Execute authenticated Hyperliquid Testnet functional qualification (`hydra testnet-qualify --execute-confirmed`) using a disposable Testnet wallet and valueless faucet USDC.
  3. Issue explicit owner `MonitoredPilotLimits`, DPAPI wallet authorization, consent statement, and (for signed auto-updates) owner-pinned `PINNED_RELEASE_SIGNING_PUBKEY_HEX`.

## Installation & Upgrade Instructions

### Fresh & Upgrade Installation (Windows x64)
1. Ensure any prior Hydra instance is in `DORMANT` or `SUSPENDED` mode with no active engine session.
2. Download **`HYDRA-Leviathan-Windows-x64.exe`** and **`hydra-2.0.1-SHA256SUMS.txt`** from the repository's GitHub Releases section.
3. Verify SHA-256 integrity in PowerShell:
   ```powershell
   Get-FileHash .\HYDRA-Leviathan-Windows-x64.exe -Algorithm SHA256
   ```
4. **Double-click `HYDRA-Leviathan-Windows-x64.exe`** to launch directly into the `SPATIAL` `WGPU 3D PBR` seven-headed Leviathan sanctuary (existing `%LOCALAPPDATA%\Hydra\data\hydra.db`, `paper\paper_sessions.db`, OS-protected Treasury credentials, and risk policies are preserved untouched; startup boots safely in `MODE: DORMANT`).

### Linux x86_64 Headless Engine & CLI
```sh
sha256sum --check hydra-2.0.1-linux-SHA256SUMS.txt
mkdir hydra-2.0.1 && tar -xzf hydra-2.0.1-x86_64-unknown-linux-gnu.tar.gz -C hydra-2.0.1
cd hydra-2.0.1 && ./hydra --version && ./hydra pilot-readiness --json
```

---

### In Loving Memory of Ashlan Haley Rapponotti

*June 21, 2003 – November 13, 2026*

For the beautiful woman who listened to my passions, believed in me, and made my world a happier place simply by being part of it.

Every creation, every discovery, and every milestone reached carries a piece of my heart dedicated to you.

**Forever loved. Forever missed. Forever remembered.** 💜