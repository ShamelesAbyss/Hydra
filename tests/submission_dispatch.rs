//! Engine capability dispatch. The adapter is entirely offline and cannot send.
use anyhow::Result;
use async_trait::async_trait;
use hydra::{
    adapters::{OrderReceipt, VenueAdapter, VenuePosition},
    core::{MarketSnapshot, OrderIntent, OrderResult, Side, VenueKind},
    execution::ExecutionEngine,
    storage::Storage,
    submission_wire::SubmissionJournal,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
struct JournalAware {
    db: Storage,
    calls: Arc<AtomicUsize>,
}
#[async_trait]
impl VenueAdapter for JournalAware {
    fn kind(&self) -> VenueKind {
        VenueKind::Hyperliquid
    }
    async fn ping(&self) -> Result<()> {
        panic!("no health probe")
    }
    async fn snapshot(&self, _: &str, _: usize) -> Result<MarketSnapshot> {
        panic!("no snapshot for protective submission")
    }
    async fn open_positions(&self) -> Result<Vec<VenuePosition>> {
        panic!("no position probe")
    }
    async fn account_equity_usd(&self) -> Result<f64> {
        panic!("no equity probe")
    }
    async fn close_position(&self, _: &str, _: f64, _: bool) -> Result<OrderResult> {
        panic!("no direct close")
    }
    async fn submit_order(&self, _: &OrderIntent) -> Result<OrderResult> {
        panic!("journal-aware dispatch required")
    }
    async fn submit_order_with_evidence(&self, _: &OrderIntent) -> Result<OrderReceipt> {
        panic!("journal-aware override required")
    }
    async fn submit_order_journaled(
        &self,
        i: &OrderIntent,
        journal: &SubmissionJournal<'_>,
    ) -> Result<OrderReceipt> {
        assert_eq!(journal.intent_id(), i.id);
        let state: String = self.db.with_conn(|c| {
            c.query_row(
                "SELECT state FROM execution_intents WHERE intent_id=?1",
                [&i.id],
                |r| r.get(0),
            )
        })?;
        assert_eq!(state, "submitted");
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(OrderReceipt {
            result: OrderResult {
                intent_id: i.id.clone(),
                accepted: true,
                venue_order_id: Some("42".into()),
                filled_size: i.size,
                avg_fill_price: Some(100.0),
                message: "offline legacy/custom adapter".into(),
            },
            precise_fill: None,
        })
    }
}
#[tokio::test]
async fn controller_uses_journal_aware_boundary_after_durable_intent() {
    let db = Storage::open(":memory:").unwrap();
    db.migrate().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let adapter = JournalAware {
        db: db.clone(),
        calls: calls.clone(),
    };
    let mut engine = ExecutionEngine::new(db.clone(), true);
    let intent = OrderIntent {
        id: uuid::Uuid::new_v4().to_string(),
        venue: "hyperliquid".into(),
        symbol: "ETH".into(),
        side: Side::Short,
        size: 1.0,
        limit_price: Some(100.0),
        reduce_only: true,
        stop_price: None,
        take_profit: None,
        source_signal: None,
        created_at: chrono::Utc::now(),
    };
    assert!(engine.submit(&intent, &adapter).await.unwrap().accepted);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let state: String = db
        .with_conn(|c| {
            c.query_row(
                "SELECT state FROM execution_intents WHERE intent_id=?1",
                [&intent.id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(state, "filled");
    let wire: i64 = db
        .with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM execution_wire_requests", [], |r| {
                r.get(0)
            })
        })
        .unwrap();
    assert_eq!(wire, 0);
    assert!(db.ensure_execution_resolved().is_err());
}
