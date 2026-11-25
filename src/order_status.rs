//! Bounded native lifecycle observations. Observations never resolve execution
//! or financial holds by themselves; status is not execution-price/cost evidence.
use crate::{core::OrderIntent, storage::Storage};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
const MAX_BYTES: usize = 64 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub id: String,
    pub intent_id: String,
    pub account: String,
    pub venue: String,
    pub environment: String,
    pub requested_cloid: String,
    pub acquired_started_ms: i64,
    pub acquired_finished_ms: i64,
    pub outcome: Outcome,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "order", rename_all = "snake_case")]
pub enum Outcome {
    Missing,
    Order(Box<OrderState>),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderState {
    pub order_id: String,
    pub cloid: String,
    pub symbol: String,
    pub buy: bool,
    pub reduce_only: bool,
    pub original_size: Decimal,
    pub remaining_size: Decimal,
    pub limit_price: Decimal,
    pub order_type: String,
    pub tif: Option<String>,
    pub status: String,
    pub order_time_ms: i64,
    pub status_time_ms: i64,
}
pub fn client_order_id(intent_id: &str) -> Result<String> {
    let id = uuid::Uuid::parse_str(intent_id)?;
    ensure!(
        id.to_string() == intent_id,
        "noncanonical client order UUID"
    );
    Ok(format!("0x{}", id.simple()))
}
fn text(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 64
            && value.bytes().all(|b| b.is_ascii_graphic() || b == b' '),
        "unsupported bounded order status text"
    );
    Ok(())
}
fn symbol(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)),
        "unsupported default-perpetual symbol"
    );
    Ok(())
}
fn decimal(value: &str) -> Result<Decimal> {
    ensure!(value.len() <= 128, "order status decimal exceeds budget");
    Decimal::from_str_exact(value).context("unsupported order status precision")
}
impl Observation {
    pub fn validate(&self, intent: &OrderIntent, account: &str, now_ms: i64) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(&self.id)?.to_string() == self.id,
            "noncanonical observation identity"
        );
        ensure!(
            self.account == crate::ownership::canonical_account(account)?
                && self.account == account
                && self.venue == "hyperliquid"
                && intent.venue == self.venue
                && self.environment == "mainnet"
                && self.intent_id == intent.id
                && self.requested_cloid == client_order_id(&intent.id)?,
            "order status identity scope mismatch"
        );
        symbol(&intent.symbol)?;
        ensure!(
            intent.size.is_finite() && intent.size > 0.0,
            "invalid durable intent size"
        );
        ensure!(
            self.acquired_started_ms > 0
                && self.acquired_started_ms >= intent.created_at.timestamp_millis()
                && self.acquired_finished_ms >= self.acquired_started_ms
                && self
                    .acquired_finished_ms
                    .checked_sub(self.acquired_started_ms)
                    .is_some_and(|n| n <= 15_000)
                && now_ms >= self.acquired_finished_ms
                && now_ms
                    .checked_sub(self.acquired_started_ms)
                    .is_some_and(|n| (0..=30_000).contains(&n)),
            "order status acquisition is stale, future-dated or invalid"
        );
        if let Outcome::Order(order) = &self.outcome {
            let oid = order.order_id.parse::<u64>()?;
            ensure!(
                oid > 0
                    && oid.to_string() == order.order_id
                    && order.cloid == self.requested_cloid
                    && order.symbol == intent.symbol
                    && order.buy == intent.side.is_buy()
                    && order.reduce_only == intent.reduce_only,
                "returned order identity disagrees with intent"
            );
            ensure!(
                order.original_size > Decimal::ZERO
                    && order.original_size <= decimal(&intent.size.to_string())?
                    && order.remaining_size >= Decimal::ZERO
                    && order.remaining_size <= order.original_size
                    && order.limit_price > Decimal::ZERO,
                "invalid exact order status values"
            );
            ensure!(
                order.order_time_ms > 0
                    && order.status_time_ms >= order.order_time_ms
                    && order.status_time_ms <= self.acquired_finished_ms,
                "invalid order lifecycle timestamps"
            );
            text(&order.status)?;
            text(&order.order_type)?;
            if let Some(tif) = &order.tif {
                text(tif)?;
            }
        }
        Ok(())
    }
}
pub fn request_body(account: &str, intent: &OrderIntent) -> Result<serde_json::Value> {
    ensure!(
        crate::ownership::canonical_account(account)? == account && intent.venue == "hyperliquid",
        "unsupported order status request scope"
    );
    symbol(&intent.symbol)?;
    ensure!(
        intent.size.is_finite() && intent.size > 0.0,
        "invalid durable intent size"
    );
    Ok(serde_json::json!({"type":"orderStatus","user":account,"oid":client_order_id(&intent.id)?}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawResponse {
    status: String,
    order: Option<RawEnvelope>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEnvelope {
    order: RawOrder,
    status: String,
    status_timestamp: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawOrder {
    coin: String,
    side: String,
    limit_px: String,
    sz: String,
    orig_sz: String,
    oid: u64,
    timestamp: i64,
    cloid: Option<String>,
    reduce_only: bool,
    is_trigger: bool,
    is_position_tpsl: bool,
    children: Vec<serde_json::Value>,
    order_type: String,
    tif: Option<String>,
}
/// Decode selected identity/lifecycle fields, never derive filled size or price
/// from remaining quantity or a terminal-looking lifecycle label.
pub fn parse_hyperliquid(
    account: &str,
    intent: &OrderIntent,
    body: &str,
    started: i64,
    finished: i64,
) -> Result<Observation> {
    ensure!(
        body.len() <= MAX_BYTES,
        "order status response exceeds byte budget"
    );
    request_body(account, intent)?;
    let raw: RawResponse = serde_json::from_str(body)?;
    let outcome = match raw.status.as_str() {
        "unknownOid" => {
            ensure!(
                raw.order.is_none(),
                "missing-order response contains contradictory order"
            );
            Outcome::Missing
        }
        "order" => {
            let raw = raw.order.context("order status payload missing")?;
            let order = raw.order;
            ensure!(
                !order.is_trigger && !order.is_position_tpsl && order.children.is_empty(),
                "unsupported conditional order evidence"
            );
            let buy = match order.side.as_str() {
                "B" => true,
                "A" => false,
                _ => anyhow::bail!("unsupported order status side"),
            };
            Outcome::Order(Box::new(OrderState {
                order_id: order.oid.to_string(),
                cloid: order
                    .cloid
                    .context("returned order lacks client identity")?,
                symbol: order.coin,
                buy,
                reduce_only: order.reduce_only,
                original_size: decimal(&order.orig_sz)?,
                remaining_size: decimal(&order.sz)?,
                limit_price: decimal(&order.limit_px)?,
                order_type: order.order_type,
                tif: order.tif,
                status: raw.status,
                order_time_ms: order.timestamp,
                status_time_ms: raw.status_timestamp,
            }))
        }
        _ => anyhow::bail!("unsupported order status response"),
    };
    let observation = Observation {
        id: uuid::Uuid::new_v4().to_string(),
        intent_id: intent.id.clone(),
        account: account.into(),
        venue: "hyperliquid".into(),
        environment: "mainnet".into(),
        requested_cloid: client_order_id(&intent.id)?,
        acquired_started_ms: started,
        acquired_finished_ms: finished,
        outcome,
    };
    observation.validate(intent, account, finished)?;
    Ok(observation)
}
pub(crate) fn migrate_on(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS execution_status_observations (
        observation_id TEXT PRIMARY KEY,intent_id TEXT NOT NULL REFERENCES execution_intents(intent_id),
        account TEXT NOT NULL,requested_cloid TEXT NOT NULL,acquired_started_ms INTEGER NOT NULL,
        acquired_finished_ms INTEGER NOT NULL,observation_json TEXT NOT NULL,recorded_at_ms INTEGER NOT NULL,
        source TEXT NOT NULL CHECK(source='native_hyperliquid_info_order_status')
    );")?;
    Ok(())
}
/// Append immutable selected native evidence. Exact replays are idempotent;
/// conflicting reuse of a local observation identity never overwrites history.
pub fn record_observation(
    storage: &Storage,
    observation: &Observation,
    now_ms: i64,
) -> Result<bool> {
    storage.with_conn(|conn|->Result<bool>{
        let tx=conn.unchecked_transaction()?;
        let account:String=tx.query_row("SELECT value FROM meta_kv WHERE key='trading_account_binding'",[],|r|r.get(0))?;
        let encoded:Option<String>=tx.query_row("SELECT CASE WHEN length(CAST(intent_json AS BLOB))<=32768 THEN intent_json END FROM execution_intents WHERE intent_id=?1",[&observation.intent_id],|r|r.get(0))?;
        let intent:OrderIntent=serde_json::from_str(&encoded.context("durable intent missing or oversized")?)?;
        observation.validate(&intent,&account,observation.acquired_finished_ms)?;
        let encoded=serde_json::to_string(observation)?;ensure!(encoded.len()<=MAX_BYTES,"order status evidence exceeds byte budget");
        let prior:Option<Option<String>>=tx.query_row("SELECT CASE WHEN length(CAST(observation_json AS BLOB))<=65536 THEN observation_json END FROM execution_status_observations WHERE observation_id=?1",[&observation.id],|r|r.get(0)).optional()?;
        if let Some(prior)=prior {ensure!(prior.as_deref()==Some(encoded.as_str()),"observation identity conflict");return Ok(false);}
        observation.validate(&intent,&account,now_ms)?;
        tx.execute("INSERT INTO execution_status_observations(observation_id,intent_id,account,requested_cloid,acquired_started_ms,acquired_finished_ms,observation_json,recorded_at_ms,source) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'native_hyperliquid_info_order_status')",
            params![observation.id,observation.intent_id,account,observation.requested_cloid,observation.acquired_started_ms,observation.acquired_finished_ms,encoded,now_ms])?;
        tx.commit()?;Ok(true)
    })
}
