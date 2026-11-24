# HYDRA — Project Leviathan 1.7.0-alpha.1

First recoverable execution-foundation checkpoint. **Prerelease; not certified for
unattended live trading.** The original engine, strategies, SQLite history, Ratatui
and headless interface are preserved. This is not the native desktop GUI release.

## Changes

- Durable UUID intent journal and exchange client order IDs before submission.
- Typed order responses and conservative handling of ambiguous outcomes.
- Configuration-authoritative live gate; CLI cannot bypass disabled trading.
- Confirmed-fill and exchange-residual close handling, including partial shutdown
  reductions and emergency flatten. Failed/dry-run closes preserve positions.
- Correct instrument-dependent price precision and configured slippage bounds.
- No synthetic zero-PnL recovery exits; critical storage failures propagate.
- Gross reductions are retained separately until net fee/funding accounting exists.

## Verification

115 offline tests pass (95 original + 20 new), formatting passes, Clippy completes
with documented warnings, and the Windows release executable is built and launch
checked. Consult TEST_RESULTS.md for the final recorded evidence. Linux status is
defined by the linked CI build job, not inferred from Windows results.

Dependency scanning reports unresolved advisories; no clean-audit claim is made.
No mainnet trades or authenticated exchange probes were used for validation.

## Important limitations

Automated order/fill/funding recovery and authoritative equity/account identity are
unfinished. A confirmed reduction awaiting net accounting blocks new exposure;
there is no unsafe journal-clear button. Existing historical estimates are not
retroactively certified. Protected credential storage, account-wide process
exclusion, native GUI, simulations and optimization remain future milestones.

Portable Windows artifacts are unsigned. Verify SHA-256 before use, extract with
the supplied config directory, and start with `hydra.exe --help`. Do not treat this
alpha as a routine live upgrade. Keep private keys, `.env`, databases and personal
trade records out of source/release packages.

See PROJECT_STATUS.md for exact recovery and continuation instructions.
