# Changelog

## 1.7.0-alpha.1 — Project Leviathan execution foundation

Experimental engineering checkpoint. Not a stable-live release.

- Make configuration the live authorization source; prevent CLI override.
- Persist UUID order intents before network writes; transmit client order IDs.
- Reject duplicate IDs and block new exposure on ambiguous/unapplied execution.
- Route shutdown and emergency reductions through the journaled submit path.
- Require typed confirmed fills and matching exchange residuals before changing positions.
- Persist partial remaining quantities and reduction evidence atomically.
- Keep unknown net PnL out of learning, compounding and realized-trade statistics.
- Refuse startup position mismatches instead of fabricating zero-PnL exits.
- Require instrument precision metadata and respect configured slippage/tick limits.
- Propagate corrupt position rows and critical storage read errors.
- Preserve existing strategy/learning tests and add offline execution/parser fault tests.
- Add Windows/Linux verification and portable packaging CI, source audits, roadmap,
  feature parity ledger and engineering handoff.
- Apply standard rustfmt formatting across the existing Rust source and tests.

Known incomplete areas: account/fill/open-order recovery; authoritative fees/funding;
equity/account-mode validation; account process ownership; dependency advisories;
protected credentials; native GUI; shared planner; historical research and optimization.

## 1.6.2 — baseline

Existing repository state at `5720ecde7565583e85dde3ebf48acafa71af35e0`.
No GitHub release/tag existed at initial inspection. See the archived original README
and Git history for earlier behavior; this project does not retroactively certify it.
