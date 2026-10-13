//! US-94 — what a share of collections or collection tracks reaches. Which
//! tracks that is comes from the `share_track_reach` view alone, so every
//! check here agrees on it; a waypoint is reached with its collection, or
//! with a named track it belongs to. Every read is scoped by the share's id.

use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use crate::models::{
    SharedCollection, SharedCollectionName, SharedCollectionTrack, SharedCollectionTrackDetail,
    SharedWaypoint,
};
use crate::server::repo::collection::TRACK_ACTIVITY;

/// Whether `share_id` names collection `collection_id`.
pub async fn share_covers_collection(
    pool: &SqlitePool,
    share_id: i64,
    collection_id: i64,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM share_collection WHERE share_id = ? AND collection_id = ?)",
    )
    .bind(share_id)
    .bind(collection_id)
    .fetch_one(pool)
    .await
}

/// Whether `share_id` reaches collection track `track_id`.
pub async fn share_covers_collection_track(
    pool: &SqlitePool,
    share_id: i64,
    track_id: i64,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM share_track_reach \
         WHERE share_id = ? AND collection_track_id = ?)",
    )
    .bind(share_id)
    .bind(track_id)
    .fetch_one(pool)
    .await
}

/// The collections `share_id` names, alphabetically, as the Collections
/// page lists them.
pub async fn list_shared_collections(
    pool: &SqlitePool,
    share_id: i64,
) -> Result<Vec<SharedCollection>, sqlx::Error> {
    sqlx::query(
        "SELECT c.id, c.name, c.description, c.activity_type, \
                (SELECT COUNT(*) FROM collection_track k WHERE k.collection_id = c.id) \
                    AS track_count, \
                (SELECT COUNT(*) FROM waypoint w WHERE w.collection_id = c.id) AS waypoint_count \
         FROM collection c JOIN share_collection sc ON sc.collection_id = c.id \
         WHERE sc.share_id = ? ORDER BY c.name COLLATE NOCASE, c.id",
    )
    .bind(share_id)
    .map(|row: SqliteRow| SharedCollection {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        activity_type: row.get("activity_type"),
        track_count: row.get("track_count"),
        waypoint_count: row.get("waypoint_count"),
    })
    .fetch_all(pool)
    .await
}

/// The columns a [`SharedCollectionTrack`] is read from, over
/// `collection_track k JOIN collection c`.
fn track_columns() -> String {
    format!(
        "k.id, k.name, {TRACK_ACTIVITY} AS activity_type, k.distance_m, k.ascent_m, k.descent_m"
    )
}

fn shared_track(row: &SqliteRow) -> SharedCollectionTrack {
    SharedCollectionTrack {
        id: row.get("id"),
        name: row.get("name"),
        activity_type: row.get("activity_type"),
        distance_m: row.get("distance_m"),
        ascent_m: row.get("ascent_m"),
        descent_m: row.get("descent_m"),
    }
}

/// Every collection track `share_id` reaches, in its collections' order
/// (alphabetical), then the file's.
pub async fn list_shared_collection_tracks(
    pool: &SqlitePool,
    share_id: i64,
) -> Result<Vec<SharedCollectionTrack>, sqlx::Error> {
    sqlx::query(&format!(
        "SELECT {} FROM collection_track k \
         JOIN collection c ON c.id = k.collection_id \
         JOIN share_track_reach r ON r.collection_track_id = k.id \
         WHERE r.share_id = ? ORDER BY c.name COLLATE NOCASE, c.id, k.position",
        track_columns()
    ))
    .bind(share_id)
    .map(|row: SqliteRow| shared_track(&row))
    .fetch_all(pool)
    .await
}

