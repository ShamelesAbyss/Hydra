# HYDRA — PRD & Build Log

## Product
Production-grade, mainnet-only, terminal-only live-fire crypto trading bot framework, built in Rust. Supports Hyperliquid perps + Ethereum / Uniswap V3 swaps.

## Stack
- Rust 1.95 / Tokio / Clap 4
- SQLite (rusqlite bundled, WAL)
- Tracing + tracing-appender (daily rolling)
- hyperliquid_rust_sdk 0.6 + ethers 2.0 + abigen!
- owo-colors (truecolor banner) + comfy-table

## Key features
- Coloured ASCII banner, section headings, coloured tables.
- Multi-RPC pool (Alchemy → Chainstack → Ankr → Infura → Llama → Cloudflare → Publicnode), exponential-backoff cooldowns, preferred-endpoint memoisation.
- Baked-in production API keys (Alchemy / Chainstack / Ankr / Infura / Etherscan) as fallbacks; env overrides if provided.
- Mainnet-only (testnet removed per user requirement).
- All execution behind `ENABLE_LIVE_TRADING` gate.
- Strategies: divergence, momentum, mean-reversion.
- Regime classifier: Trending / Choppy / Panic / VolSpike / NoTrade.
- Risk engine: daily/weekly loss caps, exposure caps, consec-loss cooldown, compounding-aware sizing.
- Portfolio manager: trailing stops, TP, time-stops, realized-PnL ledger.
- Restart reconcile (loud-log mismatches; never auto-trade on recovery).
- Three shutdown modes: keep_open / reduce / flatten.
- Per-session JSON report + colour-table in terminal.
- Rolling log files at `logs/hydra.log.<date>`.

## Verified
- `cargo build --release` → 11 MB `./target/release/hydra`
- `hydra config-check` → OK (banner + config summary + pool listing)
- `hydra health` → 5/7 RPCs passing (Infura 403 likely needs project-level allowlist, Cloudflare rate-limited)
- `hydra run --venue hyperliquid --symbols ETH` → live HL feed, SIGINT → clean session summary + JSON
- 4 unit tests pass (EMA monotonic, RSI bounded, ATR ≥ 0, regime classifies Choppy)

## User-supplied credentials (embedded)
- Hyperliquid private key (user-supplied)
- ETH private key (same as HL)
- Alchemy, Infura, Ankr, Chainstack, Etherscan keys (all hardcoded in `src/config/defaults.rs`)
- User-supplied keys are flagged as "compromised — rotate before serious funding"

## Backlog
P0 (before any material capital):
1. Rotate all keys that were pasted in chat
2. 24-h testnet... **removed per user requirement**. Instead: start with `MAX_NOTIONAL_PER_MARKET_USD=200` first 24h, `DEFAULT_RISK_PCT=0.25`
3. Confirm Infura project allowlist (403 in health check)

P1:
- Flashbots Protect RPC for Uniswap > $5k
- HL WebSocket feed for sub-second data
- `SHUTDOWN_MODE=reduce` size persistence to DB
- Venue-side stop orders as belt-and-braces

P2:
- Ratatui full-screen TUI
- Funding carry + cross-venue stat-arb strategies
- `hydra serve` read-only JSON API
