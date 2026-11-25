<div align="center">

<img src="docs/assets/leviathan-banner.svg" width="100%" alt="HYDRA — Project Leviathan. Seven heads. One engine. Continuous evolution." />

# Quantitative intelligence, engineered around evidence.

**A native Rust trading engine and research command center for Hyperliquid.**<br>
Seven coordinated disciplines. Local persistence. Explicit uncertainty. Continuous evolution.

[![Release](https://img.shields.io/badge/release-v1.7.0--alpha.5-29F0B4?style=flat-square&labelColor=101C29)](https://github.com/ShamelesAbyss/Hydra/releases/tag/v1.7.0-alpha.5) [![Development workflow status](https://github.com/ShamelesAbyss/Hydra/actions/workflows/verify.yml/badge.svg?branch=leviathan%2Fexecution-foundation)](https://github.com/ShamelesAbyss/Hydra/actions/workflows/verify.yml?query=branch%3Aleviathan%2Fexecution-foundation) [![Offline tests at 4ad92a1](https://img.shields.io/badge/offline_tests-210_at_4ad92a1-26D9FF?style=flat-square&labelColor=101C29)](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/TEST_RESULTS.md) [![Language](https://img.shields.io/badge/engine-Rust_2021-FFCA58?style=flat-square&labelColor=101C29)](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/Cargo.toml) [![License](https://img.shields.io/badge/license-MIT-B19BFF?style=flat-square&labelColor=101C29)](LICENSE)

[![Windows native](https://img.shields.io/badge/Windows-x64_native-48A8FF?style=flat-square&labelColor=101C29)](https://github.com/ShamelesAbyss/Hydra/releases/tag/v1.7.0-alpha.5) [![Linux CLI](https://img.shields.io/badge/Linux-x64_CLI-FFCA58?style=flat-square&labelColor=101C29)](https://github.com/ShamelesAbyss/Hydra/actions/runs/45560484753/job/139022720854) [![Dependency audit at a941c68](https://img.shields.io/badge/audit-0_vulnerabilities_%7C_1_warning-29F0B4?style=flat-square&labelColor=101C29)](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/DEPENDENCY_AUDIT.md)

[**Download**](#download) · [**Cockpit**](#cockpit) · [**Seven heads**](#seven-heads) · [**Architecture**](#architecture) · [**Quick start**](#quick-start) · [**Readiness**](#readiness) · [**Roadmap**](#roadmap)

</div>

> [!IMPORTANT]
> **Engineering prerelease. Live-trading readiness is incomplete.** Exchange integration exists and live execution defaults disabled. Unresolved orders and unknown net settlement block new exposure. The new source checkpoint passes 210 offline tests and a dependency audit with zero vulnerability entries and one maintenance warning. [Follow its GitHub verification](https://github.com/ShamelesAbyss/Hydra/actions/runs/45563826769). Alpha.5 includes the dependency and lock-lifetime fixes; older alpha.4 downloads remain available as historical checkpoints.

## Project Leviathan

Hydra brings market observation, strategy, adaptive context, risk, execution and research into one local quantitative workspace. Its identity is a seven-headed organism: each head has a distinct responsibility, while one engine owns the trading lifecycle and its durable evidence.

The design puts recoverable facts at the center. Intents are journaled before submission. Ambiguous acknowledgments stay ambiguous. Confirmed reductions preserve the exchange residual. Missing fees and funding remain unknown rather than becoming invented profit.

The native cockpit makes that evidence visible. Oracle replays historical candles; Genesis searches bounded parameter spaces using those simulations. Research runs independently of signing and proposes candidates for review.

### Know which Hydra you are looking at

| Track | What it contains | Where to go |
| :--- | :--- | :--- |
| **Default branch** | Original 1.6.2 code plus this presentation update | [Browse main](https://github.com/ShamelesAbyss/Hydra/tree/main) |
| **Downloadable prerelease** | **1.7.0-alpha.5**, native cockpit/research, entry recovery and dependency fixes; 210 offline tests | [Release and checksums](https://github.com/ShamelesAbyss/Hydra/releases/tag/v1.7.0-alpha.5) |
| **Development checkpoint** | `4ad92a1`, entry recovery, dependency remediation and cross-platform lock lifetime; **210 offline tests** | [Experimental branch](https://github.com/ShamelesAbyss/Hydra/tree/leviathan/execution-foundation) |

The capabilities below describe the prerelease/development implementation, with newer checkpoint changes called out. Updating this README does not merge experimental trading code into `main`.

<a id="cockpit"></a>
## A native command center

![Actual Hydra native cockpit connected to an empty local history, showing seven heads and explicitly unavailable ledger records](docs/assets/cockpit-local-history.png)

*Actual local rendering of the alpha.4-versioned development interface. Empty local history, initialized offline: no account, exchange session, performance or trading activity is implied. Zero row counts and unavailable ledger tables are shown as observed. The documentation capture uses the real eframe application; no UI mockup or generated trading data is shown.*

The obsidian interface uses cyan, amber, violet, emerald, blue, rose and orchid to distinguish its seven heads. Recorded database changes drive telemetry pulses. Missing data is shown explicitly.

| Surface | Available today |
| :--- | :--- |
| **Native cockpit** | Read-only SQLite views, seven subsystem selectors, positions, execution evidence, risk state, source/schema diagnostics, refresh and pause |
| **Research workspaces** | Local candle/config/options inputs; one background job; actual Oracle equity reports and Genesis Pareto results; JSON export by copy |
| **Terminal dashboard** | Ratatui monitoring with bounded buffers, alongside headless operation |
| **CLI** | Engine commands, configuration checks, local initialization, read-only telemetry, historical research and ledger ingestion |

The cockpit runs independently of the trading engine. Closing its window sends no trading or stop command. First-launch history initialization is an explicit local action that preserves existing files. [Cockpit guide](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/COCKPIT_GUIDE.md).

<a id="seven-heads"></a>
## Seven heads. Distinct responsibilities.

[Hunter](#hunter) · [Fang](#fang) · [Cognition](#cognition) · [Aegis](#aegis) · [Nexus](#nexus) · [Oracle](#oracle) · [Genesis](#genesis)

These are logical subsystem boundaries in an evolving Rust application. They are not seven independently deployed services, and a visible head is not a claim that every planned capability is complete.

<a id="hunter"></a>
<img src="docs/assets/head-hunter.svg" width="100%" alt="01 HUNTER — Market perception" />

**Observe the field.** Configured instruments, market snapshots, candle inputs and liquidity/spread checks supply the engine with market context. The cockpit displays recorded instrument activity.

**Next:** bounded market discovery, eligibility ranking, rotation and hysteresis. Autonomous whole-market selection is not implemented.

<a id="fang"></a>
<img src="docs/assets/head-fang.svg" width="100%" alt="02 FANG — Strategy and intent" />

**Turn context into a candidate.** Existing divergence, momentum/breakout and mean-reversion strategies produce signals and order intents. Oracle uses the existing strategy factory for deterministic candle evaluation.

**Next:** a shared deterministic live/research planner, explicit multi-strategy ownership and bounded grid planning. Grid parity, Auto-Unstuck and Martingale systems are not implemented.

<a id="cognition"></a>
<img src="docs/assets/head-cognition.svg" width="100%" alt="03 COGNITION — Adaptive context" />

**Remember with provenance.** Rolling expectancy, historical similarity, multi-timeframe regime context, adaptive strictness and bounded exploration exist. Unknown net outcomes from new reductions are excluded from learning and compounding.

**Development checkpoint:** immutable entry context and original risk distance persist atomically with positions, surviving restart and stop ratcheting. Legacy original risk remains explicitly unknown.

**Next:** authoritative fee/funding-adjusted outcomes and stronger historical provenance. Adaptive components are not a profitability claim.

<a id="aegis"></a>
<img src="docs/assets/head-aegis.svg" width="100%" alt="04 AEGIS — Persistent risk authority" />

**Protect the boundary.** Persistent equity peaks, drawdown latches, explicit audited rearm, immutable risk policies and transactional exposure reservations include pending exposure. Reservations do not expire automatically. Entry checks require fresh account evidence.

Only explicitly standard Hyperliquid account abstraction (`disabled`) is supported for valuation. Unknown, unified and portfolio-margin modes are refused; spot balances are not added to perp equity.

**Next:** evidence-driven runtime reservation reconciliation and broader independently validated account models. Full protective automation is not certified.

<a id="nexus"></a>
<img src="docs/assets/head-nexus.svg" width="100%" alt="05 NEXUS — Execution and evidence" />

**Commit facts before taking the next step.** Durable UUID/cloid intents precede submission; typed acknowledgments distinguish filled, resting, rejected and unresolved orders. Reductions require validated fills and matching remote residuals. Decimal fill/funding ingestion is account-scoped, deduplicated and bounded.

**Development checkpoint:** fresh pre-entry position ownership checks and recovery of the narrow crash window after durable position insertion, backed by a known receipt and matching account position. Unknown acknowledgments do not qualify.

**Development checkpoint:** the legacy SDK/ethers graph is replaced by a narrow mainnet IOC client using maintained secp256k1 signing primitives, ordered MessagePack and independently checked EIP-712 framing. Signed transport disables redirects/retries and bounds time and response size.

**Next:** broader cloid/fill replay and complete settlement reconciliation. Unknown net PnL continues to hold new exposure.

<a id="oracle"></a>
<img src="docs/assets/head-oracle.svg" width="100%" alt="06 ORACLE — Historical simulation" />

**Make the model explicit.** Deterministic single-instrument replay evaluates completed candles and enters at the next open. It applies fees/slippage on both legs, conservative stop-first handling and adverse stop-gap pricing. Reports retain options, date range, trades, equity, costs and drawdown.

![Actual Oracle research input workspace, cropped from a native application capture](docs/assets/oracle-workspace-detail.png)

*Oracle input workspace before loading a dataset. [Full unedited capture](docs/assets/oracle-workspace.png). No simulation result is implied.*

**Model limits:** full fills and fixed costs; no funding, order-book queues, partial fills, liquidation, adaptive learning, trailing/time-stop parity or portfolio margin. [Simulation contract](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/BACKTESTING.md).

<a id="genesis"></a>
<img src="docs/assets/head-genesis.svg" width="100%" alt="07 GENESIS — Bounded evolution" />

**Search, measure, propose.** Seeded CPU search evaluates momentum parameters through Oracle, using mutation, crossover, deduplication and Pareto filtering over return, drawdown and fee burden. Evaluation and work budgets are enforced. Reports preserve the algorithm identity and inputs.

**Next:** resumable checkpoints, out-of-sample evaluation, richer scenarios and constrained search. This is not NSGA-II, GPU optimization or automatic live deployment. [Optimization contract](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/OPTIMIZATION.md).

<a id="architecture"></a>
## Anatomy of the engine

![Actual boundaries: three user surfaces, Rust trading engine, independent Oracle/Genesis research, SQLite evidence and Hyperliquid](docs/assets/architecture.svg)

The native client reads consistent local snapshots; the headless engine owns signing and execution. Research jobs run in a bounded background worker so the interface can continue refreshing telemetry. SQLite stores durable execution, position, ledger and risk evidence.

![Execution pipeline: observe, interpret, authorize, journal, submit and confirm; uncertainty holds new exposure](docs/assets/execution-pipeline.svg)

| Invariant | Implemented behavior |
| :--- | :--- |
| **Authorization** | `ENABLE_LIVE_TRADING` is authoritative; `--live` cannot override false |
| **Durability** | Store intent before the exchange write; canonical UUID maps to cloid |
| **Uncertainty** | Preserve unresolved state; no blind resend or guessed fill |
| **Account identity** | Persistent database binding; same-user/host account ownership lock |
| **Reduction accounting** | Atomically record residual and gross evidence; net PnL remains unknown until proved |
| **Protective exits** | One instrument's close failure does not starve later instruments |
| **Restart context** | At `a941c68`, original risk/context survives; legacy unknowns are preserved |

[Architecture notes](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/ARCHITECTURE.md) · [Hyperliquid boundary](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/HYPERLIQUID_INTEGRATION.md) · [Risk authority](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/RISK_MANAGEMENT.md)

<a id="download"></a>
## Download a verified checkpoint

| Artifact | Release | Contents / verification |
| :--- | :--- | :--- |
| [**Windows x86_64 portable ZIP**](https://github.com/ShamelesAbyss/Hydra/releases/download/v1.7.0-alpha.5/hydra-1.7.0-alpha.5-x86_64-pc-windows-msvc.zip) | **alpha.5** | `hydra.exe` + `hydra-cockpit.exe`, config and guides; locally built and smoke checked |
| [**Source ZIP**](https://github.com/ShamelesAbyss/Hydra/releases/download/v1.7.0-alpha.5/hydra-1.7.0-alpha.5-source.zip) | **alpha.5** | Exact release source export; excludes inherited legacy binaries |
| [**SHA-256 manifest**](https://github.com/ShamelesAbyss/Hydra/releases/download/v1.7.0-alpha.5/hydra-1.7.0-alpha.5-SHA256SUMS.txt) | **alpha.5** | Verify the archive hash before extraction |
| [**Linux x86_64 release assets**](https://github.com/ShamelesAbyss/Hydra/releases/tag/v1.7.0-alpha.3) | **alpha.3** | Older published headless build; alpha.4 Linux CI passed, but no alpha.4 Linux executable is attached |

**Dependency fix delivery:** alpha.5 includes the remediated graph. Older alpha.4 assets retain their original dependencies and are preserved as historical downloads.

Windows executables are **unsigned**. There is no installer or code-signing claim. macOS and Linux native-desktop distribution are not currently verified. Release artifacts and source history are retained; newer branch changes are not silently substituted into a published ZIP.

<a id="quick-start"></a>
## Start locally

### Windows portable

Verify the downloaded ZIP against the release manifest, then extract it into a writable folder. Open PowerShell in that folder and keep `config/` beside the executable. Run `Get-FileHash` against the ZIP at its download location.

```powershell
.\hydra.exe --version
.\hydra.exe --help
.\hydra.exe --no-banner config-check
.\hydra-cockpit.exe
```

For a local empty history and read-only snapshot, without credentials or exchange access:

```powershell
.\hydra.exe init-history --db data/hydra.db
.\hydra.exe telemetry --db data/hydra.db --limit 100
.\hydra-cockpit.exe --db data/hydra.db
```

`init-history` refuses to overwrite an existing file. Existing history can be opened directly in the cockpit.

### Build the experimental implementation

Use Rust **1.94 or newer** for the tested toolchain. Native desktop requires the platform's graphics and build dependencies; Windows MSVC is the verified desktop target.

```sh
git clone --branch leviathan/execution-foundation https://github.com/ShamelesAbyss/Hydra.git
cd Hydra
cargo build --locked --release --bin hydra
cargo build --locked --release --features desktop --bin hydra-cockpit
cargo test --locked --features desktop --all-targets
cargo fmt --check
cargo clippy --locked --features desktop --all-targets
```

For exactly the packaged release, run `git checkout v1.7.0-alpha.5` before building. The experimental branch continues to evolve.

### Configuration

Strategy TOML lives in [`config/strategies.toml`](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/config/strategies.toml). Environment settings are documented in [`.env.example`](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/.env.example). Keep secrets outside version control.

```dotenv
ENABLE_LIVE_TRADING=false
HYPERLIQUID_PRIVATE_KEY=
HYPERLIQUID_ACCOUNT_ADDRESS=
DB_PATH=./data/hydra.db
LOG_LEVEL=info
```

No credentials are required for local history, telemetry or research. Dry-run engine operation can read public market data; it is not a paper exchange simulator. With `ENABLE_LIVE_TRADING=false`, these commands do not submit orders:

```powershell
.\hydra.exe --config config/strategies.toml run --symbols ETH,BTC
.\hydra.exe --config config/strategies.toml run --symbols ETH,BTC --no-tui
```

Account-bound commands require explicit identity and additional checks. Read [the integration guide](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/HYPERLIQUID_INTEGRATION.md) before considering account access. This README does not establish live readiness.

### Research without an engine session

```powershell
.\hydra.exe --config config/strategies.toml backtest --candles historical-candles.json
.\hydra.exe --config config/strategies.toml optimize --candles historical-candles.json
```

Candle input is an ordered JSON array containing RFC3339 `ts`, `open`, `high`, `low`, `close` and `volume`. Inputs are bounded to 32 MiB. Use sufficient warmup history; the default replay warmup is 240 bars. Add `--options path/to/options.json` for explicit settings. The GUI provides default-options JSON for copying. [Oracle guide](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/BACKTESTING.md) · [Genesis guide](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/OPTIMIZATION.md).

<a id="readiness"></a>
## Verification, security and live readiness

| Evidence | Verified state |
| :--- | :--- |
| **Local test suite** | 210 offline tests passed at `4ad92a1`; 210 in alpha.5; 196 in the historical alpha.4 release |
| **GitHub builds** | Earlier alpha.4/recovery builds passed on Windows, Linux and desktop; [new verification is running](https://github.com/ShamelesAbyss/Hydra/actions/runs/45563826769) |
| **Formatting / Clippy** | Passed locally; Clippy retains style warnings |
| **Signing references** | Eight offline cross-language fixtures against published official Python SDK constants |
| **Dependency audit** | **Source checkpoint passes**: 0 vulnerability entries, 1 Chrono maintenance warning; no advisory ignores. Alpha.5 includes the remediated dependency graph |
| **Formal security scan** | No successful canonical plugin scan is claimed; the plugin could not resolve the Windows worktree |
| **Live verification** | No real exchange orders, credential probes or transfers were used during this development verification |
| **Profitability / stable operation** | Not established; no return, benchmark, win-rate or live-safety claim |

Build success and security-audit success are separate checks. The workflow badge above shows the current aggregate result. The former failing audit is remediated in the new source checkpoint; alpha.5 ships the fix while older downloads remain unchanged. A passing test suite is evidence about tested contracts, not a trading recommendation.

Open readiness work includes complete trade settlement, broader recovery, risk-reservation reconciliation, legacy learning provenance and Chrono modernization. Legacy history is preserved without retroactive accuracy certification. [Test evidence](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/TEST_RESULTS.md) · [Dependency inventory](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/DEPENDENCY_AUDIT.md) · [Engineering audit](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/HYDRA_AUDIT.md).

<a id="roadmap"></a>
## Continuous evolution

| Stage | State | Delivery boundary |
| :--- | :--- | :--- |
| **01 / Execution foundations** | Implemented, incomplete certification | Durable intents, typed receipts, residual confirmation and uncertainty gates |
| **02 / Account evidence & Aegis** | Implemented foundation | Decimal ledger, account binding, ownership, persistent risk reservations/breakers |
| **03 / Native command center** | Implemented in alpha.3+ | Read-only telemetry, first-launch local setup, seven workspaces |
| **04 / Oracle & Genesis** | Implemented in alpha.4 | Deterministic candle replay and bounded CPU parameter search |
| **05 / Recovery & settlement** | Active | Original entry context landed; full cloid/fill replay, net accounting and reservation proof remain |
| **06 / Secure execution modernization** | Implemented source checkpoint | Legacy affected dependency paths removed; signing conformance preserved; Chrono maintenance work remains |
| **07 / Shared planning & discovery** | Planned | Hunter ranking/rotation, deterministic Fang planner, explicit strategy ownership |
| **08 / Research depth** | Planned | Out-of-sample testing, resumable optimization, multi-scenario/portfolio models |
| **09 / Operational certification** | Planned | Verified complete recovery, financial/security review and distribution hardening |

Unimplemented capabilities remain visible in the [capability ledger](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/FEATURE_PARITY.md) and [implementation plan](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/IMPLEMENTATION_PLAN.md). GPU research, NSGA-II and automatic candidate deployment are not part of the current executable contract.

## Engineering atlas

| Build and operate | Understand and extend |
| :--- | :--- |
| [Project status](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/PROJECT_STATUS.md) | [Architecture](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/ARCHITECTURE.md) |
| [Cockpit guide](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/COCKPIT_GUIDE.md) | [Implementation plan](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/IMPLEMENTATION_PLAN.md) |
| [Hyperliquid integration](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/HYPERLIQUID_INTEGRATION.md) | [Capability ledger](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/FEATURE_PARITY.md) |
| [Risk management](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/RISK_MANAGEMENT.md) | [Passivbot research](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/PASSIVBOT_AUDIT.md) |
| [Test results](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/TEST_RESULTS.md) | [Oracle simulation](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/BACKTESTING.md) |
| [Dependency audit](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/DEPENDENCY_AUDIT.md) | [Genesis optimization](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/OPTIMIZATION.md) |
| [Release notes](https://github.com/ShamelesAbyss/Hydra/blob/4ad92a1df6b1dacf656bbf572bfc3fdb2c5d4846/RELEASE_NOTES.md) | [MIT license](LICENSE) |

Supporting documents are linked to the inspected development checkpoint so they resolve from the default branch. The architecture, capability ledger and verification evidence have been refreshed from the inspected implementation. Historical milestone records remain explicitly dated.

---

<div align="center">

**SEVEN HEADS. ONE ENGINE. CONTINUOUS EVOLUTION.**

Native computation · Durable evidence · Deliberate execution

[Releases](https://github.com/ShamelesAbyss/Hydra/releases) · [Development](https://github.com/ShamelesAbyss/Hydra/tree/leviathan/execution-foundation) · [Issues](https://github.com/ShamelesAbyss/Hydra/issues)

</div>
