use super::*;
use crate::server::db::testing::TestDb;
use crate::server::geojson::build_track_geojson;
use crate::server::gpx::{compute_stats, parse_gpx};
use time::macros::datetime;

const SAMPLE_GPX: &[u8] = include_bytes!("../../../../tests/fixtures/sample.gpx");

async fn insert_sample_trip(pool: &SqlitePool) -> i64 {
    let track = parse_gpx(SAMPLE_GPX).unwrap();
    let stats = compute_stats(&track.points);
    let geojson = build_track_geojson(&track.points);
    insert_trip(
        pool,
        "Oslo Hills Walk",
        ActivityType::Hiking,
        "Europe/Oslo",
        &stats,
        &geojson,
        SAMPLE_GPX,
    )
    .await
    .expect("insert_trip")
}

/// Minimal stats with a chosen start time, for ordering tests.
fn stats_at(start: OffsetDateTime) -> TrackStats {
    stats(1_000.0, start)
}

/// Minimal stats with a chosen distance and start time, for filter tests
/// (US-13) that need trips with distinct `distance_m`/`start_time` values —
/// not practical to get from a fixed GPX fixture.
fn stats(distance_m: f64, start: OffsetDateTime) -> TrackStats {
    TrackStats {
        distance_m,
        ascent_m: 10.0,
        descent_m: 5.0,
        duration_secs: Some(600),
        start_time: Some(start),
        end_time: Some(start),
        min_lat: 0.0,
        min_lon: 0.0,
        max_lat: 0.0,
        max_lon: 0.0,
    }
}

#[tokio::test]
async fn us1_insert_trip_returns_positive_id() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    assert!(id > 0);
}

#[tokio::test]
async fn us1_inserted_trip_can_be_retrieved_by_id() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    let detail = get_trip(&db.pool, id).await.unwrap().expect("trip exists");
    assert_eq!(detail.id, id);
    assert_eq!(detail.name, "Oslo Hills Walk");
    assert_eq!(detail.activity_type, ActivityType::Hiking);
}

#[tokio::test]
async fn us1_inserted_trip_stores_derived_stats() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    let detail = get_trip(&db.pool, id).await.unwrap().unwrap();
    assert!(
        detail.distance_m > 1_000.0 && detail.distance_m < 2_500.0,
        "distance {:.1} m out of expected range",
        detail.distance_m
    );
}

#[tokio::test]
async fn us1_inserted_trip_stores_track_geojson_blob() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    let geojson: String = sqlx::query_scalar("SELECT geojson FROM track WHERE trip_id = ?")
        .bind(id)
        .fetch_one(&db.pool)
        .await
        .expect("track row exists");
    let parsed: serde_json::Value = serde_json::from_str(&geojson).unwrap();
    assert_eq!(parsed["geometry"]["type"], "LineString");
}

#[tokio::test]
async fn us21_stores_and_returns_original_gpx_verbatim() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    let download = get_original_gpx(&db.pool, id)
        .await
        .unwrap()
        .expect("original GPX exists");
    assert_eq!(download.name, "Oslo Hills Walk");
    assert_eq!(
        download.bytes, SAMPLE_GPX,
        "stored GPX must match the uploaded bytes exactly"
    );
}

#[tokio::test]
async fn us21_original_gpx_is_none_for_unknown_trip() {
    let db = TestDb::new().await;
    assert!(get_original_gpx(&db.pool, 999).await.unwrap().is_none());
}

#[tokio::test]
async fn us1_deleting_trip_cascades_to_track() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    sqlx::query("DELETE FROM trip WHERE id = ?")
        .bind(id)
        .execute(&db.pool)
        .await
        .unwrap();
    let remaining: Option<String> =
        sqlx::query_scalar("SELECT geojson FROM track WHERE trip_id = ?")
            .bind(id)
            .fetch_optional(&db.pool)
            .await
            .unwrap();
    assert!(
        remaining.is_none(),
        "cascade delete should remove the track row"
    );
}

// ── US-7: relive a trip (track geometry for the map + elevation chart) ────

#[tokio::test]
async fn us7_get_track_geojson_returns_stored_blob() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    let geojson = get_track_geojson(&db.pool, id)
        .await
        .unwrap()
        .expect("track geometry exists");
    let parsed: serde_json::Value = serde_json::from_str(&geojson).unwrap();
    assert_eq!(parsed["geometry"]["type"], "LineString");
}

#[tokio::test]
async fn us7_get_track_geojson_is_none_for_unknown_trip() {
    let db = TestDb::new().await;
    assert!(get_track_geojson(&db.pool, 999).await.unwrap().is_none());
}

// ── US-6: browse the trip list ───────────────────────────────────────────

