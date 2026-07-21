//! Trip and track CRUD (US-1, US-6, US-7, US-9, US-21). Photo association
//! rows live in the sibling `photo` module.

use sqlx::{sqlite::SqliteRow, Row, Sqlite, SqlitePool, Transaction};
use time::OffsetDateTime;

use crate::models::{ActivityType, TripDetail, TripKind, TripSummary};
use crate::server::gpx::TrackStats;

use super::to_rfc3339;

/// Fields for a new trip + its derived geometry and original GPX file
/// (mirrors `NewPhoto` in the sibling `photo` module). `trip_kind`
/// (US-31/US-32): manual GPX import lets the owner choose either variant;
/// Komoot sync/backfill (US-29) passes whichever kind the source tour was
/// listed under (`Recorded` or `Planned`).
pub struct NewTrip<'a> {
    pub name: &'a str,
    pub activity_type: ActivityType,
    /// (US-4, ADR-0009/0019) always a concrete IANA timezone by the time this
    /// reaches the repo layer — the caller resolves either an explicit owner
    /// override or an auto-guess from the track's start coordinate first.
    pub tz_name: &'a str,
    pub stats: &'a TrackStats,
    pub geojson: &'a str,
    pub gpx: &'a [u8],
    pub trip_kind: TripKind,
}

/// Insert a new trip together with its derived geometry and the original GPX
/// file, in a single transaction (ADR-0003). Returns the new trip id.
pub async fn insert_trip(pool: &SqlitePool, trip: &NewTrip<'_>) -> Result<i64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let trip_id = insert_trip_in_tx(&mut tx, trip).await?;
    tx.commit().await?;
    Ok(trip_id)
}

/// Insert the trip + track rows on an existing transaction, without committing.
/// Import drives this directly so the trip, its track and its photos all land in
/// one transaction — a failed import leaves no partial trip (ADR-0004).
pub async fn insert_trip_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    trip: &NewTrip<'_>,
) -> Result<i64, sqlx::Error> {
    let created_at = to_rfc3339(OffsetDateTime::now_utc());
    let start_time = trip.stats.start_time.map(to_rfc3339);
    let end_time = trip.stats.end_time.map(to_rfc3339);

    let trip_id = sqlx::query(
        r#"INSERT INTO trip
               (name, activity_type, tz_name, start_time, end_time, duration_secs,
                distance_m, ascent_m, descent_m,
                min_lat, min_lon, max_lat, max_lon,
                created_at, trip_kind)
           VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"#,
    )
    .bind(trip.name)
    .bind(trip.activity_type)
    .bind(trip.tz_name)
    .bind(&start_time)
    .bind(&end_time)
    .bind(trip.stats.duration_secs)
    .bind(trip.stats.distance_m)
    .bind(trip.stats.ascent_m)
    .bind(trip.stats.descent_m)
    .bind(trip.stats.min_lat)
    .bind(trip.stats.min_lon)
    .bind(trip.stats.max_lat)
    .bind(trip.stats.max_lon)
    .bind(&created_at)
    .bind(trip.trip_kind)
    .execute(&mut **tx)
    .await?
    .last_insert_rowid();

    sqlx::query("INSERT INTO track (trip_id, geojson, gpx) VALUES (?,?,?)")
        .bind(trip_id)
        .bind(trip.geojson)
        .bind(trip.gpx)
        .execute(&mut **tx)
        .await?;

    Ok(trip_id)
}

