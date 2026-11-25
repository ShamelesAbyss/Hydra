//! Account-scoped authoritative exchange facts and exact decimal cash accounting.
//!
//! This module records evidence, not an inferred position history. A close-fill
//! subtotal is not a round-trip trade outcome: entry fees, funding coverage and
//! external account activity must be attributed before learning or compounding.
//! No API here clears execution safeguards or marks a trade financially settled.
use anyhow::{ensure, Context, Result};
use rusqlite::{params, OptionalExtension};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::str::FromStr;

use crate::core::{OrderIntent, OrderResult};
use crate::storage::Storage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerEnvironment {
    Mainnet,
    Paper,
    Replay,
}
impl LedgerEnvironment {
    fn key(self) -> &'static str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Paper => "paper",
            Self::Replay => "replay",
        }
    }
}

/// Trading account identity, deliberately separate from an API agent's signer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountScope {
    environment: LedgerEnvironment,
    account: String,
}
impl AccountScope {
    pub fn hyperliquid(environment: LedgerEnvironment, account: &str) -> Result<Self> {
        ensure!(
            account.len() == 42
                && account.starts_with("0x")
                && account[2..].bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid Hyperliquid trading-account address"
        );
        ensure!(
            account[2..].bytes().any(|b| b != b'0'),
            "zero account address is not valid"
        );
        Ok(Self {
            environment,
            account: account.to_ascii_lowercase(),
        })
    }
    pub fn key(&self) -> String {
        format!("{}:hyperliquid:{}", self.environment.key(), self.account)
    }
    pub fn account(&self) -> &str {
        &self.account
    }
    pub fn environment(&self) -> LedgerEnvironment {
        self.environment
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    RestSnapshot,
    UserWebSocket,
    OfflineFixture,
}
impl EvidenceSource {
    fn key(self) -> &'static str {
        match self {
            Self::RestSnapshot => "rest",
            Self::UserWebSocket => "websocket",
            Self::OfflineFixture => "fixture",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Evidence {
    pub source: EvidenceSource,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillSide {
    Buy,
    Sell,
}

/// Hyperliquid's raw /info closedPnl and UI closed-PnL documentation must
/// not be conflated. An integration must independently establish semantics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PnlConvention {
    #[default]
    Unverified,
    BeforeFees,
    AfterFees,
}

/// Signed fee: positive means a charge, negative a rebate. Never assume USD
/// equivalence for a token merely because its name resembles a stablecoin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenAmount {
    pub token: String,
    pub amount: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeFill {
    pub fill_id: String,
    pub order_id: u64,
    pub client_order_id: Option<String>,
    pub symbol: String,
    pub side: FillSide,
    pub quantity: Decimal,
    pub price: Decimal,
    pub closed_pnl_usdc: Option<Decimal>,
    #[serde(default)]
    pub pnl_convention: PnlConvention,
    pub fee: Option<TokenAmount>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundingPayment {
    pub funding_id: String,
    pub symbol: String,
    /// Signed cash movement: positive means funding received.
    pub payment: Option<TokenAmount>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IngestResult {
    pub new_fills: u64,
    pub new_funding: u64,
    pub replays: u64,
    pub enriched: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RealizedCashFlow {
    pub fill_count: usize,
    pub funding_count: usize,
    pub known_closed_pnl_usdc: Decimal,
    pub known_fees_usdc: Decimal,
    pub known_funding_usdc: Decimal,
    pub unknown_pnl_count: usize,
    pub unknown_fee_count: usize,
    pub unknown_funding_count: usize,
    pub unsupported_tokens: BTreeSet<String>,
    /// None if any required economic component is unknown or unconverted.
    /// This is realized account cash flow, not equity or a strategy profit claim.
    pub net_usdc: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrderFillEvidence {
    pub intent_id: String,
    pub fill_ids: Vec<String>,
    pub filled_quantity: Decimal,
    pub weighted_average_price: Decimal,
    pub closed_pnl_usdc: Option<Decimal>,
    pub fees_usdc: Option<Decimal>,
    /// Only these order fills. Does NOT include other entry fees or funding.
    pub net_before_funding_usdc: Option<Decimal>,
    pub reduce_only: bool,
}

/// Additive migration, safe to call independently after Storage::migrate.
/// Coverage of one native acknowledged quantity, not a round-trip settlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcknowledgedOrderEvidence {
    pub scope: String,
    pub acknowledged_quantity: Decimal,
    pub evidence: OrderFillEvidence,
    pub round_trip_settlement_verified: bool,
}

pub fn migrate(storage: &Storage) -> Result<()> {
    storage.with_conn(|conn| -> Result<()> {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS financial_ledger_schema(version INTEGER PRIMARY KEY);
            CREATE TABLE IF NOT EXISTS ledger_accounts(
                scope TEXT PRIMARY KEY, environment TEXT NOT NULL, account TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS ledger_fills(
                scope TEXT NOT NULL REFERENCES ledger_accounts(scope), fill_id TEXT NOT NULL,
                order_id TEXT NOT NULL, client_order_id TEXT, symbol TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL, payload_json TEXT NOT NULL,
                PRIMARY KEY(scope,fill_id));
            CREATE INDEX IF NOT EXISTS idx_ledger_fill_order ON ledger_fills(scope,order_id);
            CREATE INDEX IF NOT EXISTS idx_ledger_fill_cloid ON ledger_fills(scope,client_order_id);
            CREATE TABLE IF NOT EXISTS ledger_funding(
                scope TEXT NOT NULL REFERENCES ledger_accounts(scope), funding_id TEXT NOT NULL,
                symbol TEXT NOT NULL, timestamp_ms INTEGER NOT NULL, payload_json TEXT NOT NULL,
                PRIMARY KEY(scope,funding_id));
            CREATE TABLE IF NOT EXISTS ledger_observations(
                scope TEXT NOT NULL, event_kind TEXT NOT NULL, event_id TEXT NOT NULL,
                source TEXT NOT NULL, first_seen_ms INTEGER NOT NULL, last_seen_ms INTEGER NOT NULL,
                observations INTEGER NOT NULL, PRIMARY KEY(scope,event_kind,event_id,source));
            CREATE TABLE IF NOT EXISTS ledger_intent_accounts(
                intent_id TEXT PRIMARY KEY REFERENCES execution_intents(intent_id),
                scope TEXT NOT NULL REFERENCES ledger_accounts(scope));
        ",
        )?;
        let version: i64 = tx.query_row(
            "SELECT COALESCE(MAX(version),0) FROM financial_ledger_schema",
            [],
            |r| r.get(0),
        )?;
        ensure!(
            version <= 1,
            "financial ledger schema is newer than this application"
        );
        tx.execute(
            "INSERT OR IGNORE INTO financial_ledger_schema(version) VALUES(1)",
            [],
        )?;
        tx.commit()?;
        Ok(())
    })
}

#[derive(Clone)]
pub struct FinancialLedger {
    storage: Storage,
    scope: AccountScope,
}
impl FinancialLedger {
    pub fn new(storage: Storage, scope: AccountScope) -> Result<Self> {
        migrate(&storage)?;
        storage.with_conn(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO ledger_accounts(scope,environment,account) VALUES(?1,?2,?3)",
                params![scope.key(), scope.environment.key(), scope.account],
            )
        })?;
        Ok(Self { storage, scope })
    }
    pub fn scope(&self) -> &AccountScope {
        &self.scope
    }

    /// All new facts and their provenance commit together. Conflicting reuse of
    /// an exchange identifier is an error, never INSERT OR REPLACE.
    pub fn ingest(
        &self,
        fills: &[ExchangeFill],
        funding: &[FundingPayment],
        evidence: Evidence,
    ) -> Result<IngestResult> {
        ensure!(
            fills.len() <= 2_000 && funding.len() <= 2_000,
            "ledger ingestion exceeds batch budget"
        );
        ensure!(evidence.observed_at_ms > 0, "invalid observation time");
        ensure!(
            evidence.source != EvidenceSource::OfflineFixture
                || self.scope.environment != LedgerEnvironment::Mainnet,
            "offline fixture evidence cannot enter the mainnet ledger"
        );
        let fills: Vec<_> = fills
            .iter()
            .cloned()
            .map(validate_fill)
            .collect::<Result<_>>()?;
        for payment in funding {
            validate_funding(payment)?;
        }
        self.storage.with_conn(|conn| -> Result<IngestResult> {
            let tx = conn.unchecked_transaction()?;
            let scope = self.scope.key();
            let mut result = IngestResult::default();
            for fill in &fills {
                let previous: Option<String> = tx.query_row("SELECT payload_json FROM ledger_fills WHERE scope=?1 AND fill_id=?2",
                    params![scope,fill.fill_id], |r| r.get(0)).optional()?;
                if let Some(previous) = previous {
                    let previous: ExchangeFill = serde_json::from_str(&previous)?;
                    let merged = merge_fill(&previous, fill)?;
                    if merged == previous { result.replays += 1; }
                    else {
                        tx.execute("UPDATE ledger_fills SET client_order_id=?1,payload_json=?2 WHERE scope=?3 AND fill_id=?4",
                            params![merged.client_order_id,serde_json::to_string(&merged)?,scope,merged.fill_id])?;
                        result.enriched += 1;
                    }
                } else {
                    tx.execute("INSERT INTO ledger_fills(scope,fill_id,order_id,client_order_id,symbol,timestamp_ms,payload_json) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                        params![scope,fill.fill_id,fill.order_id.to_string(),fill.client_order_id,fill.symbol,fill.timestamp_ms,serde_json::to_string(fill)?])?;
                    result.new_fills += 1;
                }
                observe(&tx,&scope,"fill",&fill.fill_id,evidence)?;
            }
            for payment in funding {
                let previous: Option<String> = tx.query_row("SELECT payload_json FROM ledger_funding WHERE scope=?1 AND funding_id=?2",
                    params![scope,payment.funding_id], |r| r.get(0)).optional()?;
                if let Some(previous) = previous {
                    let previous: FundingPayment = serde_json::from_str(&previous)?;
                    ensure!(previous.funding_id == payment.funding_id && previous.symbol == payment.symbol && previous.timestamp_ms == payment.timestamp_ms,
                        "conflicting exchange funding identifier");
                    let merged = FundingPayment { payment: enrich(&previous.payment,&payment.payment)?, ..previous.clone() };
                    if merged == previous { result.replays += 1; }
                    else {
                        tx.execute("UPDATE ledger_funding SET payload_json=?1 WHERE scope=?2 AND funding_id=?3",
                            params![serde_json::to_string(&merged)?,scope,payment.funding_id])?;
                        result.enriched += 1;
                    }
                } else {
                    tx.execute("INSERT INTO ledger_funding(scope,funding_id,symbol,timestamp_ms,payload_json) VALUES(?1,?2,?3,?4,?5)",
                        params![scope,payment.funding_id,payment.symbol,payment.timestamp_ms,serde_json::to_string(payment)?])?;
                    result.new_funding += 1;
                }
                observe(&tx,&scope,"funding",&payment.funding_id,evidence)?;
            }
            tx.commit()?;
            Ok(result)
        })
    }

    /// The caller supplies the verified trading-account context. The legacy
    /// journal has no account field, so this binding must never be guessed from
    /// a symbol or from an agent wallet's address.
    pub fn bind_intent(&self, intent_id: &str) -> Result<()> {
        self.storage.with_conn(|conn| -> Result<()> {
            let tx = conn.unchecked_transaction()?;
            let venue: String = tx.query_row(
                "SELECT venue FROM execution_intents WHERE intent_id=?1",
                [intent_id],
                |r| r.get(0),
            )?;
            ensure!(venue == "hyperliquid", "intent venue does not match ledger");
            let existing: Option<String> = tx
                .query_row(
                    "SELECT scope FROM ledger_intent_accounts WHERE intent_id=?1",
                    [intent_id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(existing) = existing {
                ensure!(
                    existing == self.scope.key(),
                    "intent already belongs to another account/environment"
                );
            } else {
                tx.execute(
                    "INSERT INTO ledger_intent_accounts(intent_id,scope) VALUES(?1,?2)",
                    params![intent_id, self.scope.key()],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub fn realized_cash_flow(&self) -> Result<RealizedCashFlow> {
        self.storage.with_conn(|conn| -> Result<RealizedCashFlow> {
            // A read transaction fixes one WAL snapshot across both tables;
            // iterating rows keeps memory independent of account history size.
            let tx = conn.unchecked_transaction()?;
            let mut out = empty_cash_flow();
            {
                let mut statement =
                    tx.prepare("SELECT payload_json FROM ledger_fills WHERE scope=?1")?;
                let rows = statement.query_map([self.scope.key()], |r| r.get::<_, String>(0))?;
                for row in rows {
                    accumulate_fill(&mut out, &serde_json::from_str::<ExchangeFill>(&row?)?)?;
                }
            }
            {
                let mut statement =
                    tx.prepare("SELECT payload_json FROM ledger_funding WHERE scope=?1")?;
                let rows = statement.query_map([self.scope.key()], |r| r.get::<_, String>(0))?;
                for row in rows {
                    accumulate_funding(&mut out, &serde_json::from_str::<FundingPayment>(&row?)?)?;
                }
            }
            finalize_cash_flow(&mut out)?;
            tx.commit()?;
            Ok(out)
        })
    }

    /// Recover fill evidence by durable cloid and/or the persisted exchange oid.
    /// Multiple partial fills are summed exactly. Missing or contradictory IDs
    /// fail closed. This does not certify complete order history or net trade PnL.
    pub fn order_evidence(&self, intent_id: &str) -> Result<OrderFillEvidence> {
        self.storage.with_conn(|conn| -> Result<_> {
            let tx = conn.unchecked_transaction()?;
            let evidence = self.order_evidence_on(&tx, intent_id)?;
            tx.commit()?;
            Ok(evidence)
        })
    }
    fn native_quantity_on(
        &self,
        conn: &rusqlite::Connection,
        intent_id: &str,
    ) -> Result<Option<Decimal>> {
        let Some((account, quantity)) =
            crate::execution_facts::native_ack_quantity_on(conn, intent_id)?
        else {
            return Ok(None);
        };
        ensure!(
            self.scope.environment() == LedgerEnvironment::Mainnet
                && self.scope.account() == account,
            "native acknowledgment ledger scope mismatch"
        );
        Ok(Some(quantity))
    }
    fn order_evidence_on(
        &self,
        conn: &rusqlite::Connection,
        intent_id: &str,
    ) -> Result<OrderFillEvidence> {
        let (intent_json,result_json,has_result):(Option<String>,Option<String>,bool)=conn.query_row(
            "SELECT CASE WHEN length(CAST(e.intent_json AS BLOB))<=32768 THEN e.intent_json END,CASE WHEN length(CAST(e.result_json AS BLOB))<=32768 THEN e.result_json END,e.result_json IS NOT NULL FROM execution_intents e JOIN ledger_intent_accounts a ON a.intent_id=e.intent_id WHERE e.intent_id=?1 AND a.scope=?2",
            params![intent_id,self.scope.key()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let intent: OrderIntent =
            serde_json::from_str(&intent_json.context("intent evidence missing or oversized")?)?;
        ensure!(
            intent.id == intent_id && intent.venue == "hyperliquid",
            "intent identity or venue mismatch"
        );
        ensure!(
            !has_result || result_json.is_some(),
            "persisted receipt exceeds evidence budget"
        );
        let result = result_json
            .map(|s| serde_json::from_str::<OrderResult>(&s))
            .transpose()?;
        if let Some(result) = &result {
            ensure!(
                result.intent_id == intent_id
                    && result.accepted
                    && result.filled_size.is_finite()
                    && result.filled_size >= 0.0
                    && result
                        .avg_fill_price
                        .is_none_or(|p| p.is_finite() && p > 0.0),
                "persisted receipt contradicts fill evidence"
            );
        }
        let expected_oid = result
            .and_then(|r| r.venue_order_id)
            .map(|s| s.parse::<u64>())
            .transpose()
            .context("invalid persisted exchange order ID")?;
        let canonical_id = canonical_cloid(intent_id)?;
        // Two bounded indexed arms avoid sorting an unbounded matching set.
        // The second excludes the first arm's matches instead of duplicating fills.
        const COLUMNS:&str="CASE WHEN length(CAST(fill_id AS BLOB))<=160 THEN fill_id END,CASE WHEN length(CAST(order_id AS BLOB))<=20 THEN order_id END,CASE WHEN client_order_id IS NULL THEN NULL WHEN length(CAST(client_order_id AS BLOB))=36 THEN client_order_id ELSE '#oversized' END,CASE WHEN length(CAST(symbol AS BLOB))<=160 THEN symbol END,timestamp_ms,CASE WHEN length(CAST(payload_json AS BLOB))<=32768 THEN payload_json END";
        let query=format!("SELECT {COLUMNS} FROM ledger_fills INDEXED BY idx_ledger_fill_cloid WHERE scope=?1 AND client_order_id=?2 UNION ALL SELECT {COLUMNS} FROM ledger_fills INDEXED BY idx_ledger_fill_order WHERE scope=?1 AND order_id=?3 AND (client_order_id IS NULL OR client_order_id<>?2) LIMIT 10001");
        let mut statement = conn.prepare(&query)?;
        let mut rows = statement.query(params![
            self.scope.key(),
            canonical_id,
            expected_oid.map(|o| o.to_string())
        ])?;
        let mut fills = Vec::new();
        let mut bytes = 0usize;
        while let Some(row) = rows.next()? {
            ensure!(fills.len() < 10_000, "order evidence exceeds record budget");
            let payload: Option<String> = row.get(5)?;
            let payload = payload.context("fill evidence missing or oversized")?;
            bytes = bytes
                .checked_add(payload.len())
                .context("fill evidence budget overflow")?;
            ensure!(
                bytes <= 16 * 1024 * 1024,
                "order evidence exceeds byte budget"
            );
            let fill = validate_fill(serde_json::from_str(&payload)?)?;
            let (id, oid, cloid, symbol, time): (String, String, Option<String>, String, i64) = (
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            );
            ensure!(
                id == fill.fill_id
                    && oid == fill.order_id.to_string()
                    && cloid == fill.client_order_id
                    && symbol == fill.symbol
                    && time == fill.timestamp_ms,
                "fill payload/index provenance disagrees"
            );
            fills.push(fill);
        }
        ensure!(!fills.is_empty(), "no authoritative fills for intent");
        fills.sort_by(|a, b| (a.timestamp_ms, &a.fill_id).cmp(&(b.timestamp_ms, &b.fill_id)));
        let mut quantity = Decimal::ZERO;
        let mut cost = Decimal::ZERO;
        for fill in &fills {
            ensure!(
                fill.symbol == intent.symbol
                    && (fill.side == FillSide::Buy) == intent.side.is_buy(),
                "fill instrument/side mismatch"
            );
            ensure!(
                fill.client_order_id
                    .as_ref()
                    .is_none_or(|id| id == &canonical_id),
                "fill client order ID mismatch"
            );
            ensure!(
                expected_oid.is_none_or(|oid| oid == fill.order_id),
                "fill exchange order ID mismatch"
            );
            quantity = add(quantity, fill.quantity)?;
            cost = add(
                cost,
                fill.quantity
                    .checked_mul(fill.price)
                    .context("fill notional overflow")?,
            )?;
        }
        ensure!(
            fills.iter().all(|f| f.order_id == fills[0].order_id),
            "client order ID maps to multiple exchange orders"
        );
        ensure!(
            intent.size.is_finite() && intent.size > 0.0,
            "invalid persisted intent quantity"
        );
        let requested = Decimal::from_str(&intent.size.to_string())?;
        ensure!(
            quantity <= requested,
            "exchange fills exceed persisted intent quantity"
        );
        if let Some(native) = self.native_quantity_on(conn, intent_id)? {
            ensure!(
                quantity <= native,
                "ledger fills exceed native acknowledged quantity"
            );
        }
        let summary = cash_flow(&fills, &[])?;
        Ok(OrderFillEvidence {
            intent_id: intent_id.into(),
            fill_ids: fills.iter().map(|f| f.fill_id.clone()).collect(),
            filled_quantity: quantity,
            weighted_average_price: cost
                .checked_div(quantity)
                .context("invalid weighted fill price")?,
            closed_pnl_usdc: if summary.unknown_pnl_count == 0 {
                Some(summary.known_closed_pnl_usdc)
            } else {
                None
            },
            fees_usdc: if summary.unknown_fee_count == 0 && summary.unsupported_tokens.is_empty() {
                Some(summary.known_fees_usdc)
            } else {
                None
            },
            net_before_funding_usdc: summary.net_usdc,
            reduce_only: intent.reduce_only,
        })
    }
    /// Require exact coverage of a known native acknowledgment. This does not
    /// prove entry-cost/funding completeness or change any settlement state.
    pub fn acknowledged_order_evidence(
        &self,
        intent_id: &str,
    ) -> Result<AcknowledgedOrderEvidence> {
        self.storage.with_conn(|conn| -> Result<_> {
            let tx = conn.unchecked_transaction()?;
            let acknowledged = self
                .native_quantity_on(&tx, intent_id)?
                .context("exact native acknowledgment unavailable")?;
            let evidence = self.order_evidence_on(&tx, intent_id)?;
            ensure!(
                evidence.filled_quantity == acknowledged,
                "native acknowledged quantity is not completely covered"
            );
            tx.commit()?;
            Ok(AcknowledgedOrderEvidence {
                scope: self.scope.key(),
                acknowledged_quantity: acknowledged,
                evidence,
                round_trip_settlement_verified: false,
            })
        })
    }
    /// Verify one reduction's exact known quantity. Legacy-only records retain
    /// their conservative float compatibility check. Net PnL stays unavailable.
    pub fn verify_reduction(&self, intent_id: &str) -> Result<OrderFillEvidence> {
        self.storage.with_conn(|conn| -> Result<_> {
            let tx = conn.unchecked_transaction()?;
            let evidence = self.order_evidence_on(&tx, intent_id)?;
            ensure!(
                evidence.reduce_only,
                "reduction evidence is not reduce-only"
            );
            let legacy: f64 = tx.query_row(
                "SELECT filled_size FROM confirmed_reductions WHERE intent_id=?1",
                [intent_id],
                |r| r.get(0),
            )?;
            ensure!(
                legacy.is_finite() && legacy > 0.0,
                "invalid reduction quantity"
            );
            let quantity = if let Some(native) = self.native_quantity_on(&tx, intent_id)? {
                let compatible: f64 = native.to_string().parse()?;
                ensure!(
                    compatible.is_finite() && compatible == legacy,
                    "reduction/native compatibility quantity disagrees"
                );
                native
            } else {
                Decimal::from_str(&legacy.to_string())?
            };
            ensure!(
                quantity == evidence.filled_quantity,
                "reduction quantity disagrees with authoritative fills"
            );
            tx.commit()?;
            Ok(evidence)
        })
    }
}

fn identifier(value: &str, name: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 160 && value.bytes().all(|b| b.is_ascii_graphic()),
        "invalid {name}"
    );
    Ok(())
}
fn validate_amount(amount: &TokenAmount) -> Result<()> {
    ensure!(
        !amount.token.is_empty()
            && amount.token.len() <= 16
            && amount.token.bytes().all(|b| b.is_ascii_alphanumeric()),
        "invalid fee/funding token"
    );
    Ok(())
}
fn canonical_cloid(id: &str) -> Result<String> {
    Ok(uuid::Uuid::parse_str(id.strip_prefix("0x").unwrap_or(id))
        .context("invalid client order ID")?
        .to_string())
}
fn validate_fill(mut fill: ExchangeFill) -> Result<ExchangeFill> {
    identifier(&fill.fill_id, "fill ID")?;
    identifier(&fill.symbol, "instrument")?;
    ensure!(
        fill.order_id > 0
            && fill.quantity > Decimal::ZERO
            && fill.price > Decimal::ZERO
            && fill.timestamp_ms > 0,
        "invalid exchange fill"
    );
    fill.quantity
        .checked_mul(fill.price)
        .context("fill notional overflow")?;
    if let Some(fee) = &fill.fee {
        validate_amount(fee)?;
    }
    fill.client_order_id = fill
        .client_order_id
        .map(|id| canonical_cloid(&id))
        .transpose()?;
    Ok(fill)
}
fn validate_funding(payment: &FundingPayment) -> Result<()> {
    identifier(&payment.funding_id, "funding ID")?;
    identifier(&payment.symbol, "instrument")?;
    ensure!(payment.timestamp_ms > 0, "invalid funding time");
    if let Some(amount) = &payment.payment {
        validate_amount(amount)?;
    }
    Ok(())
}
fn enrich<T: Clone + PartialEq>(previous: &Option<T>, incoming: &Option<T>) -> Result<Option<T>> {
    match (previous, incoming) {
        (Some(a), Some(b)) => {
            ensure!(a == b, "conflicting known exchange accounting field");
            Ok(Some(a.clone()))
        }
        (Some(a), None) => Ok(Some(a.clone())),
        (None, value) => Ok(value.clone()),
    }
}
fn merge_fill(previous: &ExchangeFill, incoming: &ExchangeFill) -> Result<ExchangeFill> {
    ensure!(
        previous.fill_id == incoming.fill_id
            && previous.order_id == incoming.order_id
            && previous.symbol == incoming.symbol
            && previous.side == incoming.side
            && previous.quantity == incoming.quantity
            && previous.price == incoming.price
            && previous.timestamp_ms == incoming.timestamp_ms,
        "conflicting exchange fill identifier"
    );
    let convention = match (previous.pnl_convention, incoming.pnl_convention) {
        (PnlConvention::Unverified, next) => next,
        (known, PnlConvention::Unverified) => known,
        (a, b) => {
            ensure!(a == b, "conflicting closed-PnL fee convention");
            a
        }
    };
    Ok(ExchangeFill {
        pnl_convention: convention,
        client_order_id: enrich(&previous.client_order_id, &incoming.client_order_id)?,
        closed_pnl_usdc: enrich(&previous.closed_pnl_usdc, &incoming.closed_pnl_usdc)?,
        fee: enrich(&previous.fee, &incoming.fee)?,
        ..previous.clone()
    })
}
fn observe(
    tx: &rusqlite::Transaction<'_>,
    scope: &str,
    kind: &str,
    id: &str,
    evidence: Evidence,
) -> Result<()> {
    tx.execute("INSERT INTO ledger_observations(scope,event_kind,event_id,source,first_seen_ms,last_seen_ms,observations) VALUES(?1,?2,?3,?4,?5,?5,1) ON CONFLICT(scope,event_kind,event_id,source) DO UPDATE SET first_seen_ms=MIN(first_seen_ms,excluded.first_seen_ms),last_seen_ms=MAX(last_seen_ms,excluded.last_seen_ms),observations=MIN(observations+1,2147483647)",
        params![scope,kind,id,evidence.source.key(),evidence.observed_at_ms])?;
    Ok(())
}
fn add(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_add(b).context("financial decimal overflow")
}
fn cash_flow(fills: &[ExchangeFill], funding: &[FundingPayment]) -> Result<RealizedCashFlow> {
    let mut out = empty_cash_flow();
    for fill in fills {
        accumulate_fill(&mut out, fill)?;
    }
    for payment in funding {
        accumulate_funding(&mut out, payment)?;
    }
    finalize_cash_flow(&mut out)?;
    Ok(out)
}
fn empty_cash_flow() -> RealizedCashFlow {
    RealizedCashFlow {
        fill_count: 0,
        funding_count: 0,
        known_closed_pnl_usdc: Decimal::ZERO,
        known_fees_usdc: Decimal::ZERO,
        known_funding_usdc: Decimal::ZERO,
        unknown_pnl_count: 0,
        unknown_fee_count: 0,
        unknown_funding_count: 0,
        unsupported_tokens: BTreeSet::new(),
        net_usdc: None,
    }
}
fn accumulate_fill(out: &mut RealizedCashFlow, fill: &ExchangeFill) -> Result<()> {
    out.fill_count += 1;
    let gross_pnl = match (fill.closed_pnl_usdc, fill.pnl_convention) {
        (Some(pnl), PnlConvention::BeforeFees) => Some(pnl),
        (Some(pnl), PnlConvention::AfterFees) => match &fill.fee {
            Some(fee) if fee.token == "USDC" => Some(add(pnl, fee.amount)?),
            _ => None,
        },
        _ => None,
    };
    match gross_pnl {
        Some(pnl) => out.known_closed_pnl_usdc = add(out.known_closed_pnl_usdc, pnl)?,
        None => out.unknown_pnl_count += 1,
    }
    match &fill.fee {
        Some(fee) if fee.token == "USDC" => {
            out.known_fees_usdc = add(out.known_fees_usdc, fee.amount)?
        }
        Some(fee) => {
            unsupported_token(out, &fee.token)?;
        }
        None => out.unknown_fee_count += 1,
    }
    Ok(())
}
fn accumulate_funding(out: &mut RealizedCashFlow, payment: &FundingPayment) -> Result<()> {
    out.funding_count += 1;
    match &payment.payment {
        Some(amount) if amount.token == "USDC" => {
            out.known_funding_usdc = add(out.known_funding_usdc, amount.amount)?
        }
        Some(amount) => {
            unsupported_token(out, &amount.token)?;
        }
        None => out.unknown_funding_count += 1,
    }
    Ok(())
}
fn unsupported_token(out: &mut RealizedCashFlow, token: &str) -> Result<()> {
    ensure!(
        out.unsupported_tokens.contains(token) || out.unsupported_tokens.len() < 64,
        "unsupported token report exceeds resource budget"
    );
    out.unsupported_tokens.insert(token.into());
    Ok(())
}
fn finalize_cash_flow(out: &mut RealizedCashFlow) -> Result<()> {
    if out.unknown_pnl_count == 0
        && out.unknown_fee_count == 0
        && out.unknown_funding_count == 0
        && out.unsupported_tokens.is_empty()
    {
        out.net_usdc = Some(add(
            out.known_closed_pnl_usdc
                .checked_sub(out.known_fees_usdc)
                .context("financial decimal overflow")?,
            out.known_funding_usdc,
        )?);
    }
    Ok(())
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    #[test]
    fn journal_and_fills_share_one_snapshot_across_an_external_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snapshot.db");
        let db = Storage::open(&path).unwrap();
        db.migrate().unwrap();
        let ledger = FinancialLedger::new(
            db.clone(),
            AccountScope::hyperliquid(
                LedgerEnvironment::Replay,
                "0x1111111111111111111111111111111111111111",
            )
            .unwrap(),
        )
        .unwrap();
        let intent = OrderIntent {
            id: uuid::Uuid::new_v4().to_string(),
            venue: "hyperliquid".into(),
            symbol: "ETH".into(),
            side: crate::core::Side::Long,
            size: 2.0,
            limit_price: Some(100.0),
            reduce_only: false,
            stop_price: None,
            take_profit: None,
            source_signal: None,
            created_at: chrono::Utc::now(),
        };
        let mut result = OrderResult {
            intent_id: intent.id.clone(),
            accepted: true,
            venue_order_id: Some("42".into()),
            filled_size: 2.0,
            avg_fill_price: Some(100.0),
            message: "offline snapshot fixture".into(),
        };
        db.reserve_intent(&intent).unwrap();
        db.finish_intent(&intent.id, "filled", Some(&result))
            .unwrap();
        ledger.bind_intent(&intent.id).unwrap();
        let mut fill = ExchangeFill {
            fill_id: "snapshot-fill".into(),
            order_id: 42,
            client_order_id: Some(intent.id.clone()),
            symbol: "ETH".into(),
            side: FillSide::Buy,
            quantity: Decimal::from(2),
            price: Decimal::from(100),
            closed_pnl_usdc: None,
            pnl_convention: PnlConvention::Unverified,
            fee: None,
            timestamp_ms: 1_800_000_000_000,
        };
        ledger
            .ingest(
                &[fill.clone()],
                &[],
                Evidence {
                    source: EvidenceSource::OfflineFixture,
                    observed_at_ms: 1_800_000_000_001,
                },
            )
            .unwrap();
        let writer = Storage::open(&path).unwrap();
        db.with_conn(|conn| -> Result<()> {
            let tx = conn.unchecked_transaction()?;
            let _: String = tx.query_row(
                "SELECT result_json FROM execution_intents WHERE intent_id=?1",
                [&intent.id],
                |r| r.get(0),
            )?;
            // A synthetic atomic correction on another connection tests snapshot
            // isolation; normal ingest does not rewrite established fill facts.
            result.venue_order_id = Some("43".into());
            result.filled_size = 1.0;
            fill.order_id = 43;
            fill.quantity = Decimal::ONE;
            writer.with_conn(|conn| -> Result<()> {
                let update = conn.unchecked_transaction()?;
                update.execute(
                    "UPDATE execution_intents SET result_json=?1 WHERE intent_id=?2",
                    params![serde_json::to_string(&result)?, intent.id],
                )?;
                update.execute(
                    "UPDATE ledger_fills SET order_id='43',payload_json=?1 WHERE scope=?2",
                    params![serde_json::to_string(&fill)?, ledger.scope.key()],
                )?;
                update.commit()?;
                Ok(())
            })?;
            assert_eq!(
                ledger.order_evidence_on(&tx, &intent.id)?.filled_quantity,
                Decimal::from(2)
            );
            tx.commit()?;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            ledger.order_evidence(&intent.id).unwrap().filled_quantity,
            Decimal::ONE
        );
    }
}
