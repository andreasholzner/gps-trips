//! "Sync now" push-and-pull orchestration (US-20/US-22, ADR-0021).
//!
//! Three entry points: [`push_pending_edits`] (US-20 — pushes every
//! Komoot-sourced trip's pending name/activity_type edit back to Komoot),
//! [`list_sync_candidates`] (drives the review page — every Komoot tour not
//! yet in `trip_komoot_link`), and [`sync_selected_tours`] (imports the
//! owner's chosen subset). Each tour's GPX + photos land in the **same**
//! transaction as its `trip_komoot_link` row (ADR-0021) — reusing
//! `repo::insert_trip_in_tx`, `photos::ingest_photos`, and
//! `import::derive_track`, the exact same pipeline `import.rs`'s
//! `handle_import` uses to turn GPX bytes into a trip's stats/GeoJSON/
//! timezone guess. `KomootClient` is blocking
//! (`reqwest::blocking`, ADR-0021); every call runs on `spawn_blocking` so it
//! never stalls the async runtime (ADR-0004), the same pattern `photos.rs`
//! already uses for `BlobStore`/EXIF work.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::Deserialize;
use sqlx::SqlitePool;

use crate::server::{
    error::AppError,
    import::derive_track,
    komoot::{KomootClient, KomootError, KomootPhoto, KomootTourSummary},
    komoot_sport,
    photos::{ingest_photos, UploadedPhoto},
    placement::TripPhotoContext,
    repo,
    storage::BlobStore,
    thumbnail,
};

/// A not-yet-synced Komoot tour, as offered to the owner on the "Sync now"
/// review page.
pub struct SyncCandidate {
    pub tour_id: String,
    pub name: String,
    pub sport: String,
    pub date: String,
    pub distance_m: f64,
}

/// The result of a `sync_selected_tours` run: every tour successfully
/// imported, plus the first failure (if any) that halted the run before
/// later selected tours were attempted (ADR-0021: halt on first failure).
#[derive(Default)]
pub struct SyncSummary {
    pub imported: Vec<(String, i64)>,
    pub failed: Option<(String, String)>,
}

/// The result of a `push_pending_edits` run (US-20): every trip whose edit
/// was successfully pushed to Komoot, plus the first failure (if any) that
/// halted the run before later pending edits were attempted — mirrors
/// `SyncSummary`'s halt-on-first-failure (ADR-0021).
#[derive(Default)]
pub struct PushSummary {
    pub pushed: Vec<(String, i64)>,
    pub failed: Option<(String, String)>,
}

/// The result of a `push_pending_deletes` run (US-24): every tour
/// successfully deleted on Komoot, plus the first failure (if any) that
/// halted the run before later pending deletes were attempted — mirrors
/// `PushSummary`'s halt-on-first-failure (ADR-0021). There's no trip id to
/// pair each tour id with here (the trip is already gone by the time a link
/// row is `delete_pending`), unlike `PushSummary::pushed`.
#[derive(Default)]
pub struct PushDeleteSummary {
    pub deleted: Vec<String>,
    pub failed: Option<(String, String)>,
}

/// Query params on the "Sync now" review page's redirect after a run: how
/// many pending edits/deletes were pushed and tours imported, and which
/// trip/tour (if any) halted the run and in which phase — echoed back into
/// the page as a one-line result banner (no session/flash mechanism in this
/// app; matches how every other server-rendered page here carries its own
/// state via the query string).
#[derive(Debug, Default, Deserialize)]
pub struct SyncResultQuery {
    pub pushed: Option<usize>,
    /// US-24: tours deleted on Komoot this run.
    pub deleted: Option<usize>,
    pub synced: Option<usize>,
    pub failed_tour: Option<String>,
    pub failed_msg: Option<String>,
    pub failed_phase: Option<String>,
}

const PAGE_SIZE: u32 = 200;

/// Run `f` on the blocking pool — every `KomootClient` call goes through
/// this, matching `photos.rs`'s `put_blob`/`extract_photo_metadata`.
async fn blocking_call<T, F>(f: F) -> Result<T, KomootError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, KomootError> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .expect("komoot client task panicked")
}

/// Every tour on the account, paginating until a short (or empty) page.
async fn list_all_tours(
    client: &Arc<dyn KomootClient>,
    username: &str,
) -> Result<Vec<KomootTourSummary>, KomootError> {
    let mut all = Vec::new();
    let mut page = 0u32;
    loop {
        let batch = blocking_call({
            let client = Arc::clone(client);
            let username = username.to_string();
            move || client.list_tours(&username, Some(PAGE_SIZE), Some(page))
        })
        .await?;
        let len = batch.len() as u32;
        all.extend(batch);
        if len < PAGE_SIZE {
            break;
        }
        page += 1;
    }
    Ok(all)
}

