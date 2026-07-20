// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
// Split into komoot_sync/tests.rs to keep the parent module under the repo's
// 500-line cap (mirrors repo/trip.rs -> repo/trip/tests.rs).

use super::*;
use crate::models::{ActivityType, LocationSource, TripKind};
use crate::server::db::testing::TestDb;
use crate::server::gpx;
use crate::server::komoot::{KomootLocation, KomootPhoto};
use crate::server::repo::list_photos;
use crate::server::storage::LocalDisk;
use crate::server::thumbnail::fixtures::{valid_jpeg_bytes, valid_png_bytes};
use std::sync::Mutex;

const SAMPLE_GPX: &[u8] = include_bytes!("../../../tests/fixtures/sample.gpx");

mod mock;
use mock::{a_tour, test_store, MockKomootClient};

// ── list_sync_candidates: anti-join dedup ──────────────────────────────

#[tokio::test]
async fn list_sync_candidates_excludes_already_linked_tours() {
    let db = TestDb::new().await;

    // A real trip to link against (the FK requires an existing trip).
    let trip_id = crate::server::repo::insert_trip(
        &db.pool,
        "Existing",
        ActivityType::Hiking,
        "UTC",
        &gpx::compute_stats(&[]),
        "{}",
        b"x",
        TripKind::Recorded,
    )
    .await
    .unwrap();
    let mut tx = db.pool.begin().await.unwrap();
    repo::komoot::insert_link_in_tx(&mut tx, trip_id, "111")
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![
            a_tour("111", "Already synced", "hike"),
            a_tour("222", "New", "mtb"),
        ],
        ..Default::default()
    });

    let candidates = list_sync_candidates(&db.pool, client).await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].tour_id, "222");
}

// ── sync_selected_tours: happy path ────────────────────────────────────

#[tokio::test]
async fn sync_selected_tours_imports_gpx_and_photos_in_one_transaction_with_the_link_row() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();

    let photo_with_location = KomootPhoto {
        id: "p1".to_string(),
        src: "https://cdn.example/p1?width={width}&height={height}&crop={crop}".to_string(),
        location: Some(KomootLocation {
            lat: 69.7,
            lng: 18.9,
        }),
        width_px: 20,
        height_px: 10,
    };
    let photo_without_location = KomootPhoto {
        id: "p2".to_string(),
        src: "https://cdn.example/p2?width={width}&height={height}&crop={crop}".to_string(),
        location: None,
        width_px: 20,
        height_px: 10,
    };
    let resolved_p1 =
        crate::server::komoot::resolve_photo_url(&photo_with_location.src, 20, 10, false);
    let resolved_p2 =
        crate::server::komoot::resolve_photo_url(&photo_without_location.src, 20, 10, false);

    let mut gpx = HashMap::new();
    gpx.insert("999".to_string(), SAMPLE_GPX.to_vec());
    let mut photos = HashMap::new();
    photos.insert(
        "999".to_string(),
        vec![photo_with_location, photo_without_location],
    );
    let mut photo_bytes = HashMap::new();
    photo_bytes.insert(resolved_p1, valid_jpeg_bytes(20, 10));
    photo_bytes.insert(resolved_p2, valid_jpeg_bytes(20, 10));

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![a_tour("999", "Mountain Loop", "mtb")],
        gpx,
        photos,
        photo_bytes,
        ..Default::default()
    });

    let summary = sync_selected_tours(&db.pool, &store, client, &["999".to_string()])
        .await
        .unwrap();

    assert!(summary.failed.is_none());
    assert_eq!(summary.imported.len(), 1);
    let (tour_id, trip_id) = &summary.imported[0];
    assert_eq!(tour_id, "999");

    let trip = repo::get_trip(&db.pool, *trip_id).await.unwrap().unwrap();
    assert_eq!(trip.name, "Mountain Loop");
    assert_eq!(trip.activity_type, ActivityType::Cycling); // mtb -> Cycling

    let linked = repo::komoot::list_linked_tour_ids(&db.pool).await.unwrap();
    assert!(linked.contains("999"));

    let photos = list_photos(&db.pool, *trip_id).await.unwrap();
    assert_eq!(photos.len(), 2);
    let with_loc = photos
        .iter()
        .find(|p| p.original_name == "komoot-p1.jpg")
        .unwrap();
    assert_eq!(with_loc.location_source, LocationSource::Provided);
    assert_eq!(with_loc.lat, Some(69.7));
    assert_eq!(with_loc.lon, Some(18.9));
    let without_loc = photos
        .iter()
        .find(|p| p.original_name == "komoot-p2.jpg")
        .unwrap();
    assert_eq!(without_loc.location_source, LocationSource::None);
}

