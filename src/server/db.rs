use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
    ConnectOptions, Connection, Sqlite, SqlitePool, Transaction,
};
use std::{path::Path, time::Duration};

/// Open (or create) the SQLite database at `db_path` and run all pending migrations.
///
/// Pragmas applied per ADR-0002: WAL journal mode, foreign keys on, busy timeout.
pub async fn create_pool(db_path: &Path) -> anyhow::Result<SqlitePool> {
    let pool = SqlitePool::connect_with(options(db_path).create_if_missing(true)).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

fn options(db_path: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(db_path)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5))
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

/// How long [`close`] waits for the pool's last connections to finish
/// closing, at most.
const CLOSE_PATIENCE: Duration = Duration::from_secs(2);

/// Close the archive's database for good (US-47), checkpointing its WAL into
/// the database file and removing it.
///
/// SQLite does that when the last connection closes, but the pool's may
/// close at the same moment — a dropped one goes back in a task of its own —
/// and each then sees another still open and leaves the WAL. So once the pool
/// is closed, one connection more is opened and closed: alone, it is the last.
///
/// Alone only once the pool's connections have finished closing, though,
/// and one may still be closing when the pool says it is closed — on a
/// single CPU, often. It finishes only once the pool itself is dropped, so
/// that comes first; and the last connection is tried again until the WAL
/// is gone, since the process exits next and would cut a slower one short.
pub async fn close(pool: SqlitePool, db_path: &Path) -> anyhow::Result<()> {
    pool.close().await;
    drop(pool);
    let wal = wal_path(db_path);
    let deadline = tokio::time::Instant::now() + CLOSE_PATIENCE;
    loop {
        options(db_path).connect().await?.close().await?;
        if !wal.exists() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!("The database's WAL outlived closing it; the next start recovers it");
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The WAL SQLite keeps beside the database at `db_path`.
fn wal_path(db_path: &Path) -> std::path::PathBuf {
    let mut wal = db_path.as_os_str().to_owned();
    wal.push("-wal");
    wal.into()
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn us47_closing_checkpoints_the_wal_when_connections_close_at_once() {
        // A connection a request drops goes back to the pool in a task of its
        // own, so on shutdown several can close at the same moment. SQLite
        // checkpoints and removes the WAL only on closing the last
        // connection, and two closing together may each see the other open.
        for _ in 0..50 {
            let dir = tempfile::TempDir::new().unwrap();
            let path = dir.path().join("test.db");
            let pool = create_pool(&path).await.unwrap();
            let mut conns = Vec::new();
            for _ in 0..4 {
                conns.push(pool.acquire().await.unwrap());
            }
            for conn in &mut conns {
                sqlx::query("INSERT INTO tag (name) VALUES (hex(randomblob(8)))")
                    .execute(&mut **conn)
                    .await
                    .unwrap();
            }
            drop(conns);

            close(pool, &path).await.unwrap();

            assert!(!dir.path().join("test.db-wal").exists());
        }
    }
}