/// Every photo attached to a tour, paginating until a short (or empty) page
/// — mirrors `list_all_tours` above. A tour with more photos than one page
/// would otherwise silently lose the rest.
async fn list_all_tour_photos(
    client: &Arc<dyn KomootClient>,
    tour_id: &str,
) -> Result<Vec<KomootPhoto>, KomootError> {
    let mut all = Vec::new();
    let mut page = 0u32;
    loop {
        let batch = blocking_call({
            let client = Arc::clone(client);
            let tour_id = tour_id.to_string();
            move || client.get_tour_photos(&tour_id, Some(PAGE_SIZE), Some(page))
        })
        .await?;
        let len = batch.len() as u32;
        all.extend(batch);
        if len < PAGE_SIZE {
            break;
        }
        page += 1;
    }
    Ok(all)
}

/// Every Komoot tour not yet linked to a trip (anti-join dedup, ADR-0021),
/// for the "Sync now" review page.
pub async fn list_sync_candidates(
    pool: &SqlitePool,
    client: Arc<dyn KomootClient>,
) -> Result<Vec<SyncCandidate>, AppError> {
    let username = blocking_call({
        let client = Arc::clone(&client);
        move || client.login()
    })
    .await?;
    let all_tours = list_all_tours(&client, &username).await?;
    let linked = repo::komoot::list_linked_tour_ids(pool).await?;

    Ok(all_tours
        .into_iter()
        .filter(|t| !linked.contains(&t.id))
        .map(|t| SyncCandidate {
            tour_id: t.id,
            name: t.name,
            sport: t.sport,
            date: t.date,
            distance_m: t.distance,
        })
        .collect())
}

/// Import the owner's selected tours, in the given order, halting on the
/// first failure. A selected tour that's already linked (e.g. synced by a
/// concurrent run since the review page was rendered) is silently skipped,
/// not treated as a failure — the anti-join dedup applies here too.
pub async fn sync_selected_tours(
    pool: &SqlitePool,
    store: &Arc<dyn BlobStore>,
    client: Arc<dyn KomootClient>,
    tour_ids: &[String],
) -> Result<SyncSummary, AppError> {
    let username = blocking_call({
        let client = Arc::clone(&client);
        move || client.login()
    })
    .await?;
    let all_tours = list_all_tours(&client, &username).await?;
    let mut by_id: HashMap<String, KomootTourSummary> =
        all_tours.into_iter().map(|t| (t.id.clone(), t)).collect();
    let already_linked: HashSet<String> = repo::komoot::list_linked_tour_ids(pool).await?;

    let mut summary = SyncSummary::default();
    // `tour_ids` comes straight from the request body (`POST
    // /api/komoot/sync`) with no dedup applied at that boundary — a repeat
    // id would otherwise look like "already imported by an earlier entry
    // in this same run" on its second occurrence and spuriously halt the
    // run. Track what's already been *attempted this run* to skip repeats
    // instead.
    let mut attempted = HashSet::new();

    for tour_id in tour_ids {
        if !attempted.insert(tour_id.as_str()) {
            continue;
        }
        if already_linked.contains(tour_id) {
            continue;
        }
        let Some(tour) = by_id.remove(tour_id) else {
            summary.failed = Some((
                tour_id.clone(),
                "tour is no longer listed by Komoot".to_string(),
            ));
            break;
        };

        match sync_one_tour(pool, store, &client, &tour).await {
            Ok(trip_id) => summary.imported.push((tour_id.clone(), trip_id)),
            Err(e) => {
                summary.failed = Some((tour_id.clone(), e.to_string()));
                break;
            }
        }
    }

    Ok(summary)
}

/// Fetch one tour's GPX + photos and import it through the same pipeline
/// `handle_import` uses, in one transaction with its `trip_komoot_link` row.
async fn sync_one_tour(
    pool: &SqlitePool,
    store: &Arc<dyn BlobStore>,
    client: &Arc<dyn KomootClient>,
    tour: &KomootTourSummary,
) -> Result<i64, AppError> {
    let gpx_bytes = blocking_call({
        let client = Arc::clone(client);
        let tour_id = tour.id.clone();
        move || client.get_tour_gpx(&tour_id)
    })
    .await?;

    let derived = derive_track(&gpx_bytes)?;
    let activity = komoot_sport::map_sport(&tour.sport);

    let komoot_photos = list_all_tour_photos(client, &tour.id).await?;

    let mut uploaded_photos = Vec::with_capacity(komoot_photos.len());
    for photo in komoot_photos {
        let url = crate::server::komoot::resolve_photo_url(
            &photo.src,
            photo.width_px,
            photo.height_px,
            false,
        );
        let bytes = blocking_call({
            let client = Arc::clone(client);
            let url = url.clone();
            move || client.fetch_photo_bytes(&url)
        })
        .await?;
        // Komoot's photo response carries no filename/Content-Type of its
        // own, so the extension baked into `original_name` here is what
        // later decides the served Content-Type (`http.rs`,
        // `content_type_from_path`) — sniff the real format rather than
        // assuming JPEG (the same trap `thumbnail_key`'s doc comment
        // describes for the generated thumbnail).
        let (ext, content_type) = thumbnail::guess_image_format(&bytes);
        uploaded_photos.push(UploadedPhoto {
            original_name: format!("komoot-{}.{ext}", photo.id),
            content_type: Some(content_type.to_string()),
            bytes,
            known_location: photo.location.map(|l| (l.lat, l.lng)),
        });
    }

    let ctx = TripPhotoContext {
        timed_points: &derived.timed_points,
        tz_name: Some(&derived.guessed_tz),
    };

    let mut tx = pool.begin().await?;
    let trip_id = repo::insert_trip_in_tx(
        &mut tx,
        &tour.name,
        activity,
        &derived.guessed_tz,
        &derived.stats,
        &derived.geojson,
        &gpx_bytes,
    )
    .await?;
    // The link row is inserted (and can fail on its `komoot_tour_id`
    // UNIQUE constraint if a concurrent sync linked this tour first)
    // *before* photos are ingested: `ingest_photos` writes blob files to
    // the (non-transactional) `BlobStore`, so failing here first means
    // that race never leaves orphaned blobs behind.
    repo::komoot::insert_link_in_tx(&mut tx, trip_id, &tour.id).await?;
    ingest_photos(&mut tx, store, trip_id, &ctx, uploaded_photos).await?;
    tx.commit().await?;

    Ok(trip_id)
}

