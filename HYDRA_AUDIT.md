# HYDRA source audit — execution foundation

Baseline: `5720ecde7565583e85dde3ebf48acafa71af35e0`, version 1.6.2.
The remote default branch matched that commit at initial inspection. The repository
had 53 tracked files, no CI workflow directory, no tags and no published releases.
The baseline Windows `cargo test --locked --all-targets` passed 95 tests (79 unit,
8 learning integration, 4 indicator smoke, 4 TUI smoke). The initial build took
4m50s including dependency acquisition; this is not a performance benchmark.

## Findings and disposition

| Area | Evidence in baseline | Foundation disposition |
|---|---|---|
| Position exits | `manage_open_positions` discarded close result and archived midpoint PnL | Require fill and matching residual; failures preserve position |
| Shutdown reduce | Submitted half-size close without persisting residual | Persist confirmed residual atomically |
| Shutdown flatten | Ignored response and archived zero-PnL exit | Use same confirmed reduction path |
| Emergency flatten | Wrote closed records even with execution disabled | Refuse before constructing adapter when disabled; shared path when enabled |
| Authorization | `live_override || cfg.live_trading` | Configuration authoritative; CLI cannot bypass |
| Order response | Parsed Debug strings; defaulted unknown fill size to requested size | Match SDK enums; malformed/unknown becomes unresolved |
| Client ID | SDK `cloid: None` | Canonical UUID, persisted before send, transmitted as cloid |
| Persistence | Intent only recorded after network response | Durable intent before network; unique key prevents resend |
| Recovery | Symbol-only matching; missing positions archived at entry/zero PnL | Quantity/direction/entry mismatches fail closed; preserve records |
| Position read | SQL errors/defaults and corrupt rows could appear empty | Row decode and exposure read errors propagate |
| Tick metadata | Unknown instrument used 2dp; price ignored size-decimal relation | Metadata required; price cap uses `6 - szDecimals` |
| Slippage | Market IOC used fixed 3% cross | Configured bps; round toward the price bound |
| Realized outcomes | Midpoint estimates entered learning/compounding | Confirmed reductions quarantined until net accounting exists |

## Preserved capabilities

Three strategy modules remain intact: RSI divergence, momentum/breakout, and
Bollinger/RSI mean reversion. ATR stops, break-even/chandelier trailing, trend and
regime filters, cooldowns, statistical expectancy, feature similarity, adaptive
strictness, bounded exploration, and SQLite learning history remain present.
Existing tests continue exercising their behavior. Strategy profitability is not
established by these tests.

The active adapter registry is Hyperliquid-only. The Ethereum source file is
unregistered; README claims of active Uniswap support were stale. CLI includes
run, status, positions, PnL, sessions, logs, stop, config-check, recover, health,
cooldown reset, and emergency flatten. Ratatui uses bounded display buffers and
best-effort dashboard writes independent of order execution.

## Important unresolved limitations

This is an initial execution-focused audit, not a completed security assessment.
No live orders or authenticated exchange account probes were used in development.

1. Fill-history ingestion, exchange order lookup, deduplicated fees/funding and
   automatic restart repair are absent. Journal evidence survives restart but
   unresolved records require investigation; no unsafe clear/reset command is supplied.
2. Account identity and agent signer identity still need explicit verification,
   account modes need validation, and equity composition can be wrong for unified
   margin. Equity failures can become zero, with a stale balance
   used by risk sizing. This prevents stable-live certification.
3. Exchange position reconciliation does not yet include open orders, fills,
   funding, account mode, or external account mutations during a running session.
4. Account-wide multi-process exclusion, protected credential storage, key rotation
   guidance from a full history scan, and complete dependency remediation are pending.
5. Polling refetches mids and candles per symbol; no incremental stream cache.
   Sequential fetch latency can dominate strategy processing.
6. Some configuration switches are not fully implemented (including partial TP);
   numerical config validation is incomplete. Do not infer runtime guarantees
   from a field's existence.
7. Entry learning context/initial R are in memory. Fee/funding-adjusted outcomes,
   learning recovery and environment isolation need implementation.
8. No native GUI, independent local IPC daemon, backtester, optimizer, GPU engine,
   installer or measured resource baseline exists yet.

## Verification scope

Offline fake exchanges reproduce partial/rejected/ambiguous fills, dry-run closes,
unknown account positions, stale residuals, SQLite failures, duplicate IDs and
restart journal recovery. Parser fixtures use the installed SDK's actual types.
No claim of mainnet readiness follows from offline test success.
