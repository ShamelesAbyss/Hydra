# Release verification — 2.0.0

All **552 offline desktop-wgpu all-target tests pass** (`cargo test --locked --features desktop-wgpu --all-targets`) and all **506 offline headless all-target tests pass** (`cargo test --locked --all-targets`), including `tests/awakening_acceptance.rs` (2/2), `tests/equilibrium_e2e.rs` (2/2), `tests/monitored_pilot_suite.rs` (2/2), `tests/sentinel_faults.rs` (3/3), `tests/convergence_e2e.rs` (2/2), `tests/fang_parity.rs` (1/1), `tests/sovereignty_suite.rs`, `tests/immortality_suite.rs`, `tests/live_gate_adversarial.rs`, and `tests/hyperliquid_conformance.rs`. 5x stress loops on `awakening_acceptance`, `monitored_pilot_suite`, `sentinel_faults`, and `engine_ipc` passed 5/5. `hydra --no-banner pilot-readiness --json` and `certification-status --json` fail closed (`eligible: false`, `execution_tier: "dormant"`, `unattended_awakened_eligible: false`).

`cargo fmt --check`, `cargo audit` (0 vulnerabilities, 1 allowed warning `RUSTSEC-2026-0543` for `chrono 0.4.44`), `cargo check --locked --features desktop --no-default-features` (Glow), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.

### Awakening Acceptance, Standalone Binary & Strategy Laboratory (`v2.0.0 — LOCKED QUALIFICATION BUILD`)
- **Standalone Windows Binary (`HYDRA-Leviathan-Windows-x64.exe`, `28,314,112 B`, local release build SHA-256 `3885D3B646F1B05113675F20793F512545D741EEFB2A0C78B43BE1E19FEAD0C4`, `SIBLING_HYDRA=False`):** Verified `--version` (`hydra 2.0.0`), `--help`, all 5 GUI launch modes (`spatial`, `classic --head 0`, `--renderer glow --head 5`, `--treasury`, `--updates`) staying cleanly alive with isolated clean-profile `Hydra\data\hydra.db` `PRAGMA quick_check(1) = ok`, detached `--watchdog-supervisor --watchdog-target engine --engine-offline-fixture` forced-kill auto-restart (`pid1=43316 ticks1=2` -> `pid2=25868 ticks2=2`, `watchdog-state.json` `total_restarts: 1`, `GracefulShutdown` exit `0`, `engine.db` `PRAGMA quick_check(1) = ok`), and standalone `--paper-service-worker` live Hyperliquid WS/L2 lifecycle (`public_paper-847c5bb7bac0`: `cycles=3`, `stream_connected: true`, `Pause`/`Resume`/`Stop`, clean exit `0`, `paper_sessions.db` `PRAGMA quick_check(1) = ok`).
- **Bounded Strategy Laboratory (`BTC/ETH/SOL` `1h` bars `2026-05-03T22:00:00Z`..`2026-11-28T06:00:00Z`, 5,001 common bars; DEV `0..=3499`, embargo `3500..=3523`, untouched HOLDOUT `3524..=5000` `1,477` bars, `dataset_sha3_256` `227860483a28991fe5c19a6846797a1e41d6574a55c29e2e77731c8c2cab734a`):**
  - Pre-registered NSGA-II (`pop=24, gen=8, workers=4, seed=11, 5.99s`, 183 evaluated, 13 Pareto, 4 dev-eligible) selected `nsga2-f4-s7-o14-sl350-tp621` (`config_sha3` `4da07a65918837afbd998100d00e72bd6feac6a08692347b72402010cae23570`; dev val `+7.302%`, maxDD `6.738%`, 53 trades).
  - Final Holdout Evaluation: **`NO_QUALIFIED_STRATEGY`** (`rejected_keep_disabled`): net **`-9.27%`** (`$9,073.10`), maxDD `12.40%`, `77` trades, Sharpe `-3.35`, profit factor `0.678`, fees `$101.95`, slippage `$35.44`, funding `+$12.53`, 3-fold WF mean OOS **`-0.83%`** (worst fold DD `11.35%`), bootstrap p05 `-0.0163%`; equal-weight B&H `-34.78%` (`+25.51%` excess != profitability). Default baseline on holdout: `-4.98%` net, maxDD `9.36%`, 187 trades.
- **Explicit Qualification Disclosure:** `v2.0.0` ships explicitly as a **`LOCKED QUALIFICATION BUILD`** (`MODE: DORMANT`) with zero qualified strategies, no owner-executed `--execute-confirmed` Testnet functional proof, no owner `MonitoredPilotGrant`, and `PINNED_RELEASE_SIGNING_PUBKEY_HEX` still unset.

# Release verification — 1.19.0

All **545 offline desktop-wgpu all-target tests pass** (across 41 test suites) and all **499 offline headless all-target tests pass** (across 40 test suites, including `tests/equilibrium_e2e.rs` 2/2, `tests/monitored_pilot_suite.rs` 2/2, `tests/sentinel_faults.rs` 3/3, `tests/convergence_e2e.rs` 2/2, actual-engine `tests/fang_parity.rs` 1/1, `tests/sovereignty_suite.rs`, `tests/immortality_suite.rs`, `tests/live_gate_adversarial.rs`, and `tests/hyperliquid_conformance.rs`). `hydra --no-banner pilot-readiness --json` truthfully fails closed (`eligible: false`, `execution_tier: "dormant"`, `unattended_awakened_eligible: false`).

