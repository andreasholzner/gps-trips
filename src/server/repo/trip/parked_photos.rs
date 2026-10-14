//! Photos Komoot parked at their trip's start or end (US-101): where Komoot
//! puts a photo it could not place, so it still needs placing although it has
//! a position. Only a position Komoot provided counts — EXIF, timestamp and
//! hand placements are meant, wherever they lie.

use geo::HaversineDistance;
use sqlx::SqlitePool;

use crate::config::unplaced_photos::PARKED_REACH_M;
use crate::models::LocationSource;

/// A Komoot-provided photo's trip, `lat` and `lon`, and its track's first and
/// last positions as GeoJSON.
type ProvidedPhoto = (i64, f64, f64, Option<String>, Option<String>);

/// The ids of the trips holding a photo at Komoot's provided position within
/// [`PARKED_REACH_M`] of their track's first or last point. A trip without a
/// track has neither, so it is left out.
///
/// SQLite picks only the two endpoints out of each blob (`json_extract`); the
/// distance is measured here, as the bundled SQLite has no trigonometry.
pub(super) async fn trips_with_parked_photos(pool: &SqlitePool) -> Result<Vec<i64>, sqlx::Error> {
    let rows: Vec<ProvidedPhoto> = sqlx::query_as(
        "SELECT p.trip_id, p.lat, p.lon, \
           json_extract(tr.geojson, '$.geometry.coordinates[0]'), \
           json_extract(tr.geojson, '$.geometry.coordinates[#-1]') \
         FROM photo p JOIN track tr ON tr.trip_id = p.trip_id \
         WHERE p.location_source = ? AND p.lat IS NOT NULL AND p.lon IS NOT NULL",
    )
    .bind(LocationSource::Provided)
    .fetch_all(pool)
    .await?;

    let mut ids: Vec<i64> = rows
        .into_iter()
        .filter(|(_, lat, lon, first, last)| {
            let photo = geo::Point::new(*lon, *lat);
            [first, last]
                .into_iter()
                .filter_map(|end| point(end.as_deref()?))
                .any(|end| photo.haversine_distance(&end) <= PARKED_REACH_M)
        })
        .map(|(trip_id, ..)| trip_id)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

/// A GeoJSON position `[lon, lat, ..]`, or `None` for anything else.
fn point(position: &str) -> Option<geo::Point> {
    match serde_json::from_str::<Vec<f64>>(position).ok()?[..] {
        [lon, lat, ..] => Some(geo::Point::new(lon, lat)),
        _ => None,
    }
}
