# Passivbot reference audit

Reference: https://github.com/enarjord/passivbot, isolated outside Hydra's source.
Development snapshot: `15d6404f33bd5623d46a46e04ea694188ea7acea`.
Highest stable-version tag observed: `v8.1.0`, commit
`7af64f3e930f4cf0cbb5fb88f31e9ce9594b0eb9`.
This tag comparison does not establish production quality of the upstream release.

The root LICENSE is the Unlicense/public-domain dedication with warranty disclaimer.
Hydra keeps MIT. No upstream implementation code is copied in this milestone.
Any future port must record its exact source commit, license and changed contracts.

## Source inspected

`passivbot-rust/src/orchestrator.rs` centralizes ideal-order construction over
explicit state/configuration inputs and deterministic exposure gating. Both the
stable tag and development snapshot have this architecture. The Rust library also
contains PyO3 bindings; it cannot simply be linked wholesale without considering
its Python-facing integration and internal module boundaries.

`strategies/ema_anchor.rs` computes inventory-shifted EMA-band quotes with dynamic
volatility offsets, wallet exposure, bid/ask constraints and directional tick
rounding. Stable/development differences include close remainder minimum handling.
`risk.rs` contains exposure gating and protective/un-stuck calculations. Those
functions depend on upstream types, exchange multipliers and signed side semantics;
copying a formula without its input contract would be unsafe.

`src/exchanges/hyperliquid.py` handles Python/CCXT exchange I/O, order update semantics,
unified balance normalization and HIP-3-specific data paths. Hydra should reproduce
validated exchange contracts natively, not inherit this runtime just to access math.

## Research status

Initial source-based architecture comparison is complete. Exhaustive review of
every requested feature, simulations, optimization, execution recovery and GPU
behavior remains scheduled. No feature is claimed integrated merely because its
upstream module exists. See FEATURE_PARITY.md for explicit gaps.

## Integration decision

Prioritize Hydra's execution/account ledger before importing strategy expansion.
Extract small deterministic numerical kernels behind Hydra-owned input types and
reference fixtures. Preserve exact side, wallet exposure, price-step and minimum
order contracts. Avoid coupling mainnet execution to unvalidated optimizer output.
