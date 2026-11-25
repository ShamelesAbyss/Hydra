//! Independent, persistent risk authority. No strategy callbacks or exchange writes.
//!
//! Callers must supply authoritative account snapshots including both positions
//! and outstanding entry orders. Reservations never expire automatically. They
//! are released only atomically with a newer reconciled account snapshot.
use anyhow::{ensure, Context, Result};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::storage::Storage;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskLimits {
    /// Ratios, not percentages: 0.5 permits notional of half the account equity.
    pub max_total_exposure_ratio: Decimal,
    pub max_market_notional: Decimal,
    pub max_leverage: Decimal,
    pub max_drawdown_ratio: Decimal,
    pub max_snapshot_age_ms: i64,
}

impl RiskLimits {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.max_total_exposure_ratio > Decimal::ZERO,
            "total exposure limit must be positive"
        );
        ensure!(
            self.max_market_notional > Decimal::ZERO,
            "market limit must be positive"
        );
        ensure!(
            self.max_leverage >= Decimal::ONE,
            "leverage limit must be at least one"
        );
        ensure!(
            self.max_drawdown_ratio > Decimal::ZERO && self.max_drawdown_ratio < Decimal::ONE,
            "drawdown ratio must be in (0,1)"
        );
        ensure!(
            self.max_snapshot_age_ms > 0,
            "snapshot freshness limit must be positive"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountSnapshot {
    /// Accepted revisions cannot regress. The current adapter uses acquisition-
    /// start milliseconds; clock regression retains holds rather than inventing
    /// a later observation. A persistent ingestion counter remains future work.
    pub revision: i64,
    pub observed_at_ms: i64,
    pub equity: Decimal,
    /// Absolute notional by instrument, including all outstanding entry orders.
    pub exposure_by_market: BTreeMap<String, Decimal>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reservation {
    pub intent_id: String,
    pub symbol: String,
    pub notional: Decimal,
    pub snapshot_revision: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskDecision {
    Approved(Reservation),
    Blocked {
        reason: String,
        breaker_latched: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskStatus {
    pub account: String,
    pub high_water_equity: Decimal,
    pub breaker_reason: Option<String>,
    pub active_reservations: usize,
    pub reserved_notional: Decimal,
    pub latest_snapshot_revision: Option<i64>,
}

/// Construct only from an explicit operator interaction, never strategy output.
#[derive(Clone, Debug)]
pub struct RearmAuthorization {
    operator_id: String,
    reason: String,
    reset_equity_peak: bool,
}

impl RearmAuthorization {
    pub fn explicit(operator_id: &str, reason: &str, reset_equity_peak: bool) -> Result<Self> {
        validate_label(operator_id, "operator identifier")?;
        validate_label(reason, "rearm explanation")?;
        Ok(Self {
            operator_id: operator_id.into(),
            reason: reason.into(),
            reset_equity_peak,
        })
    }
}

#[derive(Clone)]
pub struct Aegis {
    storage: Storage,
    account: String,
    limits: RiskLimits,
}

impl Aegis {
    /// One guard per Hyperliquid trading-account address; signer addresses do not
    /// establish ownership. This database scope is not an OS process lock.
    pub fn new(storage: Storage, trading_account: &str, limits: RiskLimits) -> Result<Self> {
        limits.validate()?;
        ensure!(
            trading_account.len() == 42
                && trading_account.starts_with("0x")
                && trading_account[2..].bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid trading account address"
        );
        ensure!(
            trading_account[2..].bytes().any(|b| b != b'0'),
            "zero trading account is invalid"
        );
        let account = trading_account.to_ascii_lowercase();
        storage.with_conn(|conn| -> Result<()> {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS aegis_accounts (
                    account TEXT PRIMARY KEY, limits_json TEXT NOT NULL,
                    high_water_equity TEXT NOT NULL, breaker_reason TEXT,
                    snapshot_json TEXT, snapshot_revision INTEGER
                );
                CREATE TABLE IF NOT EXISTS aegis_reservations (
                    intent_id TEXT PRIMARY KEY, account TEXT NOT NULL REFERENCES aegis_accounts(account),
                    symbol TEXT NOT NULL, notional TEXT NOT NULL, snapshot_revision INTEGER NOT NULL,
                    created_at_ms INTEGER NOT NULL, state TEXT NOT NULL CHECK(state IN ('active','reconciled')),
                    reconciliation_id TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_aegis_reservations_scope ON aegis_reservations(account,state);
                CREATE TABLE IF NOT EXISTS aegis_events (
                    id INTEGER PRIMARY KEY, account TEXT NOT NULL, kind TEXT NOT NULL,
                    reason TEXT NOT NULL, operator_id TEXT, at_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS aegis_reconciliations (
                    account TEXT NOT NULL REFERENCES aegis_accounts(account),
                    snapshot_revision INTEGER NOT NULL,
                    evidence_json TEXT NOT NULL,
                    recorded_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(account,snapshot_revision)
                );"
            )?;
            let serialized = serde_json::to_string(&limits)?;
            conn.execute("INSERT OR IGNORE INTO aegis_accounts (account,limits_json,high_water_equity) VALUES (?1,?2,'0')",
                params![account, serialized])?;
            let stored: String = conn.query_row("SELECT limits_json FROM aegis_accounts WHERE account=?1", [&account], |r| r.get(0))?;
            ensure!(serde_json::from_str::<RiskLimits>(&stored)? == limits,
                "risk limits differ from persistent account policy; explicit policy migration required");
            Ok(())
        })?;
        Ok(Self {
            storage,
            account,
            limits,
        })
    }

    /// Atomically check positions, orders and every outstanding reservation.
    /// A blocked decision is not an error; drawdown latches are committed before
    /// returning it. SQL/validation failures return errors and permit no action.
    pub fn reserve_entry(
        &self,
        intent_id: &str,
        symbol: &str,
        notional: Decimal,
        requested_leverage: Decimal,
        snapshot: &AccountSnapshot,
        now_ms: i64,
    ) -> Result<RiskDecision> {
        let request = EntryRiskRequest {
            intent_id,
            symbol,
            notional,
            requested_leverage,
            snapshot,
            now_ms,
        };
        self.storage.with_conn(|conn| -> Result<RiskDecision> {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let decision = self.reserve_entry_on(&tx, &request)?;
            tx.commit()?;
            Ok(decision)
        })
    }

    pub(crate) fn uses_storage(&self, storage: &Storage) -> bool {
        self.storage.shares_connection(storage)
    }

    /// Live admission persists the intent and risk reservation in one transaction.
    /// Rejections commit their breaker state but never create a submitted intent.
    pub fn reserve_journaled_entry(
        &self,
        intent: &crate::core::OrderIntent,
        notional: Decimal,
        requested_leverage: Decimal,
        evidence: &crate::adapters::account_evidence::AccountExposureEvidence,
        now_ms: i64,
    ) -> Result<RiskDecision> {
        evidence.validate(now_ms, self.limits.max_snapshot_age_ms)?;
        ensure!(
            evidence.account == self.account
                && intent.venue == evidence.venue
                && !intent.reduce_only,
            "entry admission account, venue or action mismatch"
        );
        ensure!(
            intent.size.is_finite() && intent.size > 0.0,
            "invalid journaled entry size"
        );
        let price = intent
            .limit_price
            .context("journaled entry requires a bounded price")?;
        ensure!(
            price.is_finite() && price > 0.0,
            "invalid journaled entry price"
        );
        for price in [intent.stop_price, intent.take_profit]
            .into_iter()
            .flatten()
        {
            ensure!(
                price.is_finite() && price > 0.0,
                "invalid entry protective price"
            );
        }
        let minimum = parse(&intent.size.to_string())?
            .checked_mul(parse(&price.to_string())?)
            .context("entry price/size overflow")?;
        ensure!(
            notional >= minimum && minimum > Decimal::ZERO,
            "entry reservation understates bounded notional"
        );
        let request = EntryRiskRequest {
            intent_id: &intent.id,
            symbol: &intent.symbol,
            notional,
            requested_leverage,
            snapshot: &evidence.snapshot,
            now_ms,
        };
        self.storage.with_conn(|conn| -> Result<RiskDecision> {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let account:Option<String>=tx.query_row("SELECT value FROM meta_kv WHERE key='trading_account_binding'",[],|r|r.get(0)).optional()?;
            ensure!(account.as_deref()==Some(self.account.as_str()), "entry admission requires bound trading history");
            validate_owned_evidence(&tx,evidence)?;
            let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM positions WHERE venue=?1 AND symbol=?2 AND closed_at IS NULL)",params![intent.venue,intent.symbol],|r|r.get(0))?;
            ensure!(!owned && !evidence.open_orders.iter().any(|order|order.symbol==intent.symbol),
                "instrument already has a position or pending order");
            let unresolved:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM execution_intents WHERE state NOT IN ('rejected','applied')) OR EXISTS(SELECT 1 FROM confirmed_reductions WHERE net_pnl IS NULL)",[],|r|r.get(0))?;
            ensure!(!unresolved, "execution or financial settlement remains unresolved");
            let decision = self.reserve_entry_on(&tx,&request)?;
            if matches!(&decision,RiskDecision::Approved(_)) {
                Storage::insert_intent_on(&tx,intent)?;
            }
            tx.commit()?;
            Ok(decision)
        })
    }

    fn reserve_entry_on(
        &self,
        tx: &Transaction<'_>,
        request: &EntryRiskRequest<'_>,
    ) -> Result<RiskDecision> {
        let EntryRiskRequest {
            intent_id,
            symbol,
            notional,
            requested_leverage,
            snapshot,
            now_ms,
        } = *request;
        ensure!(
            uuid::Uuid::parse_str(intent_id)?.to_string() == intent_id,
            "noncanonical intent UUID"
        );
        validate_symbol(symbol)?;
        ensure!(notional > Decimal::ZERO, "entry notional must be positive");
        ensure!(
            requested_leverage >= Decimal::ONE,
            "requested leverage must be at least one"
        );
        self.validate_snapshot(snapshot, now_ms)?;
        let duplicate: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM aegis_reservations WHERE intent_id=?1)",
            [intent_id],
            |r| r.get(0),
        )?;
        ensure!(
            !duplicate,
            "intent already has a risk reservation; never reserve twice"
        );
        self.accept_snapshot(tx, snapshot)?;
        let (peak, breaker) = self.account_state(tx)?;
        let mut latched = breaker.is_some();
        let reason = if let Some(reason) = breaker {
            Some(format!("persistent circuit breaker: {reason}"))
        } else if peak > Decimal::ZERO
            && snapshot.equity <= mul(peak, sub(Decimal::ONE, self.limits.max_drawdown_ratio)?)?
        {
            let reason = "equity drawdown limit reached";
            self.latch(tx, reason, now_ms)?;
            latched = true;
            Some(reason.into())
        } else if snapshot.equity <= Decimal::ZERO {
            Some("account equity unavailable or zero".into())
        } else if requested_leverage > self.limits.max_leverage {
            Some("requested leverage exceeds hard limit".into())
        } else {
            self.exposure_block(tx, snapshot, symbol, notional)?
        };
        if let Some(reason) = reason {
            return Ok(RiskDecision::Blocked {
                reason,
                breaker_latched: latched,
            });
        }
        tx.execute("INSERT INTO aegis_reservations (intent_id,account,symbol,notional,snapshot_revision,created_at_ms,state) VALUES (?1,?2,?3,?4,?5,?6,'active')",
                params![intent_id,self.account,symbol,notional.to_string(),snapshot.revision,now_ms])?;
        Ok(RiskDecision::Approved(Reservation {
            intent_id: intent_id.into(),
            symbol: symbol.into(),
            notional,
            snapshot_revision: snapshot.revision,
        }))
    }

    /// No timeout-based releases. The caller must have reconciled every named
    /// intent against exchange order/fill evidence and provide a newer snapshot
    /// which already includes its resulting exposure. Ambiguous orders stay held.
    pub fn apply_reconciled_snapshot(
        &self,
        snapshot: &AccountSnapshot,
        resolved_intents: &[String],
        reconciliation_id: &str,
        now_ms: i64,
    ) -> Result<()> {
        validate_label(reconciliation_id, "reconciliation identifier")?;
        self.validate_snapshot(snapshot, now_ms)?;
        self.storage.with_conn(|conn| -> Result<()> {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let prior: Option<i64> = tx.query_row("SELECT snapshot_revision FROM aegis_accounts WHERE account=?1", [&self.account], |r| r.get(0))?;
            if prior == Some(snapshot.revision) {
                let stored: String = tx.query_row("SELECT snapshot_json FROM aegis_accounts WHERE account=?1",[&self.account],|r|r.get(0))?;
                ensure!(stored == serde_json::to_string(snapshot)?, "snapshot revision reused with changed financial data");
                let mut seen=std::collections::HashSet::new();
                for id in resolved_intents {
                    ensure!(seen.insert(id), "duplicate reconciliation intent");
                    let evidence: Option<(String,Option<String>)>=tx.query_row("SELECT state,reconciliation_id FROM aegis_reservations WHERE intent_id=?1 AND account=?2",params![id,self.account],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
                    ensure!(evidence == Some(("reconciled".into(),Some(reconciliation_id.into()))), "reconciliation replay does not match committed evidence");
                }
                tx.commit()?;
                return Ok(());
            }
            ensure!(prior.is_none_or(|revision| snapshot.revision > revision), "reconciliation needs a newer account snapshot");
            let mut seen = std::collections::HashSet::new();
            for id in resolved_intents {
                ensure!(seen.insert(id), "duplicate reconciliation intent");
                let row: Option<(String, i64)> = tx.query_row("SELECT state,snapshot_revision FROM aegis_reservations WHERE intent_id=?1 AND account=?2",
                    params![id,self.account], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
                let (state, revision) = row.context("reservation is not owned by this account")?;
                ensure!(state == "active" && snapshot.revision > revision, "reservation is already settled or snapshot predates intent");
                tx.execute("UPDATE aegis_reservations SET state='reconciled',reconciliation_id=?1 WHERE intent_id=?2",
                    params![reconciliation_id,id])?;
            }
            self.accept_snapshot(&tx, snapshot)?;
            // Snapshot ingestion cannot silently rearm a breaker. Drawdowns latch
            // even if no strategy asks to reserve an entry on this cycle.
            let (peak, _) = self.account_state(&tx)?;
            if peak > Decimal::ZERO && snapshot.equity <= mul(peak, sub(Decimal::ONE, self.limits.max_drawdown_ratio)?)? {
                self.latch(&tx, "equity drawdown limit reached", now_ms)?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    /// Whether a bounded reconciliation read may make progress. Orphan,
    /// submitted, ambiguous and resting reservations are deliberately excluded.
    pub fn has_terminal_reservations(&self) -> Result<bool> {
        self.storage.with_conn(|conn| -> Result<bool> {
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM aegis_reservations r JOIN execution_intents e ON e.intent_id=r.intent_id
                 WHERE r.account=?1 AND r.state='active' AND e.state IN ('rejected','applied'))",
                [&self.account], |row| row.get(0))?)
        })
    }

    /// Replace only proven terminal entry reservations with current exposure.
    /// Journal and position proof is re-read under the same immediate transaction
    /// that records the snapshot and releases reservations. No financial/journal
    /// state or breaker is cleared by this operation.
    pub fn reconcile_terminal_entries(
        &self,
        evidence: &crate::adapters::account_evidence::AccountExposureEvidence,
        now_ms: i64,
    ) -> Result<Vec<String>> {
        evidence.validate(now_ms, self.limits.max_snapshot_age_ms)?;
        ensure!(
            evidence.account == self.account,
            "account evidence belongs to another account"
        );
        self.validate_snapshot(&evidence.snapshot, now_ms)?;
        let evidence_json = serde_json::to_string(evidence)?;
        ensure!(
            evidence_json.len() <= 4 * 1024 * 1024,
            "reservation reconciliation proof exceeds byte budget"
        );
        self.storage.with_conn(|conn| -> Result<Vec<String>> {
            let tx=Transaction::new_unchecked(conn,TransactionBehavior::Immediate)?;
            let binding:Option<String>=tx.query_row("SELECT value FROM meta_kv WHERE key='trading_account_binding'",[],|row|row.get(0)).optional()?;
            ensure!(binding.as_deref()==Some(self.account.as_str()),"reservation reconciliation requires bound trading history");
            validate_owned_evidence(&tx,evidence)?;
            let metadata_exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='position_entry_state')",[],|r|r.get(0))?;
            let rows={
                let mut statement=tx.prepare(
                    "SELECT r.intent_id,r.symbol,r.snapshot_revision,r.created_at_ms,e.state,e.intent_json,e.result_json,e.updated_at,e.venue,e.symbol
                     FROM aegis_reservations r JOIN execution_intents e ON e.intent_id=r.intent_id
                     WHERE r.account=?1 AND r.state='active' AND e.state IN ('rejected','applied') LIMIT 1025")?;
                let values=statement.query_map([&self.account],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?,
                    row.get::<_,i64>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?,row.get::<_,Option<String>>(6)?,
                    row.get::<_,String>(7)?,row.get::<_,String>(8)?,row.get::<_,String>(9)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                values
            };
            ensure!(rows.len()<=1024,"reservation reconciliation exceeds work budget");
            let mut resolved=Vec::new();
            for (id,symbol,revision,reserved_at,state,intent_json,result_json,updated_at,venue,journal_symbol) in rows {
                ensure!(intent_json.len()<=32768 && result_json.as_ref().is_some_and(|s|s.len()<=8192) && updated_at.len()<=64,"invalid or unbounded terminal entry evidence");
                let intent:crate::core::OrderIntent=serde_json::from_str(&intent_json)?;
                let receipt:crate::core::OrderResult=serde_json::from_str(result_json.as_deref().unwrap())?;
                ensure!(uuid::Uuid::parse_str(&id)?.to_string()==id && intent.id==id && receipt.intent_id==id
                    && intent.venue==evidence.venue && venue==evidence.venue && intent.symbol==symbol && journal_symbol==symbol,
                    "terminal intent identity does not match reservation");
                // Only entry reservations are in scope. A manually attached
                // reduction journal cannot be used to cancel an entry hold.
                ensure!(!intent.reduce_only && intent.size.is_finite() && intent.size>0.0,"reservation does not describe a valid entry");
                let terminal_at=chrono::DateTime::parse_from_rfc3339(&updated_at)?.timestamp_millis();
                ensure!(terminal_at>=0 && evidence.acquired_started_ms>=terminal_at && evidence.exchange_time_ms>=terminal_at
                    && evidence.acquired_started_ms>=reserved_at && evidence.snapshot.revision>revision,
                    "account evidence predates entry outcome or reservation");
                let mut position_id=None;
                if state=="rejected" {
                    ensure!(!receipt.accepted && receipt.filled_size==0.0 && receipt.avg_fill_price.is_none() && receipt.venue_order_id.is_none(),
                        "rejection does not prove a definite zero-fill outcome");
                    if metadata_exists {
                        let applied:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM position_entry_state WHERE entry_intent_id=?1)",[&id],|r|r.get(0))?;
                        ensure!(!applied,"rejected entry has persisted position metadata");
                    }
                } else {
                    if !metadata_exists { continue; }
                    let recorded:Option<EntryPositionRow>=tx.query_row(
                        "SELECT p.id,p.venue,p.symbol,p.side,p.size,p.entry_price,p.closed_at FROM position_entry_state m JOIN positions p ON p.id=m.position_id WHERE m.entry_intent_id=?1",
                        [&id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional()?;
                    let Some((pid,pvenue,psymbol,side,size,price,closed))=recorded else {continue;};
                    if closed.is_some() {continue;}
                    let reduced:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM confirmed_reductions WHERE position_id=?1)",[pid],|r|r.get(0))?;
                    if reduced {continue;}
                    ensure!(pvenue==venue && psymbol==symbol && side==intent.side.as_str() && receipt.accepted
                        && receipt.filled_size.is_finite() && receipt.filled_size>0.0 && receipt.filled_size<=intent.size
                        && receipt.filled_size==size && receipt.avg_fill_price==Some(price),"applied entry receipt and original position disagree");
                    let oid=receipt.venue_order_id.as_deref().context("applied entry lacks order identity")?.parse::<u64>()?;
                    ensure!(oid>0,"invalid applied entry order identity");
                    // An order still visible as outstanding cannot be assumed
                    // fully represented by the known position receipt.
                    if evidence.open_orders.iter().any(|order|order.oid==oid) {continue;}
                    position_id=Some(pid);
                }
                let reconciliation_id=format!("auto-entry:{}",evidence.snapshot.revision);
                let changed=tx.execute("UPDATE aegis_reservations SET state='reconciled',reconciliation_id=?1 WHERE intent_id=?2 AND account=?3 AND state='active'",
                    params![reconciliation_id,id,self.account])?;
                ensure!(changed==1,"reservation changed during reconciliation");
                let proof=serde_json::json!({"intent_id":id,"journal_state":state,"position_id":position_id,
                    "snapshot_revision":evidence.snapshot.revision,"acquired_started_ms":evidence.acquired_started_ms,
                    "acquired_finished_ms":evidence.acquired_finished_ms,"exchange_time_ms":evidence.exchange_time_ms,
                    "venue":evidence.venue,"environment":evidence.environment,"perp_dex":evidence.perp_dex});
                tx.execute("INSERT INTO aegis_events(account,kind,reason,at_ms) VALUES(?1,'reservation_reconciled',?2,?3)",
                    params![self.account,serde_json::to_string(&proof)?,now_ms])?;
                resolved.push(id);
            }
            // A prior/newer accepted snapshot must not be overwritten. Replays
            // with no active candidates are accepted only for identical data.
            if !resolved.is_empty() {
                let prior:Option<i64>=tx.query_row("SELECT snapshot_revision FROM aegis_accounts WHERE account=?1",[&self.account],|r|r.get(0))?;
                ensure!(prior.is_none_or(|old|evidence.snapshot.revision>old),"reservation release requires a newer accepted snapshot");
                // One immutable bundle per batch, referenced by the events'
                // account/revision. Later snapshots cannot erase release proof.
                tx.execute("INSERT INTO aegis_reconciliations(account,snapshot_revision,evidence_json,recorded_at_ms) VALUES(?1,?2,?3,?4)",
                    params![self.account,evidence.snapshot.revision,evidence_json,now_ms])?;
            }
            self.accept_snapshot(&tx,&evidence.snapshot)?;
            let (peak,_)=self.account_state(&tx)?;
            if peak>Decimal::ZERO && evidence.snapshot.equity<=mul(peak,sub(Decimal::ONE,self.limits.max_drawdown_ratio)?)? {
                self.latch(&tx,"equity drawdown limit reached",now_ms)?;
            }
            tx.commit()?;
            Ok(resolved)
        })
    }

    pub fn trip(&self, reason: &str, now_ms: i64) -> Result<()> {
        validate_label(reason, "circuit breaker reason")?;
        ensure!(now_ms >= 0, "invalid circuit breaker timestamp");
        self.storage.with_conn(|conn| -> Result<()> {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            self.latch(&tx, reason, now_ms)?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn rearm(
        &self,
        authorization: RearmAuthorization,
        snapshot: &AccountSnapshot,
        now_ms: i64,
    ) -> Result<()> {
        self.validate_snapshot(snapshot, now_ms)?;
        ensure!(
            snapshot.equity > Decimal::ZERO,
            "positive equity required to rearm"
        );
        self.storage.with_conn(|conn| -> Result<()> {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            self.accept_snapshot(&tx, snapshot)?;
            ensure!(self.exposure_block(&tx, snapshot, "", Decimal::ZERO)?.is_none(), "current exposure exceeds hard limits");
            let (peak, breaker) = self.account_state(&tx)?;
            ensure!(breaker.is_some(), "no latched breaker to rearm");
            if !authorization.reset_equity_peak {
                ensure!(snapshot.equity > mul(peak, sub(Decimal::ONE, self.limits.max_drawdown_ratio)?)?,
                    "drawdown persists; explicit equity-peak reset required");
            }
            let next_peak = if authorization.reset_equity_peak { snapshot.equity } else { peak };
            tx.execute("UPDATE aegis_accounts SET breaker_reason=NULL,high_water_equity=?1 WHERE account=?2", params![next_peak.to_string(),self.account])?;
            let reason = format!("{}; reset_equity_peak={}; prior_peak={peak}", authorization.reason, authorization.reset_equity_peak);
            tx.execute("INSERT INTO aegis_events (account,kind,reason,operator_id,at_ms) VALUES (?1,'operator_rearm',?2,?3,?4)",
                params![self.account,reason,authorization.operator_id,now_ms])?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Entry breakers and exposure caps never reject a verified reduce-only size.
    /// The execution layer must still enforce reduce-only, authorization and fresh
    /// exchange-position evidence; this function grants no exchange-write authority.
    pub fn validate_reduction(
        confirmed_position_qty: Decimal,
        requested_qty: Decimal,
    ) -> Result<()> {
        ensure!(
            confirmed_position_qty > Decimal::ZERO
                && requested_qty > Decimal::ZERO
                && requested_qty <= confirmed_position_qty,
            "reduction must not increase or reverse exposure"
        );
        Ok(())
    }

    pub fn status(&self) -> Result<RiskStatus> {
        self.storage.with_conn(|conn| -> Result<RiskStatus> {
            let (peak, breaker, revision): (String, Option<String>, Option<i64>) = conn.query_row(
                "SELECT high_water_equity,breaker_reason,snapshot_revision FROM aegis_accounts WHERE account=?1",
                [&self.account], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            let mut statement = conn.prepare("SELECT notional FROM aegis_reservations WHERE account=?1 AND state='active'")?;
            let rows = statement.query_map([&self.account], |r| r.get::<_,String>(0))?;
            let mut reserved = Decimal::ZERO;
            let mut count = 0;
            for row in rows {
                let notional=parse(&row?)?;
                ensure!(notional > Decimal::ZERO, "corrupt reservation notional");
                reserved = add(reserved, notional)?; count += 1;
            }
            Ok(RiskStatus { account:self.account.clone(), high_water_equity:parse(&peak)?, breaker_reason:breaker,
                active_reservations:count, reserved_notional:reserved, latest_snapshot_revision:revision })
        })
    }

    fn validate_snapshot(&self, snapshot: &AccountSnapshot, now_ms: i64) -> Result<()> {
        ensure!(
            snapshot.revision > 0 && snapshot.observed_at_ms >= 0 && now_ms >= 0,
            "invalid snapshot identity or timestamp"
        );
        let age = now_ms
            .checked_sub(snapshot.observed_at_ms)
            .context("snapshot timestamp overflow")?;
        ensure!(
            age >= 0 && age <= self.limits.max_snapshot_age_ms,
            "account snapshot is stale or future-dated"
        );
        ensure!(snapshot.equity >= Decimal::ZERO, "negative account equity");
        for (symbol, notional) in &snapshot.exposure_by_market {
            validate_symbol(symbol)?;
            ensure!(
                *notional >= Decimal::ZERO,
                "negative absolute account exposure"
            );
        }
        Ok(())
    }

    fn accept_snapshot(&self, tx: &Transaction<'_>, snapshot: &AccountSnapshot) -> Result<()> {
        let (old, revision): (Option<String>, Option<i64>) = tx.query_row(
            "SELECT snapshot_json,snapshot_revision FROM aegis_accounts WHERE account=?1",
            [&self.account],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let serialized = serde_json::to_string(snapshot)?;
        if let Some(revision) = revision {
            ensure!(
                snapshot.revision >= revision,
                "account snapshot revision regressed"
            );
            if snapshot.revision == revision {
                ensure!(
                    old.as_deref() == Some(&serialized),
                    "snapshot revision reused with changed financial data"
                );
            }
        }
        let (peak, _) = self.account_state(tx)?;
        tx.execute("UPDATE aegis_accounts SET snapshot_json=?1,snapshot_revision=?2,high_water_equity=?3 WHERE account=?4",
            params![serialized,snapshot.revision,peak.max(snapshot.equity).to_string(),self.account])?;
        Ok(())
    }

    fn account_state(&self, tx: &Transaction<'_>) -> Result<(Decimal, Option<String>)> {
        let (peak, breaker): (String, Option<String>) = tx.query_row(
            "SELECT high_water_equity,breaker_reason FROM aegis_accounts WHERE account=?1",
            [&self.account],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let peak = parse(&peak)?;
        ensure!(peak >= Decimal::ZERO, "corrupt high-water equity");
        Ok((peak, breaker))
    }

    fn exposure_block(
        &self,
        tx: &Transaction<'_>,
        snapshot: &AccountSnapshot,
        symbol: &str,
        new: Decimal,
    ) -> Result<Option<String>> {
        let mut by_market = snapshot.exposure_by_market.clone();
        let mut stmt = tx.prepare(
            "SELECT symbol,notional FROM aegis_reservations WHERE account=?1 AND state='active'",
        )?;
        let rows = stmt.query_map([&self.account], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (market, notional) = row?;
            let notional = parse(&notional)?;
            ensure!(notional > Decimal::ZERO, "corrupt reservation notional");
            let prior = by_market.get(&market).copied().unwrap_or(Decimal::ZERO);
            by_market.insert(market, add(prior, notional)?);
        }
        if new > Decimal::ZERO {
            let prior = by_market.get(symbol).copied().unwrap_or(Decimal::ZERO);
            by_market.insert(symbol.into(), add(prior, new)?);
        }
        let mut total = Decimal::ZERO;
        for exposure in by_market.values() {
            ensure!(
                *exposure >= Decimal::ZERO,
                "corrupt negative exposure reservation"
            );
            total = add(total, *exposure)?;
            if *exposure > self.limits.max_market_notional {
                return Ok(Some("per-market exposure limit reached".into()));
            }
        }
        if total > mul(snapshot.equity, self.limits.max_total_exposure_ratio)? {
            return Ok(Some(
                "total exposure limit reached including reservations".into(),
            ));
        }
        if total > mul(snapshot.equity, self.limits.max_leverage)? {
            return Ok(Some("portfolio leverage limit reached".into()));
        }
        Ok(None)
    }

    fn latch(&self, tx: &Transaction<'_>, reason: &str, now_ms: i64) -> Result<()> {
        // First cause is immutable until explicit rearm; do not flood the log.
        let changed=tx.execute("UPDATE aegis_accounts SET breaker_reason=?1 WHERE account=?2 AND breaker_reason IS NULL",params![reason,self.account])?;
        if changed == 1 {
            tx.execute("INSERT INTO aegis_events (account,kind,reason,at_ms) VALUES (?1,'circuit_breaker',?2,?3)",params![self.account,reason,now_ms])?;
        }
        Ok(())
    }
}

fn validate_label(value: &str, label: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control),
        "invalid {label}"
    );
    Ok(())
}
fn validate_symbol(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b)),
        "invalid instrument identifier"
    );
    Ok(())
}
fn parse(s: &str) -> Result<Decimal> {
    Decimal::from_str_exact(s).context("corrupt decimal risk state")
}
fn add(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_add(b).context("risk arithmetic overflow")
}
fn sub(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_sub(b).context("risk arithmetic overflow")
}
fn mul(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_mul(b).context("risk arithmetic overflow")
}

fn validate_owned_evidence(
    tx: &Transaction<'_>,
    evidence: &crate::adapters::account_evidence::AccountExposureEvidence,
) -> Result<()> {
    let mut statement=tx.prepare("SELECT symbol,side,size,entry_price FROM positions WHERE venue=?1 AND closed_at IS NULL LIMIT 1025")?;
    let local = statement
        .query_map([&evidence.venue], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, f64>(2)?,
                r.get::<_, f64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        local.len() <= 1024 && local.len() == evidence.positions.len(),
        "local and observed position ownership differs"
    );
    let mut symbols = std::collections::BTreeSet::new();
    for (symbol, side, size, price) in local {
        ensure!(
            symbols.insert(symbol.clone())
                && size.is_finite()
                && size > 0.0
                && price.is_finite()
                && price > 0.0
                && matches!(side.as_str(), "LONG" | "SHORT"),
            "invalid or duplicate persisted position"
        );
        let remote = evidence
            .positions
            .iter()
            .find(|p| p.symbol == symbol)
            .context("persisted position is missing from account evidence")?;
        let signed = parse(&size.to_string())?
            * if side == "LONG" {
                Decimal::ONE
            } else {
                -Decimal::ONE
            };
        ensure!(
            remote.signed_size == signed && remote.entry_price == parse(&price.to_string())?,
            "account position does not match local ownership"
        );
    }
    Ok(())
}

// Position row read atomically with entry receipt and reservation proof.
type EntryPositionRow = (i64, String, String, String, f64, f64, Option<String>);

#[derive(Clone, Copy)]
struct EntryRiskRequest<'a> {
    intent_id: &'a str,
    symbol: &'a str,
    notional: Decimal,
    requested_leverage: Decimal,
    snapshot: &'a AccountSnapshot,
    now_ms: i64,
}
