//! Exact native fill facts are preserved alongside the legacy floating-point view.
use crate::{
    core::{OpenPosition, OrderIntent, OrderResult},
    storage::Storage,
};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreciseFill {
    pub quantity: String,
    pub average_price: String,
    pub order_id: String,
}
impl PreciseFill {
    pub fn values(&self) -> Result<(Decimal, Decimal)> {
        let qty = decimal(&self.quantity)?;
        let price = decimal(&self.average_price)?;
        ensure!(
            qty > Decimal::ZERO && price > Decimal::ZERO,
            "invalid exact fill values"
        );
        let oid = self.order_id.parse::<u64>()?;
        ensure!(
            oid > 0 && oid.to_string() == self.order_id,
            "invalid exact fill order identity"
        );
        Ok((qty, price))
    }
    pub fn validate_wire_bounds(
        &self,
        maximum: Decimal,
        limit: Decimal,
        buy: bool,
        size_decimals: u8,
    ) -> Result<()> {
        let (quantity, price) = self.values()?;
        ensure!(
            size_decimals <= 6 && maximum > Decimal::ZERO && limit > Decimal::ZERO,
            "unsupported submitted fill bounds"
        );
        ensure!(
            quantity <= maximum
                && quantity.normalize().scale() <= u32::from(size_decimals)
                && (if buy { price <= limit } else { price >= limit }),
            "exact fill exceeds submitted order bounds"
        );
        Ok(())
    }
    pub fn validate(&self, intent: &OrderIntent, result: &OrderResult) -> Result<()> {
        let (qty, price) = self.values()?;
        ensure!(
            result.accepted
                && result.intent_id == intent.id
                && result.venue_order_id.as_deref() == Some(self.order_id.as_str())
                && result.filled_size == as_float(qty)?
                && result.avg_fill_price == Some(as_float(price)?)
                && qty <= decimal(&intent.size.to_string())?,
            "exact fill disagrees with intent or result"
        );
        Ok(())
    }
}
fn decimal(value: &str) -> Result<Decimal> {
    ensure!(
        !value.is_empty() && value.len() <= 128,
        "exact fill decimal exceeds budget"
    );
    Decimal::from_str_exact(value).context("unsupported exact fill precision")
}
fn as_float(value: Decimal) -> Result<f64> {
    let result: f64 = value.to_string().parse()?;
    ensure!(
        result.is_finite() && result > 0.0,
        "invalid floating-point compatibility value"
    );
    Ok(result)
}

pub(crate) fn migrate_on(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS execution_fill_facts (
        intent_id TEXT PRIMARY KEY REFERENCES execution_intents(intent_id),
        account TEXT NOT NULL,venue TEXT NOT NULL,symbol TEXT NOT NULL,side TEXT NOT NULL,
        quantity TEXT NOT NULL,average_price TEXT NOT NULL,order_id TEXT NOT NULL,
        environment TEXT NOT NULL CHECK(environment='mainnet'),
        source TEXT NOT NULL CHECK(source='native_hyperliquid_response')
    );
    CREATE TABLE IF NOT EXISTS position_precision_state (
        position_id INTEGER PRIMARY KEY REFERENCES positions(id),
        entry_intent_id TEXT NOT NULL UNIQUE REFERENCES execution_fill_facts(intent_id),
        current_quantity TEXT
    );
    CREATE TABLE IF NOT EXISTS execution_residual_evidence (
        intent_id TEXT PRIMARY KEY REFERENCES confirmed_reductions(intent_id),
        position_id INTEGER NOT NULL REFERENCES positions(id),
        remaining_quantity TEXT NOT NULL, evidence_json TEXT NOT NULL
    );",
    )?;
    Ok(())
}

