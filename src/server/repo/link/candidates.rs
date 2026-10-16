//! What a check for proposals (US-103) judges, read with its geometry: a
//! trip's or a collection item's line or position, and whatever lies within
//! an extent of it.

use sqlx::SqlitePool;

use super::super::trip::track_positions;
use crate::models::TripKind;
use crate::server::line_grid::Position;

/// A trip's or a collection track's line, by its id.
pub struct Line {
    pub id: i64,
    pub line: Vec<Position>,
}

/// A waypoint's position, by its id.
pub struct Place {
    pub id: i64,
    pub at: Position,
}

/// A rectangle of latitude and longitude.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent {
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_lat: f64,
    pub max_lon: f64,
}

impl Extent {
    /// The rectangle around `positions`; `None` around nothing.
    pub fn around(positions: impl IntoIterator<Item = Position>) -> Option<Self> {
        positions.into_iter().fold(None, |extent, (lat, lon)| {
            Some(match extent {
                None => Self {
                    min_lat: lat,
                    min_lon: lon,
                    max_lat: lat,
                    max_lon: lon,
                },
                Some(e) => Self {
                    min_lat: e.min_lat.min(lat),
                    min_lon: e.min_lon.min(lon),
                    max_lat: e.max_lat.max(lat),
                    max_lon: e.max_lon.max(lon),
                },
            })
        })
    }

    /// This rectangle with `metres` more on every side.
    pub fn widened(self, metres: f64) -> Self {
        const METRES_PER_DEGREE: f64 = 111_195.0;
        let lat = metres / METRES_PER_DEGREE;
        // A degree of longitude is shortest farthest from the equator.
        let widest = (self.min_lat - lat).abs().max((self.max_lat + lat).abs());
        let lon = metres / (METRES_PER_DEGREE * widest.to_radians().cos().max(0.01));
        Self {
            min_lat: self.min_lat - lat,
            min_lon: self.min_lon - lon,
            max_lat: self.max_lat + lat,
            max_lon: self.max_lon + lon,
        }
    }
}

/// A trip's kind and line; `None` if it does not exist.
pub async fn trip_line(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<(TripKind, Line)>, sqlx::Error> {
    let row: Option<(TripKind, Option<String>)> = sqlx::query_as(
        "SELECT t.trip_kind, json_extract(k.geojson, '$.geometry.coordinates') \
         FROM trip t JOIN track k ON k.trip_id = t.id WHERE t.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row.map(|(kind, positions)| Ok((kind, line(id, positions.as_deref())?)))
        .transpose()
}

/// The collection tracks and waypoints of `ids`, those that exist.
pub async fn collection_items(
    pool: &SqlitePool,
    track_ids: &[i64],
    waypoint_ids: &[i64],
) -> Result<(Vec<Line>, Vec<Place>), sqlx::Error> {
    let mut tracks = Vec::with_capacity(track_ids.len());
    for &id in track_ids {
        let positions: Option<Option<String>> = sqlx::query_scalar(
            "SELECT json_extract(geojson, '$.geometry.coordinates') FROM collection_track \
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        if let Some(positions) = positions {
            tracks.push(line(id, positions.as_deref())?);
        }
    }
    let mut waypoints = Vec::with_capacity(waypoint_ids.len());
    for &id in waypoint_ids {
        let at: Option<Position> = sqlx::query_as("SELECT lat, lon FROM waypoint WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        waypoints.extend(at.map(|at| Place { id, at }));
    }
    Ok((tracks, waypoints))
}

/// The ids of a collection's tracks and waypoints; `None` if it does not
/// exist.
pub async fn collection_item_ids(
    pool: &SqlitePool,
    collection_id: i64,
) -> Result<Option<(Vec<i64>, Vec<i64>)>, sqlx::Error> {
    if !super::exists(pool, "collection", collection_id).await? {
        return Ok(None);
    }
    let tracks = sqlx::query_scalar("SELECT id FROM collection_track WHERE collection_id = ?")
        .bind(collection_id)
        .fetch_all(pool)
        .await?;
    let waypoints = sqlx::query_scalar("SELECT id FROM waypoint WHERE collection_id = ?")
        .bind(collection_id)
        .fetch_all(pool)
        .await?;
    Ok(Some((tracks, waypoints)))
}

/// The lines of the trips of `kind` whose extent overlaps `extent`.
pub async fn trips_near(
    pool: &SqlitePool,
    kind: TripKind,
    extent: Extent,
) -> Result<Vec<Line>, sqlx::Error> {
    lines_near(
        pool,
        "SELECT t.id, json_extract(k.geojson, '$.geometry.coordinates') \
         FROM trip t JOIN track k ON k.trip_id = t.id \
         WHERE t.trip_kind = ? AND t.max_lat >= ? AND t.min_lat <= ? \
           AND t.max_lon >= ? AND t.min_lon <= ?",
        Some(kind),
        extent,
    )
    .await
}

/// The lines of the collection tracks whose extent overlaps `extent`.
pub async fn tracks_near(pool: &SqlitePool, extent: Extent) -> Result<Vec<Line>, sqlx::Error> {
    lines_near(
        pool,
        "SELECT id, json_extract(geojson, '$.geometry.coordinates') FROM collection_track \
         WHERE max_lat >= ? AND min_lat <= ? AND max_lon >= ? AND min_lon <= ?",
        None,
        extent,
    )
    .await
}

async fn lines_near(
    pool: &SqlitePool,
    sql: &str,
    kind: Option<TripKind>,
    extent: Extent,
) -> Result<Vec<Line>, sqlx::Error> {
    let mut query = sqlx::query_as::<_, (i64, Option<String>)>(sql);
    if let Some(kind) = kind {
        query = query.bind(kind);
    }
    query
        .bind(extent.min_lat)
        .bind(extent.max_lat)
        .bind(extent.min_lon)
        .bind(extent.max_lon)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(id, positions)| line(id, positions.as_deref()))
        .collect()
}

/// The waypoints within `extent`.
pub async fn waypoints_near(pool: &SqlitePool, extent: Extent) -> Result<Vec<Place>, sqlx::Error> {
    let rows: Vec<(i64, f64, f64)> = sqlx::query_as(
        "SELECT id, lat, lon FROM waypoint \
         WHERE lat BETWEEN ? AND ? AND lon BETWEEN ? AND ?",
    )
    .bind(extent.min_lat)
    .bind(extent.max_lat)
    .bind(extent.min_lon)
    .bind(extent.max_lon)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, lat, lon)| Place { id, at: (lat, lon) })
        .collect())
}

/// A line read from a track's coordinates; one without any — a track
/// stored without a line — has no points, and is never judged followed.
fn line(id: i64, positions: Option<&str>) -> Result<Line, sqlx::Error> {
    let Some(positions) = positions else {
        return Ok(Line {
            id,
            line: Vec::new(),
        });
    };
    let line = track_positions(id, positions)?
        .coordinates
        .into_iter()
        .map(|[lon, lat]| (lat, lon))
        .collect();
    Ok(Line { id, line })
}
