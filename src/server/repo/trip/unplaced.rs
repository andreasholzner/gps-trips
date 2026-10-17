//! The trips with photos still to place (US-66, US-101): a photo left
//! unplaced, or one Komoot parked at the trip's start or end — where Komoot
//! puts a photo it could not place, so it still needs placing although it
//! has a position. Only a position Komoot provided counts as parked: EXIF,
//! timestamp and hand placements are meant, wherever they lie. The trip's
//! page marks those same photos (US-106).

use std::collections::HashSet;

use geo::HaversineDistance;
use sqlx::{QueryBuilder, Sqlite, SqliteConnection, SqlitePool};

use crate::config::unplaced_photos::PARKED_REACH_M;
use crate::models::LocationSource;

/// A Komoot-provided photo's id, trip, `lat` and `lon`, and its track's
/// first and last positions as JSON text.
type ProvidedPhoto = (i64, i64, f64, f64, Option<String>, Option<String>);

/// Narrow the list query to the trips that may have a photo to place: one
/// unplaced or provided. `EXISTS` rather than a join, so a trip with several
/// is still listed once; served by `idx_photo_trip`. Which of them really
/// have one is up to [`trips_with_photos_to_place`].
pub(super) fn push_candidates(query: &mut QueryBuilder<'_, Sqlite>) {
    query
        .push(
            " AND EXISTS (SELECT 1 FROM photo p WHERE p.trip_id = t.id \
             AND p.location_source IN (",
        )
        .push_bind(LocationSource::None)
        .push(", ")
        .push_bind(LocationSource::Provided)
        .push("))");
}

/// Which of the trips `ids` hold a photo still to place (see
/// [`photos_in`]).
pub(super) async fn trips_with_photos_to_place(
    conn: &mut SqliteConnection,
    ids: &[i64],
) -> Result<HashSet<i64>, sqlx::Error> {
    Ok(photos_in(conn, ids)
        .await?
        .into_iter()
        .map(|(_, trip_id)| trip_id)
        .collect())
}

/// Which of trip `trip_id`'s photos are still to place (US-106) — by the
/// same rule as the filter, so a trip it lists always has one to mark.
pub async fn photos_to_place(pool: &SqlitePool, trip_id: i64) -> Result<HashSet<i64>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    Ok(photos_in(&mut conn, &[trip_id])
        .await?
        .into_iter()
        .map(|(photo_id, _)| photo_id)
        .collect())
}

/// The photos of the trips `ids` still to place, as `(photo, trip)`: each
/// left unplaced, or at Komoot's provided position within
/// [`PARKED_REACH_M`] of its track's first or last point. A trip without a
/// track has no such position, nor has a track whose first or last entry is
/// not a position.
///
/// SQLite picks only the two endpoints out of each track, once per trip; the
/// distance is measured here, as the bundled SQLite has no trigonometry.
async fn photos_in(
    conn: &mut SqliteConnection,
    ids: &[i64],
) -> Result<Vec<(i64, i64)>, sqlx::Error> {
    let ids = serde_json::to_string(ids).map_err(|err| sqlx::Error::Encode(Box::new(err)))?;
    let unplaced: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT id, trip_id FROM photo WHERE location_source = ? \
         AND trip_id IN (SELECT value FROM json_each(?))",
    )
    .bind(LocationSource::None)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    // `->` rather than `json_extract`: it always gives JSON text, so an entry
    // that is a number rather than a position still decodes, and is skipped.
    let provided: Vec<ProvidedPhoto> = sqlx::query_as(
        "WITH ends AS MATERIALIZED ( \
           SELECT tr.trip_id, \
             tr.geojson -> '$.geometry.coordinates[0]' AS first, \
             tr.geojson -> '$.geometry.coordinates[#-1]' AS last \
           FROM track tr WHERE tr.trip_id IN (SELECT value FROM json_each(?)) \
             AND EXISTS (SELECT 1 FROM photo p \
               WHERE p.trip_id = tr.trip_id AND p.location_source = ?)) \
         SELECT p.id, p.trip_id, p.lat, p.lon, e.first, e.last \
         FROM photo p JOIN ends e ON e.trip_id = p.trip_id \
         WHERE p.location_source = ? AND p.lat IS NOT NULL AND p.lon IS NOT NULL",
    )
    .bind(&ids)
    .bind(LocationSource::Provided)
    .bind(LocationSource::Provided)
    .fetch_all(&mut *conn)
    .await?;

    let parked = provided
        .into_iter()
        .filter(|(_, _, lat, lon, first, last)| {
            let photo = geo::Point::new(*lon, *lat);
            [first, last]
                .into_iter()
                .filter_map(|end| point(end.as_deref()?))
                .any(|end| photo.haversine_distance(&end) <= PARKED_REACH_M)
        })
        .map(|(photo_id, trip_id, ..)| (photo_id, trip_id));
    Ok(unplaced.into_iter().chain(parked).collect())
}

/// A GeoJSON position `[lon, lat, ..]`, or `None` for anything else.
fn point(position: &str) -> Option<geo::Point> {
    match serde_json::from_str::<Vec<f64>>(position).ok()?[..] {
        [lon, lat, ..] => Some(geo::Point::new(lon, lat)),
        _ => None,
    }
}