`cargo fmt --check`, `cargo audit` (0 vulnerabilities, 1 allowed warning `RUSTSEC-2026-0543` for `chrono`), `cargo check --locked --features desktop --no-default-features` (Glow), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets` (`0` warnings across all `v1.19.0` modified/added modules), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.

### Equilibrium 60-Day Laboratory & Standalone Watchdog Measurements (`v1.19.0`)
- **Standalone Windows Binary (`HYDRA-Leviathan-Windows-x64.exe`, `25,660,416 B`, `SIBLING_HYDRA=False`):** Verified `--version` (`hydra 1.19.0`), `--help`, all 5 GUI launch modes (`spatial`, `classic --head 0`, `--renderer glow --head 5`, `--treasury`, `--updates`) with non-zero HWNDs and clean-profile `hydra.db` `PRAGMA quick_check(1) = ok`, plus live detached `--watchdog-supervisor` Hyperliquid WS/L2 paper execution (`public_paper-8de662e9592c`: auto-restarted force-killed child worker PID `19656` -> `40764` with `watchdog-state.json` `total_restarts: 1`, executed `Pause` -> `Resume` -> `Stop` cleanly at `cycle_count: 3`, `stream_connected: true`, clean supervisor exit `0`, and `paper_sessions.db` `PRAGMA quick_check(1) = ok`).
- **60-Day Historical Hyperliquid Campaign (`hl_60d_btc_eth_sol.json`, `BTC/ETH/SOL` `1h` bars `2026-09-29T03:00:00Z`..`2026-11-28T03:00:00Z`, `1,441` bars + `1,440` funding points per symbol, `dataset_sha3_256` `e92062f79d1c367a0cc8e77443471a0bca5076db7a2ce1ad55b3e4ae5553f6f3`):**
  - `oracle-portfolio --folds 3 --embargo-bars 4`: `$10,000` -> `$9,742.86` (`-2.57%` net across `184` trades after `$241.24` fees + `$84.80` slippage - `$9.20` funding) vs. `-35.45%` equal-weight B&H (`+32.88%` excess, `8.34%` max DD, `verdict: "negative_or_zero_after_costs"`).
  - `strategy-accept --folds 3 --embargo-bars 4`: **`rejected_keep_disabled`** (`-2.57%` net below `0.50%` floor; `-0.24%` mean OOS across 3 purged folds with `46`/`42`/`43` trades, `6.35%` worst OOS DD, block bootstrap `p05=-0.0106%`).
  - `genesis-nsga2 --train-bars 720 --embargo-bars 4 --population 8 --generations 3 --workers 2 --seed 7`: evaluated 5 Pareto candidates (`auto_deployment_authorized: false`, `experiment_fingerprint` `f69dfc1ba7ea27c33b40880852b4a469da601b5c94d7f085bb8410f3a263ebf4`).
- **Diagnostics & Read-Only Testnet (`v1.19.0`):** `hydra qualify-campaign` passed **10/10** fault scenarios; `hydra profile-host` measured `32` cores (`i9-14900HX`), `20,407 / 32,543 MB` RAM, `NVIDIA GeForce RTX 4070 Laptop GPU` (`1,504 / 8,188 MB` VRAM), `quick_check_ok: true`; `hydra testnet-probe` verified `213` perpetual assets / `3,533` mids (`BTC=60697.0`, `ETH=1676.65`); `hydra testnet-qualify --json` verified read-only (`executed_mutations: false`).
- **Prior Release (`v1.18.0`):** Passed CI `Verify and package` & `Release Project Leviathan` cleanly at commit `beee63471741aa536f77c9d548f192e7d77d7485` with **541** `desktop-wgpu` / **495** headless tests.

# Release verification — 1.18.0

All **541 offline desktop-wgpu all-target tests pass** (across 40 test suites) and all **495 offline headless all-target tests pass** (across 39 test suites, including `tests/monitored_pilot_suite.rs` 2/2 x 3 stress iterations, `tests/sentinel_faults.rs` 3/3, `tests/convergence_e2e.rs` 2/2, actual-engine `tests/fang_parity.rs` 1/1, `tests/sovereignty_suite.rs`, `tests/immortality_suite.rs`, `tests/live_gate_adversarial.rs`, and `tests/hyperliquid_conformance.rs`). `hydra --no-banner pilot-readiness --json` truthfully fails closed (`eligible: false`, `execution_tier: "dormant"`, `unattended_awakened_eligible: false`).

`cargo fmt --check`, `cargo audit` (596 dependencies, 0 vulnerabilities, 1 allowed warning `RUSTSEC-2026-0543` for `chrono`), `cargo check --locked --features desktop --all-targets`, `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets` (`0` warnings across all `v1.18.0` modified/added modules), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.

- **Prior Release (`v1.17.0`):** Passed `Verify and package` (`46096037097` on `main`, commit `049580d5c16c305c5f50f8640456c515b05f5013`) and `Release Project Leviathan` (`46096040198` on tag `v1.17.0`), and independently verified standalone `HYDRA-Leviathan-Windows-x64.exe` SHA-256 `566f29d00786ea9f1dfd433cd64706d3d5cbfe71681d3562f43e5a4dfd51c3ea` (`SIBLING_HYDRA=False`, clean-profile GUI `hydra.db` `ok`, and a live `38.63s` public-market WS/L2 `public_paper-9370231f3128` lifecycle completing `5` cycles, `pause`/`resume`/`stop`, `stream_connected: true`, `mtm_equity_usd: "9996.73"`, `1,114,112 B` SQLite `quick_check: ok`).

# Release verification — 1.17.0

All **539 offline desktop-wgpu all-target tests pass** (across 39 test suites) and all **493 offline headless all-target tests pass** (across 38 test suites, including `tests/sentinel_faults.rs` 3/3 x 3 stress iterations, `tests/convergence_e2e.rs` 2/2, actual-engine `tests/fang_parity.rs` 1/1, `tests/sovereignty_suite.rs`, `tests/immortality_suite.rs`, `tests/live_gate_adversarial.rs`, and `tests/hyperliquid_conformance.rs`).

`cargo fmt --check`, `cargo audit` (596 dependencies, 0 vulnerabilities, 1 allowed warning `RUSTSEC-2026-0543` for `chrono`), `cargo check --locked --features desktop --all-targets`, `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets` (`0` warnings across all `v1.17.0` modified/added modules), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.

### Accelerated Awakening Operational Field Measurements (`v1.17.0`)
- **Host Resource Profile (`hydra profile-host --json` on target Windows 11 Lenovo Legion):** `32` logical cores (`Intel i9-14900HX`), `20,806 MB` available / `32,543 MB` total RAM, `328,159,125,504 B` free disk, `NVIDIA GeForce RTX 4070 Laptop GPU` (`1,498 MB` used / `8,188 MB` total VRAM, `17%` GPU utilization).
- **Hyperliquid Testnet Read-Only Probe (`hydra testnet-probe --json`):** Reachable at `https://api.hyperliquid-testnet.xyz/info` (`VenueTransportClass::TestnetSandbox`): `213` perpetual universe assets, `3,533` mids (`BTC` `szDecimals: 5`, `maxLeverage: 40`, `marginTableId: 54`, `btc_mid: "60687.5"`, `eth_mid: "1678.55"`). Authenticated order/stop mutations require an isolated owner-authorized Testnet identity and valueless faucet funds.
- **Historical OOS Strategy Qualification (`hydra strategy-accept`):** Evaluated over 45 days of public Hyperliquid `1h` candles + hourly funding (`BTC`, `ETH`, `SOL`, `1,081` bars & `1,080` funding points per symbol from `2026-10-14T01:00:00Z` to `2026-11-28T01:00:00Z`, JSON SHA-256 `f9db44383348164fd5a8f2ccc00341c787c56f9d0e3940fbb02fb9bc88b87156`, `dataset_sha3_256` `7ff196eaadefb109ac24ff72e5e476615897ca4ff99d907e485368d50a7a31b8`, `--folds 3 --embargo-bars 4`). Result: **`rejected_keep_disabled`** (equal-weight buy-and-hold `-29.92%`; portfolio `-2.80%` net after `$193.96` fees + `$68.25` slippage across `149` trades = `+27.12%` vs. B&H, `8.09%` max DD, `profit_factor: 0.9008`; 3 purged OOS folds `[-0.17%, +0.17%, -1.53%]`, mean `-0.51%`, worst DD `2.88%`, block bootstrap `p05=-0.0160%`). Live strategy entries remain strictly disabled (`PendingCertification`).
- **Prior Release (`v1.16.0`):** Passed `Verify and package` (`46092459099` on `main`, including Windows `desktop` `engine_ipc`) and `Release Project Leviathan` (`46092462506` on tag `v1.16.0`), and independently verified standalone `HYDRA-Leviathan-Windows-x64.exe` SHA-256 `6dbf609357f38e5461a0237356d6239ae48cb2e76cbdbc2cd086af1d78875659` (`SIBLING_HYDRA=False`, clean-profile GUI `hydra.db` `ok`, and standalone `--paper-service-worker` IPC lifecycle `cycle_count: 3`, `PAPER_DB_QC=ok`).

# Release verification — 1.16.0

All **535 offline desktop-wgpu all-target tests pass** (across 38 test suites: 316 lib unit tests + integration/doc suites) and all **489 offline headless all-target tests pass** (across 37 test suites: 270 lib unit tests + integration/doc suites, including `tests/convergence_e2e.rs` 2/2, actual-engine `tests/fang_parity.rs` 1/1, `tests/sovereignty_suite.rs`, `tests/immortality_suite.rs`, `tests/live_gate_adversarial.rs`, `tests/hyperliquid_conformance.rs`, and 3 consecutive `engine_ipc` parallel socket stress iterations with 0 Windows 10054 socket resets).

