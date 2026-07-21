// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
// Split into photos/tests.rs to keep the parent module under the repo's
// 500-line cap (mirrors repo/trip.rs -> repo/trip/tests.rs).
//
// The DB and the BlobStore are internal collaborators, so both are real: a temp
// SQLite file and a `LocalDisk` rooted at a `tempdir`. The GPS/interpolation/
// none placement *decision* itself (priority, edge cases) is unit-tested
// directly and exhaustively in `placement.rs`, without a database — the tests
// here focus on the storage/DB-association mechanics `ingest_photos` owns,
// plus a couple of thin end-to-end checks that it wires `resolve_placement`'s
// output through correctly.

use super::*;
use crate::models::{ActivityType, TripKind};
use crate::server::db::testing::TestDb;
use crate::server::gpx::TrackStats;
use crate::server::repo::{insert_trip, list_photos, NewTrip};
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

fn photo(name: &str, bytes: &[u8]) -> UploadedPhoto {
    UploadedPhoto {
        original_name: name.to_string(),
        content_type: Some("image/jpeg".to_string()),
        bytes: bytes.to_vec(),
        known_location: None,
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

// ── US-22: a known location supplied by an external source (Komoot) ───

#[tokio::test]
async fn us22_ingest_uses_known_location_over_exif_when_present() {
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
        vec![UploadedPhoto {
            known_location: Some((69.7, 18.9)),
            ..photo("a.jpg", &geotagged)
        }],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let listed = list_photos(&db.pool, trip_id).await.unwrap();
    assert_eq!(listed[0].location_source, LocationSource::Provided);
    assert_eq!(listed[0].lat, Some(69.7));
    assert_eq!(listed[0].lon, Some(18.9));
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

// ── US-5: thumbnail generation ────────────────────────────────────────

#[tokio::test]
async fn us5_ingest_generates_and_stores_a_thumbnail_for_a_valid_image() {
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

    let listed = list_photos(&db.pool, trip_id).await.unwrap();
    let thumb_key = listed[0]
        .thumbnail_key
        .as_deref()
        .expect("a valid image must yield a thumbnail");
    assert_ne!(
        thumb_key, listed[0].blob_key,
        "thumbnail is a distinct blob"
    );
    let thumb_bytes = store.get(thumb_key).unwrap();
    image::load_from_memory(&thumb_bytes).expect("stored thumbnail must be decodable");
}

#[tokio::test]
async fn us5_thumbnail_key_is_always_jpg_even_for_a_non_jpeg_original() {
    // Regression guard: the thumbnail is always re-encoded as JPEG
    // (ADR-0020) regardless of source format, so its key must always end
    // in `.jpg` — otherwise `content_type_from_path` (http.rs) would
    // serve JPEG bytes labeled with the original's Content-Type.
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    let trip_id = a_trip(&db.pool).await;

    let mut tx = db.pool.begin().await.unwrap();
    ingest_photos(
        &mut tx,
        &store,
        trip_id,
        &no_track_ctx(),
        vec![photo("a.png", &valid_jpeg_bytes(20, 10))],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let listed = list_photos(&db.pool, trip_id).await.unwrap();
    let thumb_key = listed[0].thumbnail_key.as_deref().unwrap();
    assert!(
        thumb_key.ends_with(".jpg"),
        "thumbnail key must end in .jpg, got {thumb_key}"
    );
}

#[tokio::test]
async fn us5_ingest_leaves_thumbnail_key_none_when_generation_fails() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    let trip_id = a_trip(&db.pool).await;

    let mut tx = db.pool.begin().await.unwrap();
    ingest_photos(
        &mut tx,
        &store,
        trip_id,
        &no_track_ctx(),
        vec![photo("a.jpg", b"\xFF\xD8\xFF-fake-jpeg")],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let listed = list_photos(&db.pool, trip_id).await.unwrap();
    assert_eq!(listed[0].thumbnail_key, None);
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

#[test]
fn thumbnail_key_lives_under_a_thumbs_segment_distinct_from_blob_key() {
    assert_eq!(
        thumbnail_key(7, 3, "my photo!.jpg"),
        "trips/7/thumbs/0003-my_photo_.jpg"
    );
}

#[test]
fn thumbnail_key_replaces_a_non_jpeg_extension_with_jpg() {
    assert_eq!(
        thumbnail_key(7, 3, "my photo!.png"),
        "trips/7/thumbs/0003-my_photo_.jpg"
    );
}

#[test]
fn thumbnail_key_appends_jpg_when_the_original_has_no_extension() {
    assert_eq!(
        thumbnail_key(7, 0, "photo"),
        "trips/7/thumbs/0000-photo.jpg"
    );
}
