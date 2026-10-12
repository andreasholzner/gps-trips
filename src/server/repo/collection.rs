//! Collections, their tracks and their waypoints (US-86) — records
//! of their own, apart from trips (ADR-0028). Deleting a collection deletes
//! everything it owns through the schema's cascades (migration 0024).

use sqlx::{sqlite::SqliteRow, Row, Sqlite, SqlitePool, Transaction};
use time::OffsetDateTime;

use crate::models::{
    ActivityType, CollectionDetail, CollectionTrackDetail, CollectionTrackSummary, TripTrack,
    Waypoint,
};
use crate::server::db;
use crate::server::gpx::TrackStats;

use super::{to_rfc3339, trip::track_positions, GpxDownload};

/// The activity a collection track goes by: its own, else its collection's,
/// else `unknown` — worked out where it is read, so changing the
/// collection's changes every track without one of its own.
pub(super) const TRACK_ACTIVITY: &str = "COALESCE(k.activity_type, c.activity_type, 'unknown')";

/// A refused write to a collection's details.
#[derive(Debug, thiserror::Error)]
pub enum CollectionError {
    /// No two collections share a name, whatever the case of A–Z (US-86,
    /// migration 0027's unique index): the QMapShack export gives each its
    /// own folder by name (US-89).
    #[error("A collection named \"{0}\" exists already")]
    NameTaken(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// A collection with everything it holds, ready to be stored in one go.
pub struct NewCollection<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub activity_type: Option<ActivityType>,
    /// The files as they were uploaded (US-88), in the order they came.
    pub files: &'a [NewCollectionFile<'a>],
    /// In file order; a track's position is its place here.
    pub tracks: &'a [NewCollectionTrack<'a>],
    pub waypoints: &'a [NewWaypoint<'a>],
}

pub struct NewCollectionTrack<'a> {
    pub name: &'a str,
    /// Only the distance, the climb and the bounding box are stored.
    pub stats: &'a TrackStats,
    pub geojson: &'a str,
    pub gpx: &'a [u8],
}

pub struct NewWaypoint<'a> {
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    /// The `<sym>`, as the file names it.
    pub symbol: Option<&'a str>,
    /// The index into [`NewCollection::tracks`] of the track it also
    /// belongs to, if any.
    pub track: Option<usize>,
}

/// Store a collection with its tracks and waypoints in one transaction, so
/// a failed import leaves nothing behind. Returns the new collection's id;
/// refused for a name another collection has.
pub async fn insert_collection(
    pool: &SqlitePool,
    collection: &NewCollection<'_>,
) -> Result<i64, CollectionError> {
    let mut tx = db::begin_write(pool).await?;
    let id = insert_collection_in_tx(&mut tx, collection).await?;
    tx.commit().await?;
    Ok(id)
}

/// [`insert_collection`] on the caller's transaction.
pub async fn insert_collection_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    collection: &NewCollection<'_>,
) -> Result<i64, CollectionError> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO collection (name, description, activity_type, created_at) VALUES (?,?,?,?) \
         ON CONFLICT DO NOTHING RETURNING id",
    )
    .bind(collection.name)
    .bind(collection.description)
    .bind(collection.activity_type)
    .bind(to_rfc3339(OffsetDateTime::now_utc()))
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| CollectionError::NameTaken(collection.name.to_string()))?;

    insert_files(tx, id, collection.files).await?;
    let mut track_ids = Vec::with_capacity(collection.tracks.len());
    for track in collection.tracks {
        track_ids.push(insert_track(tx, id, track).await?);
    }
    for waypoint in collection.waypoints {
        let track_id = waypoint
            .track
            .and_then(|track| track_ids.get(track).copied());
        insert_waypoint(tx, id, waypoint, track_id).await?;
    }
    Ok(id)
}