/// Journal status and optional exact facts share a transaction. Failed facts
/// persistence leaves the pre-existing submitted intent unresolved, never applied.
pub fn record_receipt(
    storage: &Storage,
    state: &str,
    result: &OrderResult,
    facts: Option<&PreciseFill>,
) -> Result<()> {
    ensure!(
        matches!(state, "filled" | "resting" | "rejected"),
        "unsupported receipt state"
    );
    storage.with_conn(|conn|->Result<()> {
        let tx=conn.unchecked_transaction()?;
        let encoded:String=tx.query_row("SELECT intent_json FROM execution_intents WHERE intent_id=?1 AND state='submitted'",[&result.intent_id],|r|r.get(0))?;
        ensure!(encoded.len()<=32768,"intent evidence exceeds budget");
        let intent:OrderIntent=serde_json::from_str(&encoded)?;
        ensure!(intent.id==result.intent_id,"receipt identity disagrees with durable intent");
        let expected=if !result.accepted {"rejected"} else if result.filled_size>0.0 {"filled"} else {"resting"};
        ensure!(state==expected && intent.size.is_finite() && intent.size>0.0
            && result.filled_size.is_finite() && result.filled_size>=0.0 && result.filled_size<=intent.size
            && result.avg_fill_price.is_none_or(|price|price.is_finite() && price>0.0)
            && (result.filled_size==0.0 || (result.accepted && result.avg_fill_price.is_some_and(|p|p.is_finite() && p>0.0))),
            "receipt status or values are inconsistent");

        if let Some(facts)=facts {
            ensure!(state=="filled" && intent.venue=="hyperliquid","exact fill is not a supported filled receipt");
            facts.validate(&intent,result)?;
            let account:String=tx.query_row("SELECT value FROM meta_kv WHERE key='trading_account_binding'",[],|r|r.get(0))?;
            ensure!(crate::ownership::canonical_account(&account)?==account,"invalid exact fill account binding");
            tx.execute("INSERT INTO execution_fill_facts(intent_id,account,venue,symbol,side,quantity,average_price,order_id,environment,source)
                VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'mainnet','native_hyperliquid_response')",
                params![intent.id,account,intent.venue,intent.symbol,intent.side.as_str(),facts.quantity,facts.average_price,facts.order_id])?;
        }
        let changed=tx.execute("UPDATE execution_intents SET state=?1,result_json=?2,updated_at=?3 WHERE intent_id=?4 AND state='submitted'",
            params![state,serde_json::to_string(result)?,chrono::Utc::now().to_rfc3339(),result.intent_id])?;
        ensure!(changed==1,"receipt journal changed concurrently");
        tx.commit()?;Ok(())
    })
}

/// An exact position starts only from persisted native receipt facts. Legacy
/// positions never gain fabricated decimal precision from their floating view.
pub(crate) fn capture_position_on(
    conn: &Connection,
    id: i64,
    intent_id: &str,
    position: &OpenPosition,
) -> Result<()> {
    let row: Option<StoredFill> = stored_fill(conn, intent_id)?;
    let Some(row) = row else {
        return Ok(());
    };
    let (qty, price) = row.facts.values()?;
    ensure!(
        row.venue == position.venue
            && row.symbol == position.symbol
            && row.side == position.side
            && as_float(qty)? == position.size
            && as_float(price)? == position.entry_price,
        "position does not match exact entry facts"
    );
    let binding: String = conn.query_row(
        "SELECT value FROM meta_kv WHERE key='trading_account_binding'",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        binding == row.account,
        "position exact facts belong to another account"
    );
    conn.execute("INSERT INTO position_precision_state(position_id,entry_intent_id,current_quantity) VALUES(?1,?2,?3)",params![id,intent_id,row.facts.quantity])?;
    Ok(())
}
struct StoredFill {
    account: String,
    venue: String,
    symbol: String,
    side: String,
    facts: PreciseFill,
    environment: String,
    source: String,
    journal_state: String,
}
fn stored_fill(conn: &Connection, intent_id: &str) -> Result<Option<StoredFill>> {
    if !table_exists(conn, "execution_fill_facts")? {
        return Ok(None);
    }
    let row=conn.query_row("SELECT f.account,f.venue,f.symbol,f.side,f.quantity,f.average_price,f.order_id,f.environment,f.source,e.state FROM execution_fill_facts f JOIN execution_intents e ON e.intent_id=f.intent_id WHERE f.intent_id=?1",[intent_id],|r|Ok(StoredFill {
        account:r.get(0)?,venue:r.get(1)?,symbol:r.get(2)?,side:r.get(3)?,
        facts:PreciseFill {quantity:r.get(4)?,average_price:r.get(5)?,order_id:r.get(6)?},
        environment:r.get(7)?,source:r.get(8)?,journal_state:r.get(9)?,
    })).optional()?;
    if let Some(fill) = &row {
        ensure!(
            matches!(fill.journal_state.as_str(), "filled" | "applied")
                && fill.environment == "mainnet"
                && fill.source == "native_hyperliquid_response"
                && fill.venue == "hyperliquid"
                && matches!(fill.side.as_str(), "LONG" | "SHORT")
                && crate::ownership::canonical_account(&fill.account)? == fill.account,
            "unsupported exact receipt provenance"
        );
        fill.facts.values()?;
    }
    Ok(row)
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?)
}

