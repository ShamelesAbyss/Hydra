# HYDRA — Project Leviathan 1.7.0-alpha.10

Native receipt identity and journal consistency. Engineering prerelease on main;
this does not establish stable-live certification. No real account probes,
signed network requests or exchange orders were used in development verification.

## Implemented

- Give each account/venue/environment/native order identity one durable intent.
  A duplicate response, including a competing connection, cannot create a second
  filled receipt; failed insertion retains the submitted hold atomically.
- Revalidate exact facts against bounded durable intent and result JSON before
  seeding a position or authorizing existing precision. Missing, contradictory,
  malformed or oversized journal evidence cannot become a legacy fallback.
- Refuse migration when historical native order identities are duplicated,
  preserving every record for audited reconciliation. No automatic deduplication,
  rewriting or deletion is performed.
- Retain exact post-receipt residual proof, independent financial holds and the
  single authoritative main branch. Native screenshots show the unchanged alpha.9
  interface; no trading or simulation performance data is invented.

The release-version suite passes all 277 offline tests. The fresh audit reports
zero vulnerability entries and one Chrono maintenance warning. Both Windows CLI and native
cockpit release binaries compile; Clippy completes with warnings. Extracted
portable smoke checks are repeated during packaging. Executables remain unsigned; verify published checksums.

## Remaining limits

These checks establish local evidence consistency, not cryptographic attestation
of arbitrary database contents or complete financial settlement. Core strategy
and position compatibility views still use floating point. Complete fees/funding
coverage, broader authoritative recovery and stable-live readiness remain open.
Unknown costs and historical precision are never inferred as zero or exact.
