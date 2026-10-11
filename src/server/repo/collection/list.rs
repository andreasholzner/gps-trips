//! The collections the trip list's Collections tab lists (US-87), and the
//! tracks its map draws as lines.

use std::collections::HashMap;

use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use crate::models::{BoundingBox, CollectionSummary, CollectionTrackMark, TripTrack};

use super::super::trip::track_positions;
use super::TRACK_ACTIVITY;

/// Every collection matching `name_query` and `region`, alphabetically
/// whatever the case, each with its tracks in file order.
///
/// A collection is in `region` when one of its tracks' bounding boxes
/// overlaps it or one of its waypoints lies inside it — edges included, as
/// for trips (ADR-0011). The name search is applied in Rust, a full Unicode
/// case-fold, for the reason `list_trips` gives.
pub async fn list_collections(
    pool: &SqlitePool,
    name_query: Option<&str>,
    region: Option<BoundingBox>,
) -> Result<Vec<CollectionSummary>, sqlx::Error> {
    // The waypoints are summed up per collection before the join, so the
    // region's subqueries run once per collection, not once per waypoint.
    let mut query = sqlx::QueryBuilder::new(
        "SELECT c.id, c.name, c.activity_type, COALESCE(w.n, 0) AS waypoint_count, \
                w.west, w.south, w.east, w.north \
         FROM collection c LEFT JOIN ( \
             SELECT collection_id, COUNT(*) AS n, MIN(lon) AS west, MIN(lat) AS south, \
                    MAX(lon) AS east, MAX(lat) AS north \
             FROM waypoint GROUP BY collection_id \
         ) w ON w.collection_id = c.id",
    );
    if let Some(region) = region {
        query
            .push(
                " WHERE EXISTS (SELECT 1 FROM collection_track k \
                 WHERE k.collection_id = c.id AND k.min_lat <= ",
            )
            .push_bind(region.max_lat)
            .push(" AND k.max_lat >= ")
            .push_bind(region.min_lat)
            .push(" AND k.min_lon <= ")
            .push_bind(region.max_lon)
            .push(" AND k.max_lon >= ")
            .push_bind(region.min_lon)
            .push(
                ") OR EXISTS (SELECT 1 FROM waypoint v \
                 WHERE v.collection_id = c.id AND v.lat BETWEEN ",
            )
            .push_bind(region.min_lat)
            .push(" AND ")
            .push_bind(region.max_lat)
            .push(" AND v.lon BETWEEN ")
            .push_bind(region.min_lon)
            .push(" AND ")
            .push_bind(region.max_lon)
            .push(")");
    }
    let needle = name_query.map(str::to_lowercase);
    let mut collections: Vec<CollectionSummary> = query
        .build()
        .map(|row: SqliteRow| CollectionSummary {
            id: row.get("id"),
            name: row.get("name"),
            activity_type: row.get("activity_type"),
            waypoint_count: row.get("waypoint_count"),
            tracks: Vec::new(),
            waypoint_bounds: match (
                row.get("west"),
                row.get("south"),
                row.get("east"),
                row.get("north"),
            ) {
                (Some(west), Some(south), Some(east), Some(north)) => {
                    Some([west, south, east, north])
                }
                _ => None,
            },
        })
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter(|c| {
            needle
                .as_ref()
                .is_none_or(|needle| c.name.to_lowercase().contains(needle))
        })
        .collect();
    collections.sort_by_cached_key(|c| (c.name.to_lowercase(), c.id));

    let mut tracks = list_track_marks(pool, &collections).await?;
    for collection in &mut collections {
        collection.tracks = tracks.remove(&collection.id).unwrap_or_default();
    }
    Ok(collections)
}

/// How many collections there are, for the Collections tab's count line.
pub async fn count_collections(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM collection")
        .fetch_one(pool)
        .await
}

/// The tracks of `collections`, in file order, by collection.
async fn list_track_marks(
    pool: &SqlitePool,
    collections: &[CollectionSummary],
) -> Result<HashMap<i64, Vec<CollectionTrackMark>>, sqlx::Error> {
    let ids: Vec<i64> = collections.iter().map(|c| c.id).collect();
    let ids = serde_json::to_string(&ids).map_err(|err| sqlx::Error::Encode(Box::new(err)))?;
    let rows = sqlx::query(&format!(
        "SELECT k.collection_id, k.id, k.name, {TRACK_ACTIVITY} AS activity_type, \
                k.min_lat, k.min_lon, k.max_lat, k.max_lon \
         FROM collection_track k JOIN collection c ON c.id = k.collection_id \
         WHERE k.collection_id IN (SELECT value FROM json_each(?)) \
         ORDER BY k.collection_id, k.position"
    ))
    .bind(ids)
    .fetch_all(pool)
    .await?;
    let mut by_collection: HashMap<i64, Vec<CollectionTrackMark>> = HashMap::new();
    for row in rows {
        by_collection
            .entry(row.get("collection_id"))
            .or_default()
            .push(CollectionTrackMark {
                id: row.get("id"),
                name: row.get("name"),
                activity_type: row.get("activity_type"),
                min_lat: row.get("min_lat"),
                min_lon: row.get("min_lon"),
                max_lat: row.get("max_lat"),
                max_lon: row.get("max_lon"),
            });
    }
    Ok(by_collection)
}

/// The `[lon, lat]` positions of each collection track of `ids` there is,
/// in the order asked for.
pub async fn list_collection_tracks_by_id(
    pool: &SqlitePool,
    ids: &[i64],
) -> Result<Vec<TripTrack>, sqlx::Error> {
    let ids = serde_json::to_string(ids).map_err(|err| sqlx::Error::Encode(Box::new(err)))?;
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT k.id, json_extract(k.geojson, '$.geometry.coordinates') \
         FROM json_each(?) AS asked JOIN collection_track k ON k.id = asked.value \
         ORDER BY asked.key",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(id, positions)| track_positions(id, &positions))
        .collect()
}
