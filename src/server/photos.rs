//! Photo ingestion — the one path that stores uploaded photos and associates
//! them with a trip (US-2).
//!
//! Both entry points use it: `POST /api/import` (photos uploaded with the GPX)
//! and `POST /api/trips/:id/photos` (photos added later). Keeping it in a single
//! function means the storage/association behaviour cannot drift between the two
//! (ADR-0004). Each photo's placement (US-3 EXIF GPS, US-4 timestamp
//! interpolation) is decided by `placement::resolve_placement`; each photo's
//! size-bounded stored copy (US-54, ADR-0026) and thumbnail (US-5) come from
//! `thumbnail::process_photo`; this module only owns the upload -> blob
//! storage -> DB association mechanics.

use std::sync::Arc;

use sqlx::{Sqlite, Transaction};

use crate::models::LocationSource;
use crate::server::{
    error::AppError,
    location,
    placement::{resolve_placement, TripPhotoContext},
    repo::{self, NewPhoto},
    storage::BlobStore,
    thumbnail,
};

/// A photo received from a multipart upload, held in memory until stored.
pub struct UploadedPhoto {
    pub original_name: String,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
    /// A location already known for this photo from outside this app's own
    /// EXIF/interpolation pipeline (US-22: Komoot's own per-photo GPS).
    /// `None` for both multipart entry points (`handle_import`,
    /// `handle_add_photos`) — only the Komoot sync path sets this.
    pub known_location: Option<(f64, f64)>,
}

/// A photo ready to store: its EXIF read from the upload, and its bytes
/// already the size-bounded copy when the upload exceeded the bound (US-54,
/// ADR-0026). Holding these rather than [`UploadedPhoto`]s is what keeps a
/// batch from holding every full-size upload at once.
pub struct PreparedPhoto {
    original_name: String,
    content_type: Option<String>,
    bytes: Vec<u8>,
    /// `bytes` is a re-encoded JPEG rather than the upload itself.
    reencoded: bool,
    thumbnail: Option<Vec<u8>>,
    metadata: location::PhotoMetadata,
    known_location: Option<(f64, f64)>,
}

/// Read a photo's EXIF from the upload itself, then derive the copy to store
/// and the thumbnail from it (US-3, US-4, US-5, US-54) — best-effort
/// throughout, never a failed import. The full-size upload is dropped here
/// whenever a smaller copy replaces it.
pub async fn prepare_photo(photo: UploadedPhoto) -> PreparedPhoto {
    let (upload, metadata, processed) = extract_photo_metadata(photo.bytes).await;
    let (bytes, content_type, reencoded) = match processed.stored {
        Some(copy) => (copy, Some("image/jpeg".to_string()), true),
        None => (upload, photo.content_type, false),
    };
    PreparedPhoto {
        original_name: photo.original_name,
        content_type,
        bytes,
        reencoded,
        thumbnail: processed.thumbnail,
        metadata,
        known_location: photo.known_location,
    }
}

/// [`prepare_photo`] each upload, then [`store_photos`] them.
pub async fn ingest_photos(
    tx: &mut Transaction<'_, Sqlite>,
    store: &Arc<dyn BlobStore>,
    trip_id: i64,
    ctx: &TripPhotoContext<'_>,
    photos: Vec<UploadedPhoto>,
) -> Result<Vec<i64>, AppError> {
    let mut prepared = Vec::with_capacity(photos.len());
    for photo in photos {
        prepared.push(prepare_photo(photo).await);
    }
    store_photos(tx, store, trip_id, ctx, prepared).await
}

/// Store each prepared photo in the `BlobStore` and insert its association row
/// on the caller's transaction (so an import commits trip + track + photos as
/// one unit), placing it on the map as it goes (US-3, US-4). Photos are keyed
/// per trip with a running ordinal, continuing past any already attached, so
/// adding photos later never collides with earlier keys. Consumes the photos —
/// each one's bytes are moved to the store, not copied. Returns the new photo
/// ids in order.
pub async fn store_photos(
    tx: &mut Transaction<'_, Sqlite>,
    store: &Arc<dyn BlobStore>,
    trip_id: i64,
    ctx: &TripPhotoContext<'_>,
    photos: Vec<PreparedPhoto>,
) -> Result<Vec<i64>, AppError> {
    let mut ordinal = repo::count_photos(tx, trip_id).await?;
    let mut ids = Vec::with_capacity(photos.len());

    for photo in photos {
        let (lat, lon, location_source) =
            resolve_placement(photo.metadata, ctx, photo.known_location);
        // A re-encoded copy is a JPEG whatever the upload was, and its key's
        // extension is what `content_type_from_path` (`http.rs`) serves it by.
        let key = if photo.reencoded {
            blob_key(trip_id, ordinal, &jpg_name(&photo.original_name))
        } else {
            blob_key(trip_id, ordinal, &photo.original_name)
        };
        let byte_len = photo.bytes.len() as i64;
        if location_source == LocationSource::None {
            tracing::debug!(
                photo = %photo.original_name,
                "no usable EXIF GPS or timestamp; location_source = none"
            );
        }
        put_blob(store, key.clone(), photo.bytes).await?;

        // Storing the thumbnail is best-effort too (US-5, ADR-0020): a
        // storage failure here must not fail the whole import when the
        // original photo and its metadata already succeeded.
        let thumb_key = match photo.thumbnail {
            Some(thumb_bytes) => {
                let thumb_key = thumbnail_key(trip_id, ordinal, &photo.original_name);
                match put_blob(store, thumb_key.clone(), thumb_bytes).await {
                    Ok(()) => Some(thumb_key),
                    Err(e) => {
                        tracing::warn!(
                            photo = %photo.original_name,
                            error = %e,
                            "failed to store generated thumbnail; falling back to the original"
                        );
                        None
                    }
                }
            }
            None => {
                tracing::debug!(
                    photo = %photo.original_name,
                    "thumbnail generation failed; falling back to the original"
                );
                None
            }
        };

        let id = repo::insert_photo(
            tx,
            trip_id,
            &NewPhoto {
                original_name: &photo.original_name,
                content_type: photo.content_type.as_deref(),
                byte_len,
                blob_key: &key,
                thumbnail_key: thumb_key.as_deref(),
                lat,
                lon,
                location_source,
            },
        )
        .await?;
        ids.push(id);
        ordinal += 1;
    }

    Ok(ids)
}

