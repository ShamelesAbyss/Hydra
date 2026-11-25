# ARCHIVED LEGACY CODE — RETIRED — DO NOT USE OR DEVELOP

This branch preserves the historical Hydra **1.6.2** implementation. It has been
permanently superseded by Project Leviathan and is no longer maintained.
It contains known historical defects and **must not be used for live trading**.

All users, contributors and coding agents must use the authoritative
[Project Leviathan main branch](https://github.com/ShamelesAbyss/Hydra/tree/main).
Download current engineering prereleases from
[Hydra releases](https://github.com/ShamelesAbyss/Hydra/releases).
Promotion to main does not constitute stable-live certification.

This archive exists exclusively for historical preservation. Do not develop on
this branch. Original commits and release tags remain intact.

---

## Historical README (retired implementation)

# 🐉 HYDRA

**Live-fire terminal trading engine for Hyperliquid perps + Ethereum (Uniswap V3).**

A production-style, mainnet-only, always-on crypto trading bot framework written in Rust. Multi-strategy, multi-RPC failover, SQLite-persisted, restart-safe, and coloured-terminal gorgeous.

```
    ██╗  ██╗██╗   ██╗██████╗ ██████╗  █████╗
    ██║  ██║╚██╗ ██╔╝██╔══██╗██╔══██╗██╔══██╗
    ███████║ ╚████╔╝ ██║  ██║██████╔╝███████║
    ██╔══██║  ╚██╔╝  ██║  ██║██╔══██╗██╔══██║
    ██║  ██║   ██║   ██████╔╝██║  ██║██║  ██║
    ╚═╝  ╚═╝   ╚═╝   ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝
    ─────── live-fire trading engine ───────
```

> ⚠️ **This is real money**. All execution is behind the `ENABLE_LIVE_TRADING` gate. Read this entire README before you flip it to `true`.

---

## 1. What you get

| Capability | File |
|---|---|
| Forever trading loop with Ctrl-C graceful shutdown | `src/core/engine.rs` |
| Hyperliquid adapter (info + exchange, mainnet-only) | `src/adapters/hyperliquid.rs` |
| Ethereum adapter (Uniswap V3 WETH/USDC swap) | `src/adapters/ethereum.rs` |
| **Multi-RPC pool** — Alchemy → Chainstack → Ankr → Infura → public, auto-failover + exponential cooldown | `src/utils/rpc_pool.rs` |
| Strategies: divergence, momentum, mean-reversion | `src/strategies/*.rs` |
| Regime classifier | `src/strategies/regime.rs` |
| Risk engine (daily/weekly/drawdown caps, spread filter, consecutive-loss cooldown, compounding-aware sizing) | `src/risk/mod.rs` |
| Portfolio manager (trailing stops, TP, time stops) | `src/portfolio/mod.rs` |
| Restart recovery + reconcile | `src/state/mod.rs` |
| Session reports (coloured terminal + JSON on disk) | `src/reporting/mod.rs` |
| Health probes per-RPC + Hyperliquid + SQLite | `src/monitoring/mod.rs` |
| SQLite storage (WAL, forward-only migrations) | `src/storage/*.rs` |
| Rolling daily log files | `logs/hydra.log.YYYY-MM-DD` |
| Boot banner / coloured tables / on-disk session JSON | `src/utils/banner.rs` |

---

## 2. Install & build

```bash
# 1. Install Rust (skip if you already have 1.85+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env

# 2. Install pkg-config + libssl headers (Debian/Ubuntu example)
sudo apt-get install -y pkg-config libssl-dev

# 3. Unpack + build
unzip hydra-1.0.0.zip && cd hydra
cp .env.example .env    # fill in HYPERLIQUID_PRIVATE_KEY / ETH_PRIVATE_KEY
cargo build --release   # first build ≈ 4–6 minutes, then cached
```

Binary: `./target/release/hydra` (≈ 11 MB, statically linked except glibc).

---

## 3. Configure

### 3a. Secrets in `.env`

```
HYPERLIQUID_PRIVATE_KEY=   # hex, no 0x needed; same key may be reused for ETH
ETH_PRIVATE_KEY=
ENABLE_LIVE_TRADING=false  # flip to `true` only when you're ready
```

Everything else in `.env` has sensible defaults — **you can leave them blank** and the baked-in production endpoints will be used for the RPC pool:

| Slot | Provider | Credential used |
|---|---|---|
| 1 | Alchemy   | `ALCHEMY_API_KEY` (or compiled-in default) |
| 2 | Chainstack | `CHAINSTACK_URL` (or compiled-in default) |
| 3 | Ankr      | `ANKR_URL`    (or compiled-in default) |
| 4 | Infura    | `INFURA_API_KEY` (or compiled-in default) |
| 5 | Llama     | public |
| 6 | Cloudflare| public |
| 7 | Publicnode| public |

Endpoints are tried in priority order; on failure they are parked in an exponential-backoff cooldown (4s → 8s → … → 60s cap) and the next is tried. The most recently successful endpoint becomes "preferred" for the next call.

### 3b. Strategy / risk parameters in `config/strategies.toml`

Hot-editable between restarts. Live-edit during a run requires a restart.

---

## 4. Launch it

> **Default shipping profile: SMALL-SCALE** — `.env` has `ENABLE_LIVE_TRADING=true`, `DEFAULT_RISK_PCT=1.0`, `MAX_NOTIONAL_PER_MARKET_USD=150`. Designed for starting with **$50–$300 USDC on Hyperliquid** and working up. Ethereum is wired but *off* until you pass `--venue ethereum` or `--venue both`.

```bash
# 0. Quick sanity
./target/release/hydra config-check
./target/release/hydra health            # per-RPC probe grid + overall

# 1. Fund the HL account FIRST (USDC on Arbitrum → Hyperliquid bridge).
#    HYDRA's preflight check will print a loud "⚠ UNFUNDED" banner if equity < $10
#    and wait quietly for your deposit.

# 2. Launch (Hyperliquid only, default for small-scale ramp)
./target/release/hydra run --venue hyperliquid --symbols ETH,BTC

# Later, once you're comfortably funded (~$1k+), switch to the balanced preset:
./target/release/hydra --config config/strategies-balanced.toml run --venue hyperliquid --symbols ETH,BTC

# Even later, flip Ethereum on (Uniswap V3 swaps require ETH gas + USDC + WETH)
./target/release/hydra run --venue both --symbols ETH,BTC

# Inspect state (works while the engine is running)
./target/release/hydra status
./target/release/hydra positions
./target/release/hydra pnl --days 7
./target/release/hydra sessions --last 10
./target/release/hydra logs --tail 200

# Graceful stop (drops a STOP flag the engine polls)
./target/release/hydra stop

# Force reconcile local DB state against venues
./target/release/hydra recover

# Emergency close everything we track
./target/release/hydra emergency-flatten --confirm
```

### Running it 24/7

Use a terminal multiplexer or systemd:

```bash
# tmux
tmux new -d -s hydra "cd $(pwd) && ./target/release/hydra run --venue both --symbols ETH,BTC"
tmux attach -t hydra

# systemd
sudo tee /etc/systemd/system/hydra.service <<'UNIT'
[Unit]
Description=Hydra Trading Engine
After=network-online.target
Wants=network-online.target

[Service]
WorkingDirectory=/opt/hydra
ExecStart=/opt/hydra/target/release/hydra run --venue both --symbols ETH,BTC
Restart=on-failure
RestartSec=5
User=hydra

[Install]
WantedBy=multi-user.target
UNIT
sudo systemctl daemon-reload && sudo systemctl enable --now hydra
```

---

## 5. Restart / recovery behaviour

1. Each `run` writes a new `sessions` row.
2. `StateManager::reconcile_with_venues()` pulls positions from every venue.
   - Venue-has / DB-missing → **logged only** (never auto-created).
   - DB-has / venue-missing → marked closed in DB with a warning (zero realized).
3. Each open position retains its stop / take-profit / trailing-high in SQLite so management continues exactly where it left off.
4. Shutdown modes (set `SHUTDOWN_MODE`):
   - `keep_open` (default) — persisted state is authoritative; positions untouched.
   - `reduce` — halves each tracked position on the venue.
   - `flatten` — closes every tracked position.
5. A JSON session summary is always written to `data/session_reports/session_<id>.json`.

---

## 6. Compounding model

```
realized_profit += pnl_of_trade
scale            = 1 + min(max(realized_profit, 0) / starting_capital, 2)
risk_mult        = scale       (drops to 60% of scale if realized_profit < 0)
risk_budget_usd  = equity * (DEFAULT_RISK_PCT / 100) * risk_mult
order_size_coin  = (risk_budget_usd / stop_distance) * strategy_position_frac
```

Guardrails that prevent reckless martingale behaviour:
- size always re-capped by `MAX_NOTIONAL_PER_MARKET_USD`
- `MAX_CONSECUTIVE_LOSSES` → trading pauses until a non-losing trade
- strategy-level `position_frac` caps each signal individually
- per-market and portfolio exposure caps (`MAX_OPEN_EXPOSURE_PCT`)

---

## 7. Known limitations

- Ethereum 1-minute **candles** are sourced from the Hyperliquid ETH feed (HL spot/perp ≈ Uniswap mid within 5 bps). Swap **execution price** is still read on-chain from the Uniswap V3 pool.
- Only **WETH ↔ USDC (0.05% pool)** on Uniswap V3 for day 1 — extend `default_eth_pair` for more pairs.
- **No MEV protection** yet (Flashbots Protect planned). Keep `MAX_NOTIONAL_PER_MARKET_USD` modest until added.
- Hyperliquid orders are IOC by default. Add resting-GTC / post-only via `adapters/hyperliquid.rs` if you need maker execution.
- `SHUTDOWN_MODE=reduce` halves on-venue but does not yet update the DB row's size.

---

## 8. Future extensions

- Flashbots-Protect RPC slot in the pool for txns > $5k
- Hyperliquid WebSocket stream for sub-second book/trade feeds
- Ratatui full-screen TUI dashboard (live PnL, per-RPC latency heatmap)
- Funding-carry & cross-venue stat-arb strategies
- `hydra serve --port 8787` read-only JSON API over SQLite for Grafana / Discord / Telegram

---

## 9. License

MIT. Trade at your own risk. The safety gate is `false` by default for a reason.
