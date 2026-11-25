# HYDRA — Project Leviathan 1.7.0-alpha.5

Native execution dependency remediation and restart evidence, with the redesigned
seven-head project presentation. This is an engineering prerelease, not a
stable-live certification.

## Changes since alpha.4

- Persist immutable original risk and learning context atomically with each new
  position and its durable intent. Restart/stop ratcheting preserves original risk;
  legacy unknowns remain explicit.
- Recover only the narrow known-fill crash window after durable position storage,
  requiring matching bound account, receipt and fresh remote position.
- Validate net-position ownership again before entries; unknown exposure blocks
  submission.
- Replace the active legacy Hyperliquid SDK/ethers graph with a narrow mainnet
  IOC client using maintained secp256k1/libsecp256k1 primitives, Keccak, ordered
  MessagePack and independently verified EIP-712 framing. No transfer/withdrawal
  interface is exposed.
- Signed transport uses a fixed destination, no redirects or retries, a 15-second
  timeout and a 1 MiB response bound. Ambiguous failures retain unresolved state.
- Targeted rust_decimal and Ratatui parent upgrades remove affected dependency
  copies. Fresh audit: zero vulnerability entries and one Chrono maintenance warning,
  with no advisory suppressions.
- Explicit ownership unlock fixes a Linux descriptor-lifetime race while retaining
  exclusivity; deterministic duplicate-handle regression added.
- Publish original repository-owned SVG branding, subsystem panels, architecture
  and execution diagrams, plus actual empty-history/research UI captures. Default
  branch presentation changes do not merge experimental trading code.

## Verification and artifacts

The preceding source checkpoint passes 210 offline desktop-feature tests, including
all eight published cross-language signing fixtures. Release verification records
are maintained in TEST_RESULTS.md. Windows x86_64 portable packaging includes
hydra.exe, hydra-cockpit.exe, configuration and supporting guides. Executables are
unsigned. Compare the archive hashes with the attached SHA-256 manifest.

No real exchange orders, credential probes, signed requests or transfers were used
in development verification. A dependency audit pass does not establish full
security, financial correctness, profitability or live operational readiness.

## Remaining gates

Unknown acknowledgments and pending net settlement remain blocked. Complete
fee/funding settlement, broad fill/cloid recovery, runtime reservation reconciliation,
legacy learning provenance and the Chrono migration remain open. Oracle's fixed-cost
full-fill model and Genesis's bounded CPU search retain their documented limits.
