//! US-66 — quickly find the trips still to tidy up.
//!
//! Acceptance criteria: the trip list can be narrowed to trips without a
//! proper name — one that does not start with a real `yyyy-mm-dd` date
//! followed by a title — and to trips with photos left unplaced
//! (`location_source = none`, US-4). The two filters combine with each other
//! and with every other filter as an AND. Exercised through
//! `GET /api/trips` (ADR-0008), which the SPA's list reads.

use crate::common::{body_string, get, import_request_with_fields, send, test_app, SAMPLE_GPX};
use axum::http::StatusCode;
use trip_archive::server::location::fixtures::capture_time_bytes;

/// Import the sample GPX (Oslo, 2024-06-01 08:00–09:00 UTC) under `name`,
/// with `photos`, and return the new trip's id.
async fn import_named(app: &axum::Router, name: &str, photos: &[(&str, &[u8])]) -> i64 {
    let response = send(
        app,
        import_request_with_fields(SAMPLE_GPX, &[("name", name)], photos),
    )
    .await;
    crate::common::trip_id_from_redirect(&response)
}

/// A photo taken inside the track's time range, which US-4 places.
fn placed_photo() -> Vec<u8> {
    capture_time_bytes("2024:06:01 10:15:00", None)
}

/// A photo taken long after the track ended, which US-4 leaves unplaced.
fn unplaced_photo() -> Vec<u8> {
    capture_time_bytes("2024:06:01 23:00:00", None)
}

async fn listed_ids(app: &axum::Router, uri: &str) -> Vec<i64> {
    let response = get(app, uri).await;
    assert_eq!(response.status(), StatusCode::OK);
    let trips: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    let mut ids: Vec<i64> = trips
        .as_array()
        .expect("JSON array")
        .iter()
        .map(|trip| trip["id"].as_i64().unwrap())
        .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn us66_unnamed_lists_only_trips_without_a_proper_name() {
    let (app, _dir) = test_app().await;
    let proper = import_named(&app, "2024-06-01 Oslo Hills Walk", &[]).await;
    let bare_date = import_named(&app, "2024-06-01", &[]).await;
    let no_date = import_named(&app, "Morning walk", &[]).await;
    let bad_date = import_named(&app, "2024-06-31 Oslo", &[]).await;

    let ids = listed_ids(&app, "/api/trips?unnamed=true").await;
    assert_eq!(ids, vec![bare_date, no_date, bad_date]);
    assert!(!ids.contains(&proper));
}

#[tokio::test]
async fn us66_unplaced_lists_only_trips_with_a_photo_left_unplaced() {
    let (app, _dir) = test_app().await;
    let (placed, unplaced) = (placed_photo(), unplaced_photo());
    import_named(&app, "2024-06-01 No photos", &[]).await;
    import_named(&app, "2024-06-01 Placed", &[("a.jpg", &placed)]).await;
    let mixed = import_named(
        &app,
        "2024-06-01 Mixed",
        &[("a.jpg", &placed), ("b.jpg", &unplaced)],
    )
    .await;

    assert_eq!(
        listed_ids(&app, "/api/trips?unplaced=true").await,
        vec![mixed]
    );
}

#[tokio::test]
async fn us66_both_filters_together_list_trips_matching_both() {
    let (app, _dir) = test_app().await;
    let unplaced = unplaced_photo();
    import_named(&app, "2024-06-01 Named", &[("a.jpg", &unplaced)]).await;
    import_named(&app, "Unnamed, no photos", &[]).await;
    let both = import_named(&app, "Unnamed", &[("a.jpg", &unplaced)]).await;

    assert_eq!(
        listed_ids(&app, "/api/trips?unnamed=true&unplaced=true").await,
        vec![both]
    );
}

#[tokio::test]
async fn us66_the_filters_combine_with_the_others() {
    let (app, _dir) = test_app().await;
    let walk = import_named(&app, "Morning walk", &[]).await;
    import_named(&app, "Evening ride", &[]).await;

    assert_eq!(
        listed_ids(&app, "/api/trips?unnamed=true&q=walk").await,
        vec![walk]
    );
}

#[tokio::test]
async fn us66_an_unrecognized_flag_value_is_rejected() {
    let (app, _dir) = test_app().await;
    for uri in ["/api/trips?unnamed=yes", "/api/trips?unplaced=false"] {
        assert_eq!(
            get(&app, uri).await.status(),
            StatusCode::BAD_REQUEST,
            "{uri}"
        );
    }
}
