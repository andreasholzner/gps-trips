//! `trip_komoot_link` rows (US-22/US-20, ADR-0021): dedup + sync-state for
//! Komoot-sourced trips. Trip/track and photo CRUD live in the sibling
//! `trip`/`photo` modules.

use std::collections::HashSet;

use sqlx::{sqlite::SqliteRow, Row, Sqlite, SqlitePool, Transaction};

use crate::models::ActivityType;

/// Every `komoot_tour_id` already linked to a trip (or pending Komoot-side
/// deletion) — the anti-join dedup set US-22's "Sync now" filters Komoot's
/// tour list against before offering candidates to import.
pub async fn list_linked_tour_ids(pool: &SqlitePool) -> Result<HashSet<String>, sqlx::Error> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT komoot_tour_id FROM trip_komoot_link")
        .fetch_all(pool)
        .await?;
    Ok(ids.into_iter().collect())
}

/// Insert a `trip_komoot_link` row on the caller's transaction, so it commits
/// atomically with the trip/track/photos it links to (ADR-0021: a crash
/// mid-pull must never leave an imported trip without its link row, or vice
/// versa).
pub async fn insert_link_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    trip_id: i64,
    komoot_tour_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO trip_komoot_link (trip_id, komoot_tour_id) VALUES (?, ?)")
        .bind(trip_id)
        .bind(komoot_tour_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// A trip whose edit hasn't been pushed to Komoot yet (US-20): the current
/// name/activity_type `push_pending_edits` needs to call Komoot's
/// update-tour API with, plus the tour id to call it on.
pub struct EditPending {
    pub trip_id: i64,
    pub komoot_tour_id: String,
    pub name: String,
    pub activity_type: ActivityType,
}

/// Every trip with a pending edit to push to Komoot (US-20): an inner join
/// on `trip` — a link row whose `trip_id` was `SET NULL` by a delete (still
/// `delete_pending`, not `edit_pending`) is never returned here, so this
/// never tries to push an edit for a trip that no longer exists.
pub async fn list_edit_pending(pool: &SqlitePool) -> Result<Vec<EditPending>, sqlx::Error> {
    sqlx::query(
        r#"SELECT l.trip_id AS trip_id, l.komoot_tour_id AS komoot_tour_id,
                  t.name AS name, t.activity_type AS activity_type
           FROM trip_komoot_link l
           JOIN trip t ON t.id = l.trip_id
           WHERE l.edit_pending = 1"#,
    )
    .map(|row: SqliteRow| EditPending {
        trip_id: row.get("trip_id"),
        komoot_tour_id: row.get("komoot_tour_id"),
        name: row.get("name"),
        activity_type: row.get("activity_type"),
    })
    .fetch_all(pool)
    .await
}

/// How many trips have a pending edit to push to Komoot (US-20) — drives the
/// "Sync now" review page's pending-edit count without fetching every row's
/// name/activity_type just to count them.
pub async fn count_edit_pending(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM trip_komoot_link WHERE edit_pending = 1")
        .fetch_one(pool)
        .await
}

/// Clear a trip's `edit_pending` flag after `push_pending_edits` has
/// successfully called Komoot's update-tour API for it (US-20).
pub async fn clear_edit_pending(pool: &SqlitePool, trip_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE trip_komoot_link SET edit_pending = 0 WHERE trip_id = ?")
        .bind(trip_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::db::testing::TestDb;
    use crate::server::gpx::TrackStats;
    use crate::server::repo::insert_trip;

    async fn a_trip(pool: &SqlitePool) -> i64 {
        let stats = TrackStats {
            distance_m: 1.0,
            ascent_m: 0.0,
            descent_m: 0.0,
            duration_secs: None,
            start_time: None,
            end_time: None,
            min_lat: 0.0,
            min_lon: 0.0,
            max_lat: 0.0,
            max_lon: 0.0,
        };
        insert_trip(
            pool,
            "Trip",
            ActivityType::Hiking,
            "Europe/Oslo",
            &stats,
            "{}",
            b"x",
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn list_linked_tour_ids_is_empty_with_no_links() {
        let db = TestDb::new().await;
        assert!(list_linked_tour_ids(&db.pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn insert_link_in_tx_then_list_linked_tour_ids_returns_it() {
        let db = TestDb::new().await;
        let trip_id = a_trip(&db.pool).await;

        let mut tx = db.pool.begin().await.unwrap();
        insert_link_in_tx(&mut tx, trip_id, "123456").await.unwrap();
        tx.commit().await.unwrap();

        let linked = list_linked_tour_ids(&db.pool).await.unwrap();
        assert_eq!(linked, HashSet::from(["123456".to_string()]));
    }

    #[tokio::test]
    async fn insert_link_in_tx_rolls_back_with_the_rest_of_the_transaction() {
        // ADR-0021: the link row commits atomically with the trip it links —
        // a rolled-back transaction must leave no link row behind either.
        let db = TestDb::new().await;
        let trip_id = a_trip(&db.pool).await;

        let mut tx = db.pool.begin().await.unwrap();
        insert_link_in_tx(&mut tx, trip_id, "123456").await.unwrap();
        tx.rollback().await.unwrap();

        assert!(list_linked_tour_ids(&db.pool).await.unwrap().is_empty());
    }

    // ── US-20: pending-edit push-phase queries ───────────────────────────

    async fn a_linked_trip(pool: &SqlitePool, tour_id: &str) -> i64 {
        let trip_id = a_trip(pool).await;
        let mut tx = pool.begin().await.unwrap();
        insert_link_in_tx(&mut tx, trip_id, tour_id).await.unwrap();
        tx.commit().await.unwrap();
        trip_id
    }

    async fn mark_edit_pending(pool: &SqlitePool, trip_id: i64) {
        sqlx::query("UPDATE trip_komoot_link SET edit_pending = 1 WHERE trip_id = ?")
            .bind(trip_id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn list_edit_pending_is_empty_with_no_pending_edits() {
        let db = TestDb::new().await;
        a_linked_trip(&db.pool, "123456").await;
        assert!(list_edit_pending(&db.pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_edit_pending_returns_the_trips_current_name_and_activity_type() {
        let db = TestDb::new().await;
        let trip_id = a_linked_trip(&db.pool, "123456").await;
        mark_edit_pending(&db.pool, trip_id).await;

        let pending = list_edit_pending(&db.pool).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].trip_id, trip_id);
        assert_eq!(pending[0].komoot_tour_id, "123456");
        assert_eq!(pending[0].name, "Trip");
        assert_eq!(pending[0].activity_type, ActivityType::Hiking);
    }

    #[tokio::test]
    async fn list_edit_pending_excludes_an_orphaned_link_row() {
        // A link row whose trip was deleted (trip_id SET NULL by the FK) is
        // delete_pending, not edit_pending — but even if edit_pending were
        // somehow set on it, the inner join on `trip` must exclude it: there
        // is no trip left to read a name/activity_type from.
        let db = TestDb::new().await;
        let trip_id = a_linked_trip(&db.pool, "123456").await;
        mark_edit_pending(&db.pool, trip_id).await;

        sqlx::query("DELETE FROM trip WHERE id = ?")
            .bind(trip_id)
            .execute(&db.pool)
            .await
            .unwrap();

        assert!(list_edit_pending(&db.pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn count_edit_pending_counts_only_pending_rows() {
        let db = TestDb::new().await;
        let a = a_linked_trip(&db.pool, "111").await;
        a_linked_trip(&db.pool, "222").await;
        mark_edit_pending(&db.pool, a).await;

        assert_eq!(count_edit_pending(&db.pool).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn clear_edit_pending_removes_the_trip_from_the_pending_list() {
        let db = TestDb::new().await;
        let trip_id = a_linked_trip(&db.pool, "123456").await;
        mark_edit_pending(&db.pool, trip_id).await;

        clear_edit_pending(&db.pool, trip_id).await.unwrap();

        assert!(list_edit_pending(&db.pool).await.unwrap().is_empty());
        assert_eq!(count_edit_pending(&db.pool).await.unwrap(), 0);
    }
}
