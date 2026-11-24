# HYDRA — Project Leviathan 1.7.0-alpha.2

Second recovery checkpoint: account evidence, persistent independent risk and
dependency modernization. **Alpha; not certified for unattended live use.**

Implemented: scoped decimal fill/funding ledger with exactly-once replay/conflict
handling; bounded read-only history synchronization; account/agent validation;
same-user/host ownership locks; fresh equity gates; persistent Aegis reservations,
drawdown breakers and explicit rearm API; standard-mode account valuation;
transactional core migrations; configuration redaction and SDK payload-log filtering.

**162 offline tests pass.** Compatible upgrades reduce the audit from 39 to 19
vulnerability entries and from 20 to 17 warnings. The audit still fails; no alerts
are suppressed. Signing primitive behavior is preserved pending independent fixtures.

Unknown net trade PnL, entry-cost/funding coverage and raw closedPnl convention are
not guessed. Pending reduction holds and risk reservations are not bypassed.
Unified/portfolio/default account modes and vault/subaccount entry sizing currently
fail explicitly. Full streaming recovery, reservation settlement, protective native
orders and protected credential storage remain incomplete.

Portable executables are unsigned. Verify checksums and keep live disabled during
development. Native cockpit/research work follows immediately in the next milestone;
it is not claimed implemented by this release. Original recovery points are preserved.
