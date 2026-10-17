//! What a tag summary lists without counting (US-107): the planned trips and
//! the recorded ones without timestamps under the chosen tags, the
//! collections carrying them and the collection tracks they reach on their
//! own. Collections and their tracks stay records of their own, read through
//! links of their own (ADR-0028); only the summary's view takes them in.
//! The owner's alone: a summary share reads [`super::list_tag_summaries`]
//! and nothing here.

use std::collections::HashSet;

use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use super::{local_date, stats_trip, COLUMNS};
use crate::models::{
    SummaryCollection, SummaryTrack, TagSummaries, TagTrips, TripKind, UncountedTrip,
};
use crate::server::repo::collection::TRACK_ACTIVITY;

/// The columns a [`SummaryTrack`] is read from, over
/// `collection_track k JOIN collection c`.
fn track_columns() -> String {
    format!(
        "k.id, k.collection_id, k.name, c.name AS collection_name, \
         {TRACK_ACTIVITY} AS activity_type, k.distance_m"
    )
}

/// Fill in `summary`'s uncounted trips, collections and collection tracks,
/// for the tags it already names (normalized).
pub async fn add_tag_holdings(
    pool: &SqlitePool,
    summary: &mut TagSummaries,
) -> Result<(), sqlx::Error> {
    if summary.tags.is_empty() {
        return Ok(());
    }
    let names: Vec<String> = summary.tags.iter().map(|tag| tag.name.clone()).collect();
    summary.uncounted = uncounted_trips(pool, &names, &mut summary.tags).await?;
    summary.collections = collections(pool, &names, &mut summary.tags).await?;
    summary.collection_tracks = lone_tracks(pool, &names, &mut summary.tags).await?;
    Ok(())
}

fn placeholders(names: &[String]) -> String {
    vec!["?"; names.len()].join(", ")
}

/// The chosen tag `row`'s `tag_name` column names.
fn tag_of<'a>(tags: &'a mut [TagTrips], row: &SqliteRow) -> Option<&'a mut TagTrips> {
    let name: String = row.get("tag_name");
    tags.iter_mut().find(|tag| tag.name == name)
}

/// Every trip under the tags the figures do not count: planned ones, and
/// recorded ones without a date to count them by.
async fn uncounted_trips(
    pool: &SqlitePool,
    names: &[String],
    tags: &mut [TagTrips],
) -> Result<Vec<UncountedTrip>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS}, trip.trip_kind, tag.name AS tag_name FROM trip \
         JOIN trip_tag ON trip_tag.trip_id = trip.id \
         JOIN tag ON tag.id = trip_tag.tag_id \
         WHERE tag.name IN ({}) \
         ORDER BY trip.start_time IS NULL, trip.start_time, trip.name COLLATE NOCASE, trip.id",
        placeholders(names)
    );
    let mut query = sqlx::query(&sql);
    for name in names {
        query = query.bind(name);
    }
    let rows = query.fetch_all(pool).await?;

    let mut trips = Vec::new();
    let mut seen = HashSet::new();
    for row in &rows {
        let kind: TripKind = row.get("trip_kind");
        if kind == TripKind::Recorded && stats_trip(row).is_some() {
            continue;
        }
        let Some(tag) = tag_of(tags, row) else {
            continue;
        };
        let id: i64 = row.get("id");
        tag.uncounted_trip_ids.push(id);
        if seen.insert(id) {
            let tz_name: Option<String> = row.get("tz_name");
            let start: Option<String> = row.get("start_time");
            trips.push(UncountedTrip {
                id,
                name: row.get("name"),
                activity_type: row.get("activity_type"),
                trip_kind: kind,
                start_date: local_date(start.as_deref(), tz_name.as_deref().unwrap_or("UTC")),
                distance_m: row.get("distance_m"),
            });
        }
    }
    Ok(trips)
}

/// The collections carrying the tags, by name, each with all its tracks.
async fn collections(
    pool: &SqlitePool,
    names: &[String],
    tags: &mut [TagTrips],
) -> Result<Vec<SummaryCollection>, sqlx::Error> {
    let sql = format!(
        "SELECT c.id, c.name, tag.name AS tag_name FROM collection c \
         JOIN collection_tag ct ON ct.collection_id = c.id \
         JOIN tag ON tag.id = ct.tag_id \
         WHERE tag.name IN ({}) ORDER BY c.name COLLATE NOCASE, c.id",
        placeholders(names)
    );
    let mut query = sqlx::query(&sql);
    for name in names {
        query = query.bind(name);
    }
    let rows = query.fetch_all(pool).await?;

    let mut collections: Vec<SummaryCollection> = Vec::new();
    for row in &rows {
        let Some(tag) = tag_of(tags, row) else {
            continue;
        };
        let id: i64 = row.get("id");
        tag.collection_ids.push(id);
        if !collections.iter().any(|collection| collection.id == id) {
            collections.push(SummaryCollection {
                id,
                name: row.get("name"),
                tracks: Vec::new(),
            });
        }
    }
    if collections.is_empty() {
        return Ok(collections);
    }

    let sql = format!(
        "SELECT {} FROM collection_track k JOIN collection c ON c.id = k.collection_id \
         WHERE k.collection_id IN ({}) ORDER BY k.position, k.id",
        track_columns(),
        vec!["?"; collections.len()].join(", ")
    );
    let mut query = sqlx::query(&sql);
    for collection in &collections {
        query = query.bind(collection.id);
    }
    for row in query.fetch_all(pool).await? {
        let collection_id: i64 = row.get("collection_id");
        if let Some(collection) = collections.iter_mut().find(|c| c.id == collection_id) {
            collection.tracks.push(summary_track(&row));
        }
    }
    Ok(collections)
}

/// The collection tracks carrying a tag their collection does not — the
/// ones US-93's Tags screen counts apart — by collection name, then in file
/// order.
async fn lone_tracks(
    pool: &SqlitePool,
    names: &[String],
    tags: &mut [TagTrips],
) -> Result<Vec<SummaryTrack>, sqlx::Error> {
    let sql = format!(
        "SELECT {}, tag.name AS tag_name FROM collection_track k \
         JOIN collection c ON c.id = k.collection_id \
         JOIN collection_track_tag kt ON kt.collection_track_id = k.id \
         JOIN tag ON tag.id = kt.tag_id \
         WHERE tag.name IN ({}) AND NOT EXISTS ( \
             SELECT 1 FROM collection_tag ct \
             WHERE ct.collection_id = k.collection_id AND ct.tag_id = tag.id) \
         ORDER BY c.name COLLATE NOCASE, c.id, k.position, k.id",
        track_columns(),
        placeholders(names)
    );
    let mut query = sqlx::query(&sql);
    for name in names {
        query = query.bind(name);
    }
    let rows = query.fetch_all(pool).await?;

    let mut tracks: Vec<SummaryTrack> = Vec::new();
    for row in &rows {
        let Some(tag) = tag_of(tags, row) else {
            continue;
        };
        let track = summary_track(row);
        tag.collection_track_ids.push(track.id);
        if !tracks.iter().any(|seen| seen.id == track.id) {
            tracks.push(track);
        }
    }
    Ok(tracks)
}

fn summary_track(row: &SqliteRow) -> SummaryTrack {
    SummaryTrack {
        id: row.get("id"),
        name: row.get("name"),
        collection_name: row.get("collection_name"),
        activity_type: row.get("activity_type"),
        distance_m: row.get("distance_m"),
    }
}
