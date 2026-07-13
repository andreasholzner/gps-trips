//! `trip_komoot_link` rows (US-22, ADR-0021): dedup + sync-state for
//! Komoot-sourced trips. Trip/track and photo CRUD live in the sibling
//! `trip`/`photo` modules.

use std::collections::HashSet;

use sqlx::{Sqlite, SqlitePool, Transaction};

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

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ActivityType;
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
}
