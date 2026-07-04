//! Photo ingestion — the one path that stores uploaded photos and associates
//! them with a trip (US-2).
//!
//! Both entry points use it: `POST /api/import` (photos uploaded with the GPX)
//! and `POST /api/trips/:id/photos` (photos added later). Keeping it in a single
//! function means the storage/association behaviour cannot drift between the two
//! (ADR-0004). Each photo's placement (US-3 EXIF GPS, US-4 timestamp
//! interpolation) is decided by `placement::resolve_placement`; this module
//! only owns the upload -> blob storage -> DB association mechanics.
//! Thumbnails (US-5) extend this path in their own story.

use std::sync::Arc;

use sqlx::{Sqlite, Transaction};

use crate::models::LocationSource;
use crate::server::{
    error::AppError,
    location,
    placement::{resolve_placement, TripPhotoContext},
    repo::{self, NewPhoto},
    storage::BlobStore,
};

/// A photo received from a multipart upload, held in memory until stored.
pub struct UploadedPhoto {
    pub original_name: String,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// Store each photo's bytes in the `BlobStore` and insert its association row on
/// the caller's transaction (so an import commits trip + track + photos as one
/// unit). Photos are keyed per trip with a running ordinal, continuing past any
/// already attached, so adding photos later never collides with earlier keys.
/// Consumes the uploads — each photo's bytes are moved to the store, not copied.
/// Returns the new photo ids in upload order.
pub async fn ingest_photos(
    tx: &mut Transaction<'_, Sqlite>,
    store: &Arc<dyn BlobStore>,
    trip_id: i64,
    ctx: &TripPhotoContext<'_>,
    photos: Vec<UploadedPhoto>,
) -> Result<Vec<i64>, AppError> {
    let mut ordinal = repo::count_photos(tx, trip_id).await?;
    let mut ids = Vec::with_capacity(photos.len());

    for photo in photos {
        let key = blob_key(trip_id, ordinal, &photo.original_name);
        let byte_len = photo.bytes.len() as i64;
        // Extract EXIF metadata before the bytes move into the store, then
        // decide where the photo goes (US-3, US-4) — best-effort throughout,
        // never a failed import.
        let (bytes, metadata) = extract_photo_metadata(photo.bytes).await;
        let (lat, lon, location_source) = resolve_placement(metadata, ctx);
        if location_source == LocationSource::None {
            tracing::debug!(
                photo = %photo.original_name,
                "no usable EXIF GPS or timestamp; location_source = none"
            );
        }
        put_blob(store, key.clone(), bytes).await?;
        let id = repo::insert_photo(
            tx,
            trip_id,
            &NewPhoto {
                original_name: &photo.original_name,
                content_type: photo.content_type.as_deref(),
                byte_len,
                blob_key: &key,
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

/// A unique, human-readable blob key for a photo: `trips/{id}/{ordinal}-{name}`.
/// The ordinal guarantees uniqueness within a trip even when two uploads share a
/// filename; the name is sanitised so it is safe as a path segment.
fn blob_key(trip_id: i64, ordinal: i64, original_name: &str) -> String {
    format!("trips/{trip_id}/{ordinal:04}-{}", sanitize(original_name))
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

/// Run EXIF extraction off the async runtime (US-3, US-4): parsing an
/// untrusted upload's EXIF/TIFF structure is synchronous, potentially
/// expensive work, the same class of risk `put_blob` offloads for the same
/// reason (ADR-0004). Returns the bytes back so the caller can still move
/// them into the `BlobStore` afterward without a copy.
async fn extract_photo_metadata(bytes: Vec<u8>) -> (Vec<u8>, location::PhotoMetadata) {
    tokio::task::spawn_blocking(move || {
        let metadata = location::extract_photo_metadata(&bytes);
        (bytes, metadata)
    })
    .await
    .expect("EXIF extraction task panicked")
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
//
// The DB and the BlobStore are internal collaborators, so both are real: a temp
// SQLite file and a `LocalDisk` rooted at a `tempdir`. The GPS/interpolation/
// none placement *decision* itself (priority, edge cases) is unit-tested
// directly and exhaustively in `placement.rs`, without a database — the tests
// here focus on the storage/DB-association mechanics `ingest_photos` owns,
// plus a couple of thin end-to-end checks that it wires `resolve_placement`'s
// output through correctly.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ActivityType;
    use crate::server::db::testing::TestDb;
    use crate::server::gpx::TrackStats;
    use crate::server::repo::{insert_trip, list_photos};
    use crate::server::storage::LocalDisk;

    fn no_track_ctx() -> TripPhotoContext<'static> {
        TripPhotoContext {
            timed_points: &[],
            tz_name: None,
        }
    }

    fn test_store() -> (Arc<dyn BlobStore>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store: Arc<dyn BlobStore> = Arc::new(LocalDisk::new(dir.path().join("blobs")));
        (store, dir)
    }

    fn photo(name: &str, bytes: &[u8]) -> UploadedPhoto {
        UploadedPhoto {
            original_name: name.to_string(),
            content_type: Some("image/jpeg".to_string()),
            bytes: bytes.to_vec(),
        }
    }

    async fn a_trip(pool: &sqlx::SqlitePool) -> i64 {
        let stats = TrackStats {
            distance_m: 1.0,
            ascent_m: 0.0,
            descent_m: 0.0,
            duration_secs: None,
            start_time: None,
            end_time: None,
            min_lat: 0.0,
            min_lon: 0.0,
            max_lat: 0.0,
            max_lon: 0.0,
        };
        insert_trip(
            pool,
            "Trip",
            ActivityType::Hiking,
            "Europe/Oslo",
            &stats,
            "{}",
            b"x",
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn us2_ingest_stores_blobs_and_associates_rows() {
        let db = TestDb::new().await;
        let (store, _dir) = test_store();
        let trip_id = a_trip(&db.pool).await;

        let mut tx = db.pool.begin().await.unwrap();
        let ids = ingest_photos(
            &mut tx,
            &store,
            trip_id,
            &no_track_ctx(),
            vec![photo("a.jpg", b"AAA"), photo("b.jpg", b"BBBB")],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(ids.len(), 2);

        let listed = list_photos(&db.pool, trip_id).await.unwrap();
        assert_eq!(listed.len(), 2);
        // "stored": the bytes are retrievable from the BlobStore under the key.
        assert_eq!(store.get(&listed[0].blob_key).unwrap(), b"AAA");
        assert_eq!(store.get(&listed[1].blob_key).unwrap(), b"BBBB");
        assert_eq!(listed[0].byte_len, 3);
    }

    #[tokio::test]
    async fn us2_photos_added_later_do_not_collide_with_earlier_keys() {
        let db = TestDb::new().await;
        let (store, _dir) = test_store();
        let trip_id = a_trip(&db.pool).await;

        for batch in [b"first".as_slice(), b"second", b"third"] {
            let mut tx = db.pool.begin().await.unwrap();
            ingest_photos(
                &mut tx,
                &store,
                trip_id,
                &no_track_ctx(),
                vec![photo("p.jpg", batch)],
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }

        let listed = list_photos(&db.pool, trip_id).await.unwrap();
        assert_eq!(listed.len(), 3);
        let keys: std::collections::HashSet<_> = listed.iter().map(|p| &p.blob_key).collect();
        assert_eq!(keys.len(), 3, "every photo must get a distinct blob key");
        // Each blob holds its own batch's bytes (no overwrite).
        assert_eq!(store.get(&listed[2].blob_key).unwrap(), b"third");
    }

    // ── Thin end-to-end checks: ingest_photos wires resolve_placement's
    // output through to storage/DB correctly (US-3/US-4 exhaustive coverage
    // of the decision itself lives in placement.rs) ──────────────────────

    #[tokio::test]
    async fn us3_ingest_stores_the_decided_location_source_and_coordinates() {
        let db = TestDb::new().await;
        let (store, _dir) = test_store();
        let trip_id = a_trip(&db.pool).await;
        let geotagged = location::fixtures::geotagged_bytes(45.5, 10.26);

        let mut tx = db.pool.begin().await.unwrap();
        ingest_photos(
            &mut tx,
            &store,
            trip_id,
            &no_track_ctx(),
            vec![photo("a.jpg", &geotagged)],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let listed = list_photos(&db.pool, trip_id).await.unwrap();
        assert_eq!(listed[0].location_source, LocationSource::Exif);
        assert_eq!(listed[0].lat, Some(45.5));
        assert_eq!(listed[0].lon, Some(10.26));
    }

    #[tokio::test]
    async fn us3_ingest_does_not_fail_when_photo_bytes_are_not_a_valid_image() {
        // Regression guard: EXIF extraction must never turn a non-image byte
        // string (as every other photo test in this suite uploads) into a
        // failed import.
        let db = TestDb::new().await;
        let (store, _dir) = test_store();
        let trip_id = a_trip(&db.pool).await;

        let mut tx = db.pool.begin().await.unwrap();
        let result = ingest_photos(
            &mut tx,
            &store,
            trip_id,
            &no_track_ctx(),
            vec![photo("a.jpg", b"\xFF\xD8\xFF-fake-jpeg")],
        )
        .await;
        assert!(result.is_ok());
    }

    #[test]
    fn blob_key_sanitises_and_disambiguates() {
        assert_eq!(
            blob_key(7, 3, "my photo!.jpg"),
            "trips/7/0003-my_photo_.jpg"
        );
        // Path separators in the name can never escape the trip's directory.
        assert_eq!(
            blob_key(7, 0, "../../etc/passwd"),
            "trips/7/0000-.._.._etc_passwd"
        );
    }
}
