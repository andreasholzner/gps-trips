//! Adding files to a collection (US-88): the upload parked for the owner's
//! review (migration 0026), what the collection holds for matching it, and
//! the confirmed outcome stored in one go (ADR-0029).

use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};
use time::OffsetDateTime;

use crate::server::collection_match::{Held, HeldTrack, HeldWaypoint};
use crate::server::repo::to_rfc3339;

use super::files::insert_files;
use super::{insert_track, insert_waypoint, NewCollectionFile, NewCollectionTrack, NewWaypoint};
use crate::server::repo::trip::track_positions;

/// An upload waiting for review: the files in the order they are taken.
pub struct StagedCollectionImport {
    /// `None` for a new collection.
    pub collection_id: Option<i64>,
    /// Each file's name and bytes.
    pub files: Vec<(String, Vec<u8>)>,
}

/// Park an upload and return its handle, or `None` if the collection it is
/// for is gone — deleted since it was looked up. `now` is passed in, as for
/// the trip import's parse (`insert_staged_import`).
pub async fn insert_collection_import(
    pool: &SqlitePool,
    collection_id: Option<i64>,
    files: &[NewCollectionFile<'_>],
    now: OffsetDateTime,
) -> Result<Option<i64>, sqlx::Error> {
    let mut tx = crate::server::db::begin_write(pool).await?;
    let inserted = sqlx::query(
        "INSERT INTO collection_import_staging (created_at, collection_id) \
         SELECT ?1, ?2 WHERE ?2 IS NULL OR EXISTS (SELECT 1 FROM collection WHERE id = ?2)",
    )
    .bind(to_rfc3339(now))
    .bind(collection_id)
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() == 0 {
        return Ok(None);
    }
    let id = inserted.last_insert_rowid();
    for (position, file) in files.iter().enumerate() {
        sqlx::query(
            "INSERT INTO collection_import_staging_file (staging_id, position, file_name, gpx) \
             VALUES (?,?,?,?)",
        )
        .bind(id)
        .bind(position as i64)
        .bind(file.file_name)
        .bind(file.gpx)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Some(id))
}

/// A parked upload, or `None` once spent, swept or cancelled.
pub async fn get_collection_import(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<StagedCollectionImport>, sqlx::Error> {
    read_import(&mut *pool.acquire().await?, id).await
}

/// Read a parked upload *and delete it*, on the caller's transaction, so a
/// confirmation cannot be applied twice and a failed one keeps the upload.
pub async fn take_collection_import_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
) -> Result<Option<StagedCollectionImport>, sqlx::Error> {
    let staged = read_import(tx, id).await?;
    if staged.is_some() {
        sqlx::query("DELETE FROM collection_import_staging WHERE id = ?")
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(staged)
}

async fn read_import(
    conn: &mut SqliteConnection,
    id: i64,
) -> Result<Option<StagedCollectionImport>, sqlx::Error> {
    let Some((collection_id,)) = sqlx::query_as::<_, (Option<i64>,)>(
        "SELECT collection_id FROM collection_import_staging WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    let files = sqlx::query_as(
        "SELECT file_name, gpx FROM collection_import_staging_file \
         WHERE staging_id = ? ORDER BY position",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Some(StagedCollectionImport {
        collection_id,
        files,
    }))
}

/// Drop an upload the owner cancelled. `false` if there was none.
pub async fn delete_collection_import(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let affected = sqlx::query("DELETE FROM collection_import_staging WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(affected > 0)
}

/// Delete every upload parked before `cutoff`, returning how many went.
pub async fn sweep_collection_imports(
    pool: &SqlitePool,
    cutoff: OffsetDateTime,
) -> Result<u64, sqlx::Error> {
    let affected = sqlx::query("DELETE FROM collection_import_staging WHERE created_at < ?")
        .bind(to_rfc3339(cutoff))
        .execute(pool)
        .await?
        .rows_affected();
    Ok(affected)
}

/// Everything a collection holds, in its order, as matching reads it.
pub async fn load_held(
    conn: &mut SqliteConnection,
    collection_id: i64,
) -> Result<Held, sqlx::Error> {
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT id, name, json_extract(geojson, '$.geometry.coordinates') FROM collection_track \
         WHERE collection_id = ? ORDER BY position",
    )
    .bind(collection_id)
    .fetch_all(&mut *conn)
    .await?;
    let tracks = rows
        .into_iter()
        .map(|(id, name, positions)| {
            let line = track_positions(id, &positions)?
                .coordinates
                .into_iter()
                .map(|[lon, lat]| (lat, lon))
                .collect();
            Ok(HeldTrack { id, name, line })
        })
        .collect::<Result<_, sqlx::Error>>()?;

    type Row = (
        i64,
        Option<String>,
        Option<String>,
        f64,
        f64,
        Option<f64>,
        Option<String>,
    );
    let waypoints = sqlx::query_as::<_, Row>(
        "SELECT id, name, description, lat, lon, ele, symbol FROM waypoint \
         WHERE collection_id = ? ORDER BY position",
    )
    .bind(collection_id)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(
        |(id, name, description, lat, lon, ele, symbol)| HeldWaypoint {
            id,
            name,
            description,
            lat,
            lon,
            ele,
            symbol,
        },
    )
    .collect();
    Ok(Held { tracks, waypoints })
}

/// A track of the confirmed outcome.
pub enum PlannedTrack<'a> {
    /// Held, left as it is; listed so waypoints can name it.
    Keep(i64),
    /// Held, updated in place: it keeps its id, position and own activity
    /// (ADR-0029).
    Update(i64, NewCollectionTrack<'a>),
    /// New, after the collection's last.
    Insert(NewCollectionTrack<'a>),
}

/// Which track a stored waypoint belongs to as well.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackRef {
    /// As it is.
    Unchanged,
    /// The plan's track at this index, or none.
    Set(Option<usize>),
}

/// A waypoint the outcome changes; one it leaves as it is is not listed.
pub enum PlannedWaypoint<'a> {
    Update(i64, NewWaypoint<'a>, TrackRef),
    Insert(NewWaypoint<'a>, TrackRef),
}

/// The owner's confirmed outcome for a collection.
pub struct CollectionImportPlan<'a> {
    /// Everything held — tracks, waypoints and kept files — goes first.
    pub replace_all: bool,
    pub remove_tracks: &'a [i64],
    pub remove_waypoints: &'a [i64],
    pub tracks: &'a [PlannedTrack<'a>],
    pub waypoints: &'a [PlannedWaypoint<'a>],
    pub files: &'a [NewCollectionFile<'a>],
}

