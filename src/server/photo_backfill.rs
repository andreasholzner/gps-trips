//! US-62: give the photos stored before the archive kept capture times the
//! one their EXIF names. ADR-0026 keeps each stored copy's EXIF verbatim, so
//! the time is read back out of the store, without the originals, and
//! resolved exactly as it would have been at ingestion
//! (`placement::capture_instant`) — the caption then agrees with where the
//! photo was placed. Driven by the `photo_taken_at_backfill` binary.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::server::{
    error::AppError, import::resolve_photo_context, location, placement, repo, storage::BlobStore,
};

/// What a run did. A photo whose bytes could not be read is reported rather
/// than stopping the run: the others do not depend on it, and a rerun tries
/// it again.
#[derive(Debug, Default)]
pub struct BackfillSummary {
    pub filled: usize,
    /// Photos whose EXIF names no capture time, or one no reading resolves.
    pub without_time: usize,
    /// `(blob key, error)` for each photo whose stored copy could not be read.
    pub unreadable: Vec<(String, String)>,
}

/// Fill in `taken_at` for every photo that has none. Safe to rerun: a photo
/// already carrying one is not read again.
pub async fn backfill_taken_at(
    pool: &SqlitePool,
    store: &Arc<dyn BlobStore>,
) -> Result<BackfillSummary, AppError> {
    let photos = repo::list_photos_without_taken_at(pool).await?;
    let mut summary = BackfillSummary::default();

    for trip_photos in photos.chunk_by(|a, b| a.trip_id == b.trip_id) {
        let trip_id = trip_photos[0].trip_id;
        let Some(trip) = repo::get_trip(pool, trip_id).await? else {
            continue;
        };
        let (timed_points, tz_name) = resolve_photo_context(pool, trip_id, trip).await?;
        let ctx = placement::TripPhotoContext::new(&timed_points, Some(&tz_name));

        for photo in trip_photos {
            let metadata = match read_metadata(store, &photo.blob_key).await {
                Ok(metadata) => metadata,
                Err(err) => {
                    summary.unreadable.push((photo.blob_key.clone(), err));
                    continue;
                }
            };
            match placement::capture_instant(&metadata, &ctx) {
                Some(taken_at) => {
                    repo::set_photo_taken_at(pool, photo.id, taken_at).await?;
                    summary.filled += 1;
                }
                None => summary.without_time += 1,
            }
        }
    }
    Ok(summary)
}

/// A stored copy's EXIF, read off the async runtime — both the read and the
/// parse block.
async fn read_metadata(
    store: &Arc<dyn BlobStore>,
    key: &str,
) -> Result<location::PhotoMetadata, String> {
    let store = Arc::clone(store);
    let key = key.to_string();
    tokio::task::spawn_blocking(move || {
        let bytes = store.get(&key).map_err(|e| e.to_string())?;
        Ok(location::extract_photo_metadata(&bytes))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
