use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
    Sqlite, SqlitePool, Transaction,
};
use std::{path::Path, time::Duration};

/// Open (or create) the SQLite database at `db_path` and run all pending migrations.
///
/// Pragmas applied per ADR-0002: WAL journal mode, foreign keys on, busy timeout.
pub async fn create_pool(db_path: &Path) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePool::connect_with(opts).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// Begin a transaction that is going to write, holding the write lock from
/// its first statement (`BEGIN IMMEDIATE`). A deferred transaction that
/// reads and then writes fails outright if another writer committed in
/// between — and since US-70 every request writes an access-log row, so that
/// is ordinary. Holding the lock instead makes other writers wait their turn,
/// within the busy timeout above. Read-only transactions keep `begin()`.
pub async fn begin_write(pool: &SqlitePool) -> Result<Transaction<'static, Sqlite>, sqlx::Error> {
    pool.begin_with("BEGIN IMMEDIATE").await
}

// ── Test helpers ─────────────────────────────────────────────────────────────

#[cfg(test)]
pub mod testing {
    use super::*;
    use tempfile::TempDir;

    /// A short-lived test database. Drop `TempDir` to clean up.
    /// Uses a real file (not :memory:) per ADR-0012 so WAL semantics match production.
    pub struct TestDb {
        pub pool: SqlitePool,
        _dir: TempDir,
    }

    impl TestDb {
        pub async fn new() -> Self {
            let dir = TempDir::new().expect("temp dir");
            let db_path = dir.path().join("test.db");
            let pool = create_pool(&db_path).await.expect("test pool");
            TestDb { pool, _dir: dir }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::TestDb;
    use super::*;

    /// Some write on a connection of its own, as a concurrent request's.
    async fn another_write(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO tag (name) VALUES ('elsewhere')")
            .execute(pool)
            .await
            .map(|_| ())
    }

    #[tokio::test]
    async fn a_write_transaction_that_reads_first_survives_a_concurrent_writer() {
        // Every request writes an access-log row since US-70, so another
        // writer between a transaction's read and its write is the normal
        // case, not a rare one. A deferred transaction fails there at once
        // (SQLITE_BUSY_SNAPSHOT, no busy wait); one begun for writing holds
        // the lock, and the other writer waits its turn instead.
        let db = TestDb::new().await;
        let mut tx = begin_write(&db.pool).await.unwrap();
        let _: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tag")
            .fetch_one(&mut *tx)
            .await
            .unwrap();

        let pool = db.pool.clone();
        let other = tokio::spawn(async move { another_write(&pool).await });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        sqlx::query("INSERT INTO tag (name) VALUES ('mine')")
            .execute(&mut *tx)
            .await
            .expect("the transaction's own write");
        tx.commit().await.unwrap();
        other
            .await
            .unwrap()
            .expect("the other writer, after waiting");
    }
}