`cargo fmt --check`, `cargo audit` (596 dependencies, 0 vulnerabilities, 1 allowed warning `RUSTSEC-2026-0543` for `chrono`), `cargo check --locked --features desktop --all-targets`, `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets` (`0` warnings across all modified/added `v1.16.0` modules), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass. Prior release **`v1.15.0`** passed `Release Project Leviathan` (`46085968686`) across all 5 jobs (`windows-desktop` included) and verified `HYDRA-Leviathan-Windows-x64.exe` SHA-256 `ab8cc14b84fef66de29ff23e2ecc26cd017df70fd7091ab4ce447026433f0150` (7-cycle standalone `--paper-service-worker` execution without `hydra.exe`), while `46085963871` surfaced the Windows IPC 10054 socket defect now fixed cleanly at root cause in `v1.16.0`. No real exchange orders, live wallet credentials, or transfers were used.

# Release verification — 1.15.0

All **530 offline desktop-wgpu all-target tests pass** (across 37 test suites: 313 lib unit tests + integration/doc suites) and all **484 offline headless all-target tests pass** (across 36 test suites: 267 lib unit tests + integration/doc suites, including `tests/sovereignty_suite.rs`, `tests/immortality_suite.rs`, `tests/fang_parity.rs`, `tests/live_gate_adversarial.rs`, and `tests/synapse_pipeline.rs`), adding tests for external OS-locked watchdog supervision, `PRAGMA quick_check(1)` & DB size limits, real Win32/POSIX disk free space, exponential restart escalation & `PostRestartRevalidationChecklist` (`src/watchdog.rs`), 10-scenario fault-injection qualification, honest real-clock qualification snapshots, scientific strategy acceptance gating & `ResourceBudgetScheduler` (`src/qualification.rs`), all 7 `HeadId`s connected via `SynapseBus`, and 6-pillar `PendingCertification` readiness evaluation (`src/live_gate.rs`).

`hydra --no-banner qualify-campaign` passed **10/10** fault-injection scenarios (`all_passed: true`) alongside an honest real-clock local snapshot (`cycles_completed: 5`, `open_positions: 3`, `plans_accepted: 3`, `simulated_executions: 3`, `equity_usd: "9998.2000"`, `total_fees_usd: "1.8000"`). `cargo fmt --check`, `cargo audit` (596 dependencies, 0 vulnerabilities, 1 allowed warning `RUSTSEC-2026-0543` for `chrono`), `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets` (0 warnings in new `v1.15.0` files), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass. Prior release **`v1.14.0`** passed CI runs `46081720851` / `46081725075` and verified `HYDRA-Leviathan-Windows-x64.exe` SHA-256 `06430f63ba53fb72169d53d5ea89d485cc8b8282a02f28c1efbeb4fb6535469c` (isolated clean-profile GUI startup + 2-cycle standalone `--paper-service-worker` IPC `Status`/`Stop` lifecycle with no sibling `hydra.exe`). No real exchange orders, live wallet credentials, or transfers were used.

# Release verification — 1.14.0

All **527 offline desktop-wgpu all-target tests pass** (across 36 test suites: 311 lib unit tests + integration/doc suites) and all **481 offline headless all-target tests pass** (across 35 test suites: 265 lib unit tests + integration/doc suites, including `tests/immortality_suite.rs`, `tests/fang_parity.rs`, `tests/live_gate_adversarial.rs`, and `tests/synapse_pipeline.rs`), adding tests for authoritative FIFO partial-fill settlement with maker rebates, funding attribution, deposit/withdrawal separation & at-most-once learning emission (`src/settlement.rs`), REST/WS execution recovery, tick/lot precision & blind-retry refusal (`src/execution_recovery.rs`), exchange-native `reduce_only` protective stops authorized during blocked entries (`src/protective_orders.rs`), fail-closed AEGIS sovereign portfolio risk (`src/aegis_sovereign.rs`), `SigningAccessScope` Treasury isolation & authorization revocation (`src/treasury.rs`), authenticated replay/token/frame-bounded `EngineIpcServer` (`src/engine_ipc.rs`), and 5-pillar `PendingCertification` readiness evaluation (`src/live_gate.rs`).

`cargo fmt --check`, `cargo audit` (596 dependencies, 0 vulnerabilities), `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass. Prior release **`v1.13.0`** passed CI runs `46078025562` / `46078031185` and verified `HYDRA-Leviathan-Windows-x64.exe` SHA-256 `262efc82ec5a17d42cdf70b9047a33a7bd3a3301705dada8e3ecf0b321f9e2ca` (clean-profile startup + isolated `--paper-service-worker` IPC lifecycle without `hydra.exe`). No real exchange orders, live wallet credentials, or transfers were used.

# Release verification — 1.13.0

All **517 offline desktop-wgpu all-target tests pass** (across 35 test suites: 302 lib unit tests + integration/doc suites) and all **471 offline headless all-target tests pass** (across 34 test suites: 256 lib unit tests + integration/doc suites, including `tests/fang_parity.rs`, `tests/live_gate_adversarial.rs`, and `tests/synapse_pipeline.rs`), adding tests for HUNTER L2/trade-flow/OI microstructure, timestamp-aligned Pearson correlation, utility lift evaluation & empty-watchlist stream bootstrap (`src/hunter_microstructure.rs`, `src/hunter_stream.rs`), COGNITION conjugate Bayesian R shrinkage, empirical Brier/ECE calibration & Page-Hinkley deterioration alerts (`src/cognition_stats.rs`), FANG 7-strategy `fang-registry-v1`, trailing state, adaptive TP & cross-consumer parity across direct Planner, ORACLE, Engine & Synapse Paper (`src/fang_engine.rs`, `tests/fang_parity.rs`), ORACLE multi-market portfolio simulation & purged walk-forward + block-bootstrap CI (`src/oracle_research.rs`), GENESIS parallel constraint-aware NSGA-II (`src/genesis_nsga2.rs`), and `v1.12.0` audit remediation (funding attribution into `normalized_r`, `equity_complete` completeness tracking, lock-held TOCTOU-free session guards).

`cargo fmt --check`, `cargo audit` (596 dependencies, 0 vulnerabilities), `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass. Prior release **`v1.12.0`** passed CI runs `46068616032` / `46068621889` and verified `HYDRA-Leviathan-Windows-x64.exe` SHA-256 `e57039944a756a65c58f8f8ac70c824b4baa3e5907e891354f429de279b786c2`. No real exchange orders, live wallet credentials, or transfers were used.

# Release verification — 1.12.0

All **504 offline desktop-wgpu all-target tests pass** (across 34 test suites: 290 lib unit tests + integration/doc suites) and all **458 offline headless all-target tests pass** (across 33 test suites: 244 lib unit tests + integration/doc suites, including `tests/live_gate_adversarial.rs` 2/2 and `tests/synapse_pipeline.rs` 2/2), adding tests for structural `VenueTransportClass` isolation (`src/live_gate.rs`), independent per-field freshness & `observed_history_minutes` vs. `instrument_listing_age_days: None` (`src/hunter_stream.rs`), stale-mark rejection before both simulated exits and entries & non-double-charged hourly funding attribution (`src/paper_pipeline.rs`), full checkpoint recovery equivalence, SQLite `BEFORE INSERT` trigger atomic rollback & running/paused guard enforcement (`src/paper_store.rs`), and oversized IPC frame rejection, held-lock guard enforcement & DB fault `Faulted` service halt (`src/paper_service.rs`).