// ── sync_selected_tours: dedup on the execute step too ─────────────────

#[tokio::test]
async fn sync_selected_tours_skips_a_selected_tour_that_is_already_linked() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();

    let trip_id = crate::server::repo::insert_trip(
        &db.pool,
        "Existing",
        ActivityType::Hiking,
        "UTC",
        &gpx::compute_stats(&[]),
        "{}",
        b"x",
        TripKind::Recorded,
    )
    .await
    .unwrap();
    let mut tx = db.pool.begin().await.unwrap();
    repo::komoot::insert_link_in_tx(&mut tx, trip_id, "111")
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![a_tour("111", "Already synced", "hike")],
        ..Default::default()
    });

    let summary = sync_selected_tours(&db.pool, &store, client, &["111".to_string()])
        .await
        .unwrap();

    assert!(summary.imported.is_empty());
    assert!(summary.failed.is_none());
}

// ── sync_selected_tours: halt on first failure (ADR-0021) ──────────────

#[tokio::test]
async fn sync_selected_tours_halts_after_the_first_failed_tour() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();

    let mut gpx = HashMap::new();
    gpx.insert("222".to_string(), SAMPLE_GPX.to_vec());

    let client_inner = MockKomootClient {
        tours: vec![
            a_tour("111", "Fails", "hike"),
            a_tour("222", "Never attempted", "hike"),
        ],
        gpx,
        fail_gpx_for: HashSet::from(["111".to_string()]),
        ..Default::default()
    };
    let client: Arc<dyn KomootClient> = Arc::new(client_inner);

    let summary = sync_selected_tours(
        &db.pool,
        &store,
        Arc::clone(&client),
        &["111".to_string(), "222".to_string()],
    )
    .await
    .unwrap();

    assert!(summary.imported.is_empty());
    let (failed_tour, _msg) = summary.failed.expect("first tour must fail");
    assert_eq!(failed_tour, "111");

    // The second tour must never have been attempted.
    let candidates = list_sync_candidates(&db.pool, client).await.unwrap();
    assert_eq!(candidates.len(), 2, "neither tour got linked");
}

// ── sync_selected_tours: a repeated tour_id in the request is harmless ──

#[tokio::test]
async fn sync_selected_tours_dedupes_a_repeated_tour_id_in_the_request() {
    // The request body (`POST /api/komoot/sync`) is arbitrary client JSON
    // with no dedup applied at that boundary. A repeat id must not look
    // like "already imported earlier in this same run" and spuriously
    // halt the run.
    let db = TestDb::new().await;
    let (store, _dir) = test_store();

    let mut gpx = HashMap::new();
    gpx.insert("999".to_string(), SAMPLE_GPX.to_vec());

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![a_tour("999", "Mountain Loop", "mtb")],
        gpx,
        ..Default::default()
    });

    let summary = sync_selected_tours(
        &db.pool,
        &store,
        client,
        &["999".to_string(), "999".to_string()],
    )
    .await
    .unwrap();

    assert!(summary.failed.is_none());
    assert_eq!(summary.imported.len(), 1);
}

// ── sync_one_tour: photo format detection (US-22) ───────────────────────

#[tokio::test]
async fn sync_one_tour_names_a_photo_by_its_sniffed_format_not_a_hardcoded_jpg() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();

    let png_photo = KomootPhoto {
        id: "p1".to_string(),
        src: "https://cdn.example/p1?width={width}&height={height}&crop={crop}".to_string(),
        location: None,
        width_px: 20,
        height_px: 10,
    };
    let resolved = crate::server::komoot::resolve_photo_url(&png_photo.src, 20, 10, false);

    let mut gpx = HashMap::new();
    gpx.insert("999".to_string(), SAMPLE_GPX.to_vec());
    let mut photos = HashMap::new();
    photos.insert("999".to_string(), vec![png_photo]);
    let mut photo_bytes = HashMap::new();
    photo_bytes.insert(resolved, valid_png_bytes(20, 10));

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![a_tour("999", "Mountain Loop", "mtb")],
        gpx,
        photos,
        photo_bytes,
        ..Default::default()
    });

    let summary = sync_selected_tours(&db.pool, &store, client, &["999".to_string()])
        .await
        .unwrap();

    let (_, trip_id) = summary.imported.first().expect("tour must import");
    let photos = list_photos(&db.pool, *trip_id).await.unwrap();
    let photo = photos.first().expect("one photo");
    assert_eq!(photo.original_name, "komoot-p1.png");
    assert_eq!(photo.content_type.as_deref(), Some("image/png"));
}

