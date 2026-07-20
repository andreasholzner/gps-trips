//! Trip deletion — coordinates the DB delete (which cascades to `track` and
//! `photo` rows) with removing the trip's photo blobs, and their generated
//! thumbnails (US-5), from the `BlobStore` (US-9). The DB delete itself
//! (`repo::delete_trip`) also marks a Komoot-sourced trip's link row
//! `delete_pending` (US-24, ADR-0021) rather than dropping it, so a later
//! "Sync now" push phase can delete it on Komoot too. Kept separate from
//! `repo.rs` (DB-only) and `photos.rs` (ingestion), mirroring how
//! `import.rs`/`photos.rs` sit alongside `repo.rs` rather than folding every
//! concern into one file.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::server::{error::AppError, repo, storage::BlobStore};

/// Delete a trip and its photo blobs, including generated thumbnails (US-5).
/// Returns `false` if no such trip existed (nothing was deleted, no blobs
/// touched); `true` otherwise.
///
/// Ordering: the trip row is deleted *first* (cascading to `track`/`photo`),
/// then each former photo's blob (and thumbnail blob, if it has one) is
/// best-effort removed from the `BlobStore`. The DB delete is the atomicity
/// boundary — once it commits, the DB has no dangling references, which is
/// what the rest of the app depends on. Blob removal afterwards is filesystem
/// I/O with no transactional relationship to the DB; if a blob delete fails
/// it is logged and skipped rather than failing the whole request — a stray
/// orphaned file is a better failure mode here than a response that looks
/// like the delete didn't happen when the trip is, in fact, already gone.
pub async fn delete_trip(
    pool: &SqlitePool,
    store: &Arc<dyn BlobStore>,
    id: i64,
) -> Result<bool, AppError> {
    let photos = repo::list_photos(pool, id).await?;
    let deleted = repo::delete_trip(pool, id).await?;
    if !deleted {
        return Ok(false);
    }
    for photo in photos {
        delete_blob(store, photo.blob_key).await;
        if let Some(thumbnail_key) = photo.thumbnail_key {
            delete_blob(store, thumbnail_key).await;
        }
    }
    Ok(true)
}

/// Delete one blob off the async runtime (ADR-0004: synchronous `BlobStore`
/// I/O must not block the runtime), logging rather than propagating a
/// failure — see `delete_trip`'s doc comment for why.
async fn delete_blob(store: &Arc<dyn BlobStore>, key: String) {
    let store = Arc::clone(store);
    let result = tokio::task::spawn_blocking(move || store.delete(&key))
        .await
        .expect("blob store task panicked");
    if let Err(e) = result {
        tracing::warn!("failed to delete a photo blob after trip delete: {e}");
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ActivityType, TripKind};
    use crate::server::db::testing::TestDb;
    use crate::server::gpx::TrackStats;
    use crate::server::photos::{ingest_photos, UploadedPhoto};
    use crate::server::placement::TripPhotoContext;
    use crate::server::repo::{get_trip, insert_trip, list_photos};
    use crate::server::storage::LocalDisk;
    use crate::server::thumbnail::fixtures::valid_jpeg_bytes;

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

    async fn a_trip(pool: &SqlitePool) -> i64 {
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
            TripKind::Recorded,
        )
        .await
        .unwrap()
    }

    fn photo(name: &str, bytes: &[u8]) -> UploadedPhoto {
        UploadedPhoto {
            original_name: name.to_string(),
            content_type: Some("image/jpeg".to_string()),
            bytes: bytes.to_vec(),
            known_location: None,
        }
    }

    #[tokio::test]
    async fn us9_delete_trip_removes_db_rows_and_photo_blobs() {
        let db = TestDb::new().await;
        let (store, _dir) = test_store();
        let trip_id = a_trip(&db.pool).await;

        let mut tx = db.pool.begin().await.unwrap();
        ingest_photos(
            &mut tx,
            &store,
            trip_id,
            &no_track_ctx(),
            vec![photo("a.jpg", b"AAA"), photo("b.jpg", b"BBB")],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let blob_keys: Vec<String> = list_photos(&db.pool, trip_id)
            .await
            .unwrap()
            .into_iter()
            .map(|p| p.blob_key)
            .collect();
        assert_eq!(blob_keys.len(), 2);

        let deleted = delete_trip(&db.pool, &store, trip_id).await.unwrap();

        assert!(deleted);
        assert!(get_trip(&db.pool, trip_id).await.unwrap().is_none());
        assert!(list_photos(&db.pool, trip_id).await.unwrap().is_empty());
        for key in blob_keys {
            assert!(store.get(&key).is_err(), "blob {key} should be gone");
        }
    }

    #[tokio::test]
    async fn us9_delete_trip_returns_false_for_an_unknown_id_and_touches_no_blobs() {
        let db = TestDb::new().await;
        let (store, _dir) = test_store();
        assert!(!delete_trip(&db.pool, &store, 999).await.unwrap());
    }

    #[tokio::test]
    async fn us5_delete_trip_also_removes_a_photos_thumbnail_blob() {
        let db = TestDb::new().await;
        let (store, _dir) = test_store();
        let trip_id = a_trip(&db.pool).await;

        let mut tx = db.pool.begin().await.unwrap();
        ingest_photos(
            &mut tx,
            &store,
            trip_id,
            &no_track_ctx(),
            vec![photo("a.jpg", &valid_jpeg_bytes(20, 10))],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let thumbnail_key = list_photos(&db.pool, trip_id).await.unwrap()[0]
            .thumbnail_key
            .clone()
            .expect("a valid image must yield a thumbnail");

        let deleted = delete_trip(&db.pool, &store, trip_id).await.unwrap();

        assert!(deleted);
        assert!(
            store.get(&thumbnail_key).is_err(),
            "thumbnail blob {thumbnail_key} should be gone"
        );
    }

    #[tokio::test]
    async fn us9_delete_trip_succeeds_even_if_a_blob_is_already_missing_on_disk() {
        let db = TestDb::new().await;
        let (store, dir) = test_store();
        let trip_id = a_trip(&db.pool).await;

        let mut tx = db.pool.begin().await.unwrap();
        ingest_photos(
            &mut tx,
            &store,
            trip_id,
            &no_track_ctx(),
            vec![photo("a.jpg", b"AAA")],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        // Simulate a prior partial failure: the blob is already gone from disk.
        std::fs::remove_file(dir.path().join("blobs/trips/1/0000-a.jpg")).unwrap();

        let deleted = delete_trip(&db.pool, &store, trip_id).await.unwrap();

        assert!(deleted);
        assert!(get_trip(&db.pool, trip_id).await.unwrap().is_none());
    }
}
