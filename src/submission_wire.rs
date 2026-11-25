//! Opaque engine capability for native unsigned action journaling before write.
//! It contains no signer, private key, signature or permission to resolve holds.
use crate::{core::OrderIntent, hyperliquid_wire::OrderAction, storage::Storage};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection};
use rust_decimal::Decimal;

pub struct SubmissionJournal<'a> {
    storage: &'a Storage,
    intent: &'a OrderIntent,
}
impl<'a> SubmissionJournal<'a> {
    pub(crate) fn new(storage: &'a Storage, intent: &'a OrderIntent) -> Self {
        Self { storage, intent }
    }
    pub fn intent_id(&self) -> &str {
        &self.intent.id
    }
    pub(crate) fn verify_intent(&self, intent: &OrderIntent) -> Result<()> {
        ensure!(
            serde_json::to_string(self.intent)? == serde_json::to_string(intent)?,
            "native journal capability intent mismatch"
        );
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn assert_persisted_action(&self, action: &OrderAction) -> Result<()> {
        let expected = serde_json::to_string(&action.unsigned_provenance()?)?;
        let saved: String = self.storage.with_conn(|conn| {
            conn.query_row(
                "SELECT action_json FROM execution_wire_requests WHERE intent_id=?1",
                [&self.intent.id],
                |r| r.get(0),
            )
        })?;
        ensure!(
            saved == expected,
            "native action was not durably recorded before submission"
        );
        Ok(())
    }
    pub(crate) fn record(
        &self,
        account: &str,
        action: &OrderAction,
        size_decimals: u8,
    ) -> Result<()> {
        let facts = action.unsigned_provenance()?;
        ensure!(size_decimals <= 6, "unsupported native size precision");
        let quantity = Decimal::from_str_exact(&facts.size)?;
        let price = Decimal::from_str_exact(&facts.price)?;
        ensure!(
            quantity > Decimal::ZERO
                && price > Decimal::ZERO
                && quantity.normalize().scale() <= u32::from(size_decimals),
            "invalid native wire decimal provenance"
        );
        ensure!(
            crate::ownership::canonical_account(account)? == account,
            "noncanonical native wire account"
        );
        let action_json = serde_json::to_string(&facts)?;
        ensure!(
            action_json.len() <= 4096 && facts.messagepack_hex.len() <= 2048,
            "native wire provenance exceeds budget"
        );
        self.storage.with_conn(|conn|->Result<()>{
            let tx=conn.unchecked_transaction()?;
            let binding:String=tx.query_row("SELECT CASE WHEN length(CAST(value AS BLOB))=42 THEN value END FROM meta_kv WHERE key='trading_account_binding'",[],|r|r.get(0))?;
            ensure!(binding==account,"native wire account binding mismatch");
            let intent_json:Option<String>=tx.query_row("SELECT CASE WHEN length(CAST(intent_json AS BLOB))<=32768 THEN intent_json END FROM execution_intents WHERE intent_id=?1 AND state='submitted'",[&self.intent.id],|r|r.get(0))?;
            let submitted_json=intent_json.context("submitted intent evidence missing or oversized")?;
            ensure!(submitted_json==serde_json::to_string(self.intent)?,"submitted intent changed before native write");
            let intent=self.intent;
            ensure!(intent.venue=="hyperliquid" && intent.side.is_buy()==facts.buy && intent.reduce_only==facts.reduce_only
                && intent.size.is_finite() && intent.size>0.0 && quantity<=Decimal::from_str_exact(&intent.size.to_string())?
                && facts.client_order_id==crate::order_status::client_order_id(&self.intent.id)?,"native wire action disagrees with submitted intent");
            tx.execute("INSERT INTO execution_wire_requests(intent_id,account,venue,environment,size_decimals,action_json,recorded_at_ms,source) VALUES(?1,?2,'hyperliquid','mainnet',?3,?4,?5,'native_ioc_unsigned_action')",
                params![self.intent.id,account,size_decimals,action_json,chrono::Utc::now().timestamp_millis()])?;
            tx.commit()?;Ok(())
        })
    }
}
pub(crate) fn migrate_on(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS execution_wire_requests (
        intent_id TEXT PRIMARY KEY REFERENCES execution_intents(intent_id),account TEXT NOT NULL,
        venue TEXT NOT NULL CHECK(venue='hyperliquid'),environment TEXT NOT NULL CHECK(environment='mainnet'),
        size_decimals INTEGER NOT NULL CHECK(size_decimals BETWEEN 0 AND 6),
        action_json TEXT NOT NULL,recorded_at_ms INTEGER NOT NULL,
        source TEXT NOT NULL CHECK(source='native_ioc_unsigned_action')
    );")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";
    fn intent() -> OrderIntent {
        OrderIntent {
            id: uuid::Uuid::new_v4().to_string(),
            venue: "hyperliquid".into(),
            symbol: "ETH".into(),
            side: crate::core::Side::Long,
            size: 2.0,
            limit_price: Some(101.0),
            reduce_only: false,
            stop_price: None,
            take_profit: None,
            source_signal: None,
            created_at: chrono::Utc::now(),
        }
    }
    fn setup() -> (Storage, OrderIntent) {
        let db = Storage::open(":memory:").unwrap();
        db.migrate().unwrap();
        db.bind_trading_account(ACCOUNT, false).unwrap();
        let i = intent();
        db.reserve_intent(&i).unwrap();
        (db, i)
    }
    fn action(i: &OrderIntent) -> OrderAction {
        OrderAction::ioc(4, true, 101.0, 1.2345, false, &i.id).unwrap()
    }
    fn count(db: &Storage) -> i64 {
        db.with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM execution_wire_requests", [], |r| {
                r.get(0)
            })
        })
        .unwrap()
    }
    #[test]
    fn canonical_unsigned_action_is_durable_without_keys_signatures_or_nonce() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wire.db");
        let db = Storage::open(&path).unwrap();
        db.migrate().unwrap();
        db.bind_trading_account(ACCOUNT, false).unwrap();
        let i = intent();
        db.reserve_intent(&i).unwrap();
        let a = action(&i);
        let journal = SubmissionJournal::new(&db, &i);
        journal.record(ACCOUNT, &a, 4).unwrap();
        journal.assert_persisted_action(&a).unwrap();
        let saved: String = db
            .with_conn(|c| {
                c.query_row("SELECT action_json FROM execution_wire_requests", [], |r| {
                    r.get(0)
                })
            })
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(value["size"], "1.2345");
        assert_eq!(value["tif"], "Ioc");
        assert_eq!(
            value["client_order_id"],
            crate::order_status::client_order_id(&i.id).unwrap()
        );
        assert_eq!(
            hex::decode(value["messagepack_hex"].as_str().unwrap()).unwrap(),
            a.packed().unwrap()
        );
        for key in ["signature", "private_key", "nonce", "wallet_key"] {
            assert!(value.get(key).is_none());
        }
        drop(db);
        let db = Storage::open(&path).unwrap();
        db.migrate().unwrap();
        assert_eq!(count(&db), 1);
        assert!(SubmissionJournal::new(&db, &i)
            .record(ACCOUNT, &a, 4)
            .is_err());
        assert!(db.ensure_execution_resolved().is_err());
    }
    #[test]
    fn action_scope_quantity_and_precision_mismatches_never_record() {
        for variant in 0..7 {
            let (db, i) = setup();
            let other = uuid::Uuid::new_v4().to_string();
            let account = if variant == 6 {
                "0x2222222222222222222222222222222222222222"
            } else {
                ACCOUNT
            };
            let a = OrderAction::ioc(
                4,
                variant != 0,
                101.0,
                if variant == 3 { 2.1 } else { 1.2345 },
                variant == 1,
                if variant == 2 { &other } else { &i.id },
            )
            .unwrap();
            let dp = if variant == 4 {
                3
            } else if variant == 5 {
                7
            } else {
                4
            };
            assert!(SubmissionJournal::new(&db, &i)
                .record(account, &a, dp)
                .is_err());
            assert_eq!(count(&db), 0);
        }
    }
    #[test]
    fn changed_durable_intent_and_terminal_state_refuse_prewrite() {
        for variant in 0..2 {
            let (db, i) = setup();
            let a = action(&i);
            if variant == 0 {
                let mut changed = i.clone();
                changed.symbol = "BTC".into();
                db.with_conn(|c| {
                    c.execute(
                        "UPDATE execution_intents SET intent_json=?1 WHERE intent_id=?2",
                        params![serde_json::to_string(&changed).unwrap(), i.id],
                    )
                })
                .unwrap();
            } else {
                db.finish_intent(&i.id, "rejected", None).unwrap();
            }
            assert!(SubmissionJournal::new(&db, &i)
                .record(ACCOUNT, &a, 4)
                .is_err());
            assert_eq!(count(&db), 0);
        }
    }
    #[test]
    fn insert_failure_and_conflicting_reuse_do_not_replace_wire_history() {
        let (db, i) = setup();
        let a = action(&i);
        let journal = SubmissionJournal::new(&db, &i);
        db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_wire BEFORE INSERT ON execution_wire_requests BEGIN SELECT RAISE(ABORT,'offline-wire-failure'); END;")).unwrap();
        assert!(journal.record(ACCOUNT, &a, 4).is_err());
        assert_eq!(count(&db), 0);
        db.with_conn(|c| c.execute_batch("DROP TRIGGER fail_wire"))
            .unwrap();
        journal.record(ACCOUNT, &a, 4).unwrap();
        let changed = OrderAction::ioc(4, true, 102.0, 1.2345, false, &i.id).unwrap();
        assert!(journal.record(ACCOUNT, &changed, 4).is_err());
        journal.assert_persisted_action(&a).unwrap();
        assert_eq!(count(&db), 1);
    }
    #[test]
    fn separate_connections_record_only_one_prewrite_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("race.db");
        let a = Storage::open(&path).unwrap();
        a.migrate().unwrap();
        a.bind_trading_account(ACCOUNT, false).unwrap();
        let i = intent();
        a.reserve_intent(&i).unwrap();
        let b = Storage::open(&path).unwrap();
        b.migrate().unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let workers: Vec<_> = [a, b]
            .into_iter()
            .map(|db| {
                let i = i.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let a = action(&i);
                    barrier.wait();
                    SubmissionJournal::new(&db, &i)
                        .record(ACCOUNT, &a, 4)
                        .is_ok()
                })
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .map(|w| w.join().unwrap())
                .filter(|ok| *ok)
                .count(),
            1
        );
        let db = Storage::open(&path).unwrap();
        assert_eq!(count(&db), 1);
        assert!(db.ensure_execution_resolved().is_err());
    }
    #[test]
    fn journal_capability_rejects_a_different_in_memory_intent() {
        let (db, i) = setup();
        let journal = SubmissionJournal::new(&db, &i);
        let mut changed = i.clone();
        changed.symbol = "BTC".into();
        assert!(journal.verify_intent(&changed).is_err());
        assert_eq!(count(&db), 0);
    }
}
