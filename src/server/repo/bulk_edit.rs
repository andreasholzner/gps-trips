//! Editing many trips at once from the list screen (US-63). Kept out of
//! `trip.rs`, which holds one trip at a time and is near the repo's
//! file-length cap.

use sqlx::SqlitePool;

use crate::models::ActivityType;

/// Set `activity_type` on every trip in `trip_ids` (US-63), in one
/// transaction, queueing a Komoot push for each linked one — the same
/// `edit_pending` flag a single trip's edit sets (US-22).
///
/// All or nothing: existence is read off each `UPDATE`'s own row count, so
/// a trip that does not exist rolls the whole transaction back and returns
/// `false`, leaving every other trip as it was rather than half-changed.
pub async fn set_activity_type(
    pool: &SqlitePool,
    trip_ids: &[i64],
    activity_type: ActivityType,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    for &id in trip_ids {
        let updated = sqlx::query("UPDATE trip SET activity_type = ? WHERE id = ?")
            .bind(activity_type)
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if updated == 0 {
            // Dropping the transaction uncommitted rolls it back.
            return Ok(false);
        }
        sqlx::query("UPDATE trip_komoot_link SET edit_pending = 1 WHERE trip_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(true)
}
