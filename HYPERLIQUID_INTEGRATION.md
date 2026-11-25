# NEXUS account evidence and identity

The post-alpha.4 source checkpoint replaces the legacy SDK/ethers path with a
narrow native mainnet IOC order client. No live requests were used to test it.
Eight offline conformance tests exercise the native signer against published
official Python SDK hashes/signatures. Existing alpha.4 release binaries retain
the older SDK until a separately verified replacement release is published.

## Identity and account modes

Local address parsing is canonical and rejects zero/path-like identities. Before
submission, userRole must identify the master as a user; a separate signer must be
an agent whose owner is that exact account. Vault/subaccount variants are currently
refused. Identity evidence is refreshed after at most 30 seconds.

Account abstraction is queried explicitly. Only `disabled` (standard) is currently
supported for sizing. Standard perp accountValue is used alone; spot stablecoins
are not added. Unified, portfolioMargin, default and discontinued abstraction modes
require distinct collateral models and currently produce an explicit refusal.

Sources: [official account modes](https://hyperliquid.gitbook.io/hyperliquid-docs/trading/account-abstraction-modes),
[official info endpoint](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint).

## FinancialLedger

An account/environment scope separates mainnet, paper and replay records. Fill tid
and funding hash/time/coin identify economic events. Decimal values retain actual
precision. Duplicate REST/WebSocket observations do not apply cash flow twice.
Conflicting known facts abort the entire batch; previously unavailable fields can
be enriched without replacing known values. A fixture cannot enter a mainnet scope.

`realized_cash_flow` streams account facts in a consistent read transaction. Missing
fees/funding/conversions or unverified closedPnl convention leave net unavailable.
USDT is not silently treated as USDC. Builder fees are not subtracted twice.
`order_evidence` matches actual oid/cloid against a separately bound intent and
rejects excess fills, conflicting IDs and instrument/side mismatches.

## Operator read-only synchronization

```text
hydra sync-ledger --since-ms <inclusive-start> --end-ms <inclusive-end>
```

Requires the explicit HYPERLIQUID_ACCOUNT_ADDRESS. Retrieves fill/funding info only;
no order submission, cancellation, signature or transfer is performed. Pagination
overlaps timestamp boundaries, rejects saturated same-time or unsorted pages and
has fixed page/record limits. Evidence committed before interruption is replayable.
Exchange retention may omit older data; retrieval is not a full-history certificate.

This command reports account evidence, not a settled strategy trade. It does not
clear unknown acknowledgments, net-PnL holds or Aegis reservations. Complete entry
and exit fee attribution, funding coverage and snapshot/order reconciliation are
required before those restrictions can safely be removed.

## Ownership and security

OS file locks coordinate each trading account independently of its database path.
Live engine startup and emergency flatten compete for the same user-level lock;
stop the existing controller before using another. Lock files are never unlinked
while owned. Locks do not constrain other machines or exchange clients.

The native signed transport sends only one IOC order at a time to the fixed
mainnet exchange endpoint. It owns a dedicated HTTP client with redirects and
automatic retries disabled, a 15-second timeout, and a 1 MiB response bound.
Transport, HTTP and decoding failures remain errors for the durable unresolved
execution gate. No transfer, withdrawal, vault, spot or account-management mutation
API is exposed. Signing uses maintained secp256k1/libsecp256k1 primitives and
Keccak-256; the ordered action and EIP-712 encoding are checked against independent
fixtures. See [dependency assessment](DEPENDENCY_AUDIT.md).

Signed payloads and signatures are never logged by the replacement. The defensive
legacy SDK debug/trace metadata filter remains regression-tested. Private key
fields are redacted from AppConfig Debug output. Secure OS credential storage
remains open work. A passing dependency audit is not live-trading certification.

## Database account ownership

Live startup binds an empty engine database to one trading account. A bound database rejects another account. Existing unbound history requires an explicit operator assertion through bind-history --db <path> --account <address> --adopt-existing-history. Adoption records its origin in metadata and preserves existing records. It does not settle uncertain financial outcomes.

## Scoped account evidence and reservation lifecycle

The default-perp standard-account evidence reader retains canonical account scope,
clearinghouse server time, exact signed quantities/entry prices and `positionValue`
notionals from the same clearinghouse response. Outstanding order IDs and
conservative entry-order valuations are retained separately. Acquisition has a
15-second total timeout; decoded info responses are stream-capped at 4 MiB before
JSON parsing. Diagnostic previews are redacted and bounded on character boundaries.

New entries may reconcile only definitely rejected zero-fill entries or known,
untouched applied positions. Aegis rechecks the local journal/position proof under
one SQLite transaction and archives the complete validated evidence once per
batch. It never clears unknown net accounting or ambiguous acknowledgments.
REST reads are not an atomic exchange snapshot, and wall-clock revision regression
retains holds. See RISK_MANAGEMENT.md for the accepted and held categories.

Only ordinary non-trigger limit entry orders are valued automatically; unsupported
contingent entry types are refused. The default-perp endpoint may include spot
orders; unsupported symbols are refused rather than silently omitted. The
[official info schema](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint)
is the protocol reference for the retained order-type/trigger fields.

## Exact execution facts — alpha.8

Native filled receipts preserve original decimal quantity, average price and
order identity in `execution_fill_facts`, scoped to the bound mainnet account.
The native production parser validates these against actual signed wire bounds.
Facts and receipt journal status commit together; failed persistence leaves a
submitted hold. `position_precision_state` captures new entry provenance in the
same transaction as position metadata. Aegis ownership uses exact values when
present, while legacy positions retain conservative compatibility checks.

The strategy/core position view still uses floating point. Existing reductions
atomically set current exact quantity to NULL rather than invent an exact
residual; original receipt facts remain available. A recorded unknown or invalid
precision state never falls back to legacy inference. Exact residual proofs and
complete net fee/funding settlement remain separate development work.
