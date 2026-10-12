//! One waypoint of a collection (US-98): read for its own page, corrected
//! and deleted. Re-adding a file matches an edited waypoint as any other
//! (ADR-0029): nothing records that it was edited.

use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use crate::models::{WaypointDetail, WaypointTrack};

use super::{waypoint, TRACK_ACTIVITY};

/// One waypoint with its collection and the track it belongs to, or `None`
/// if there is none.
pub async fn get_waypoint(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<WaypointDetail>, sqlx::Error> {
    sqlx::query(&format!(
        "SELECT w.id, w.collection_track_id, w.name, w.description, w.lat, w.lon, w.ele, \
                w.symbol, w.collection_id, c.name AS collection_name, \
                k.name AS track_name, {TRACK_ACTIVITY} AS track_activity \
         FROM waypoint w \
         JOIN collection c ON c.id = w.collection_id \
         LEFT JOIN collection_track k ON k.id = w.collection_track_id \
         WHERE w.id = ?"
    ))
    .bind(id)
    .map(|row: SqliteRow| {
        let waypoint = waypoint(&row);
        let track = waypoint.collection_track_id.map(|id| WaypointTrack {
            id,
            name: row.get("track_name"),
            activity_type: row.get("track_activity"),
        });
        WaypointDetail {
            waypoint,
            collection_id: row.get("collection_id"),
            collection_name: row.get("collection_name"),
            track,
        }
    })
    .fetch_optional(pool)
    .await
}

/// A change to a waypoint; `None` leaves a field as it is, and `Some(None)`
/// takes an optional one away.
#[derive(Debug, Default)]
pub struct WaypointChange<'a> {
    pub name: Option<&'a str>,
    pub description: Option<Option<&'a str>>,
    pub symbol: Option<Option<&'a str>>,
    pub ele: Option<Option<f64>>,
    /// `(lat, lon)`.
    pub position: Option<(f64, f64)>,
}

/// Change a waypoint. `false` if there is no such waypoint.
pub async fn update_waypoint(
    pool: &SqlitePool,
    id: i64,
    change: &WaypointChange<'_>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE waypoint SET \
             name = COALESCE(?, name), \
             description = CASE WHEN ? THEN ? ELSE description END, \
             symbol = CASE WHEN ? THEN ? ELSE symbol END, \
             ele = CASE WHEN ? THEN ? ELSE ele END, \
             lat = COALESCE(?, lat), \
             lon = COALESCE(?, lon) \
         WHERE id = ?",
    )
    .bind(change.name)
    .bind(change.description.is_some())
    .bind(change.description.flatten())
    .bind(change.symbol.is_some())
    .bind(change.symbol.flatten())
    .bind(change.ele.is_some())
    .bind(change.ele.flatten())
    .bind(change.position.map(|(lat, _)| lat))
    .bind(change.position.map(|(_, lon)| lon))
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Delete one waypoint; its collection and tracks stay. `false` if there
/// was none.
pub async fn delete_waypoint(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM waypoint WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
#[path = "waypoint_tests.rs"]
mod tests;
