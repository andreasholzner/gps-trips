//! Storing each trip's moving time (US-77), moving distance (US-80) and its
//! climbs' height and moving time (US-81), computed by
//! [`server::moving_time`](crate::server::moving_time) and
//! [`server::climbs`](crate::server::climbs). Kept apart from `trip.rs`,
//! which is near the repo's file-length cap.

use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::models::ActivityType;
use crate::server::{climbs, db, moving_time};

/// Store how trip `trip_id`, whose track is `geojson`, moved and climbed
/// under `activity`'s thresholds — at import, where both are at hand. A
/// track without times has neither, so its climbs' figures stay `NULL` too.
pub(super) async fn store_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    trip_id: i64,
    activity: ActivityType,
    geojson: &str,
) -> Result<(), sqlx::Error> {
    let moving = moving_time::of_track(geojson, activity);
    let climbed = moving.map(|_| climbs::totals(&climbs::of_track(geojson, activity)));
    sqlx::query(
        "UPDATE trip SET moving_secs = ?, moving_distance_m = ?, climb_gain_m = ?, \
         climb_secs = ? WHERE id = ?",
    )
    .bind(moving.map(|moving| moving.secs))
    .bind(moving.map(|moving| moving.distance_m))
    .bind(climbed.map(|(gain_m, _)| gain_m))
    .bind(climbed.map(|(_, secs)| secs))
    .bind(trip_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Work trip `trip_id`'s figures out again from its stored track and
/// current activity — after the activity has changed, since the thresholds
/// go with it. A trip without a track row is left as it is.
pub(super) async fn recompute_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    trip_id: i64,
) -> Result<(), sqlx::Error> {
    let row: Option<(ActivityType, String)> = sqlx::query_as(
        "SELECT t.activity_type, k.geojson FROM trip t JOIN track k ON k.trip_id = t.id \
         WHERE t.id = ?",
    )
    .bind(trip_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((activity, geojson)) = row {
        store_in_tx(tx, trip_id, activity, &geojson).await?;
    }
    Ok(())
}

/// Fill in the figures of every trip that has times but lacks any of them —
/// those imported before they existed — and return how many were filled.
/// One transaction per trip, so a long backfill never holds the write lock
/// for more than one track's worth of work. A trip without times has
/// nothing to work out and is not tried again on every start.
pub async fn backfill_moving_secs(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM trip WHERE start_time IS NOT NULL \
         AND (moving_secs IS NULL OR moving_distance_m IS NULL OR climb_gain_m IS NULL)",
    )
    .fetch_all(pool)
    .await?;
    let mut filled = 0;
    for id in ids {
        let mut tx = db::begin_write(pool).await?;
        recompute_in_tx(&mut tx, id).await?;
        tx.commit().await?;
        filled += 1;
    }
    Ok(filled)
}