/// The `{ordinal:04}-{sanitized_name}` shape shared by `blob_key` and
/// `thumbnail_key`, so their key layout (zero-padding width, in particular)
/// can't silently drift apart between the two.
fn ordinal_name(ordinal: i64, sanitized_name: &str) -> String {
    format!("{ordinal:04}-{sanitized_name}")
}

/// A unique, human-readable blob key for a photo: `trips/{id}/{ordinal}-{name}`.
/// The ordinal guarantees uniqueness within a trip even when two uploads share a
/// filename; the name is sanitised so it is safe as a path segment.
fn blob_key(trip_id: i64, ordinal: i64, original_name: &str) -> String {
    format!(
        "trips/{trip_id}/{}",
        ordinal_name(ordinal, &sanitize(original_name))
    )
}

/// The blob key for a photo's generated thumbnail (US-5): same shape as
/// `blob_key`, under a `thumbs/` segment so originals and thumbnails never
/// collide in the `BlobStore`. Always given a `.jpg` extension regardless of
/// the original's — the thumbnail is always re-encoded as JPEG (ADR-0020), so
/// keeping the original extension would make `content_type_from_path`
/// (`http.rs`) serve it with the wrong `Content-Type` for any non-JPEG source.
fn thumbnail_key(trip_id: i64, ordinal: i64, original_name: &str) -> String {
    format!(
        "trips/{trip_id}/thumbs/{}",
        ordinal_name(ordinal, &sanitize(&jpg_name(original_name)))
    )
}

/// `name` with its extension replaced by `.jpg`, for a blob re-encoded as JPEG.
fn jpg_name(name: &str) -> String {
    format!("{}.jpg", strip_extension(name))
}

/// The part of `name` before its last `.`, or the whole name if it has none.
fn strip_extension(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

/// Reduce a filename to a safe single path segment: keep ASCII alphanumerics and
/// `.`/`-`/`_`, replace anything else (including path separators) with `_`.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "photo".to_string()
    } else {
        cleaned
    }
}

/// Write a blob off the async runtime: `BlobStore` is synchronous filesystem I/O
/// (ADR-0007), so run it on the blocking pool to avoid stalling the runtime
/// (ADR-0004).
async fn put_blob(store: &Arc<dyn BlobStore>, key: String, bytes: Vec<u8>) -> Result<(), AppError> {
    let store = Arc::clone(store);
    tokio::task::spawn_blocking(move || store.put(&key, &bytes))
        .await
        .expect("blob store task panicked")?;
    Ok(())
}

/// Run EXIF extraction and image processing off the async runtime (US-3,
/// US-4, US-5, US-54): parsing an untrusted upload's EXIF/TIFF structure and
/// decoding/resizing/re-encoding its image data are both synchronous,
/// potentially expensive work, the same class of risk `put_blob` offloads for
/// the same reason (ADR-0004). Both operate on the same input bytes, so they
/// share this one blocking-pool hop rather than each taking their own.
/// Returns the bytes back so the caller can still move them into the
/// `BlobStore` afterward without a copy, when they are stored as uploaded.
///
/// Image processing is wrapped in `catch_unwind`: it is best-effort
/// (ADR-0020, ADR-0026), and an internal panic from the `image` crate on some
/// pathological-but-format-sniffable input must be contained to "stored as
/// uploaded, no thumbnail" rather than taking down EXIF extraction's
/// already-resolved results too, or surfacing as a misleadingly-named panic.
async fn extract_photo_metadata(
    bytes: Vec<u8>,
) -> (Vec<u8>, location::PhotoMetadata, thumbnail::ProcessedPhoto) {
    tokio::task::spawn_blocking(move || {
        let metadata = location::extract_photo_metadata(&bytes);
        let processed =
            std::panic::catch_unwind(|| thumbnail::process_photo(&bytes, metadata.orientation))
                .unwrap_or_else(|_| {
                    tracing::warn!("image processing panicked; storing the photo as uploaded");
                    thumbnail::ProcessedPhoto::default()
                });
        (bytes, metadata, processed)
    })
    .await
    .expect("photo metadata task panicked")
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
// Split into photos/tests.rs to keep this file under the repo's 500-line cap.

#[cfg(test)]
mod tests;
