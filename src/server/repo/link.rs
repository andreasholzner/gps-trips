//! US-99 — links from what the owner did to what they planned: a recorded
//! trip to a planned trip, and a recorded or planned trip to a collection
//! track or a waypoint. Links of their own beside the tags' and shares'
//! (ADR-0028); every one has a trip at one end.

use std::collections::BTreeSet;

use sqlx::{sqlite::SqliteRow, Row, Sqlite, SqlitePool};

use super::collection::TRACK_ACTIVITY;
use super::trip::local_start_date;
use crate::models::{LinkedTrack, LinkedTrip, LinkedWaypoint, Links, TripKind};
use crate::server::db;

/// Something a link can end at, other than the trip at its other end — or,
/// for [`list_links`], whatever a page lists the links of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkEnd {
    Trip(i64),
    CollectionTrack(i64),
    Waypoint(i64),
}

/// What linking a selection came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkOutcome {
    /// Something selected does not exist; nothing was linked.
    Missing,
    /// No two things selected can be linked.
    NothingToLink,
    /// Every pair that can be linked is, `pairs` of them, whether or not
    /// one was linked before.
    Linked { pairs: usize },
}

/// Link every pair the selection holds that can be linked: each recorded
/// trip to each planned trip and collection track, each planned trip to each
/// collection track. One transaction checks that everything exists and links
/// it, all or nothing; a pair already linked stays one link.
pub async fn link_selection(
    pool: &SqlitePool,
    trip_ids: &[i64],
    collection_track_ids: &[i64],
) -> Result<LinkOutcome, sqlx::Error> {
    let mut tx = db::begin_write(pool).await?;
    let (mut recorded, mut planned) = (BTreeSet::new(), BTreeSet::new());
    for &id in trip_ids {
        let kind: Option<TripKind> = sqlx::query_scalar("SELECT trip_kind FROM trip WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        match kind {
            Some(TripKind::Recorded) => recorded.insert(id),
            Some(TripKind::Planned) => planned.insert(id),
            None => return Ok(LinkOutcome::Missing),
        };
    }
    let tracks: BTreeSet<i64> = collection_track_ids.iter().copied().collect();
    for &id in &tracks {
        if !exists(&mut *tx, "collection_track", id).await? {
            return Ok(LinkOutcome::Missing);
        }
    }

    let pairs = recorded.len() * planned.len() + (recorded.len() + planned.len()) * tracks.len();
    if pairs == 0 {
        return Ok(LinkOutcome::NothingToLink);
    }
    for &r in &recorded {
        for &p in &planned {
            sqlx::query(
                "INSERT OR IGNORE INTO trip_trip_link (recorded_trip_id, planned_trip_id) \
                 VALUES (?, ?)",
            )
            .bind(r)
            .bind(p)
            .execute(&mut *tx)
            .await?;
        }
    }
    for &trip in recorded.iter().chain(&planned) {
        for &track in &tracks {
            sqlx::query(
                "INSERT OR IGNORE INTO trip_track_link (trip_id, collection_track_id) \
                 VALUES (?, ?)",
            )
            .bind(trip)
            .bind(track)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(LinkOutcome::Linked { pairs })
}

/// Link a trip to a waypoint it passed or was planned past. `false` if
/// either does not exist.
pub async fn link_waypoint(
    pool: &SqlitePool,
    trip_id: i64,
    waypoint_id: i64,
) -> Result<bool, sqlx::Error> {
    let mut tx = db::begin_write(pool).await?;
    if !exists(&mut *tx, "trip", trip_id).await?
        || !exists(&mut *tx, "waypoint", waypoint_id).await?
    {
        return Ok(false);
    }
    sqlx::query("INSERT OR IGNORE INTO trip_waypoint_link (trip_id, waypoint_id) VALUES (?, ?)")
        .bind(trip_id)
        .bind(waypoint_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

/// Remove the link between a trip and `other`, from whichever end. `true`
/// if there was one.
pub async fn unlink(pool: &SqlitePool, trip_id: i64, other: LinkEnd) -> Result<bool, sqlx::Error> {
    let query = match other {
        LinkEnd::Trip(other) => sqlx::query(
            "DELETE FROM trip_trip_link \
             WHERE (recorded_trip_id = ?1 AND planned_trip_id = ?2) \
                OR (recorded_trip_id = ?2 AND planned_trip_id = ?1)",
        )
        .bind(trip_id)
        .bind(other),
        LinkEnd::CollectionTrack(track) => {
            sqlx::query("DELETE FROM trip_track_link WHERE trip_id = ? AND collection_track_id = ?")
                .bind(trip_id)
                .bind(track)
        }
        LinkEnd::Waypoint(waypoint) => {
            sqlx::query("DELETE FROM trip_waypoint_link WHERE trip_id = ? AND waypoint_id = ?")
                .bind(trip_id)
                .bind(waypoint)
        }
    };
    Ok(query.execute(pool).await?.rows_affected() > 0)
}

/// What `end` is linked to, as its page lists it; `None` if it does not
/// exist.
pub async fn list_links(pool: &SqlitePool, end: LinkEnd) -> Result<Option<Links>, sqlx::Error> {
    let (table, id) = match end {
        LinkEnd::Trip(id) => ("trip", id),
        LinkEnd::CollectionTrack(id) => ("collection_track", id),
        LinkEnd::Waypoint(id) => ("waypoint", id),
    };
    if !exists(pool, table, id).await? {
        return Ok(None);
    }
    let links = match end {
        LinkEnd::Trip(id) => Links {
            recorded_trips: linked_trips(
                pool,
                "trip_trip_link l JOIN trip t ON t.id = l.recorded_trip_id \
                 WHERE l.planned_trip_id = ?",
                id,
                TripKind::Recorded,
            )
            .await?,
            planned_trips: linked_trips(
                pool,
                "trip_trip_link l JOIN trip t ON t.id = l.planned_trip_id \
                 WHERE l.recorded_trip_id = ?",
                id,
                TripKind::Planned,
            )
            .await?,
            collection_tracks: linked_tracks(pool, id).await?,
            waypoints: linked_waypoints(pool, id).await?,
        },
        LinkEnd::CollectionTrack(id) => {
            let from = "trip_track_link l JOIN trip t ON t.id = l.trip_id \
                        WHERE l.collection_track_id = ?";
            trips_by_kind(pool, from, id).await?
        }
        LinkEnd::Waypoint(id) => {
            let from = "trip_waypoint_link l JOIN trip t ON t.id = l.trip_id \
                        WHERE l.waypoint_id = ?";
            trips_by_kind(pool, from, id).await?
        }
    };
    Ok(Some(links))
}

/// The recorded and the planned trips `from` reaches, and nothing else.
async fn trips_by_kind(pool: &SqlitePool, from: &str, id: i64) -> Result<Links, sqlx::Error> {
    Ok(Links {
        recorded_trips: linked_trips(pool, from, id, TripKind::Recorded).await?,
        planned_trips: linked_trips(pool, from, id, TripKind::Planned).await?,
        ..Links::default()
    })
}

/// The trips of `kind` that `from` — a join reaching `trip t`, narrowed by
/// one bound id — reaches. Recorded trips come newest first, one without
/// timestamps last; planned ones by name.
async fn linked_trips(
    pool: &SqlitePool,
    from: &str,
    id: i64,
    kind: TripKind,
) -> Result<Vec<LinkedTrip>, sqlx::Error> {
    let mut trips: Vec<(LinkedTrip, Option<String>)> = sqlx::query(&format!(
        "SELECT t.id, t.name, t.activity_type, t.start_time, t.tz_name \
         FROM {from} AND t.trip_kind = ?"
    ))
    .bind(id)
    .bind(kind)
    .map(|row: SqliteRow| {
        let start_time: Option<String> = row.get("start_time");
        let tz_name: Option<String> = row.get("tz_name");
        let trip = LinkedTrip {
            id: row.get("id"),
            name: row.get("name"),
            activity_type: row.get("activity_type"),
            start_date: local_start_date(start_time.as_deref(), tz_name.as_deref()),
        };
        (trip, start_time)
    })
    .fetch_all(pool)
    .await?;
    match kind {
        TripKind::Recorded => trips.sort_by(|(a, a_start), (b, b_start)| {
            a_start
                .is_none()
                .cmp(&b_start.is_none())
                .then_with(|| b_start.cmp(a_start))
                .then_with(|| b.id.cmp(&a.id))
        }),
        TripKind::Planned => {
            trips.sort_by(|(a, _), (b, _)| by_name(&a.name, &b.name).then_with(|| a.id.cmp(&b.id)))
        }
    }
    Ok(trips.into_iter().map(|(trip, _)| trip).collect())
}

/// The collection tracks a trip is linked to, by name.
async fn linked_tracks(pool: &SqlitePool, trip_id: i64) -> Result<Vec<LinkedTrack>, sqlx::Error> {
    let mut tracks = sqlx::query(&format!(
        "SELECT k.id, k.name, {TRACK_ACTIVITY} AS activity_type, \
                c.id AS collection_id, c.name AS collection_name \
         FROM trip_track_link l \
         JOIN collection_track k ON k.id = l.collection_track_id \
         JOIN collection c ON c.id = k.collection_id \
         WHERE l.trip_id = ?"
    ))
    .bind(trip_id)
    .map(|row: SqliteRow| LinkedTrack {
        id: row.get("id"),
        name: row.get("name"),
        activity_type: row.get("activity_type"),
        collection_id: row.get("collection_id"),
        collection_name: row.get("collection_name"),
    })
    .fetch_all(pool)
    .await?;
    tracks.sort_by(|a, b| by_name(&a.name, &b.name).then_with(|| a.id.cmp(&b.id)));
    Ok(tracks)
}

/// The waypoints a trip is linked to, by name; one without a name last.
async fn linked_waypoints(
    pool: &SqlitePool,
    trip_id: i64,
) -> Result<Vec<LinkedWaypoint>, sqlx::Error> {
    let mut waypoints = sqlx::query(
        "SELECT w.id, w.name, w.symbol, c.id AS collection_id, c.name AS collection_name \
         FROM trip_waypoint_link l \
         JOIN waypoint w ON w.id = l.waypoint_id \
         JOIN collection c ON c.id = w.collection_id \
         WHERE l.trip_id = ?",
    )
    .bind(trip_id)
    .map(|row: SqliteRow| LinkedWaypoint {
        id: row.get("id"),
        name: row.get("name"),
        symbol: row.get("symbol"),
        collection_id: row.get("collection_id"),
        collection_name: row.get("collection_name"),
    })
    .fetch_all(pool)
    .await?;
    waypoints.sort_by(|a, b| match (&a.name, &b.name) {
        (Some(a_name), Some(b_name)) => by_name(a_name, b_name).then_with(|| a.id.cmp(&b.id)),
        (a_name, b_name) => b_name
            .is_some()
            .cmp(&a_name.is_some())
            .then_with(|| a.id.cmp(&b.id)),
    });
    Ok(waypoints)
}

/// Names in order whatever the case of their letters — Unicode's, unlike
/// SQLite's `NOCASE`.
fn by_name(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

/// Whether `table` holds a row with `id`.
async fn exists<'e, E>(executor: E, table: &str, id: i64) -> Result<bool, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    sqlx::query_scalar(&format!(
        "SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?)"
    ))
    .bind(id)
    .fetch_one(executor)
    .await
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