`cargo fmt --check`, `cargo audit` (596 dependencies, 0 vulnerabilities), `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass. Prior release **`v1.11.0`** passed CI runs `46059917412` / `46059922363` and verified `HYDRA-Leviathan-Windows-x64.exe` SHA-256 `de5441038019d39c1f6b479673f714b5739c1f6b479200fce077fa98b179`. No real exchange orders, live wallet credentials, or transfers were used.

# Release verification — 1.11.0

All **496 offline desktop-wgpu all-target tests pass** (across 34 test suites: 282 lib unit tests + integration/doc suites) and all **450 offline headless all-target tests pass** (across 33 test suites: 236 lib unit tests + integration/doc suites, including `tests/live_gate_adversarial.rs` 2/2 and `tests/synapse_pipeline.rs` 2/2), adding offline loopback WebSocket + REST snapshot reconnection tests (`src/hunter_stream.rs`), isolated SQLite restart recovery & confirmed-reset tests (`src/paper_store.rs`), exclusive OS session lock + authenticated loopback IPC rejection/control tests (`src/paper_service.rs`), adversarial production gate bypass refusal across every entry point (`src/live_gate.rs`, `tests/live_gate_adversarial.rs`), missing-mark fail-closed admission (`src/paper_pipeline.rs`), and `SynapseBus` v2 schema validation (`src/synapse_bus.rs`).

`cargo fmt --check`, `cargo audit` (596 dependencies, 0 vulnerabilities), `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass. Prior release **`v1.10.0`** passed CI runs `46022409195` / `46022414892` and verified `HYDRA-Leviathan-Windows-x64.exe` SHA-256 `7dd8b26a4f9c514be153af46e7bad7f9222fc34164912a4be0017ff349fc75de`. No real exchange orders, live wallet credentials, or transfers were used.

# Release verification — 1.10.0

All **485 offline desktop-wgpu all-target tests pass** (across 33 test suites: 273 lib unit tests + integration/doc suites) and all **439 offline headless all-target tests pass** (across 32 test suites: 227 lib unit tests + integration/doc suites, including `tests/synapse_pipeline.rs` 2/2), adding coverage for the versioned `SynapseBus` bounded event ring and JSONL replay (`src/synapse_bus.rs`), reproducible connected 5-head `SIMULATION_ONLY` paper execution, stale-market rejection, drawdown breaker protection with permitted exits (`src/paper_pipeline.rs`), and zero-config offline `hydra synapse-paper` CLI execution (`src/cli/mod.rs`).

`cargo fmt --check`, `cargo check --locked --features desktop --all-targets` (0 warnings), `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass. Prior release **`v1.9.0`** passed CI runs `46014147524` / `46014158071` and verified `HYDRA-Leviathan-Windows-x64.exe` SHA-256 `69c7bd8b1cc51dae2b8cef6b41de28248bf3a89ac49980ed1c154fb6fefec5b9`. No real exchange orders, live wallet credentials, or transfers were used.

# Release verification — 1.9.0

All **478 offline desktop-wgpu all-target tests pass** (across 32 test suites) and all **432 offline headless all-target tests pass** (across 31 test suites), adding coverage for pole-safe `CameraBasis` billboard frames, 360° `OrbitState` bounds and thorax/floor collision avoidance, `AwarenessPose` skull separation (`src/leviathan_3d_math.rs`), seven-personality critically damped springs and dwell hysteresis (`src/leviathan_awareness.rs`), `SpatialPreferences` sanitization/roundtrip, and safe-zone memorial preservation (`src/leviathan_spatial.rs`).

`cargo fmt --check`, `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, Glow compilation (`--features desktop --all-targets`, zero warnings), Glow runtime startup (`--renderer glow`), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass. Native Windows 11 RTX 4070 `--release --features desktop-wgpu` (`1600 × 960`, `--quality ultra`) captures verify `SPATIAL` Overview (`docs/assets/leviathan-awakened-wgpu.png`), floating FANG spatial workspace (`docs/assets/leviathan-spatial-workspaces.png`), 52° orbital AEGIS profile (`docs/assets/leviathan-orbit-profile.png`), unobstructed 165° rear dorsal armor (`synapse-orbit-rear-v2.png`), `CLASSIC` focused view (`docs/assets/leviathan-focused-head.png`), Treasury (`docs/assets/leviathan-treasury-offering.png`), and `v1.9.0` Evolution (`docs/assets/leviathan-evolution-updates.png`) measuring `0.14–0.53 ms GPU` pass duration (`UI 144.3/s · cpu 0.51 ms · 0.53 ms GPU` interactive Spatial overview; `UI 300.3/s · cpu 0.52 ms · 0.24 ms GPU` rear orbit). Prior release **`v1.8.0`** passed CI runs `46000276227` / `46000283811` and verified `HYDRA-Leviathan-Windows-x64.exe` SHA-256 `5aa37c17805af37b30151273d503042c2bdcc5a8241a83a523323c338b1df29f`. No real exchange orders, live wallet credentials, or transfers were used.

# Release verification — 1.8.0

All **470 offline desktop-wgpu all-target tests pass** (across 32 test suites) and all **432 offline headless all-target tests pass** (across 31 test suites), adding coverage for the 96-bone hierarchical 3D skeleton (`src/leviathan_3d_math.rs`), `LeviathanCamera` Sovereign Overview ↔ 3/4 Focused Head projection & transitions, 96-byte `GpuVertex` skinned mesh invariants (`src/leviathan_mesh.rs`), 4-pass native WGPU 3D PBR / Shadow / HDR Bloom / `TIMESTAMP_QUERY` renderer helpers (`src/leviathan_renderer_3d.rs`), and `SceneQuality::Cinematic`.

`cargo fmt --check`, `cargo clippy --locked --all-targets`, `cargo clippy --locked --features desktop-wgpu --all-targets`, Glow compilation (`--features desktop --all-targets`, zero warnings), `verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass. Native Windows 11 RTX 4070 `--release --features desktop-wgpu` (`1600 × 960`, `--quality ultra`) captures measure `UI 144.9/s · cpu 0.66 ms · 0.32 ms GPU` in Overview (`docs/assets/leviathan-awakened-wgpu.png`), `UI 143.1/s · cpu 0.68 ms · 0.27 ms GPU` in FANG Focused Head (`docs/assets/leviathan-focused-head.png`), and `UI 144.2–144.4/s · cpu 0.64–0.77 ms · 0.28 ms GPU` in Treasury/Updates (`docs/assets/leviathan-treasury-offering.png`, `docs/assets/leviathan-evolution-updates.png`), while `--quality cinematic` measures `UI 144.1/s · cpu 0.94 ms · 0.38 ms GPU`. Public distribution does not certify unattended live trading; no real exchange orders, live wallet credentials, or transfers were used.

# Release verification — 1.7.0

All **463 offline desktop-wgpu all-target tests pass** (across 32 test binaries), adding coverage
for the 512-byte aligned PBR WGSL Leviathan shader uniform buffer & NaN/Inf sanitization, `Ultra`
quality tessellation & Adaptive LOD stepping, public `v1.7.0` vs. prerelease `v1.7.1-alpha.1`
channel selection in **LEVIATHAN EVOLUTION**, and read-only SQLite `PRAGMA quick_check(1)`
startup database health validation (verifying clean databases pass and corrupt headers are flagged).

`cargo fmt --check`, `cargo clippy --locked --all-targets`, Glow compilation (`--features desktop`),
`verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass. Native Windows 11 RTX 4070
WGPU captures confirm real-time PBR rendering of the reborn seven-headed Leviathan sanctuary with
overlapping obsidian scales, amethyst sub-scale bioluminescence, sculpted horns/fangs, and glowing
tracking eyes. Prior release **`v1.7.0-alpha.29`** passed CI run `45980192880` and verified
`HYDRA-Leviathan-Windows-x64.exe` SHA-256 `6fdf404426f50bbc3ed00f034a9e33a3dbe895322828ccb04c81343808b1dd89`
with clean-profile launch. Public distribution does not certify unattended live trading; no real
exchange orders, live wallet credentials, or transfers were used.

# Release verification — 1.7.0-alpha.29

All **462 offline desktop-wgpu all-target tests pass** (across 32 test binaries), adding five
multi-head Project Awakening regression suites:

