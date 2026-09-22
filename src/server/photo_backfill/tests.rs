//! Against a real SQLite file and a `LocalDisk` in a `tempdir` (ADR-0012):
//! the backfill reads what is actually stored, so that is what it is given.

use std::sync::Arc;

use super::*;
use crate::models::{ActivityType, LocationSource, TripKind};
use crate::server::db::testing::TestDb;
use crate::server::gpx::TrackStats;
use crate::server::location::fixtures::capture_time_bytes;
use crate::server::repo::{insert_photo, insert_trip, list_photos, NewPhoto, NewTrip};
use crate::server::storage::LocalDisk;

/// A trip in Oslo with no timed track: a photo's wall clock is read in the
/// trip's zone, as US-4 reads it when the track cannot answer.
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
        &NewTrip {
            name: "Trip",
            activity_type: ActivityType::Hiking,
            tz_name: "Europe/Oslo",
            stats: &stats,
            geojson: "{}",
            gpx: b"x",
            trip_kind: TripKind::Recorded,
        },
    )
    .await
    .unwrap()
}

/// A photo stored the way one was before US-62: its bytes in the store, and a
/// row with no capture time.
async fn a_stored_photo(
    pool: &sqlx::SqlitePool,
    store: &Arc<dyn BlobStore>,
    trip_id: i64,
    key: &str,
    bytes: Option<&[u8]>,
) {
    if let Some(bytes) = bytes {
        store.put(key, bytes).unwrap();
    }
    let mut tx = pool.begin().await.unwrap();
    insert_photo(
        &mut tx,
        trip_id,
        &NewPhoto {
            original_name: key,
            content_type: Some("image/jpeg"),
            byte_len: 1,
            blob_key: key,
            thumbnail_key: None,
            lat: None,
            lon: None,
            location_source: LocationSource::None,
            taken_at: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

fn test_store() -> (Arc<dyn BlobStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let store: Arc<dyn BlobStore> = Arc::new(LocalDisk::new(dir.path().join("blobs")));
    (store, dir)
}

#[tokio::test]
async fn a_stored_photo_gets_back_the_capture_time_its_exif_names() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    let trip_id = a_trip(&db.pool).await;
    let bytes = capture_time_bytes("2024:06:01 10:15:00", None);
    a_stored_photo(&db.pool, &store, trip_id, "morning.jpg", Some(&bytes)).await;

    let summary = backfill_taken_at(&db.pool, &store).await.unwrap();

    assert_eq!(summary.filled, 1);
    let photos = list_photos(&db.pool, trip_id).await.unwrap();
    assert_eq!(
        photos[0].taken_at.as_deref(),
        Some("2024-06-01T08:15:00Z"),
        "10:15 in Oslo in June, stored as UTC"
    );
}

#[tokio::test]
async fn a_photo_whose_exif_names_no_time_is_left_without_one() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    let trip_id = a_trip(&db.pool).await;
    a_stored_photo(&db.pool, &store, trip_id, "plain.jpg", Some(b"no exif")).await;

    let summary = backfill_taken_at(&db.pool, &store).await.unwrap();

    assert_eq!((summary.filled, summary.without_time), (0, 1));
    let photos = list_photos(&db.pool, trip_id).await.unwrap();
    assert_eq!(photos[0].taken_at, None);
}

#[tokio::test]
async fn a_photo_whose_bytes_are_gone_is_reported_and_the_rest_still_filled() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    let trip_id = a_trip(&db.pool).await;
    a_stored_photo(&db.pool, &store, trip_id, "gone.jpg", None).await;
    let bytes = capture_time_bytes("2024:06:01 10:15:00", None);
    a_stored_photo(&db.pool, &store, trip_id, "here.jpg", Some(&bytes)).await;

    let summary = backfill_taken_at(&db.pool, &store).await.unwrap();

    assert_eq!(summary.filled, 1);
    assert_eq!(summary.unreadable.len(), 1);
    assert_eq!(summary.unreadable[0].0, "gone.jpg");
}

#[tokio::test]
async fn a_second_run_leaves_what_the_first_filled_alone() {
    // Safe to rerun after an interruption: only photos still without a
    // capture time are read.
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    let trip_id = a_trip(&db.pool).await;
    let bytes = capture_time_bytes("2024:06:01 10:15:00", None);
    a_stored_photo(&db.pool, &store, trip_id, "morning.jpg", Some(&bytes)).await;
    backfill_taken_at(&db.pool, &store).await.unwrap();

    let again = backfill_taken_at(&db.pool, &store).await.unwrap();

    assert_eq!((again.filled, again.without_time), (0, 0));
}