/// Add a track after the collection's last one; returns its id.
pub(super) async fn insert_track(
    tx: &mut Transaction<'_, Sqlite>,
    collection_id: i64,
    track: &NewCollectionTrack<'_>,
) -> Result<i64, sqlx::Error> {
    let id = sqlx::query(
        "INSERT INTO collection_track \
             (collection_id, position, name, distance_m, ascent_m, descent_m, \
              min_lat, min_lon, max_lat, max_lon, geojson, gpx) \
         VALUES (?1, (SELECT COALESCE(MAX(position), 0) + 1 FROM collection_track \
                      WHERE collection_id = ?1), \
                 ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )
    .bind(collection_id)
    .bind(track.name)
    .bind(track.stats.distance_m)
    .bind(track.stats.ascent_m)
    .bind(track.stats.descent_m)
    .bind(track.stats.min_lat)
    .bind(track.stats.min_lon)
    .bind(track.stats.max_lat)
    .bind(track.stats.max_lon)
    .bind(track.geojson)
    .bind(track.gpx)
    .execute(&mut **tx)
    .await?
    .last_insert_rowid();
    Ok(id)
}

/// Add a waypoint after the collection's last one.
pub(super) async fn insert_waypoint(
    tx: &mut Transaction<'_, Sqlite>,
    collection_id: i64,
    waypoint: &NewWaypoint<'_>,
    track_id: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO waypoint \
             (collection_id, collection_track_id, position, name, description, lat, lon, ele, \
              symbol) \
         VALUES (?1, ?2, (SELECT COALESCE(MAX(position), 0) + 1 FROM waypoint \
                          WHERE collection_id = ?1), \
                 ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(collection_id)
    .bind(track_id)
    .bind(waypoint.name)
    .bind(waypoint.description)
    .bind(waypoint.lat)
    .bind(waypoint.lon)
    .bind(waypoint.ele)
    .bind(waypoint.symbol)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// A collection with its tracks in file order, or `None` if there is none.
pub async fn get_collection(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<CollectionDetail>, sqlx::Error> {
    let Some(row) = sqlx::query(
        "SELECT c.id, c.name, c.description, c.activity_type, \
                (SELECT COUNT(*) FROM waypoint w WHERE w.collection_id = c.id) AS waypoint_count \
         FROM collection c WHERE c.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    let tracks = sqlx::query(&format!(
        "SELECT k.id, k.name, {TRACK_ACTIVITY} AS activity_type, \
                k.distance_m, k.ascent_m, k.descent_m \
         FROM collection_track k JOIN collection c ON c.id = k.collection_id \
         WHERE k.collection_id = ? ORDER BY k.position"
    ))
    .bind(id)
    .map(|row: SqliteRow| CollectionTrackSummary {
        id: row.get("id"),
        name: row.get("name"),
        activity_type: row.get("activity_type"),
        distance_m: row.get("distance_m"),
        ascent_m: row.get("ascent_m"),
        descent_m: row.get("descent_m"),
    })
    .fetch_all(pool)
    .await?;

    Ok(Some(CollectionDetail {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        activity_type: row.get("activity_type"),
        waypoint_count: row.get("waypoint_count"),
        tracks,
    }))
}

/// Whether there is a collection `id`, for the routes that read what it
/// holds rather than the collection itself.
pub async fn collection_exists(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM collection WHERE id = ?)")
        .bind(id)
        .fetch_one(pool)
        .await
}

/// A collection's name alone, or `None` if there is no such collection.
pub async fn collection_name(pool: &SqlitePool, id: i64) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT name FROM collection WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// The place of a collection's last track, 0 for one without tracks: an
/// added track goes after it.
pub async fn last_collection_track_position(
    pool: &SqlitePool,
    collection_id: i64,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE(MAX(position), 0) FROM collection_track WHERE collection_id = ?",
    )
    .bind(collection_id)
    .fetch_one(pool)
    .await
}

/// A change to a collection's details; `None` leaves a field as it is.
#[derive(Default)]
pub struct CollectionChange<'a> {
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
    /// `Some(None)` takes the activity away.
    pub activity_type: Option<Option<ActivityType>>,
}

/// Change a collection's details. `false` if there is no such collection;
/// refused, changing nothing, for a name another collection has.
pub async fn update_collection(
    pool: &SqlitePool,
    id: i64,
    change: &CollectionChange<'_>,
) -> Result<bool, CollectionError> {
    let mut tx = db::begin_write(pool).await?;
    let updated: Option<i64> = sqlx::query_scalar(
        "UPDATE OR IGNORE collection SET \
             name = COALESCE(?, name), \
             description = COALESCE(?, description), \
             activity_type = CASE WHEN ? THEN ? ELSE activity_type END \
         WHERE id = ? RETURNING id",
    )
    .bind(change.name)
    .bind(change.description)
    .bind(change.activity_type.is_some())
    .bind(change.activity_type.flatten())
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    if updated.is_none() {
        // Ignored for a taken name, or there is no such collection.
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM collection WHERE id = ?)")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        return match (exists, change.name) {
            (true, Some(name)) => Err(CollectionError::NameTaken(name.to_string())),
            _ => Ok(false),
        };
    }
    tx.commit().await?;
    Ok(true)
}

