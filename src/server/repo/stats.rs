//! What the statistics screen adds up (US-77).

use sqlx::{Row, SqlitePool};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::models::{StatsTrip, TripKind};
use crate::server::import::date_prefix;

/// Every recorded trip with times, reduced to [`StatsTrip`], oldest first,
/// and how many recorded trips have no times to count them by. Reads the
/// `trip` table only — never a track.
pub async fn list_stats_trips(pool: &SqlitePool) -> Result<(Vec<StatsTrip>, u32), sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, name, activity_type, start_time, end_time, tz_name, distance_m, ascent_m, \
         moving_secs FROM trip WHERE trip_kind = ? ORDER BY start_time, id",
    )
    .bind(TripKind::Recorded)
    .fetch_all(pool)
    .await?;

    let mut trips = Vec::with_capacity(rows.len());
    let mut undated = 0;
    for row in rows {
        let tz_name: Option<String> = row.get("tz_name");
        let tz_name = tz_name.as_deref().unwrap_or("UTC");
        let start: Option<String> = row.get("start_time");
        let end: Option<String> = row.get("end_time");
        let Some(start_date) = local_date(start.as_deref(), tz_name) else {
            undated += 1;
            continue;
        };
        // A track's last point can lack the time its first had; it then
        // ended, as far as anyone can tell, the day it started.
        let end_date = local_date(end.as_deref(), tz_name).unwrap_or_else(|| start_date.clone());
        trips.push(StatsTrip {
            id: row.get("id"),
            name: row.get("name"),
            activity_type: row.get("activity_type"),
            start_date,
            end_date,
            distance_m: row.get("distance_m"),
            ascent_m: row.get("ascent_m"),
            moving_secs: row.get("moving_secs"),
        });
    }
    Ok((trips, undated))
}

/// A stored timestamp's date in `tz_name`, the way every screen dates a
/// trip (US-62).
fn local_date(timestamp: Option<&str>, tz_name: &str) -> Option<String> {
    date_prefix(
        timestamp.and_then(|t| OffsetDateTime::parse(t, &Rfc3339).ok()),
        tz_name,
    )
}
