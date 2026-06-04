use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
    SqlitePool,
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