/// Store the outcome on the caller's transaction: all of it or, with the
/// transaction rolled back, none (ADR-0029). Only rows of `collection_id`
/// are touched.
pub async fn apply_collection_import(
    tx: &mut Transaction<'_, Sqlite>,
    collection_id: i64,
    plan: &CollectionImportPlan<'_>,
) -> Result<(), sqlx::Error> {
    if plan.replace_all {
        for table in ["collection_track", "waypoint", "collection_file"] {
            sqlx::query(&format!("DELETE FROM {table} WHERE collection_id = ?"))
                .bind(collection_id)
                .execute(&mut **tx)
                .await?;
        }
    }
    for (table, ids) in [
        ("collection_track", plan.remove_tracks),
        ("waypoint", plan.remove_waypoints),
    ] {
        for id in ids {
            sqlx::query(&format!(
                "DELETE FROM {table} WHERE id = ? AND collection_id = ?"
            ))
            .bind(id)
            .bind(collection_id)
            .execute(&mut **tx)
            .await?;
        }
    }

    let mut track_ids = Vec::with_capacity(plan.tracks.len());
    for track in plan.tracks {
        let id = match track {
            PlannedTrack::Keep(id) => *id,
            PlannedTrack::Update(id, track) => {
                update_track(tx, collection_id, *id, track).await?;
                *id
            }
            PlannedTrack::Insert(track) => insert_track(tx, collection_id, track).await?,
        };
        track_ids.push(id);
    }
    let resolve = |track: TrackRef| match track {
        TrackRef::Unchanged => None,
        TrackRef::Set(index) => Some(index.and_then(|i| track_ids.get(i).copied())),
    };
    for waypoint in plan.waypoints {
        match waypoint {
            PlannedWaypoint::Update(id, waypoint, track) => {
                update_waypoint(tx, collection_id, *id, waypoint, resolve(*track)).await?;
            }
            PlannedWaypoint::Insert(waypoint, track) => {
                insert_waypoint(tx, collection_id, waypoint, resolve(*track).flatten()).await?;
            }
        }
    }
    insert_files(tx, collection_id, plan.files).await
}

async fn update_track(
    tx: &mut Transaction<'_, Sqlite>,
    collection_id: i64,
    id: i64,
    track: &NewCollectionTrack<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE collection_track SET name = ?, distance_m = ?, ascent_m = ?, descent_m = ?, \
             min_lat = ?, min_lon = ?, max_lat = ?, max_lon = ?, geojson = ?, gpx = ? \
         WHERE id = ? AND collection_id = ?",
    )
    .bind(track.name)
    .bind(track.stats.distance_m)
    .bind(track.stats.ascent_m)
    .bind(track.stats.descent_m)
    .bind(track.stats.min_lat)
    .bind(track.stats.min_lon)
    .bind(track.stats.max_lat)
    .bind(track.stats.max_lon)
    .bind(track.geojson)
    .bind(track.gpx)
    .bind(id)
    .bind(collection_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// `track` of `None` leaves the waypoint's track as it is.
async fn update_waypoint(
    tx: &mut Transaction<'_, Sqlite>,
    collection_id: i64,
    id: i64,
    waypoint: &NewWaypoint<'_>,
    track: Option<Option<i64>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE waypoint SET name = ?, description = ?, lat = ?, lon = ?, ele = ?, symbol = ?, \
             collection_track_id = CASE WHEN ? THEN ? ELSE collection_track_id END \
         WHERE id = ? AND collection_id = ?",
    )
    .bind(waypoint.name)
    .bind(waypoint.description)
    .bind(waypoint.lat)
    .bind(waypoint.lon)
    .bind(waypoint.ele)
    .bind(waypoint.symbol)
    .bind(track.is_some())
    .bind(track.flatten())
    .bind(id)
    .bind(collection_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
