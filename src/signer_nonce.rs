//! Durable nonce allocation for cooperating processes sharing one operator root.
//! This foundation does not sign, submit, or change the live sender.
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
const APPLICATION_ID: i64 = 0x48594e43;

#[derive(Clone, Debug)]
pub struct NonceCoordinator {
    root: PathBuf,
}
impl NonceCoordinator {
    pub fn at_root(root: &Path) -> Self {
        Self { root: root.into() }
    }
    pub fn operator_default() -> Result<Self> {
        let locks = crate::ownership::AccountOwnership::default_root()?;
        Ok(Self::at_root(
            &locks
                .parent()
                .context("operator state root unavailable")?
                .join("signer-nonces"),
        ))
    }
    pub fn reserve(&self, signer: &str) -> Result<u64> {
        self.reserve_with_clock(signer, || {
            Ok(u64::try_from(chrono::Utc::now().timestamp_millis())?)
        })
    }
    fn reserve_with_clock(&self, signer: &str, clock: impl FnOnce() -> Result<u64>) -> Result<u64> {
        let signer = crate::ownership::canonical_account(signer)?;
        std::fs::create_dir_all(&self.root).context("create signer nonce directory")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o700))?;
        }
        let path = self.root.join("nonces.sqlite");
        let mut conn = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "trusted_schema", false)?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        let mode: String = conn.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
        ensure!(mode == "delete", "unsupported nonce journal mode");
        let sync: i64 = conn.pragma_query_value(None, "synchronous", |r| r.get(0))?;
        ensure!(sync == 2, "durable nonce synchronization unavailable");
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let app: i64 = tx.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if app == 0 && version == 0 {
            let objects: i64 = tx.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )?;
            ensure!(objects == 0, "refusing to adopt unrelated nonce database");
            tx.execute_batch("CREATE TABLE signer_nonces(signer TEXT PRIMARY KEY NOT NULL,last_nonce TEXT NOT NULL CHECK(length(last_nonce) BETWEEN 1 AND 20)) STRICT;")?;
            tx.pragma_update(None, "application_id", APPLICATION_ID)?;
            tx.pragma_update(None, "user_version", 1)?;
        } else {
            ensure!(
                app == APPLICATION_ID && version == 1,
                "unsupported signer nonce database"
            );
        }
        ensure!(
            crate::status_history::table(&tx, "signer_nonces")?,
            "nonce history table unavailable"
        );
        let columns = {
            let mut query=tx.prepare("SELECT name,type,\"notnull\",pk FROM pragma_table_info('signer_nonces') ORDER BY cid")?;
            let rows = query.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        ensure!(
            columns
                == [
                    ("signer".into(), "TEXT".into(), 1, 1),
                    ("last_nonce".into(), "TEXT".into(), 1, 0)
                ],
            "unsupported nonce history columns or primary key"
        );
        let previous:Option<String>=tx.query_row("SELECT CASE WHEN length(CAST(last_nonce AS BLOB)) BETWEEN 1 AND 20 THEN last_nonce END FROM signer_nonces WHERE signer=?1",[&signer],|r|r.get(0)).optional()?;
        let prior = previous
            .map(|s| -> Result<u64> {
                let n = s.parse::<u64>()?;
                ensure!(n.to_string() == s, "noncanonical nonce history");
                Ok(n)
            })
            .transpose()?
            .unwrap_or(0);
        // Acquire the write transaction before sampling time: contention must not
        // turn an earlier clock sample into a stale allocation window.
        let now = clock()?;
        let candidate = now.max(prior.checked_add(1).context("nonce sequence exhausted")?);
        ensure!(
            candidate <= now.checked_add(1000).context("nonce clock overflow")?,
            "signing nonce exceeds clock window"
        );
        let changed=tx.execute("INSERT INTO signer_nonces(signer,last_nonce) VALUES(?1,?2) ON CONFLICT(signer) DO UPDATE SET last_nonce=excluded.last_nonce",params![signer,candidate.to_string()])?;
        ensure!(changed == 1, "nonce reservation was not written");
        let retained:String=tx.query_row("SELECT CASE WHEN length(CAST(last_nonce AS BLOB)) BETWEEN 1 AND 20 THEN last_nonce END FROM signer_nonces WHERE signer=?1",[&signer],|r|r.get(0))?;
        ensure!(
            retained == candidate.to_string(),
            "nonce reservation changed before commit"
        );
        tx.commit()?;
        // A returned reservation is consumed even if later preparation fails.
        Ok(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SIGNER: &str = "0x1111111111111111111111111111111111111111";
    fn reserve(c: &NonceCoordinator, now: u64) -> Result<u64> {
        c.reserve_with_clock(SIGNER, || Ok(now))
    }
    fn connection(c: &NonceCoordinator) -> Connection {
        Connection::open(c.root.join("nonces.sqlite")).unwrap()
    }
    #[test]
    fn reservation_is_burned_and_survives_restart_and_clock_rollback() {
        let d = tempfile::tempdir().unwrap();
        let c = NonceCoordinator::at_root(d.path());
        assert_eq!(reserve(&c, 100).unwrap(), 100);
        drop(c);
        let c = NonceCoordinator::at_root(d.path());
        assert_eq!(reserve(&c, 90).unwrap(), 101);
        assert_eq!(reserve(&c, 100).unwrap(), 102);
    }
    #[test]
    fn drift_overflow_and_corrupt_history_never_reset_sequence() {
        for bad in ["2000", "18446744073709551615", "0001", "invalid"] {
            let d = tempfile::tempdir().unwrap();
            let c = NonceCoordinator::at_root(d.path());
            reserve(&c, 100).unwrap();
            connection(&c)
                .execute("UPDATE signer_nonces SET last_nonce=?1", [bad])
                .unwrap();
            assert!(reserve(&c, 100).is_err());
            let saved: String = connection(&c)
                .query_row("SELECT last_nonce FROM signer_nonces", [], |r| r.get(0))
                .unwrap();
            assert_eq!(saved, bad);
        }
        let d = tempfile::tempdir().unwrap();
        let c = NonceCoordinator::at_root(d.path());
        assert!(reserve(&c, u64::MAX).is_err());
        assert_eq!(reserve(&c, 100).unwrap(), 100);
    }
    #[test]
    fn canonical_signer_aliases_share_history_and_other_signers_are_independent() {
        let d = tempfile::tempdir().unwrap();
        let c = NonceCoordinator::at_root(d.path());
        let a = "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd";
        assert_eq!(c.reserve_with_clock(a, || Ok(100)).unwrap(), 100);
        assert_eq!(
            c.reserve_with_clock(&format!("0x{}", a[2..].to_ascii_uppercase()), || Ok(100))
                .unwrap(),
            101
        );
        assert_eq!(reserve(&c, 100).unwrap(), 100);
    }
    #[test]
    fn concurrent_connections_reserve_distinct_monotonic_values() {
        let d = tempfile::tempdir().unwrap();
        let c = NonceCoordinator::at_root(d.path());
        let workers: Vec<_> = (0..12)
            .map(|_| {
                let c = c.clone();
                std::thread::spawn(move || reserve(&c, 1000).unwrap())
            })
            .collect();
        let mut values: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        values.sort_unstable();
        assert_eq!(values, (1000..1012).collect::<Vec<_>>());
    }
    #[test]
    fn database_fault_does_not_return_or_commit_a_reservation() {
        let d = tempfile::tempdir().unwrap();
        let c = NonceCoordinator::at_root(d.path());
        reserve(&c, 100).unwrap();
        connection(&c).execute_batch("CREATE TRIGGER fail_nonce BEFORE UPDATE ON signer_nonces BEGIN SELECT RAISE(ABORT,'offline-nonce-fault');END;").unwrap();
        assert!(reserve(&c, 100).is_err());
        connection(&c)
            .execute_batch("DROP TRIGGER fail_nonce")
            .unwrap();
        assert_eq!(reserve(&c, 100).unwrap(), 101);
    }
    #[test]
    fn ignored_or_rewritten_updates_are_refused_without_consuming_nonce() {
        for trigger in ["CREATE TRIGGER alter_nonce BEFORE UPDATE ON signer_nonces BEGIN SELECT RAISE(IGNORE);END;",
            "CREATE TRIGGER alter_nonce AFTER UPDATE ON signer_nonces BEGIN UPDATE signer_nonces SET last_nonce='9999' WHERE signer=NEW.signer;END;"] {
            let d=tempfile::tempdir().unwrap();let c=NonceCoordinator::at_root(d.path());reserve(&c,100).unwrap();
            connection(&c).execute_batch(trigger).unwrap();assert!(reserve(&c,100).is_err());
            let saved:String=connection(&c).query_row("SELECT last_nonce FROM signer_nonces",[],|r|r.get(0)).unwrap();assert_eq!(saved,"100");
            connection(&c).execute_batch("DROP TRIGGER alter_nonce").unwrap();assert_eq!(reserve(&c,100).unwrap(),101);
        }
    }
    #[test]
    fn foreign_future_view_and_corrupt_files_are_not_reinitialized() {
        for kind in 0..5 {
            let d = tempfile::tempdir().unwrap();
            let c = NonceCoordinator::at_root(d.path());
            if kind == 0 {
                connection(&c)
                    .execute_batch("CREATE TABLE foreign_data(value TEXT)")
                    .unwrap();
            } else if kind == 3 {
                std::fs::write(c.root.join("nonces.sqlite"), b"corrupt offline fixture").unwrap();
            } else {
                reserve(&c, 100).unwrap();
                if kind == 1 {
                    connection(&c)
                        .pragma_update(None, "user_version", 999)
                        .unwrap();
                } else if kind == 4 {
                    connection(&c).execute_batch("ALTER TABLE signer_nonces RENAME TO old_nonces;CREATE TABLE signer_nonces AS SELECT * FROM old_nonces").unwrap();
                } else {
                    connection(&c)
                        .execute_batch(
                            "DROP TABLE signer_nonces;CREATE VIEW signer_nonces AS SELECT 1",
                        )
                        .unwrap();
                }
            }
            let before = std::fs::read(c.root.join("nonces.sqlite")).unwrap();
            assert!(reserve(&c, 100).is_err());
            assert_eq!(std::fs::read(c.root.join("nonces.sqlite")).unwrap(), before);
        }
    }
    #[test]
    fn invalid_signer_and_clock_failure_do_not_allocate() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("not-created");
        let c = NonceCoordinator::at_root(&p);
        assert!(c.reserve("invalid").is_err());
        assert!(!p.exists());
        assert!(c
            .reserve_with_clock(SIGNER, || anyhow::bail!("offline clock failure"))
            .is_err());
        assert_eq!(reserve(&c, 100).unwrap(), 100);
    }
    #[test]
    fn file_failure_does_not_fall_back_to_memory() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("not-a-directory");
        std::fs::write(&p, b"fixture").unwrap();
        assert!(reserve(&NonceCoordinator::at_root(&p), 100).is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"fixture");
    }
}