#[tokio::test]
async fn us6_list_trips_is_empty_for_a_new_db() {
    let db = TestDb::new().await;
    assert!(list_trips(&db.pool, &TripFilter::default())
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn us6_list_trips_returns_summary_fields() {
    let db = TestDb::new().await;
    insert_sample_trip(&db.pool).await;
    let trips = list_trips(&db.pool, &TripFilter::default()).await.unwrap();
    assert_eq!(trips.len(), 1);
    let t = &trips[0];
    assert_eq!(t.name, "Oslo Hills Walk");
    assert_eq!(t.activity_type, ActivityType::Hiking);
    assert!(t.distance_m > 1_000.0);
    assert_eq!(t.ascent_m, Some(40.0));
    assert_eq!(t.duration_secs, Some(3600));
    assert!(t.start_time.as_deref().unwrap().starts_with("2024-06-01"));
}

#[tokio::test]
async fn us6_list_trips_orders_most_recent_first() {
    let db = TestDb::new().await;
    insert_trip(
        &db.pool,
        "Older",
        ActivityType::Hiking,
        "Europe/Oslo",
        &stats_at(datetime!(2024-01-01 08:00 UTC)),
        "{}",
        b"x",
    )
    .await
    .unwrap();
    insert_trip(
        &db.pool,
        "Newer",
        ActivityType::Hiking,
        "Europe/Oslo",
        &stats_at(datetime!(2024-06-01 08:00 UTC)),
        "{}",
        b"x",
    )
    .await
    .unwrap();

    let trips = list_trips(&db.pool, &TripFilter::default()).await.unwrap();
    assert_eq!(trips[0].name, "Newer");
    assert_eq!(trips[1].name, "Older");
}

// ── US-9: delete a trip (and its files) ──────────────────────────────

#[tokio::test]
async fn us9_delete_trip_removes_the_row() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    delete_trip(&db.pool, id).await.unwrap();
    assert!(get_trip(&db.pool, id).await.unwrap().is_none());
}

#[tokio::test]
async fn us9_delete_trip_returns_true_when_a_row_was_removed() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    assert!(delete_trip(&db.pool, id).await.unwrap());
}

#[tokio::test]
async fn us9_delete_trip_returns_false_for_an_unknown_id() {
    let db = TestDb::new().await;
    assert!(!delete_trip(&db.pool, 999).await.unwrap());
}

#[tokio::test]
async fn us9_delete_trip_via_the_repo_function_cascades_to_track_and_photos() {
    use super::super::photo::{insert_photo, list_photos, NewPhoto};
    use crate::models::LocationSource;

    let db = TestDb::new().await;
    let trip_id = insert_sample_trip(&db.pool).await;
    let mut tx = db.pool.begin().await.unwrap();
    insert_photo(
        &mut tx,
        trip_id,
        &NewPhoto {
            original_name: "a.jpg",
            content_type: Some("image/jpeg"),
            byte_len: 10,
            blob_key: "trips/1/0000-a.jpg",
            thumbnail_key: None,
            lat: None,
            lon: None,
            location_source: LocationSource::None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    delete_trip(&db.pool, trip_id).await.unwrap();

    let track: Option<String> = sqlx::query_scalar("SELECT geojson FROM track WHERE trip_id = ?")
        .bind(trip_id)
        .fetch_optional(&db.pool)
        .await
        .unwrap();
    assert!(
        track.is_none(),
        "cascade delete should remove the track row"
    );
    assert!(list_photos(&db.pool, trip_id).await.unwrap().is_empty());
}

// ── US-15: edit a trip's name and activity type ──────────────────────────

#[tokio::test]
async fn us15_update_trip_persists_name_and_activity_type() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    let updated = update_trip(
        &db.pool,
        id,
        Some("Renamed Trip"),
        Some(ActivityType::Cycling),
    )
    .await
    .unwrap();
    assert!(updated);
    let detail = get_trip(&db.pool, id).await.unwrap().unwrap();
    assert_eq!(detail.name, "Renamed Trip");
    assert_eq!(detail.activity_type, ActivityType::Cycling);
}

#[tokio::test]
async fn us15_update_trip_with_name_only_leaves_activity_type_unchanged() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    update_trip(&db.pool, id, Some("Renamed Trip"), None)
        .await
        .unwrap();
    let detail = get_trip(&db.pool, id).await.unwrap().unwrap();
    assert_eq!(detail.name, "Renamed Trip");
    assert_eq!(detail.activity_type, ActivityType::Hiking);
}

#[tokio::test]
async fn us15_update_trip_with_activity_type_only_leaves_name_unchanged() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    update_trip(&db.pool, id, None, Some(ActivityType::Cycling))
        .await
        .unwrap();
    let detail = get_trip(&db.pool, id).await.unwrap().unwrap();
    assert_eq!(detail.name, "Oslo Hills Walk");
    assert_eq!(detail.activity_type, ActivityType::Cycling);
}

#[tokio::test]
async fn us15_update_trip_on_an_unknown_id_returns_false() {
    let db = TestDb::new().await;
    let updated = update_trip(&db.pool, 999, Some("Whatever"), Some(ActivityType::Hiking))
        .await
        .unwrap();
    assert!(!updated);
}

#[tokio::test]
async fn us6_list_trips_does_not_require_track_geometry() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    // Remove the geometry; the list must still work (it reads only `trip`).
    sqlx::query("DELETE FROM track WHERE trip_id = ?")
        .bind(id)
        .execute(&db.pool)
        .await
        .unwrap();
    let trips = list_trips(&db.pool, &TripFilter::default()).await.unwrap();
    assert_eq!(trips.len(), 1, "list must not depend on the track row");
}

// US-13 (filter the trip list) tests split into tests/filter.rs to keep this
// file under the repo's 500-line cap.
mod filter;