/// Collection track `track_id` as `share_id` shows it — naming its
/// collection only when the share names that too — or `None` if the share
/// does not reach it.
pub async fn shared_collection_track(
    pool: &SqlitePool,
    share_id: i64,
    track_id: i64,
) -> Result<Option<SharedCollectionTrackDetail>, sqlx::Error> {
    sqlx::query(&format!(
        "SELECT {}, sc.collection_id AS shared_collection_id, \
                c.name AS collection_name \
         FROM collection_track k \
         JOIN collection c ON c.id = k.collection_id \
         JOIN share_track_reach r ON r.collection_track_id = k.id AND r.share_id = ? \
         LEFT JOIN share_collection sc \
             ON sc.collection_id = k.collection_id AND sc.share_id = r.share_id \
         WHERE k.id = ?",
        track_columns()
    ))
    .bind(share_id)
    .bind(track_id)
    .map(|row: SqliteRow| {
        let shared: Option<i64> = row.get("shared_collection_id");
        SharedCollectionTrackDetail {
            track: shared_track(&row),
            collection: shared.map(|id| SharedCollectionName {
                id,
                name: row.get("collection_name"),
            }),
        }
    })
    .fetch_optional(pool)
    .await
}

const WAYPOINT_COLUMNS: &str = "w.id, w.name, w.description, w.lat, w.lon, w.ele, w.symbol";

fn shared_waypoint(row: &SqliteRow) -> SharedWaypoint {
    SharedWaypoint {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        lat: row.get("lat"),
        lon: row.get("lon"),
        ele: row.get("ele"),
        symbol: row.get("symbol"),
    }
}

/// Every waypoint `share_id` reaches: all of the collections it names, and
/// those belonging to the tracks it names, in their collections' order,
/// then the file's.
pub async fn list_shared_waypoints(
    pool: &SqlitePool,
    share_id: i64,
) -> Result<Vec<SharedWaypoint>, sqlx::Error> {
    sqlx::query(&format!(
        "SELECT {WAYPOINT_COLUMNS} FROM waypoint w JOIN collection c ON c.id = w.collection_id \
         WHERE w.collection_id IN \
                   (SELECT collection_id FROM share_collection WHERE share_id = ?1) \
            OR w.collection_track_id IN \
                   (SELECT collection_track_id FROM share_collection_track WHERE share_id = ?1) \
         ORDER BY c.name COLLATE NOCASE, c.id, w.position"
    ))
    .bind(share_id)
    .map(|row: SqliteRow| shared_waypoint(&row))
    .fetch_all(pool)
    .await
}

/// The waypoints of collection `collection_id`, in file order, if
/// `share_id` names it; empty otherwise.
pub async fn list_shared_collection_waypoints(
    pool: &SqlitePool,
    share_id: i64,
    collection_id: i64,
) -> Result<Vec<SharedWaypoint>, sqlx::Error> {
    sqlx::query(&format!(
        "SELECT {WAYPOINT_COLUMNS} FROM waypoint w \
         JOIN share_collection sc ON sc.collection_id = w.collection_id AND sc.share_id = ? \
         WHERE w.collection_id = ? ORDER BY w.position"
    ))
    .bind(share_id)
    .bind(collection_id)
    .map(|row: SqliteRow| shared_waypoint(&row))
    .fetch_all(pool)
    .await
}

/// The waypoints belonging to track `track_id`, in file order, if
/// `share_id` reaches it; empty otherwise.
pub async fn list_shared_track_waypoints(
    pool: &SqlitePool,
    share_id: i64,
    track_id: i64,
) -> Result<Vec<SharedWaypoint>, sqlx::Error> {
    sqlx::query(&format!(
        "SELECT {WAYPOINT_COLUMNS} FROM waypoint w \
         JOIN share_track_reach r \
             ON r.collection_track_id = w.collection_track_id AND r.share_id = ? \
         WHERE w.collection_track_id = ? ORDER BY w.position"
    ))
    .bind(share_id)
    .bind(track_id)
    .map(|row: SqliteRow| shared_waypoint(&row))
    .fetch_all(pool)
    .await
}