- **HUNTER (`src/hunter.rs`):** Loopback HTTP `metaAndAssetCtxs` acquisition, delisted/illiquid
  filtering, impact spread parsing, and portfolio-aware diversification ranking for already-held
  symbols.
- **FANG (`src/fang_planner.rs`):** Deterministic long/short geometric & arithmetic bounded grid
  generation, exact Decimal cumulative cap enforcement, monotonic average entry/take-profit/stop
  geometry, and strict rejection of Martingale multipliers `> 1.5` and level counts `> 20`.
- **COGNITION (`src/cognition_shadow.rs`):** Zero-lookahead causal market-regime inference
  (`TrendingUp`, `TrendingDown`, `MeanRevertingRange`, `HighVolatilityTurbulent`,
  `InsufficientHistory`) and prefix causality verification proving future shocks cannot alter
  prior inference.
- **AEGIS (`src/portfolio_research.rs`):** Checked-Decimal liquidation price and distance-to-
  liquidation buffer calculation, simultaneous adverse-shock stress scenarios (`-5%` to `-35%`),
  maintenance-margin breach detection, and non-positive equity refusal.
- **ORACLE (`src/oracle_validation.rs`):** Historical OHLCV dataset integrity auditing for
  timestamp reversals, missing-bar gaps, OHLC inversions, extreme spikes, and stable FIPS 180-4
  SHA-256 content fingerprinting.

`cargo fmt --check`, `cargo clippy --locked --all-targets`, Glow compilation (`--features desktop`),
`verify-memorial.ps1`, and `verify-github-compliance.ps1` all pass.

Prior release **`v1.7.0-alpha.28`** passed GitHub Actions release run `45976251961` and published
all 6 release assets; independent unauthenticated HTTPS download of `HYDRA-Leviathan-Windows-x64.exe`
verified SHA-256 `62886752aa37e5f6ec3244b2502f16e112bf21238a6116a90ba70ca0616e2277` and clean-profile
zero-argument WGPU startup (`1600 × 960`, `MODE: DORMANT`, all 7 heads + Amethyst Heart +
LEVIATHAN EVOLUTION + Ashlan memorial). No real exchange orders, live wallet credentials, or
transfers were used.

# Release verification — 1.7.0-alpha.28

All **457 offline desktop-wgpu all-target tests pass** (across 32 test binaries), adding six
**LEVIATHAN EVOLUTION** regressions in `src/evolution.rs` covering NIST FIPS 180-4 SHA-256
vectors, SemVer 2.0.0 prerelease/numeric ordering, Stable vs. Development channel and draft
filtering, `secp256k1` signed-manifest provenance verification and tamper rejection (confirming
automatic installation remains locked when unconfigured), trading-safe maintenance gate blocking
under active modes, unsafe path/host rejection, and a real loopback HTTP download, SHA-256
verification, staging, backup, and rollback roundtrip.

`cargo fmt --check`, `cargo clippy --locked --all-targets`, Glow desktop compilation
(`--features desktop --all-targets`), `verify-memorial.ps1`, and the new automated
`verify-github-compliance.ps1` check all pass. Native Windows 11 WGPU captures
(`docs/assets/leviathan-awakened-wgpu.png` and `docs/assets/leviathan-evolution-updates.png`)
verify the live `v1.7.0-alpha.28` sanctuary and Evolution workspace. No real exchange orders,
live wallet credentials, or transfers were used.

# Release verification — 1.7.0-alpha.27

All **451 offline desktop-wgpu all-target tests pass** (across 32 test binaries), adding
startup directory resolution and explicit database preservation regressions in
`src/startup_diagnostics.rs` alongside updated window-geometry coverage. Dedicated
exact-tag `.github/workflows/release.yml` validates tag-to-`Cargo.toml` version parity,
permanent memorial preservation, formatting, Clippy, desktop-wgpu and headless test suites,
`cargo audit`, standalone `HYDRA-Leviathan-Windows-x64.exe` and `hydra.exe` smoke checks,
visible non-draft GitHub Release publication, and public unauthenticated HTTP 200 + SHA-256
verification. Independent clean-folder download and native graphical launch verification
is recorded in the release handoff. No real exchange orders or live wallet credentials were used.

# Release verification — 1.7.0-alpha.26

All **449 offline desktop-wgpu all-target tests pass** (across 32 test binaries).
Added regressions cover WGSL uniform alignment and NaN sanitization (`leviathan_shader`),
physiological breathing/heartbeat/eyelid waves and genuine validation-gated Offering
illumination (`leviathan_scene`), and Treasury dedicated-wallet identity validation,
zeroizing secret redaction, Windows DPAPI / Unix `0700`/`0600` credential roundtrip
and secure overwrite-removal, loopback Hyperliquid `/info` account verification
without key transmission, readiness gating keeping `AWAKENED` locked pending
certification, and truthful 12-metric portfolio reporting (`treasury`).

Glow-only desktop compilation (`--features desktop --all-targets`), `cargo fmt --check`,
`cargo clippy --locked --all-targets`, optimized `--release` builds of both `hydra.exe`
and WGPU `hydra-cockpit.exe`, and `verify-memorial.ps1` all pass. Native Windows 11
WGPU captures (`docs/assets/leviathan-awakened-wgpu.png` and
`docs/assets/leviathan-treasury-offering.png`) confirm live shader rendering at
observed `UI 111–142/s` and `1.25–1.96 ms` UI-build CPU duration. No real exchange
orders, live wallet credentials, or transfers were used.

# Release verification — 1.7.0-alpha.25

All **418 offline desktop-feature all-target release-version tests pass**.
Four frozen-frontier regressions cover repeatability, no deployment/evidence
permission, chronology/context/completion refusal, selection independence from
held-out prices and combined candidate/fold/scenario budgets. The real CLI test
verifies input/checkpoint preservation and no engine storage/wallet even with an
invalid key and live flag. Existing 413 regressions remain green.
Formatting and Clippy complete with warnings. Fresh dependency audit exits 0:
zero vulnerability entries and one existing Chrono maintenance warning, no ignores.
Native Windows builds and extracted packages are verified before publication.
No real-account requests, signed network calls, orders or transfers were used.

