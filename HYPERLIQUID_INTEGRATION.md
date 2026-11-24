# NEXUS account evidence and identity

Alpha.2 retains the mainnet SDK transport/signing path. No live requests were used
to test it. The ledger and parser tests are offline. Signing primitive changes
require independent official fixtures before deployment.

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

The legacy SDK logs signed payloads at debug level. An independent metadata filter
blocks its debug/trace events even if LOG_LEVEL asks for a more specific target.
Private key fields are redacted from AppConfig Debug output. Secure OS credential
storage and removal of the legacy signing/TLS stack remain open work.