/// Delete a collection and everything it holds. `false` if there was none.
pub async fn delete_collection(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM collection WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// A collection's waypoints in file order.
pub async fn list_collection_waypoints(
    pool: &SqlitePool,
    collection_id: i64,
) -> Result<Vec<Waypoint>, sqlx::Error> {
    sqlx::query(
        "SELECT id, collection_track_id, name, description, lat, lon, ele, symbol FROM waypoint \
         WHERE collection_id = ? ORDER BY position",
    )
    .bind(collection_id)
    .map(waypoint)
    .fetch_all(pool)
    .await
}

/// The waypoints that belong to one collection track, in file order.
pub async fn list_collection_track_waypoints(
    pool: &SqlitePool,
    track_id: i64,
) -> Result<Vec<Waypoint>, sqlx::Error> {
    sqlx::query(
        "SELECT id, collection_track_id, name, description, lat, lon, ele, symbol FROM waypoint \
         WHERE collection_track_id = ? ORDER BY position",
    )
    .bind(track_id)
    .map(waypoint)
    .fetch_all(pool)
    .await
}

fn waypoint(row: SqliteRow) -> Waypoint {
    Waypoint {
        id: row.get("id"),
        collection_track_id: row.get("collection_track_id"),
        name: row.get("name"),
        description: row.get("description"),
        lat: row.get("lat"),
        lon: row.get("lon"),
        ele: row.get("ele"),
        symbol: row.get("symbol"),
    }
}

/// Every track of a collection as `[lon, lat]` positions, in file order —
/// for drawing them on one map.
pub async fn list_collection_track_positions(
    pool: &SqlitePool,
    collection_id: i64,
) -> Result<Vec<TripTrack>, sqlx::Error> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, json_extract(geojson, '$.geometry.coordinates') FROM collection_track \
         WHERE collection_id = ? ORDER BY position",
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(id, positions)| track_positions(id, &positions))
        .collect()
}

/// One collection track, or `None` if there is none.
pub async fn get_collection_track(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<CollectionTrackDetail>, sqlx::Error> {
    sqlx::query(&format!(
        "SELECT k.id, k.collection_id, c.name AS collection_name, k.name, \
                {TRACK_ACTIVITY} AS activity_type, k.activity_type AS own_activity, \
                c.activity_type AS collection_activity, k.distance_m, k.ascent_m, k.descent_m \
         FROM collection_track k JOIN collection c ON c.id = k.collection_id \
         WHERE k.id = ?"
    ))
    .bind(id)
    .map(|row: SqliteRow| CollectionTrackDetail {
        id: row.get("id"),
        collection_id: row.get("collection_id"),
        collection_name: row.get("collection_name"),
        name: row.get("name"),
        activity_type: row.get("activity_type"),
        own_activity: row.get("own_activity"),
        collection_activity: row.get("collection_activity"),
        distance_m: row.get("distance_m"),
        ascent_m: row.get("ascent_m"),
        descent_m: row.get("descent_m"),
    })
    .fetch_optional(pool)
    .await
}

/// A change to a collection track; `None` leaves a field as it is.
#[derive(Default)]
pub struct CollectionTrackChange<'a> {
    pub name: Option<&'a str>,
    /// `Some(None)` takes the collection's activity again.
    pub activity_type: Option<Option<ActivityType>>,
}

/// Change a collection track's name or own activity. `false` if there is no
/// such track.
pub async fn update_collection_track(
    pool: &SqlitePool,
    id: i64,
    change: &CollectionTrackChange<'_>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE collection_track SET \
             name = COALESCE(?, name), \
             activity_type = CASE WHEN ? THEN ? ELSE activity_type END \
         WHERE id = ?",
    )
    .bind(change.name)
    .bind(change.activity_type.is_some())
    .bind(change.activity_type.flatten())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Delete one collection track. Its waypoints stay with the collection.
/// `false` if there was none.
pub async fn delete_collection_track(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM collection_track WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// A collection track's geometry (ADR-0003).
pub async fn get_collection_track_geojson(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT geojson FROM collection_track WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// A collection track's own GPX, named after the track, for its download.
pub async fn get_collection_track_gpx(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<GpxDownload>, sqlx::Error> {
    sqlx::query("SELECT name, gpx FROM collection_track WHERE id = ?")
        .bind(id)
        .map(|row: SqliteRow| GpxDownload {
            name: row.get("name"),
            bytes: row.get("gpx"),
        })
        .fetch_optional(pool)
        .await
}

mod files;
mod import;
mod list;
use files::insert_files;
pub use files::{get_collection_file, list_collection_files, NewCollectionFile};
pub use import::{
    apply_collection_import, delete_collection_import, get_collection_import,
    insert_collection_import, load_held, sweep_collection_imports, take_collection_import_in_tx,
    CollectionImportPlan, PlannedTrack, PlannedWaypoint, StagedCollectionImport, TrackRef,
};
pub use list::{count_collections, list_collection_tracks_by_id, list_collections};

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