// ── sync_one_tour: the whole tour rolls back atomically (ADR-0021) ──────

#[tokio::test]
async fn sync_one_tour_rolls_back_the_trip_when_a_later_photo_fails_mid_transaction() {
    // "Same transaction" (US-22) must mean a failure anywhere in the
    // pipeline — not just before it starts — leaves no half-imported trip
    // behind.
    let db = TestDb::new().await;
    let (store, _dir) = test_store();

    let photo_ok = KomootPhoto {
        id: "p1".to_string(),
        src: "https://cdn.example/p1?width={width}&height={height}&crop={crop}".to_string(),
        location: None,
        width_px: 20,
        height_px: 10,
    };
    // Deliberately not registered in `photo_bytes` below, so
    // `fetch_photo_bytes` errors for it.
    let photo_fails = KomootPhoto {
        id: "p2".to_string(),
        src: "https://cdn.example/p2?width={width}&height={height}&crop={crop}".to_string(),
        location: None,
        width_px: 20,
        height_px: 10,
    };
    let resolved_ok = crate::server::komoot::resolve_photo_url(&photo_ok.src, 20, 10, false);

    let mut gpx = HashMap::new();
    gpx.insert("999".to_string(), SAMPLE_GPX.to_vec());
    let mut photos = HashMap::new();
    photos.insert("999".to_string(), vec![photo_ok, photo_fails]);
    let mut photo_bytes = HashMap::new();
    photo_bytes.insert(resolved_ok, valid_jpeg_bytes(20, 10));

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![a_tour("999", "Mountain Loop", "mtb")],
        gpx,
        photos,
        photo_bytes,
        ..Default::default()
    });

    let summary = sync_selected_tours(&db.pool, &store, client, &["999".to_string()])
        .await
        .unwrap();

    assert!(summary.imported.is_empty());
    assert!(summary.failed.is_some());

    // No link row, and no orphaned trip row either — the trip insert must
    // have rolled back along with everything else in the transaction.
    let linked = repo::komoot::list_linked_tour_ids(&db.pool).await.unwrap();
    assert!(!linked.contains("999"));
    let trip_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trip")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        trip_count, 0,
        "the transaction must have rolled back the trip insert too"
    );
}

// ── list_all_tour_photos: pagination (US-22) ─────────────────────────────

#[tokio::test]
async fn list_all_tour_photos_pages_through_more_than_one_page() {
    // One page short of the API's page size, plus a few more — forces the
    // loop to fetch a second (short) page rather than stopping after the
    // first full one.
    let total = PAGE_SIZE + 10;
    let photos: Vec<KomootPhoto> = (0..total)
        .map(|i| KomootPhoto {
            id: i.to_string(),
            src: "https://cdn.example/p?width={width}&height={height}&crop={crop}".to_string(),
            location: None,
            width_px: 1,
            height_px: 1,
        })
        .collect();

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        photos: HashMap::from([("999".to_string(), photos)]),
        ..Default::default()
    });

    let all = list_all_tour_photos(&client, "999").await.unwrap();
    assert_eq!(all.len(), total as usize);
}

// ── push_pending_edits (US-20) ────────────────────────────────────────────
// Split out into tests/push.rs to keep this file under the repo's 500-line cap.

mod push;

// ── push_pending_deletes (US-24) ──────────────────────────────────────────
// Split out into tests/delete.rs to keep this file under the repo's 500-line cap.

mod delete;

// ── backfill (US-23) ────────────────────────────────────────────────────
// Split out into tests/backfill.rs to keep this file under the repo's 500-line cap.

mod backfill;