Alpha.24 passed all four tagged jobs for source
`5297238f74d217e2ef945aa17b5f325a8cbf73c3`:
[Verified CI](https://github.com/ShamelesAbyss/Hydra/actions/runs/45741660923).
All five asset digests/download links match. Extracted Windows packages pass twelve
smoke checks; Linux bytes/checksum/memorial match the tagged job. Live GitHub checks
exposed third-party static badge image failures; alpha.25 replaces static badges
with repository-owned SVGs while retaining the actual dynamic CI badge.

# Release verification — 1.7.0-alpha.24

All **413 offline desktop-feature all-target release-version tests pass**.
Five GENESIS core regressions cover serialized resume/full-search parity, terminal
stability, dataset/options/state corruption rejection, structural refusal, normalized
strategy-map ordering, secret exclusion and immutable checkpoint publication.
The real CLI regression verifies resumed/full report equality, disabled deployment,
prior-file preservation and no wallet/engine storage even with invalid key/live flag.
Formatting and Clippy complete with warnings. Fresh dependency audit exits 0:
zero vulnerability entries and one existing Chrono maintenance warning, no ignores.
JSON float-roundtrip parsing is enabled; no dependencies were added. Every existing
native signing, execution, financial hold, research and telemetry regression passes.
Native builds and extracted packages are checked before publication.
No real-account probes, signed network calls, orders or transfers were used.

Alpha.23 passed all four tagged CI jobs for source
`5ef23b4d2ed1e46da4e4f9d3fe37fdd5a9323fbc`:
[Verified run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45740225803).
All five release asset digests/download links match. Extracted Windows packages
pass eleven smoke checks; Linux bytes, checksum and exact memorial were verified
against the tagged job. The COGNITION populated native capture uses the explicitly
simulated repository fixture solely for UI verification, not profitability evidence.

# Release verification — 1.7.0-alpha.23

All **407 offline desktop-feature all-target release-version tests pass**.
Twenty-two new core tests cover FANG determinism/ownership/exact budgets/precision
and EMA bounds; COGNITION sparse/recency/source-separated strict historical shadow
statistics; AEGIS exact exposure/unknown identity/budgets/correlation/resource limits.
Two integration tests verify diagnostics bypass missing engine configuration and an
invalid key without creating storage, and empty EMA input never invents a price.
Formatting and Clippy complete with existing warnings. Fresh dependency audit exits
0 with zero vulnerabilities and one existing Chrono maintenance warning, no ignores.
Native builds and extracted portable smoke are verified before publication.
No real-account requests, signed network calls, orders or transfers were used.

Alpha.22 passed all four tagged jobs for source
`2b0d8cb947bf54e4afa6a50ead986d3669437299`:
[Verified CI](https://github.com/ShamelesAbyss/Hydra/actions/runs/45738692751).
All five asset digests/download links match. Extracted Windows packages pass ten
smoke checks; Linux bytes/checksum/memorial are verified from the tagged job.

# Release verification — 1.7.0-alpha.22

All **383 offline desktop-feature all-target release-version tests pass**.
Six ORACLE validation regressions cover deterministic disjoint ranges, fresh state,
forced fold closing, adverse costs, future-price isolation, chronology, schema and
aggregate work/memory/call budgets. The existing real research CLI isolation test
now includes validation without wallet initialization or live database creation.
Formatting and Clippy complete with existing warnings. Fresh dependency audit exits
0 with zero vulnerabilities and one existing Chrono maintenance warning, no ignores.
Native builds and extracted package smoke are verified before publication.
No real account probes, signed network requests, orders or transfers were used.

Alpha.21 passed all four tagged CI jobs for source
`51010cfe67c9e41d4e326f2b5a9c53fcca75f87e`:
[Verified run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45713459367).
All five published asset digests and download links verify. Extracted Windows
CLI/cockpit packages pass nine smoke checks; Linux comes from the verified job.
Its memorial ships unchanged in README and project rules. Alpha.22 also corrects
UTF-8 encoding of the preceding changelog/test-record headings.

# Release verification — 1.7.0-alpha.21

All **377 offline desktop-feature all-target release-version tests pass**.
Eight HUNTER regressions cover approval/freshness/completeness, scoring independent
of volatility, deterministic ordering, hysteresis and invalid numeric/resource
inputs. The real CLI isolation test enables the live environment flag and supplies
an invalid private key while using a missing config; the offline command succeeds,
preserves its input bytes and creates no database or other files.

Formatting and Clippy complete with existing warnings. Fresh lockfile audit exits
0 with zero vulnerability entries and one Chrono maintenance warning, no ignores.
Native desktop check passes. Windows builds and extracted portable smoke are
verified before publication. No real account queries, signed network requests,
orders or transfers were used. Memorial verification remains mandatory.

# Release verification — 1.7.0-alpha.20

All **368 offline desktop-feature all-target release-version tests pass**.
Eight added regressions cover byte-preserving combined evidence, exact quantity
without repair/settlement permission, one WAL snapshot across an external journal
update, absent optional histories, invalid files/identity/limits, oversized native
metadata, unsupported native views/primary keys and lifecycle rowid shadowing,
and CLI configuration/wallet bypass without database creation.
The shadow-rowid corruption fixture constructs a replacement table directly;
SQLite itself rejects adding a rowid alias through ALTER TABLE.

Formatting and Clippy complete with existing warnings. Fresh lockfile audit exits
0 with zero vulnerability entries and one Chrono maintenance warning, no ignores.
Windows CLI/cockpit compilation and extracted smoke are checked before publication.
No real account probes, signed network requests, orders or transfers were used.
The permanent memorial remains the exact final README section and its rule is
included in source/portable packaging.
Alpha.20 passed all four tagged CI jobs for source
`19307270b866b02ad109ab72e15927e9760e2333`. All five published asset digests
match the local archives/manifests. Extracted Windows CLI/cockpit packages pass
eight smoke checks. Linux bytes come from the verified tagged job; its README and
maintenance rule preserve the exact memorial footer.
[Verified alpha.20 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45708423982).

# Release verification — 1.7.0-alpha.19

All **360 offline desktop-feature all-target release-version tests pass**.
Nine added regressions cover byte-preserving protocol-2 preparation inspection,
missing table/record distinction without migration, invalid scope/domain/phase and
unknown fields, hash and nonce contradictions, indexed metadata/oversize refusal,
orphan records, views/unindexed schemas, native acknowledgment refusal on corrupt
preparation, and the real CLI's configuration/wallet bypass with retained metadata.
Original times and all no-live/no-signature/no-submission-proof flags are preserved.

Formatting and Clippy complete with existing warnings. Fresh lockfile audit exits
0 with zero vulnerability entries and one Chrono maintenance warning, no ignores.
Windows CLI/cockpit compilation and extracted smoke are checked before publication.
No real account probes, signed network requests, orders or transfers were used.
The permanent memorial dedication is verified as the exact final README section.
Alpha.18 passed all four tagged CI jobs and all five release asset digests match:
[Verified alpha.18 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45702690919).

# Release verification — 1.7.0-alpha.18

All **351 offline desktop-feature all-target release-version tests pass**.
New preparation tests cover supported domain/hash/public-signer metadata, absent
secret/signature fields, scope/action/intent conflicts, invalid identity/nonce,
immutable duplicate intent/signer-nonce refusal, restart and nonce consumption on
journal failure. Ignored/rewritten JSON or indexed metadata rolls back. Native
simulated senders verify preparation before their send counter and refuse nonce
history or preparation persistence faults. The old process-local allocator test
is replaced with a durable async-reservation test; coordinator coverage remains.

Formatting and Clippy complete with existing warnings. Fresh lockfile audit exits
0 with zero vulnerability entries and one Chrono maintenance warning, no ignores.
Windows CLI/cockpit compilation and extracted smoke are checked before publication.
No real account probes, signed network requests, orders or transfers were used.
Native fixtures retain invalid keys and simulated transport as independent guards.
Alpha.17 passed all four tagged CI jobs and all five release asset digests match:
[Verified alpha.17 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45700688152).

# Release verification — 1.7.0-alpha.17

All **341 offline desktop-feature all-target release-version tests pass**.
The new coordinator tests cover restart, rollback and bounded drift/overflow,
canonical signer aliases and independent signers, concurrent SQLite connections,
foreign/future/view/invalid-column/corrupt databases, file/clock/write failures and
ignored or rewritten updates. A process fixture plus parent regression exercises
four concurrent child processes with separate bound account databases, sharing
one signer coordinator, then reopens it and reserves a later value.

Formatting and Clippy complete with warnings. Fresh lockfile audit exits 0 with
zero vulnerability entries and one Chrono maintenance warning, without ignores.
Windows CLI/cockpit release compilation and extracted smoke are checked before
publication. The live nonce sender is unchanged in this foundation milestone.
No real account probes, signed network requests, orders or transfers were used.
Alpha.16 passed all four tagged CI jobs; its five asset digests and download links
were verified:
[Verified alpha.16 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45650103078).

# Release verification — 1.7.0-alpha.16

All **330 offline desktop-feature all-target release-version tests pass**.
Seven added regressions cover retained-action acknowledgment restart, rollback on
exact overfill/sub-float limit/precision failures, sell-side limit direction,
corrupt retained evidence on subsequent reads without fallback, historical absent
record/table compatibility, incompatible table refusal, and the caller's single
WAL snapshot across an external atomic action update. Synthetic SQL mutations model
corruption/concurrency; production ingestion does not rewrite retained evidence.

Formatting and Clippy complete with warnings. Fresh lockfile audit exits 0 with
zero vulnerability entries and one Chrono maintenance warning, without ignores.
Windows CLI/cockpit release compilation and extracted smoke are checked before
publication. No real accounts, signed network requests or orders were used.
Alpha.16 passed all four tagged CI jobs for commit
`0a86095bc147e3c10bf2841de2b72ac2635f1b4b`. All five published asset digests
match and all five download URLs pass HTTP checks. Extracted Windows CLI/cockpit
packages pass seven smoke checks; Linux bytes come from the verified tagged job.
[Verified alpha.16 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45650103078).

# Release verification — 1.7.0-alpha.15

All **323 offline desktop-feature all-target release-version tests pass**.
Nine added regressions cover byte-preserving action inspection and original times,
missing files/tables/records without creation or migration, incompatible schema,
views and missing primary key, corrupt scope/precision/bytes/fields, oversized
selected data and unknown intent, internally consistent intent contradictions,
and CLI bypass of missing configuration and invalid wallet environment.

Formatting and Clippy complete with warnings. Fresh audit exits 0 with zero
vulnerability entries and one Chrono maintenance warning, no ignores. Windows CLI
and cockpit release compilation and extracted portable smoke are checked before
publication. No real account probes, signed requests, orders or transfers were used.
Alpha.15 passed all four CI jobs for tagged source `bdbae818f385d852684617acddc63a77733c2f54`.
All five published release asset digests match the local archives/manifests.
[Verified alpha.15 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45648387540).

# Release verification — 1.7.0-alpha.14

All **314 offline desktop-feature all-target release-version tests pass**.
Ten additional regressions cover durable rounded action bytes and restart,
account/intent/side/client/quantity/precision refusal, changed or terminal journal,
insert failures and immutable duplicate protection, competing SQLite connections,
native simulated sender ordering and refusal, and controller journal dispatch.
The native sender tests use an invalid signing key and a test-only simulated
sender; they send no signed requests and do not certify live transport behavior.

Formatting and Clippy complete with warnings. Fresh lockfile audit exits 0 with
zero vulnerability entries and one Chrono maintenance warning, no ignores.
Windows CLI/native release compilation and extracted portable smoke are checked
before publication. No real account probes, orders or transfers were used.
Alpha.13 passed all four tagged CI jobs and all five published asset digests match:
[verified alpha.13 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45641564303).

# Release verification — 1.7.0-alpha.13

All **304 offline desktop-feature all-target release-version tests pass**.
Nine additional tests cover exact acknowledged quantity, cloid/OID overlap without
double counting, partial/excess fills, legacy precision distinction, native scope,
reduction projection, unknown/foreign/rebate/zero fees, corrupt provenance,
unchanged holds and restart. A deterministic two-connection WAL test proves one
snapshot across an external atomic journal/fill commit.

Formatting and Clippy complete with warnings. Fresh audit exits 0 with zero
vulnerability entries and one Chrono maintenance warning, no advisory ignores.
Both Windows CLI and native cockpit release binaries compile. Extracted portable
smoke is repeated during packaging.
No real account probes, signed network requests or exchange orders were used.
Synthetic REST-decoded test data preserves the existing prohibition against
fixture-source evidence entering a mainnet-scoped ledger.
Alpha.12 passed all four tagged CI jobs:
[verified alpha.12 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45636922784).

# Release verification — 1.7.0-alpha.12

All **295 offline desktop-feature all-target release-version tests pass**.
Eight new inspector regressions cover byte-preserving read-only snapshots,
missing files/tables without creation or migration, unknown binding/identity,
unsupported versions and views, corrupt selected provenance, explicit bounded
windows and records, and the CLI configuration/wallet bypass. The partial-binding
fixture preserves existing audit metadata; no repair bypass was introduced.

Fresh lockfile audit: zero vulnerability entries and one Chrono maintenance
warning, exit 0, without advisory ignores. Formatting and Clippy complete with
warnings. Both Windows CLI and native cockpit release binaries compile. Extracted portable
smoke is repeated during packaging.
No real accounts, signed network requests or exchange orders were used.
Alpha.11 passed all four tagged CI jobs:
[verified alpha.11 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45632847453).

# Release verification — 1.7.0-alpha.11

The complete release-version desktop-feature all-target suite passes **287
offline tests**. Ten new observation tests cover native client identity, selected
exact fields, opaque/missing lifecycle states, account/intent scope, conditional
order refusal, byte/time/decimal budgets, unchanged execution/financial holds,
restart-safe replay without timestamp refresh and persistence/conflict failures.
Existing Info transport tests also verify 64 KiB and 4 MiB declared/chunked caps;
production wire serialization agrees with the observation client-ID encoding.

Formatting and Clippy complete with warnings; the new enum-size warning was
addressed before this complete verified run. Fresh audit: zero vulnerability
entries and one Chrono maintenance warning, exit 0, no advisory ignores.
Both Windows CLI and native cockpit release binaries compile; extracted
portable smoke is repeated during packaging.
No real accounts, signed network requests or exchange orders were used. HTTP
fixtures are loopback only. Alpha.10 passed all four tagged CI jobs:
[verified alpha.10 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45587952292).

# Release verification — 1.7.0-alpha.10

The complete release-version desktop-feature all-target suite passes **277
offline tests**. Six new regressions cover native aggregate order identity reuse,
competing connections, lossless refusal of historical duplicate migration,
contradictory result payloads, changed durable intent scope and missing/oversized
journal evidence. The fresh lockfile audit exits 0 with zero vulnerability entries
and one Chrono maintenance warning, without advisory ignores.

No real account probes, signed network requests or exchange orders were used.
Both Windows CLI and native cockpit release binaries compile. Clippy completes
with warnings. Extracted portable smoke and tagged CI evidence are checked
separately before publication. Alpha.9 previously passed all four tagged jobs:
[verified alpha.9 run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45585708718).

# Release verification — 1.7.0-alpha.9

The full release-version desktop-feature all-target suite passes **271 offline
tests**. The fresh lockfile audit exits 0 with zero vulnerability entries and one
Chrono maintenance warning. Sixteen new regressions beyond alpha.8 cover exact
position snapshot boundaries, residual proof, complete-zero closure, transaction
rollback, restart, runtime partial/full confirmation, failed proof/no resend,
short-side quantities and over-reduction below floating-point resolution.
No real account, signed network request or exchange order was used.

All four main consolidation CI jobs passed for source
`fbb52619a1328533062aa7fe0a8b3bf5b4312223`:
[verified main run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45581670769).
Release compilation encountered local disk exhaustion; generated build caches
were reclaimed and the release retry succeeds for both Windows CLI and native
cockpit binaries. Clippy completes with warnings. Main is the only active
development branch. Extracted portable smoke and tagged CI evidence are separate.

# Main consolidation and residual-proof verification

The complete desktop-feature all-target suite passes **267 offline tests**.
Twelve new parser/storage tests cover bounded exact position snapshots, complete
zero proofs, signed quantities, scope/time/precision contradictions, restart,
unknown legacy precision and atomic rollback at each persistence boundary.
No live account, signed request or exchange order was used.

Main promotion preserves all Leviathan and presentation commits. Main-specific
Windows/Linux/native desktop CI is tracked separately from the already verified
alpha.8 implementation and published binaries. Promotion is not stable-live
certification. Runtime-specific residual regression coverage remains next work.

# Release verification — 1.7.0-alpha.8

The release-version desktop-feature all-target suite passes all **255 offline
tests**. The fresh lockfile audit exits 0 with zero vulnerability entries and one
Chrono maintenance warning. All four implementation CI jobs passed for
`2b1560c16b8027e326bb687f43560eaaae4b699e`:
[GitHub run](https://github.com/ShamelesAbyss/Hydra/actions/runs/45579083275).
Tagged release checks and artifact smoke results are tracked separately.

# Exact native facts checkpoint after alpha.7

All **255 offline desktop-feature all-target tests pass**. The new native parser
and fourteen integration regressions verify exact quantity/price/order facts,
wire bounds below floating-point resolution, atomic journal/facts persistence,
restart ownership, account/provenance checks, position rollback, explicit unknown
residuals and no retry after a failed facts write. Nonfinite optional prices are
rejected before JSON serialization. Formatting and Clippy complete with warnings.
No live exchange account, signed network request or order was used. This source
checkpoint does not change the already published alpha.7 binaries.

# Verification — 1.7.0-alpha.7

The atomic-admission source checkpoint passes 240 offline desktop-feature tests.
The release-version suite also passes all 240 tests; the fresh release audit
exits 0 with zero vulnerability entries and one Chrono maintenance warning. Both
Windows release binaries compile; archive smoke checks are repeated in packaging.
Formatting and Clippy pass with warnings. New coverage includes both insert
failure boundaries, restart, concurrency, duplicate IDs, scoped ownership,
conservative notional, financial holds and pre-network rollback.

# Atomic admission checkpoint after alpha.6

The full desktop-feature all-target suite passes **240 offline tests**. Twelve
additional tests verify atomic reservation/journal failure rollback, durable
restart state, duplicate IDs, competing connections, authority/connection binding,
scoped inputs, bounded notional, pending orders, retained financial holds and no
network send after journal failure. Formatting and Clippy complete with warnings.
No real account access, signed requests or exchange orders were used.

# Verification — 1.7.0-alpha.6

The scoped reconciliation implementation passes 228 offline desktop-feature
all-target tests, including archive rollback, exact scoped evidence, causal proof,
runtime reuse and independent settlement holds. The release-version suite also passes all 228 tests, and the fresh release audit
exits 0 with zero vulnerability entries and one Chrono maintenance warning.
Both Windows release executables compile and pass version checks; extracted
archive smoke verification is repeated during packaging. Formatting
and Clippy pass with existing warnings. No real accounts, signed requests or
orders are used; HTTP response-limit fixtures use loopback only.

# Scoped reconciliation checkpoint after alpha.5

The full desktop-feature all-target suite passes **228 offline tests**. Eighteen
additional tests cover scoped/exact account evidence, rejected/applied release
proofs, retained ambiguous/orphan/reduced/closed holds, causality/freshness, durable
batch evidence, restart replay, injected transaction failures, independent financial
settlement holds, runtime snapshot reuse/reduction bypass and HTTP byte/diagnostic
limits. Transport fixtures use loopback only; no account probes or signed requests.

# Verification — 1.7.0-alpha.5

The release-version full desktop-feature all-target suite passes **210 offline
tests**. The fresh release lockfile audit exits 0 with **0 vulnerability entries**
and **1 Chrono maintenance warning**, without advisory ignores. Formatting and
Clippy completed on the same code with existing warnings. Both Windows release executables compile and pass version/help smoke checks.
Portable-archive isolation checks are repeated during packaging. No signed network
requests, real accounts or orders were used.

# Cross-platform lock-lifetime regression

The full desktop-feature all-target suite passes **210 offline tests** after
adding deterministic duplicate-handle ownership coverage. Linux CI exposed an
intermittent close-only lock lifetime during concurrent child-process startup.
AccountOwnership now explicitly unlocks at its RAII guard boundary. The new test
keeps an old duplicate alive, verifies immediate reacquisition after guard drop,
and verifies that closing the old duplicate cannot release the new owner's lock.
A WSL/Linux flock reproduction independently confirms the operating-system
semantics. The actual Rust Linux CI result is checked separately.

# Native dependency modernization checkpoint

At `a941c68489325afa076f5d00bee0b43e2ea97312`, the full desktop-feature,
all-target suite passes **209 offline tests**. Formatting and Clippy pass with
existing style warnings. Eight official cross-language signing fixtures remain
passing on the maintained secp256k1 signer, including a comparison of the full
production order MessagePack bytes with an independent fixture encoder.

The fresh dependency audit exits 0 with **0 vulnerability entries** and **1
unmaintained Chrono warning**. No advisory is suppressed. The legacy SDK/ethers
paths are removed from the active graph; Ratatui and rust_decimal parent upgrades
remove the remaining affected transitive copies. See DEPENDENCY_AUDIT.md for
advisory IDs, lock digest, graph changes and verification scope.

These are checkout checks. Existing alpha.4 executable downloads precede this
fix. No mainnet account probes, signed requests or orders were used. Financial
settlement, broad recovery and live-readiness certification remain outstanding.

# Entry recovery checkpoint after 1.7.0-alpha.4

The full desktop-feature suite passes 205 offline tests. Nine additional tests
cover atomic entry metadata, original risk across restart and stop ratcheting,
legacy unknown metadata, rollback on write failure, intent identity checks,
known-receipt recovery exactly once, mismatched account/position rejection,
ambiguous acknowledgment retention, and pre-entry unowned-position rejection.
No exchange access or signed actions were used in these checks.

# Verification — 1.7.0-alpha.4

The full desktop-feature suite passes 196 offline tests. New coverage includes
seven Oracle tests, five Genesis tests, research CLI storage/credential isolation,
two research-worker tests, per-instrument exit failure isolation, database account
binding, and financial decimal underflow rejection. Formatting and Clippy pass
with documented pre-existing style warnings.

The focused financial review was static. Its formal scan plugin could not resolve
the valid Windows Git worktree, so no canonical plugin scan is claimed.
Review findings drove the account binding, exit isolation and decimal fixes.
The UTF-8 diagnostic preview correction addresses a pre-existing supporting-path
panic. Complete security and financial certification remain open.

The dependency inventory still contains 19 vulnerability entries and 17 warnings.
No real exchange orders, credential probes or transfers were used in verification.

# Verification — 1.7.0-alpha.3

Full desktop-feature suite passes 178 offline tests. Both binaries compile with the optional desktop feature. The cockpit was launched and remained responsive, and operator screenshot feedback confirmed rendering. Automated computer-use interaction failed with a kernel-assets error. The telemetry CLI and explicit history initialization tests pass without a trading configuration.

# Verification — 1.7.0-alpha.2

Windows MSVC, Rust/Cargo 1.94.0. Exact source: tag v1.7.0-alpha.2.
No exchange orders, real credential probes, transfers or mainnet development tests.

- Full offline source suite: **162 passed, zero failures**.
- Includes 17 decimal ledger integration tests, 7 wire parser tests, 12 Aegis tests,
  16 execution tests, 5 ownership/equity tests and preserved strategy/learning/TUI tests.
- Formatting, Clippy and optimized build are required before publication; Clippy
  is allowed to report existing style warnings, not represented as warning-free.
- Portable Windows executable is extracted and launch checked offline before upload.
- Dependency audit: **19 vulnerability entries and 17 warnings remain**, no suppressions.
  Baseline alpha.1 had 39 and 20. See DEPENDENCY_AUDIT.md for active graph distinctions.
- Linux and Windows remote CI results are recorded on the tagged-source run/release;
  no local Linux execution claim is made.
- Original alpha.1 suite: 115 passing tests; 1.6.2 baseline: 95.

New coverage includes OS cross-process locks, agent/account binding, unknown
collateral modes, stale equity invalidation, payload-log filtering, decimal replay,
conflicting identities, fee conventions, provenance, snapshot/WebSocket duplicate
observations, transaction rollback, pending reservations, concurrency, persistent
drawdown/rearm and timestamp-boundary pagination failures.

No profitability, performance improvement, complete financial recovery, GUI or
clean-security-audit claim is made for this tag. Unknown costs/conventions and
incomplete trade/funding coverage remain explicitly unresolved.