/// Push every pending edit (US-20, ADR-0021) to Komoot: for each trip whose
/// `trip_komoot_link` row is `edit_pending`, call Komoot's update-tour API
/// with the trip's current name/activity_type, then clear the flag. Halts on
/// the first failure, leaving later pending edits untouched — mirrors
/// `sync_selected_tours`'s pull-phase halt-on-first-failure.
pub async fn push_pending_edits(
    pool: &SqlitePool,
    client: Arc<dyn KomootClient>,
) -> Result<PushSummary, AppError> {
    let pending = repo::komoot::list_edit_pending(pool).await?;
    let mut summary = PushSummary::default();

    for edit in pending {
        match push_one_edit(&client, &edit).await {
            Ok(()) => {
                repo::komoot::clear_edit_pending(pool, edit.trip_id).await?;
                summary.pushed.push((edit.komoot_tour_id, edit.trip_id));
            }
            Err(e) => {
                summary.failed = Some((edit.komoot_tour_id, e.to_string()));
                break;
            }
        }
    }

    Ok(summary)
}

/// Push one trip's pending edit to Komoot. Reads the tour's *live* current
/// sport first and only sends a remapped sport (via
/// `komoot_sport::activity_to_sport`) when it actually disagrees with the
/// trip's local `activity_type` — otherwise resends the live sport
/// unchanged, so an edit that only touched the name doesn't downgrade a
/// trip pulled from Komoot with a specific sport (e.g. `mtb`) to the generic
/// string `activity_to_sport` returns for `Cycling`.
async fn push_one_edit(
    client: &Arc<dyn KomootClient>,
    edit: &repo::komoot::EditPending,
) -> Result<(), AppError> {
    let live = blocking_call({
        let client = Arc::clone(client);
        let tour_id = edit.komoot_tour_id.clone();
        move || client.get_tour(&tour_id)
    })
    .await?;

    let outgoing_sport = if komoot_sport::map_sport(&live.sport) == edit.activity_type {
        live.sport
    } else {
        komoot_sport::activity_to_sport(edit.activity_type).to_string()
    };

    blocking_call({
        let client = Arc::clone(client);
        let tour_id = edit.komoot_tour_id.clone();
        let name = edit.name.clone();
        move || client.update_tour(&tour_id, &name, &outgoing_sport)
    })
    .await?;

    Ok(())
}

/// Push every pending delete (US-24, ADR-0021) to Komoot: for each
/// `trip_komoot_link` row that's `delete_pending`, call Komoot's
/// delete-tour API, then remove the link row. Halts on the first failure,
/// leaving later pending deletes untouched — mirrors `push_pending_edits`'s
/// halt-on-first-failure. A failed call's message is prefixed with
/// `"delete tour: "` so it stays traceable to a delete failure even though
/// the "Sync now" page's result banner reuses generic "push phase" wording
/// for both edit and delete failures.
pub async fn push_pending_deletes(
    pool: &SqlitePool,
    client: Arc<dyn KomootClient>,
) -> Result<PushDeleteSummary, AppError> {
    let pending = repo::komoot::list_delete_pending(pool).await?;
    let mut summary = PushDeleteSummary::default();

    for tour_id in pending {
        let result = blocking_call({
            let client = Arc::clone(&client);
            let tour_id = tour_id.clone();
            move || client.delete_tour(&tour_id)
        })
        .await;

        match result {
            Ok(()) => {
                repo::komoot::delete_link(pool, &tour_id).await?;
                summary.deleted.push(tour_id);
            }
            Err(e) => {
                summary.failed = Some((tour_id, format!("delete tour: {e}")));
                break;
            }
        }
    }

    Ok(summary)
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
// Split into komoot_sync/tests.rs to keep this file under the repo's 500-line cap.

#[cfg(test)]
mod tests;
