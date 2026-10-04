//! What the place and ground databases (ADR-0027) have in common: each a
//! SQLite file of its own, opened read-only and optionally by the server,
//! and written only by `places_build` — created whole, never migrated.

use std::path::Path;
use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{ConnectOptions, SqliteConnection, SqlitePool};

/// Opens the database at `path` read-only, checking that `probe` — a query
/// over its tables — runs; `Ok(None)` when there is no file there, which is
/// a supported way to run. A file that is not the database fails here, at
/// boot.
pub async fn open_read_only(path: &Path, probe: &str) -> Result<Option<SqlitePool>, sqlx::Error> {
    if !path.is_file() {
        return Ok(None);
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .immutable(true);
    let pool = SqlitePoolOptions::new().connect_with(options).await?;
    sqlx::query(probe).fetch_optional(&pool).await?;
    Ok(Some(pool))
}

/// A new database at `path` with `schema`, in a transaction for what is
/// written next. Nothing that already holds a database is written over.
pub async fn create(path: &Path, schema: &str) -> Result<SqliteConnection, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Off)
        .synchronous(SqliteSynchronous::Off);
    let mut conn = options.connect().await?;
    if sqlx::query("SELECT 1 FROM sqlite_master LIMIT 1")
        .fetch_optional(&mut conn)
        .await?
        .is_some()
    {
        return Err(sqlx::Error::Protocol(format!(
            "{} already holds a database",
            path.display()
        )));
    }
    sqlx::raw_sql(schema).execute(&mut conn).await?;
    sqlx::query("BEGIN").execute(&mut conn).await?;
    Ok(conn)
}

/// Makes the database at `source` readable as `src` — for cutting a test
/// fixture out of it. Attaching needs no open transaction, so the one
/// [`create`] began is committed first and another begun after.
pub async fn attach_source(conn: &mut SqliteConnection, source: &Path) -> Result<(), sqlx::Error> {
    sqlx::query("COMMIT").execute(&mut *conn).await?;
    sqlx::query("ATTACH DATABASE ? AS src")
        .bind(source.display().to_string())
        .execute(&mut *conn)
        .await?;
    sqlx::query("BEGIN").execute(&mut *conn).await?;
    Ok(())
}

/// Undoes [`attach_source`], keeping what was copied.
pub async fn detach_source(conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
    sqlx::query("COMMIT").execute(&mut *conn).await?;
    sqlx::query("DETACH DATABASE src")
        .execute(&mut *conn)
        .await?;
    sqlx::query("BEGIN").execute(&mut *conn).await?;
    Ok(())
}

/// Commits what was written and compacts the file.
pub async fn finish(mut conn: SqliteConnection) -> Result<(), sqlx::Error> {
    sqlx::query("COMMIT").execute(&mut conn).await?;
    sqlx::query("VACUUM").execute(&mut conn).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = "CREATE TABLE thing (id INTEGER PRIMARY KEY);";
    const PROBE: &str = "SELECT 1 FROM thing LIMIT 1";

    #[tokio::test]
    async fn us74_a_created_database_opens_read_only() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("db.sqlite");
        finish(create(&path, SCHEMA).await.expect("created"))
            .await
            .expect("finished");

        assert!(open_read_only(&path, PROBE).await.expect("opens").is_some());
    }

    #[tokio::test]
    async fn us74_an_existing_database_is_not_written_over() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("db.sqlite");
        finish(create(&path, SCHEMA).await.expect("created"))
            .await
            .expect("finished");

        assert!(create(&path, SCHEMA).await.is_err());
    }

    #[tokio::test]
    async fn us74_a_file_that_is_not_the_database_fails_to_open() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let text = dir.path().join("notes.txt");
        std::fs::write(&text, "not a database").expect("written");
        let other = dir.path().join("other.sqlite");
        finish(
            create(&other, "CREATE TABLE other (id INTEGER);")
                .await
                .expect("created"),
        )
        .await
        .expect("finished");

        assert!(open_read_only(&text, PROBE).await.is_err());
        assert!(open_read_only(&other, PROBE).await.is_err());
    }

    #[tokio::test]
    async fn us74_no_file_is_no_database() {
        let dir = tempfile::tempdir().expect("a temp dir");
        assert!(open_read_only(&dir.path().join("none.sqlite"), PROBE)
            .await
            .expect("not an error")
            .is_none());
    }
}
