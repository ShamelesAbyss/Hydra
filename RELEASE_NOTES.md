# HYDRA — Project Leviathan 1.7.0-alpha.12

Offline lifecycle inspection. Engineering prerelease on main; this is not stable-live
certification or automatic execution/financial recovery. No real account probes,
signed network requests or exchange orders were used during verification.

## Implemented

- Add `status-history --db PATH --intent UUID --limit N` and a read-only Rust
  inspector for retained native lifecycle observations.
- Dispatch before trading configuration, logging and engine storage initialization;
  no wallet configuration, migrations, venue queries or order commands occur.
- Read account binding, durable intent and selected observations in one SQLite
  read transaction. Reject incompatible/virtual/view schemas, malformed provenance,
  oversized selected payloads, unknown binding and unsupported core versions.
- Bound inspection to the newest 10,000 global observation row IDs and at most
  64 matching records. Explicit flags identify capped windows/records; local
  history is never represented as complete exchange history or a live source.
- Preserve original acquisition and record times. Missing tables are unavailable,
  missing files are never created, and inspection does not resolve any hold.
- Modernize CLI branding to Project Leviathan without altering trading capabilities.

All 295 release-version offline tests pass. The fresh audit reports zero
vulnerability entries and one Chrono maintenance warning. Clippy completes with
warnings; both Windows release binaries compile. Extracted portable smoke is repeated
during packaging. Native screenshots show the unchanged alpha.9 layout;
this milestone adds a CLI/API inspector, not a new GUI panel. Executables are unsigned.

## Remaining limits

The bounded local window may omit older observations; absence does not prove no
venue order. Retained lifecycle labels do not establish execution price, complete
fees/funding or net settlement. Operator-facing live observation, exact signed-wire
provenance, authoritative automatic recovery and stable-live readiness remain open.
