//! The two-phase import's parked parse (US-12, migration 0014).
//!
//! A row here is a GPX that has been read but not yet accepted as a trip: the
//! import screen needs the track's start date to suggest a name *before* the
//! owner types one, and parsing the file again at confirmation time would do
//! the same work twice.
//!
//! Nothing outside the import handlers touches this table, which is the
//! reason it is a table of its own rather than a `draft` flag on `trip` —
//! `qmapshack_export` and `komoot_backfill` open this database directly, and
//! a flag would have needed filtering in both.
//!
//! This module stores and returns the derived data as opaque strings, the way
//! `insert_trip_in_tx` takes the track GeoJSON its caller built (ADR-0003).
//! What is *in* the JSON is `import.rs`'s business.

use sqlx::{Sqlite, SqlitePool, Transaction};
use time::OffsetDateTime;

use super::to_rfc3339;

/// A parse on its way to becoming a trip.
pub struct NewStagedImport<'a> {
    /// The parse's own output as JSON — `import.rs`'s `StagedTrack`.
    pub derived: &'a str,
    pub geojson: &'a str,
    pub gpx: &'a [u8],
}

/// What comes back out at confirmation time, in the shape `NewTrip` wants.
pub struct StagedImportRow {
    pub derived: String,
    pub geojson: String,
    pub gpx: Vec<u8>,
}

/// Park a parse and return its handle. `now` is passed in rather than read
/// here, keeping the sweeper's clock out of the tested logic
/// ([ADR-0012](../../../../docs/adr/0012-tdd-test-strategy.md)'s 2026-07-24
/// amendment).
pub async fn insert_staged_import(
    pool: &SqlitePool,
    staged: &NewStagedImport<'_>,
    now: OffsetDateTime,
) -> Result<i64, sqlx::Error> {
    let id = sqlx::query(
        "INSERT INTO import_staging (created_at, derived, geojson, gpx) VALUES (?,?,?,?)",
    )
    .bind(to_rfc3339(now))
    .bind(staged.derived)
    .bind(staged.geojson)
    .bind(staged.gpx)
    .execute(pool)
    .await?
    .last_insert_rowid();

    Ok(id)
}

/// Read a parked parse *and delete it*, on the caller's transaction.
///
/// One statement pair rather than two calls, and on the same transaction that
/// inserts the trip, so a confirmed import cannot be confirmed twice and a
/// failed insert cannot consume the row it was promoting. `None` means the
/// handle is spent, swept, or never existed — one answer, because all three
/// mean the same thing to the owner.
pub async fn take_staged_import_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
) -> Result<Option<StagedImportRow>, sqlx::Error> {
    let row = sqlx::query_as::<_, (String, String, Vec<u8>)>(
        "SELECT derived, geojson, gpx FROM import_staging WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;

    let Some((derived, geojson, gpx)) = row else {
        return Ok(None);
    };

    sqlx::query("DELETE FROM import_staging WHERE id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await?;

    Ok(Some(StagedImportRow {
        derived,
        geojson,
        gpx,
    }))
}

/// Drop a parked parse the owner walked away from. `false` if there was
/// nothing to drop.
pub async fn delete_staged_import(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let affected = sqlx::query("DELETE FROM import_staging WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();

    Ok(affected > 0)
}

/// Delete every parse parked before `cutoff`, returning how many went.
///
/// Called on the way into a new staging request rather than from a timer:
/// abandoned rows only accumulate when imports happen, so that is exactly
/// when it is worth looking, and it needs no background task to own.
pub async fn sweep_staged_imports(
    pool: &SqlitePool,
    cutoff: OffsetDateTime,
) -> Result<u64, sqlx::Error> {
    let affected = sqlx::query("DELETE FROM import_staging WHERE created_at < ?")
        .bind(to_rfc3339(cutoff))
        .execute(pool)
        .await?
        .rows_affected();

    Ok(affected)
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::db::testing::TestDb;
    use time::macros::datetime;

    const NOW: OffsetDateTime = datetime!(2026-09-01 12:00 UTC);

    async fn stage(pool: &SqlitePool, at: OffsetDateTime) -> i64 {
        insert_staged_import(
            pool,
            &NewStagedImport {
                derived: r#"{"parked":true}"#,
                geojson: r#"{"type":"LineString"}"#,
                gpx: b"<gpx/>",
            },
            at,
        )
        .await
        .expect("insert")
    }

    #[tokio::test]
    async fn a_parked_parse_comes_back_exactly_as_it_went_in() {
        // Promotion copies these onto the trip and track rows, so a lossy
        // round trip would corrupt the imported track rather than fail.
        let db = TestDb::new().await;
        let id = stage(&db.pool, NOW).await;

        let mut tx = db.pool.begin().await.expect("tx");
        let row = take_staged_import_in_tx(&mut tx, id)
            .await
            .expect("take")
            .expect("the parse is there");
        tx.commit().await.expect("commit");

        assert_eq!(row.derived, r#"{"parked":true}"#);
        assert_eq!(row.geojson, r#"{"type":"LineString"}"#);
        assert_eq!(row.gpx, b"<gpx/>");
    }

    #[tokio::test]
    async fn taking_a_parse_consumes_it() {
        let db = TestDb::new().await;
        let id = stage(&db.pool, NOW).await;

        let mut tx = db.pool.begin().await.expect("tx");
        take_staged_import_in_tx(&mut tx, id).await.expect("take");
        let again = take_staged_import_in_tx(&mut tx, id).await.expect("take");
        tx.commit().await.expect("commit");

        assert!(again.is_none(), "a spent handle names nothing");
    }

    #[tokio::test]
    async fn a_rolled_back_take_leaves_the_parse_where_it_was() {
        // What makes a refused confirmation retryable: the owner fixes the
        // activity type rather than uploading the file again.
        let db = TestDb::new().await;
        let id = stage(&db.pool, NOW).await;

        let mut tx = db.pool.begin().await.expect("tx");
        take_staged_import_in_tx(&mut tx, id).await.expect("take");
        tx.rollback().await.expect("rollback");

        let mut tx = db.pool.begin().await.expect("tx");
        assert!(take_staged_import_in_tx(&mut tx, id)
            .await
            .expect("take")
            .is_some());
    }

    #[tokio::test]
    async fn the_sweeper_takes_the_abandoned_and_leaves_the_current() {
        let db = TestDb::new().await;
        let abandoned = stage(&db.pool, NOW - time::Duration::hours(48)).await;
        let current = stage(&db.pool, NOW - time::Duration::minutes(5)).await;

        let swept = sweep_staged_imports(&db.pool, NOW - time::Duration::hours(24))
            .await
            .expect("sweep");

        assert_eq!(swept, 1);
        let mut tx = db.pool.begin().await.expect("tx");
        assert!(take_staged_import_in_tx(&mut tx, abandoned)
            .await
            .expect("take")
            .is_none());
        assert!(take_staged_import_in_tx(&mut tx, current)
            .await
            .expect("take")
            .is_some());
    }

    #[tokio::test]
    async fn cancelling_says_whether_there_was_anything_to_cancel() {
        let db = TestDb::new().await;
        let id = stage(&db.pool, NOW).await;

        assert!(delete_staged_import(&db.pool, id).await.expect("delete"));
        assert!(!delete_staged_import(&db.pool, id).await.expect("delete"));
    }
}
