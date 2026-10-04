//! What the statistics screen (US-77) and the tag summary (US-78) add up.

use std::collections::HashSet;

use sqlx::{sqlite::SqliteRow, Row, SqlitePool};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::models::{StatsTrip, TagTrips, TripKind};
use crate::server::import::date_prefix;

/// The `trip` columns a [`StatsTrip`] is read from.
const COLUMNS: &str = "trip.id, trip.name, trip.activity_type, trip.start_time, trip.end_time, \
    trip.tz_name, trip.distance_m, trip.ascent_m, trip.descent_m, trip.moving_secs, \
    trip.moving_distance_m";

/// Every recorded trip with times, reduced to [`StatsTrip`], oldest first,
/// and how many recorded trips have no times to count them by. Reads the
/// `trip` table only — never a track.
pub async fn list_stats_trips(pool: &SqlitePool) -> Result<(Vec<StatsTrip>, u32), sqlx::Error> {
    let rows = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM trip WHERE trip_kind = ? ORDER BY start_time, id"
    ))
    .bind(TripKind::Recorded)
    .fetch_all(pool)
    .await?;

    let mut trips = Vec::with_capacity(rows.len());
    let mut undated = 0;
    for row in &rows {
        match stats_trip(row) {
            Some(trip) => trips.push(trip),
            None => undated += 1,
        }
    }
    Ok((trips, undated))
}

/// The recorded trips under any of `tags` (already normalized), as
/// [`list_stats_trips`] reduces them: one [`TagTrips`] per tag in the order
/// given, and every dated trip once, oldest first, however many of the tags
/// it is under (US-78).
pub async fn list_tag_summaries(
    pool: &SqlitePool,
    tags: &[String],
) -> Result<(Vec<TagTrips>, Vec<StatsTrip>), sqlx::Error> {
    let mut summaries: Vec<TagTrips> = tags
        .iter()
        .map(|name| TagTrips {
            name: name.clone(),
            trip_ids: Vec::new(),
            undated: 0,
        })
        .collect();
    if tags.is_empty() {
        return Ok((summaries, Vec::new()));
    }

    let placeholders = vec!["?"; tags.len()].join(", ");
    let sql = format!(
        "SELECT {COLUMNS}, tag.name AS tag_name FROM trip \
         JOIN trip_tag ON trip_tag.trip_id = trip.id \
         JOIN tag ON tag.id = trip_tag.tag_id \
         WHERE trip.trip_kind = ? AND tag.name IN ({placeholders}) \
         ORDER BY trip.start_time, trip.id"
    );
    let mut query = sqlx::query(&sql).bind(TripKind::Recorded);
    for tag in tags {
        query = query.bind(tag);
    }
    let rows = query.fetch_all(pool).await?;

    let mut trips = Vec::new();
    let mut seen = HashSet::new();
    for row in &rows {
        let tag_name: String = row.get("tag_name");
        let Some(summary) = summaries.iter_mut().find(|tag| tag.name == tag_name) else {
            continue;
        };
        let Some(trip) = stats_trip(row) else {
            summary.undated += 1;
            continue;
        };
        summary.trip_ids.push(trip.id);
        if seen.insert(trip.id) {
            trips.push(trip);
        }
    }
    Ok((summaries, trips))
}

/// A row of [`COLUMNS`] as a [`StatsTrip`]; `None` for a trip without a
/// start time, which has no date to count it by.
fn stats_trip(row: &SqliteRow) -> Option<StatsTrip> {
    let tz_name: Option<String> = row.get("tz_name");
    let tz_name = tz_name.as_deref().unwrap_or("UTC");
    let start: Option<String> = row.get("start_time");
    let end: Option<String> = row.get("end_time");
    let start_date = local_date(start.as_deref(), tz_name)?;
    // A track's last point can lack the time its first had; it then ended,
    // as far as anyone can tell, the day it started.
    let end_date = local_date(end.as_deref(), tz_name).unwrap_or_else(|| start_date.clone());
    Some(StatsTrip {
        id: row.get("id"),
        name: row.get("name"),
        activity_type: row.get("activity_type"),
        start_date,
        end_date,
        distance_m: row.get("distance_m"),
        ascent_m: row.get("ascent_m"),
        descent_m: row.get("descent_m"),
        moving_secs: row.get("moving_secs"),
        moving_distance_m: row.get("moving_distance_m"),
    })
}

/// A stored timestamp's date in `tz_name`, the way every screen dates a
/// trip (US-62).
fn local_date(timestamp: Option<&str>, tz_name: &str) -> Option<String> {
    date_prefix(
        timestamp.and_then(|t| OffsetDateTime::parse(t, &Rfc3339).ok()),
        tz_name,
    )
}
