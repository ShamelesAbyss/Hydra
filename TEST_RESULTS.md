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
