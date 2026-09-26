//! The access log's writer (US-70): records go into the database in
//! batches, off the requests they describe. SQLite has one writer, and a
//! request must neither wait for it nor fail because of it.

use sqlx::SqlitePool;
use tokio::sync::{mpsc, oneshot};

use super::AccessRecord;
use crate::config::access_log::{BATCH_SIZE, QUEUE_CAPACITY};
use crate::server::repo;

enum Message {
    Record(AccessRecord),
    /// Answered once everything queued before it is written.
    Flush(oneshot::Sender<()>),
}

/// The handle requests hand their records to. Cheap to clone; every clone
/// feeds the one writer task.
#[derive(Clone)]
pub struct AccessLog {
    queue: mpsc::Sender<Message>,
}

impl AccessLog {
    /// Start the writer for `pool`. Needs a Tokio runtime to run on.
    pub fn spawn(pool: sqlx::SqlitePool) -> Self {
        Self::spawn_with_capacity(pool, QUEUE_CAPACITY)
    }

    fn spawn_with_capacity(pool: SqlitePool, capacity: usize) -> Self {
        let (queue, received) = mpsc::channel(capacity);
        tokio::spawn(write(pool, received));
        Self { queue }
    }

    /// Queue `record` without waiting. A full queue drops it, with a
    /// warning: losing a record is better than holding up a request.
    pub fn record(&self, record: AccessRecord) {
        if let Err(mpsc::error::TrySendError::Full(_)) =
            self.queue.try_send(Message::Record(record))
        {
            tracing::warn!("access log queue is full; a request went unrecorded");
        }
    }

    /// Wait until every record queued so far is written — before the
    /// database closes on the way out (US-47), and in tests.
    pub async fn flush(&self) {
        let (written, done) = oneshot::channel();
        if self.queue.send(Message::Flush(written)).await.is_ok() {
            let _ = done.await;
        }
    }
}

/// The writer task: take what has arrived, up to a batch, and store it in
/// one transaction. A failed batch is logged and dropped rather than
/// retried, so a database that refuses writes cannot pile records up.
async fn write(pool: SqlitePool, mut received: mpsc::Receiver<Message>) {
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    let mut waiting = Vec::new();
    while let Some(first) = received.recv().await {
        let mut next = Some(first);
        while let Some(message) = next.take() {
            match message {
                Message::Record(record) => batch.push(record),
                Message::Flush(written) => waiting.push(written),
            }
            if batch.len() < BATCH_SIZE {
                next = received.try_recv().ok();
            }
        }
        if !batch.is_empty() {
            if let Err(err) = repo::insert_access_records(&pool, &batch).await {
                tracing::warn!(
                    records = batch.len(),
                    "could not store access log records: {err}"
                );
            }
            batch.clear();
        }
        for written in waiting.drain(..) {
            let _ = written.send(());
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::auth::Caller;
    use crate::server::db::testing::TestDb;

    fn a_record(path: &str) -> AccessRecord {
        AccessRecord {
            at: time::OffsetDateTime::now_utc(),
            method: "GET".to_string(),
            path: path.to_string(),
            status: 200,
            duration_ms: 1,
            caller: Caller::Owner,
            ip: None,
            user_agent: None,
        }
    }

    async fn stored_paths(pool: &SqlitePool) -> Vec<String> {
        sqlx::query_scalar("SELECT path FROM access_log ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn us70_a_flush_writes_everything_queued_before_it() {
        let db = TestDb::new().await;
        let log = AccessLog::spawn(db.pool.clone());

        let paths: Vec<String> = (0..BATCH_SIZE + 3).map(|i| format!("/r/{i}")).collect();
        for path in &paths {
            log.record(a_record(path));
        }
        log.flush().await;

        assert_eq!(stored_paths(&db.pool).await, paths);
    }

    #[tokio::test]
    async fn us70_a_full_queue_drops_records_rather_than_waiting() {
        // The test's runtime is single-threaded, so the writer cannot run
        // until this test awaits: every `record` below meets the queue as
        // the requests would meet a writer that has fallen behind.
        let db = TestDb::new().await;
        let log = AccessLog::spawn_with_capacity(db.pool.clone(), 2);

        for path in ["/1", "/2", "/3", "/4"] {
            log.record(a_record(path));
        }
        log.flush().await;

        assert_eq!(stored_paths(&db.pool).await, ["/1", "/2"]);
    }
}