/// The original GPX file plus the trip name, for serving a download (US-21).
pub struct GpxDownload {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Fetch the original GPX of a trip together with its name (for the download
/// filename), or `None` if no such trip exists. One query via the 1:1 join.
pub async fn get_original_gpx(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<GpxDownload>, sqlx::Error> {
    sqlx::query(
        r#"SELECT t.name AS name, k.gpx AS gpx
           FROM trip t JOIN track k ON k.trip_id = t.id
           WHERE t.id = ?"#,
    )
    .bind(id)
    .map(|row: SqliteRow| GpxDownload {
        name: row.get("name"),
        bytes: row.get("gpx"),
    })
    .fetch_optional(pool)
    .await
}

/// Filter criteria for `list_trips` (US-13, ADR-0011). Every field is
/// independently optional — `None` means "don't filter on this dimension" —
/// so the owner can combine any subset (e.g. activity type alone, a date
/// range alone, or several dimensions at once as an AND).
#[derive(Debug, Default)]
pub struct TripFilter {
    pub activity_type: Option<ActivityType>,
    /// Inclusive, `"YYYY-MM-DD"`.
    pub from: Option<String>,
    /// Inclusive, `"YYYY-MM-DD"`.
    pub to: Option<String>,
    pub min_dist_m: Option<f64>,
    pub max_dist_m: Option<f64>,
    /// Case-insensitive (full Unicode case-fold, not just ASCII) substring
    /// match on `name`, applied in Rust after the SQL-filtered fetch.
    pub name_query: Option<String>,
    /// Recorded vs. planned (US-32). `None` returns both — the trip-list page
    /// (`http::trip_list`) always resolves this to `Some` before querying, so
    /// each tab shows exactly one kind; the JSON API (`GET /api/trips`)
    /// leaves it optional like every other filter dimension.
    pub trip_kind: Option<TripKind>,
}

/// `"[year]-[month]-[day]"` — must match `filter::parse_filter`'s format,
/// which is what validates `TripFilter.from`/`to` before they ever reach here.
const DATE_FORMAT: &[time::format_description::FormatItem<'_>] =
    time::macros::format_description!("[year]-[month]-[day]");

/// The calendar day after `date` (a `"YYYY-MM-DD"` string already validated by
/// `filter::parse_filter`), for use as an *exclusive* upper bound on
/// `start_time`. Comparing the raw column directly against `"{day}T00:00:00"`/
/// `"{next_day}T00:00:00"` (rather than wrapping the column itself in SQL's
/// `date(...)`) keeps the predicate index-friendly — SQLite can't use a plain
/// index on `start_time` to service a comparison against `date(start_time)`.
/// Returns `None` only if `date` is already the last representable calendar
/// day, in which case the caller simply leaves the upper bound unapplied
/// rather than panicking on an all-but-impossible edge input.
fn next_day(date: &str) -> Option<String> {
    time::Date::parse(date, DATE_FORMAT)
        .ok()?
        .next_day()?
        .format(DATE_FORMAT)
        .ok()
}

/// List trips as lightweight summaries, most recent first (US-6), optionally
/// narrowed by `filter` (US-13, ADR-0011). Reads only the `trip` table — never
/// the track geometry — so it stays cheap. `start_time` may be NULL (GPX
/// without times); SQLite sorts NULLs last under DESC, and such a trip never
/// matches a `from`/`to` filter (there's no date to compare against).
///
/// The query is built dynamically — only populated filters add a clause — so
/// each is bound exactly once and, for `activity_type`/`distance_m`, stays a
/// plain, index-usable predicate (unlike a static `(? IS NULL OR col op ?)`
/// query, which SQLite's planner can't use an index to service). Name search
/// is applied afterward in Rust via `str::to_lowercase`, a full Unicode
/// case-fold — SQLite's own `LIKE` only case-folds ASCII, which would silently
/// fail to match e.g. Norwegian "Tromsø" against a query of "TROMSØ".
pub async fn list_trips(
    pool: &SqlitePool,
    filter: &TripFilter,
) -> Result<Vec<TripSummary>, sqlx::Error> {
    let mut query = sqlx::QueryBuilder::new(
        "SELECT id, name, activity_type, start_time, distance_m, ascent_m, duration_secs, trip_kind \
         FROM trip WHERE 1 = 1",
    );
    if let Some(activity_type) = filter.activity_type {
        query.push(" AND activity_type = ").push_bind(activity_type);
    }
    if let Some(trip_kind) = filter.trip_kind {
        query.push(" AND trip_kind = ").push_bind(trip_kind);
    }
    if let Some(from) = &filter.from {
        query
            .push(" AND start_time >= ")
            .push_bind(format!("{from}T00:00:00"));
    }
    if let Some(to) = &filter.to {
        if let Some(next) = next_day(to) {
            query
                .push(" AND start_time < ")
                .push_bind(format!("{next}T00:00:00"));
        }
    }
    if let Some(min_dist_m) = filter.min_dist_m {
        query.push(" AND distance_m >= ").push_bind(min_dist_m);
    }
    if let Some(max_dist_m) = filter.max_dist_m {
        query.push(" AND distance_m <= ").push_bind(max_dist_m);
    }
    query.push(" ORDER BY start_time DESC, id DESC");

    let trips: Vec<TripSummary> = query
        .build()
        .map(|row: SqliteRow| TripSummary {
            id: row.get("id"),
            name: row.get("name"),
            activity_type: row.get("activity_type"),
            start_time: row.get("start_time"),
            distance_m: row.get("distance_m"),
            ascent_m: row.get("ascent_m"),
            duration_secs: row.get("duration_secs"),
            trip_kind: row.get("trip_kind"),
        })
        .fetch_all(pool)
        .await?;

    Ok(match &filter.name_query {
        Some(q) => {
            let q = q.to_lowercase();
            trips
                .into_iter()
                .filter(|t| t.name.to_lowercase().contains(&q))
                .collect()
        }
        None => trips,
    })
}

/// Fetch a trip's track geometry as the stored GeoJSON string (US-7), or `None`
/// if the trip has no track. Reads only the `track` table; the blob is returned
/// verbatim so the map and elevation chart share a single fetch (ADR-0005/0006).
pub async fn get_track_geojson(pool: &SqlitePool, id: i64) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT geojson FROM track WHERE trip_id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Delete a trip by id (US-9). `track`/`photo` are declared `ON DELETE CASCADE`
/// (migrations 0001/0003) and `foreign_keys(true)` is enabled on every
/// connection (`db::create_pool`), so the `DELETE FROM trip` also removes the
/// trip's track row and all its photo rows via SQLite itself.
/// Returns `true` if a trip with this id existed and was deleted, `false` if
/// there was no such trip.
///
/// Also marks the trip's `trip_komoot_link` row `delete_pending` (US-24,
/// ADR-0021), in the same transaction as the trip delete, if a link row
/// exists — a no-op `UPDATE` otherwise, so trips never sourced from Komoot
/// are unaffected. This must run *before* the `DELETE FROM trip`: the link
/// row's FK is `ON DELETE SET NULL`, so once the trip row is gone there is no
/// `trip_id` left to match on. The link row itself is deliberately not
/// dropped here — it's the record of "still needs deleting on Komoot" until
/// the next "Sync now" push phase (`komoot_sync::push_pending_deletes`)
/// successfully calls Komoot's delete-tour API and removes it.
pub async fn delete_trip(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query("UPDATE trip_komoot_link SET delete_pending = 1 WHERE trip_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    let result = sqlx::query("DELETE FROM trip WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let deleted = result.rows_affected() > 0;

    tx.commit().await?;
    Ok(deleted)
}

/// Fetch full trip detail by id, or `None` if no such trip exists.
pub async fn get_trip(pool: &SqlitePool, id: i64) -> Result<Option<TripDetail>, sqlx::Error> {
    sqlx::query(
        r#"SELECT id, name, activity_type, tz_name, start_time, end_time,
                  distance_m, ascent_m, descent_m, duration_secs,
                  min_lat, min_lon, max_lat, max_lon
           FROM trip WHERE id = ?"#,
    )
    .bind(id)
    .map(row_to_detail)
    .fetch_optional(pool)
    .await
}

/// Persist a trip's timezone (US-4): used by the lazy backfill path when
/// photos are added to a trip imported before `tz_name` existed.
pub async fn set_trip_timezone(
    pool: &SqlitePool,
    id: i64,
    tz_name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE trip SET tz_name = ? WHERE id = ?")
        .bind(tz_name)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Persist a trip's name and/or activity type (US-15). Each field is
/// optional — `None` leaves that column untouched via `COALESCE`, so a
/// partial edit is a single atomic statement rather than a separate
/// read-then-write in the HTTP layer: there is no window between "check what
/// the trip currently has" and "write the merged result" for a concurrent
/// edit (or delete) of the same trip to race against. Returns `false` if no
/// trip with this id exists (nothing to update).
///
/// Also marks the trip's `trip_komoot_link` row `edit_pending` (US-20,
/// ADR-0021), in the same transaction as the `trip` update, if a link row
/// exists — a no-op `UPDATE` otherwise, so trips never sourced from Komoot
/// are unaffected. Deciding *whether the edit actually changed anything
/// Komoot needs to know about* is deferred to the push phase
/// (`komoot_sync::push_pending_edits`), which diffs against Komoot's live
/// state rather than this call trying to detect a "real" change itself.
pub async fn update_trip(
    pool: &SqlitePool,
    id: i64,
    name: Option<&str>,
    activity_type: Option<ActivityType>,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        "UPDATE trip SET name = COALESCE(?, name), activity_type = COALESCE(?, activity_type) WHERE id = ?",
    )
    .bind(name)
    .bind(activity_type)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let updated = result.rows_affected() > 0;

    if updated {
        sqlx::query("UPDATE trip_komoot_link SET edit_pending = 1 WHERE trip_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(updated)
}

fn row_to_detail(row: SqliteRow) -> TripDetail {
    TripDetail {
        id: row.get("id"),
        name: row.get("name"),
        activity_type: row.get("activity_type"),
        tz_name: row.get("tz_name"),
        start_time: row.get("start_time"),
        end_time: row.get("end_time"),
        distance_m: row.get("distance_m"),
        ascent_m: row.get("ascent_m"),
        descent_m: row.get("descent_m"),
        duration_secs: row.get("duration_secs"),
        min_lat: row.get("min_lat"),
        min_lon: row.get("min_lon"),
        max_lat: row.get("max_lat"),
        max_lon: row.get("max_lon"),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
// Split into trip/tests.rs to keep this file under the repo's 500-line cap.

#[cfg(test)]
mod tests;
