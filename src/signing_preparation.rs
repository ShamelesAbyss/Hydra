//! Pre-signing metadata only. No key, signature or signed envelope is retained.
use crate::{core::OrderIntent, hyperliquid_wire::OrderAction, storage::Storage};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Preparation {
    pub intent_id: String,
    pub account: String,
    pub signer: String,
    pub nonce: String,
    pub connection_hash: String,
    pub agent_digest: String,
    pub domain_chain_id: u64,
    pub agent_source: String,
    pub vault_address: Option<String>,
    pub expires_after_ms: Option<u64>,
    pub phase: String,
    pub prepared_at_ms: i64,
}
pub(crate) fn migrate_on(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS execution_signing_preparations(
        intent_id TEXT PRIMARY KEY REFERENCES execution_wire_requests(intent_id),
        account TEXT NOT NULL,signer TEXT NOT NULL,nonce TEXT NOT NULL,
        environment TEXT NOT NULL CHECK(environment='mainnet'),preparation_json TEXT NOT NULL,
        source TEXT NOT NULL CHECK(source='native_ioc_signing_preparation'),
        UNIQUE(signer,nonce,environment)
    );",
    )?;
    Ok(())
}
pub(crate) fn record(
    storage: &Storage,
    intent: &OrderIntent,
    signer: &str,
    nonce: u64,
    action: &OrderAction,
) -> Result<()> {
    ensure!(
        nonce > 0 && crate::ownership::canonical_account(signer)? == signer,
        "invalid signing preparation identity or nonce"
    );
    let actual = action.unsigned_provenance()?;
    storage.with_conn(|conn|->Result<()> {
        let tx=conn.unchecked_transaction()?;
        let account:String=tx.query_row("SELECT CASE WHEN length(CAST(value AS BLOB))=42 THEN value END FROM meta_kv WHERE key='trading_account_binding'",[],|r|r.get(0))?;
        let submitted:String=tx.query_row("SELECT CASE WHEN length(CAST(intent_json AS BLOB))<=32768 THEN intent_json END FROM execution_intents WHERE intent_id=?1 AND state='submitted'",[&intent.id],|r|r.get(0))?;
        ensure!(submitted==serde_json::to_string(intent)?,"submitted intent changed before signing preparation");
        let retained=crate::wire_history::load_on(&tx,intent,&account)?.1.context("retained unsigned action required before signing preparation")?;
        ensure!(retained.action==actual,"signing action differs from retained unsigned action");
        let connection=crate::hyperliquid_wire::action_hash(&action.packed()?,nonce,None,None);
        let prepared=Preparation {intent_id:intent.id.clone(),account:account.clone(),signer:signer.into(),nonce:nonce.to_string(),
            connection_hash:hex::encode(connection),agent_digest:hex::encode(crate::hyperliquid_wire::agent_digest(connection,true)),
            domain_chain_id:1337,agent_source:"a".into(),vault_address:None,expires_after_ms:None,phase:"prepared_not_sent".into(),prepared_at_ms:chrono::Utc::now().timestamp_millis()};
        let encoded=serde_json::to_string(&prepared)?;ensure!(encoded.len()<=2048,"signing preparation exceeds budget");
        let changed=tx.execute("INSERT INTO execution_signing_preparations(intent_id,account,signer,nonce,environment,preparation_json,source) VALUES(?1,?2,?3,?4,'mainnet',?5,'native_ioc_signing_preparation')",params![intent.id,account,signer,nonce.to_string(),encoded])?;
        ensure!(changed==1,"signing preparation was not persisted");
        let saved:(String,String,String,String,String,String)=tx.query_row("SELECT CASE WHEN length(CAST(account AS BLOB))=42 THEN account END,CASE WHEN length(CAST(signer AS BLOB))=42 THEN signer END,CASE WHEN length(CAST(nonce AS BLOB))<=20 THEN nonce END,CASE WHEN length(CAST(environment AS BLOB))<=32 THEN environment END,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,CASE WHEN length(CAST(preparation_json AS BLOB))<=2048 THEN preparation_json END FROM execution_signing_preparations WHERE intent_id=?1",[&intent.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
        ensure!(saved==(account.clone(),signer.into(),nonce.to_string(),"mainnet".into(),"native_ioc_signing_preparation".into(),encoded),"signing preparation changed before commit");tx.commit()?;Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{core::Side, signer_nonce::NonceCoordinator, submission_wire::SubmissionJournal};
    const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";
    const SIGNER: &str = "0x2222222222222222222222222222222222222222";
    fn setup() -> (Storage, OrderIntent, OrderAction) {
        let db = Storage::open(":memory:").unwrap();
        db.migrate().unwrap();
        db.bind_trading_account(ACCOUNT, false).unwrap();
        let i = OrderIntent {
            id: uuid::Uuid::new_v4().to_string(),
            venue: "hyperliquid".into(),
            symbol: "ETH".into(),
            side: Side::Long,
            size: 2.0,
            limit_price: Some(101.0),
            reduce_only: false,
            stop_price: None,
            take_profit: None,
            source_signal: None,
            created_at: chrono::Utc::now(),
        };
        db.reserve_intent(&i).unwrap();
        let a = OrderAction::ioc(4, true, 101.0, 1.2345, false, &i.id).unwrap();
        SubmissionJournal::new(&db, &i)
            .record(ACCOUNT, &a, 4)
            .unwrap();
        (db, i, a)
    }
    fn saved(db: &Storage, id: &str) -> Preparation {
        let s:String=db.with_conn(|c|c.query_row("SELECT preparation_json FROM execution_signing_preparations WHERE intent_id=?1",[id],|r|r.get(0))).unwrap();
        serde_json::from_str(&s).unwrap()
    }
    #[test]
    fn exact_preparation_retains_public_signer_and_supported_domain_without_authorization_material()
    {
        let (db, i, a) = setup();
        SubmissionJournal::new(&db, &i)
            .record_signing_preparation(SIGNER, 100, &a)
            .unwrap();
        let p = saved(&db, &i.id);
        assert_eq!(p.account, ACCOUNT);
        assert_eq!(p.signer, SIGNER);
        assert_eq!(p.nonce, "100");
        assert_eq!(p.phase, "prepared_not_sent");
        assert_eq!(
            p.connection_hash,
            hex::encode(crate::hyperliquid_wire::action_hash(
                &a.packed().unwrap(),
                100,
                None,
                None
            ))
        );
        assert_eq!(
            p.agent_digest,
            hex::encode(crate::hyperliquid_wire::agent_digest(
                crate::hyperliquid_wire::action_hash(&a.packed().unwrap(), 100, None, None),
                true
            ))
        );
        assert_eq!(p.domain_chain_id, 1337);
        assert_eq!(p.agent_source, "a");
        assert!(p.vault_address.is_none() && p.expires_after_ms.is_none());
        let value = serde_json::to_value(p).unwrap();
        for key in ["signature", "private_key", "signed_payload"] {
            assert!(value.get(key).is_none());
        }
        assert!(db.ensure_execution_resolved().is_err());
    }
    #[test]
    fn changed_action_or_submitted_intent_cannot_receive_preparation() {
        for change_intent in [false, true] {
            let (db, i, a) = setup();
            let passed = if change_intent {
                db.with_conn(|c| c.execute("UPDATE execution_intents SET intent_json='{}'", []))
                    .unwrap();
                a.clone()
            } else {
                OrderAction::ioc(4, true, 102.0, 1.2345, false, &i.id).unwrap()
            };
            assert!(record(&db, &i, SIGNER, 100, &passed).is_err());
            let count: i64 = db
                .with_conn(|c| {
                    c.query_row(
                        "SELECT count(*) FROM execution_signing_preparations",
                        [],
                        |r| r.get(0),
                    )
                })
                .unwrap();
            assert_eq!(count, 0);
        }
    }
    #[test]
    fn missing_wire_terminal_intent_and_account_conflict_fail() {
        for sql in ["DELETE FROM execution_wire_requests","UPDATE execution_intents SET state='filled'","UPDATE meta_kv SET value='0x3333333333333333333333333333333333333333' WHERE key='trading_account_binding'"] {
            let(db,i,a)=setup();db.with_conn(|c|c.execute_batch(sql)).unwrap();assert!(record(&db,&i,SIGNER,100,&a).is_err());
        }
    }
    #[test]
    fn invalid_signer_or_zero_nonce_never_create_preparation() {
        for (signer, nonce) in [
            ("invalid", 100),
            ("0x0000000000000000000000000000000000000000", 100),
            (SIGNER, 0),
        ] {
            let (db, i, a) = setup();
            assert!(record(&db, &i, signer, nonce, &a).is_err());
        }
    }
    #[test]
    fn duplicate_preparation_cannot_replace_original_or_reuse_signer_nonce() {
        let (db, i, a) = setup();
        record(&db, &i, SIGNER, 100, &a).unwrap();
        let first = saved(&db, &i.id);
        assert!(record(&db, &i, SIGNER, 101, &a).is_err());
        assert_eq!(saved(&db, &i.id), first);
        let mut j = i.clone();
        j.id = uuid::Uuid::new_v4().to_string();
        db.reserve_intent(&j).unwrap();
        let b = OrderAction::ioc(4, true, 101.0, 1.2345, false, &j.id).unwrap();
        SubmissionJournal::new(&db, &j)
            .record(ACCOUNT, &b, 4)
            .unwrap();
        assert!(record(&db, &j, SIGNER, 100, &b).is_err());
        record(&db, &j, SIGNER, 101, &b).unwrap();
    }
    #[test]
    fn journal_failure_burns_allocated_nonce_without_claiming_preparation() {
        let d = tempfile::tempdir().unwrap();
        let c = NonceCoordinator::at_root(d.path());
        let nonce = c.reserve(SIGNER).unwrap();
        let (db, i, a) = setup();
        db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_preparation BEFORE INSERT ON execution_signing_preparations BEGIN SELECT RAISE(ABORT,'offline-preparation-fault');END")).unwrap();
        assert!(record(&db, &i, SIGNER, nonce, &a).is_err());
        assert!(c.reserve(SIGNER).unwrap() > nonce);
        assert!(db.ensure_execution_resolved().is_err());
    }
    #[test]
    fn ignored_or_rewritten_preparation_is_not_returned_as_committed() {
        for sql in ["CREATE TRIGGER alter_preparation BEFORE INSERT ON execution_signing_preparations BEGIN SELECT RAISE(IGNORE);END;",
            "CREATE TRIGGER alter_preparation AFTER INSERT ON execution_signing_preparations BEGIN UPDATE execution_signing_preparations SET preparation_json='{}' WHERE intent_id=NEW.intent_id;END;",
            "CREATE TRIGGER alter_preparation AFTER INSERT ON execution_signing_preparations BEGIN UPDATE execution_signing_preparations SET signer='0x3333333333333333333333333333333333333333' WHERE intent_id=NEW.intent_id;END;"] {
            let(db,i,a)=setup();db.with_conn(|c|c.execute_batch(sql)).unwrap();assert!(record(&db,&i,SIGNER,100,&a).is_err());
            let count:i64=db.with_conn(|c|c.query_row("SELECT count(*) FROM execution_signing_preparations",[],|r|r.get(0))).unwrap();assert_eq!(count,0);
        }
    }
    #[test]
    fn committed_preparation_survives_database_restart() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("history.sqlite");
        let (db, i, a) = setup();
        let file = Storage::open(&path).unwrap();
        file.migrate().unwrap();
        file.bind_trading_account(ACCOUNT, false).unwrap();
        file.reserve_intent(&i).unwrap();
        SubmissionJournal::new(&file, &i)
            .record(ACCOUNT, &a, 4)
            .unwrap();
        record(&file, &i, SIGNER, 100, &a).unwrap();
        let before = saved(&file, &i.id);
        drop(file);
        drop(db);
        let file = Storage::open(&path).unwrap();
        file.migrate().unwrap();
        assert_eq!(saved(&file, &i.id), before);
        assert!(file.ensure_execution_resolved().is_err());
    }
}