/// None means legacy unknown precision; a recorded but unknown residual is an
/// error, so a floating-point fallback cannot silently restore discarded facts.
pub(crate) struct PositionView<'a> {
    pub id: i64,
    pub venue: &'a str,
    pub symbol: &'a str,
    pub side: &'a str,
    pub size: f64,
    pub entry_price: f64,
}
pub(crate) fn position_values_on(
    conn: &Connection,
    position: &PositionView<'_>,
    account: &str,
) -> Result<Option<(Decimal, Decimal)>> {
    if !table_exists(conn, "position_precision_state")? {
        return Ok(None);
    }
    let row:Option<(String,Option<String>)>=conn.query_row("SELECT entry_intent_id,current_quantity FROM position_precision_state WHERE position_id=?1",[position.id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((intent_id, quantity)) = row else {
        return Ok(None);
    };
    let fill = stored_fill(conn, &intent_id)?.context("exact position receipt missing")?;
    ensure!(
        fill.account == account
            && fill.venue == position.venue
            && fill.symbol == position.symbol
            && fill.side == position.side,
        "exact position scope mismatch"
    );
    let (_, price) = fill.facts.values()?;
    let qty = decimal(
        quantity
            .as_deref()
            .context("exact residual quantity remains unknown")?,
    )?;
    ensure!(
        qty > Decimal::ZERO
            && as_float(qty)? == position.size
            && as_float(price)? == position.entry_price,
        "exact position disagrees with compatibility view"
    );
    Ok(Some((qty, price)))
}
pub(crate) fn invalidate_position_on(conn: &Connection, id: i64) -> Result<()> {
    if table_exists(conn, "position_precision_state")? {
        conn.execute(
            "UPDATE position_precision_state SET current_quantity=NULL WHERE position_id=?1",
            [id],
        )?;
    }
    Ok(())
}

/// Return false only when exact current/close facts do not exist. Invalid known
/// facts fail closed; no float comparison can substitute for contradictory proof.
pub(crate) fn apply_residual_on(
    conn: &Connection,
    p: &OpenPosition,
    res: &OrderResult,
    remaining: f64,
    evidence: &crate::adapters::position_evidence::PositionSnapshot,
    now_ms: i64,
) -> Result<bool> {
    evidence.validate(now_ms)?;
    let binding: String = conn.query_row(
        "SELECT value FROM meta_kv WHERE key='trading_account_binding'",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        evidence.account == binding && p.venue == evidence.venue,
        "residual account scope mismatch"
    );
    let updated: String = conn.query_row(
        "SELECT updated_at FROM execution_intents WHERE intent_id=?1 AND state='filled'",
        [&res.intent_id],
        |r| r.get(0),
    )?;
    let updated = chrono::DateTime::parse_from_rfc3339(&updated)?.timestamp_millis();
    ensure!(
        evidence.acquired_started_ms >= updated && evidence.exchange_time_ms >= updated,
        "residual snapshot predates receipt"
    );
    let known: Option<Option<String>> = conn
        .query_row(
            "SELECT current_quantity FROM position_precision_state WHERE position_id=?1",
            [p.id],
            |r| r.get(0),
        )
        .optional()?;
    if !matches!(known, Some(Some(_))) {
        return Ok(false);
    }
    let Some(close) = stored_fill(conn, &res.intent_id)? else {
        return Ok(false);
    };
    ensure!(
        close.account == binding
            && close.venue == p.venue
            && close.symbol == p.symbol
            && close.side == if p.side == "LONG" { "SHORT" } else { "LONG" },
        "exact reduction scope disagrees"
    );
    let (current, entry) = position_values_on(
        conn,
        &PositionView {
            id: p.id,
            venue: &p.venue,
            symbol: &p.symbol,
            side: &p.side,
            size: p.size,
            entry_price: p.entry_price,
        },
        &binding,
    )?
    .context("exact current position missing")?;
    let (filled, price) = close.facts.values()?;
    ensure!(
        as_float(filled)? == res.filled_size
            && Some(as_float(price)?) == res.avg_fill_price
            && res.venue_order_id.as_deref() == Some(close.facts.order_id.as_str()),
        "reduction receipt disagrees with exact facts"
    );
    let residual = current
        .checked_sub(filled)
        .context("exact residual overflow")?;
    ensure!(
        residual >= Decimal::ZERO,
        "exact reduction exceeds current quantity"
    );
    let remote = evidence.positions.iter().find(|r| r.symbol == p.symbol);
    if residual == Decimal::ZERO {
        ensure!(
            remote.is_none() && remaining == 0.0,
            "exact zero residual not proven"
        );
    } else {
        let remote = remote.context("nonzero exact residual absent")?;
        let signed = if p.side == "LONG" {
            residual
        } else {
            -residual
        };
        ensure!(
            remote.signed_quantity == signed
                && remote.entry_price == entry
                && as_float(residual)? == remaining,
            "exact residual disagrees with remote position"
        );
    }
    let encoded = serde_json::to_string(evidence)?;
    ensure!(
        encoded.len() <= 4 * 1024 * 1024,
        "residual evidence exceeds budget"
    );
    conn.execute("INSERT INTO execution_residual_evidence(intent_id,position_id,remaining_quantity,evidence_json) VALUES(?1,?2,?3,?4)",params![res.intent_id,p.id,residual.to_string(),encoded])?;
    ensure!(
        conn.execute(
            "UPDATE position_precision_state SET current_quantity=?1 WHERE position_id=?2",
            params![residual.to_string(), p.id]
        )? == 1,
        "precision row changed during reduction"
    );
    Ok(true)
}
