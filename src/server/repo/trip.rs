//! Trip and track CRUD (US-1, US-6, US-7, US-9, US-21). Photo association
//! rows live in the sibling `photo` module.

use sqlx::{sqlite::SqliteRow, Row, Sqlite, SqlitePool, Transaction};
use time::OffsetDateTime;

use crate::models::{ActivityType, TripDetail, TripSummary};
use crate::server::gpx::TrackStats;

use super::to_rfc3339;

/// Insert a new trip together with its derived geometry and the original GPX
/// file, in a single transaction (ADR-0003). Returns the new trip id.
pub async fn insert_trip(
    pool: &SqlitePool,
    name: &str,
    activity_type: ActivityType,
    tz_name: &str,
    stats: &TrackStats,
    geojson: &str,
    gpx: &[u8],
) -> Result<i64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let trip_id =
        insert_trip_in_tx(&mut tx, name, activity_type, tz_name, stats, geojson, gpx).await?;
    tx.commit().await?;
    Ok(trip_id)
}

/// Insert the trip + track rows on an existing transaction, without committing.
/// Import drives this directly so the trip, its track and its photos all land in
/// one transaction — a failed import leaves no partial trip (ADR-0004).
///
/// `tz_name` (US-4, ADR-0009/0019) is always a concrete IANA timezone at this
/// point — import always resolves either an explicit owner override or an
/// auto-guess from the track's start coordinate before calling this.
pub async fn insert_trip_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    name: &str,
    activity_type: ActivityType,
    tz_name: &str,
    stats: &TrackStats,
    geojson: &str,
    gpx: &[u8],
) -> Result<i64, sqlx::Error> {
    let created_at = to_rfc3339(OffsetDateTime::now_utc());
    let start_time = stats.start_time.map(to_rfc3339);
    let end_time = stats.end_time.map(to_rfc3339);

    let trip_id = sqlx::query(
        r#"INSERT INTO trip
               (name, activity_type, tz_name, start_time, end_time, duration_secs,
                distance_m, ascent_m, descent_m,
                min_lat, min_lon, max_lat, max_lon,
                created_at)
           VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)"#,
    )
    .bind(name)
    .bind(activity_type)
    .bind(tz_name)
    .bind(&start_time)
    .bind(&end_time)
    .bind(stats.duration_secs)
    .bind(stats.distance_m)
    .bind(stats.ascent_m)
    .bind(stats.descent_m)
    .bind(stats.min_lat)
    .bind(stats.min_lon)
    .bind(stats.max_lat)
    .bind(stats.max_lon)
    .bind(&created_at)
    .execute(&mut **tx)
    .await?
    .last_insert_rowid();

    sqlx::query("INSERT INTO track (trip_id, geojson, gpx) VALUES (?,?,?)")
        .bind(trip_id)
        .bind(geojson)
        .bind(gpx)
        .execute(&mut **tx)
        .await?;

    Ok(trip_id)
}

/// The original GPX file plus the trip name, for serving a download (US-21).
pub struct GpxDownload {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Fetch the original GPX of a trip together with its name (for the download
/// filename), or `None` if no such trip exists. One query via the 1:1 join.
pub async fn get_original_gpx(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<GpxDownload>, sqlx::Error> {
    sqlx::query(
        r#"SELECT t.name AS name, k.gpx AS gpx
           FROM trip t JOIN track k ON k.trip_id = t.id
           WHERE t.id = ?"#,
    )
    .bind(id)
    .map(|row: SqliteRow| GpxDownload {
        name: row.get("name"),
        bytes: row.get("gpx"),
    })
    .fetch_optional(pool)
    .await
}

/// List all trips as lightweight summaries, most recent first (US-6).
/// Reads only the `trip` table — never the track geometry — so it stays cheap.
/// `start_time` may be NULL (GPX without times); SQLite sorts NULLs last under DESC.
pub async fn list_trips(pool: &SqlitePool) -> Result<Vec<TripSummary>, sqlx::Error> {
    sqlx::query(
        r#"SELECT id, name, activity_type, start_time, distance_m, ascent_m, duration_secs
           FROM trip
           ORDER BY start_time DESC, id DESC"#,
    )
    .map(|row: SqliteRow| TripSummary {
        id: row.get("id"),
        name: row.get("name"),
        activity_type: row.get("activity_type"),
        start_time: row.get("start_time"),
        distance_m: row.get("distance_m"),
        ascent_m: row.get("ascent_m"),
        duration_secs: row.get("duration_secs"),
    })
    .fetch_all(pool)
    .await
}

/// Fetch a trip's track geometry as the stored GeoJSON string (US-7), or `None`
/// if the trip has no track. Reads only the `track` table; the blob is returned
/// verbatim so the map and elevation chart share a single fetch (ADR-0005/0006).
pub async fn get_track_geojson(pool: &SqlitePool, id: i64) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT geojson FROM track WHERE trip_id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Delete a trip by id (US-9). `track`/`photo` are declared `ON DELETE CASCADE`
/// (migrations 0001/0003) and `foreign_keys(true)` is enabled on every
/// connection (`db::create_pool`), so this one statement also removes the
/// trip's track row and all its photo rows via SQLite itself.
/// Returns `true` if a trip with this id existed and was deleted, `false` if
/// there was no such trip.
pub async fn delete_trip(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM trip WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Fetch full trip detail by id, or `None` if no such trip exists.
pub async fn get_trip(pool: &SqlitePool, id: i64) -> Result<Option<TripDetail>, sqlx::Error> {
    sqlx::query(
        r#"SELECT id, name, activity_type, tz_name, start_time, end_time,
                  distance_m, ascent_m, descent_m, duration_secs,
                  min_lat, min_lon, max_lat, max_lon
           FROM trip WHERE id = ?"#,
    )
    .bind(id)
    .map(row_to_detail)
    .fetch_optional(pool)
    .await
}

/// Persist a trip's timezone (US-4): used by the lazy backfill path when
/// photos are added to a trip imported before `tz_name` existed.
pub async fn set_trip_timezone(
    pool: &SqlitePool,
    id: i64,
    tz_name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE trip SET tz_name = ? WHERE id = ?")
        .bind(tz_name)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Persist a trip's name and/or activity type (US-15). Each field is
/// optional — `None` leaves that column untouched via `COALESCE`, so a
/// partial edit is a single atomic statement rather than a separate
/// read-then-write in the HTTP layer: there is no window between "check what
/// the trip currently has" and "write the merged result" for a concurrent
/// edit (or delete) of the same trip to race against. Returns `false` if no
/// trip with this id exists (nothing to update).
pub async fn update_trip(
    pool: &SqlitePool,
    id: i64,
    name: Option<&str>,
    activity_type: Option<ActivityType>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE trip SET name = COALESCE(?, name), activity_type = COALESCE(?, activity_type) WHERE id = ?",
    )
    .bind(name)
    .bind(activity_type)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

fn row_to_detail(row: SqliteRow) -> TripDetail {
    TripDetail {
        id: row.get("id"),
        name: row.get("name"),
        activity_type: row.get("activity_type"),
        tz_name: row.get("tz_name"),
        start_time: row.get("start_time"),
        end_time: row.get("end_time"),
        distance_m: row.get("distance_m"),
        ascent_m: row.get("ascent_m"),
        descent_m: row.get("descent_m"),
        duration_secs: row.get("duration_secs"),
        min_lat: row.get("min_lat"),
        min_lon: row.get("min_lon"),
        max_lat: row.get("max_lat"),
        max_lon: row.get("max_lon"),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
// Split into trip/tests.rs to keep this file under the repo's 500-line cap.

#[cfg(test)]
mod tests;
