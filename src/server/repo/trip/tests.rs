use super::*;
use crate::models::TripKind;
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
        &NewTrip {
            name: "Oslo Hills Walk",
            activity_type: ActivityType::Hiking,
            tz_name: "Europe/Oslo",
            stats: &stats,
            geojson: &geojson,
            gpx: SAMPLE_GPX,
            trip_kind: TripKind::Recorded,
        },
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
        &NewTrip {
            name: "Older",
            activity_type: ActivityType::Hiking,
            tz_name: "Europe/Oslo",
            stats: &stats_at(datetime!(2024-01-01 08:00 UTC)),
            geojson: "{}",
            gpx: b"x",
            trip_kind: TripKind::Recorded,
        },
    )
    .await
    .unwrap();
    insert_trip(
        &db.pool,
        &NewTrip {
            name: "Newer",
            activity_type: ActivityType::Hiking,
            tz_name: "Europe/Oslo",
            stats: &stats_at(datetime!(2024-06-01 08:00 UTC)),
            geojson: "{}",
            gpx: b"x",
            trip_kind: TripKind::Recorded,
        },
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

// ── US-20: editing a Komoot-sourced trip marks its link row edit_pending ──

async fn edit_pending_flag(pool: &SqlitePool, trip_id: i64) -> Option<bool> {
    sqlx::query_scalar::<_, bool>("SELECT edit_pending FROM trip_komoot_link WHERE trip_id = ?")
        .bind(trip_id)
        .fetch_optional(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn us20_update_trip_sets_edit_pending_on_a_linked_trip() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    let mut tx = db.pool.begin().await.unwrap();
    crate::server::repo::komoot::insert_link_in_tx(&mut tx, id, "123456")
        .await
        .unwrap();
    tx.commit().await.unwrap();

    update_trip(&db.pool, id, Some("Renamed Trip"), None)
        .await
        .unwrap();

    assert_eq!(edit_pending_flag(&db.pool, id).await, Some(true));
}

#[tokio::test]
async fn us20_update_trip_leaves_an_unlinked_trip_without_a_link_row() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;

    update_trip(&db.pool, id, Some("Renamed Trip"), None)
        .await
        .unwrap();

    assert_eq!(edit_pending_flag(&db.pool, id).await, None);
}

// ── US-24: deleting a Komoot-sourced trip marks its link row delete_pending ──

async fn delete_pending_link_row(
    pool: &SqlitePool,
    komoot_tour_id: &str,
) -> Option<(Option<i64>, bool)> {
    sqlx::query_as::<_, (Option<i64>, bool)>(
        "SELECT trip_id, delete_pending FROM trip_komoot_link WHERE komoot_tour_id = ?",
    )
    .bind(komoot_tour_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn us24_delete_trip_marks_a_linked_trips_row_delete_pending_and_orphans_it() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;
    let mut tx = db.pool.begin().await.unwrap();
    crate::server::repo::komoot::insert_link_in_tx(&mut tx, id, "123456")
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let deleted = delete_trip(&db.pool, id).await.unwrap();

    assert!(deleted);
    assert!(get_trip(&db.pool, id).await.unwrap().is_none());
    let (trip_id, delete_pending) = delete_pending_link_row(&db.pool, "123456")
        .await
        .expect("link row must survive the trip delete");
    assert_eq!(trip_id, None, "FK's ON DELETE SET NULL must have fired");
    assert!(delete_pending);
}

#[tokio::test]
async fn us24_delete_trip_leaves_no_link_row_for_an_unlinked_trip() {
    let db = TestDb::new().await;
    let id = insert_sample_trip(&db.pool).await;

    let deleted = delete_trip(&db.pool, id).await.unwrap();

    assert!(deleted);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trip_komoot_link")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
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

// ── US-32: distinguish recorded from planned trips ───────────────────────

#[tokio::test]
async fn us32_inserted_trip_defaults_to_recorded_kind() {
    let db = TestDb::new().await;
    insert_sample_trip(&db.pool).await;
    let trips = list_trips(&db.pool, &TripFilter::default()).await.unwrap();
    assert_eq!(trips[0].trip_kind, TripKind::Recorded);
}

#[tokio::test]
async fn us32_a_pre_existing_row_without_an_explicit_trip_kind_reads_back_as_recorded() {
    // Simulates a trip inserted before this migration existed: bypass
    // `insert_trip` and rely purely on the column's DB-level DEFAULT, the
    // same mechanism that backfills every row already in the database when
    // migration 0009 runs.
    let db = TestDb::new().await;
    sqlx::query(
        r#"INSERT INTO trip (name, activity_type, tz_name, distance_m, created_at)
           VALUES ('Legacy Trip', 'hiking', 'Europe/Oslo', 1000.0, '2024-01-01T00:00:00Z')"#,
    )
    .execute(&db.pool)
    .await
    .unwrap();

    let trips = list_trips(&db.pool, &TripFilter::default()).await.unwrap();
    assert_eq!(trips[0].trip_kind, TripKind::Recorded);
}

#[tokio::test]
async fn us32_list_trips_filters_by_kind() {
    let db = TestDb::new().await;
    insert_sample_trip(&db.pool).await;
    sqlx::query(
        r#"INSERT INTO trip (name, activity_type, tz_name, distance_m, created_at, trip_kind)
           VALUES ('Planned Trip', 'hiking', 'Europe/Oslo', 1000.0, '2024-01-01T00:00:00Z', 'planned')"#,
    )
    .execute(&db.pool)
    .await
    .unwrap();

    let recorded = list_trips(
        &db.pool,
        &TripFilter {
            trip_kind: Some(TripKind::Recorded),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].name, "Oslo Hills Walk");

    let planned = list_trips(
        &db.pool,
        &TripFilter {
            trip_kind: Some(TripKind::Planned),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].name, "Planned Trip");

    let all = list_trips(&db.pool, &TripFilter::default()).await.unwrap();
    assert_eq!(all.len(), 2, "no kind filter must return both");
}

// ── US-31: the owner chooses recorded vs. planned at import time ─────────

#[tokio::test]
async fn us31_insert_trip_persists_the_chosen_planned_kind() {
    let db = TestDb::new().await;
    let track = parse_gpx(SAMPLE_GPX).unwrap();
    let stats = compute_stats(&track.points);
    let geojson = build_track_geojson(&track.points);
    insert_trip(
        &db.pool,
        &NewTrip {
            name: "Future Trip",
            activity_type: ActivityType::Hiking,
            tz_name: "Europe/Oslo",
            stats: &stats,
            geojson: &geojson,
            gpx: SAMPLE_GPX,
            trip_kind: TripKind::Planned,
        },
    )
    .await
    .expect("insert_trip");

    let trips = list_trips(&db.pool, &TripFilter::default()).await.unwrap();
    assert_eq!(trips[0].trip_kind, TripKind::Planned);
}
