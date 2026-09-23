//! US-30 — the owner places an attached photo on the map by hand.
//!
//! Acceptance criteria: a photo is placed at any point the owner selects on
//! the map (`location_source = manual`), whatever its position was before —
//! an automatically determined one (`exif`, `interpolated`, `provided`) is
//! overwritten once the owner has been warned, which is the SPA's part; the
//! API's is to store what it is sent. Exercised through
//! `PATCH /api/trips/:id/photos/:photo_id` (ADR-0008).

use crate::common::{
    body_string, get, import_request_with_fields, import_sample_with_photos, json_request, send,
    send_unauthenticated, test_app, test_app_with_state, trip_id_from_redirect, SAMPLE_GPX,
};
use axum::http::{Method, StatusCode};
use trip_archive::server::location::fixtures::{capture_time_bytes, geotagged_bytes};

fn place_request(trip: i64, photo: i64, body: &str) -> axum::http::Request<axum::body::Body> {
    json_request(
        Method::PATCH,
        &format!("/api/trips/{trip}/photos/{photo}"),
        body,
    )
}

async fn photos_of(app: &axum::Router, trip: i64) -> serde_json::Value {
    let response = get(app, &format!("/api/trips/{trip}/photos")).await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_str(&body_string(response).await).unwrap()
}

/// The id of the trip's only photo.
async fn only_photo(app: &axum::Router, trip: i64) -> i64 {
    photos_of(app, trip).await[0]["id"].as_i64().unwrap()
}

#[tokio::test]
async fn us30_a_geotagged_photo_is_moved_to_where_the_owner_placed_it() {
    let (app, _dir) = test_app().await;
    let trip = import_sample_with_photos(&app, &[("geo.jpg", &geotagged_bytes(45.5, 10.26))]).await;
    let photo = only_photo(&app, trip).await;

    let response = send(
        &app,
        place_request(trip, photo, r#"{"lat":59.95,"lon":10.8}"#),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    // The answer is the photo as the gallery reads it, so the screen can use
    // it without asking again.
    let placed: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(placed["id"], photo);
    assert_eq!(placed["location_source"], "manual");
    assert_eq!(placed["lat"], 59.95);
    assert_eq!(placed["lon"], 10.8);

    let stored = &photos_of(&app, trip).await[0];
    assert_eq!(stored["location_source"], "manual", "got: {stored}");
    assert_eq!(stored["lat"], 59.95);
    assert_eq!(stored["lon"], 10.8);
}

#[tokio::test]
async fn us30_an_unplaced_photo_is_placed_and_its_trip_no_longer_needs_tidying() {
    let (app, _dir) = test_app().await;
    // Taken long after the sample track ended, so US-4 left it unplaced.
    let late = capture_time_bytes("2024:06:01 23:00:00", None);
    let response = send(
        &app,
        import_request_with_fields(
            SAMPLE_GPX,
            &[("name", "2024-06-01 Walk")],
            &[("late.jpg", &late)],
        ),
    )
    .await;
    let trip = trip_id_from_redirect(&response);
    let photo = only_photo(&app, trip).await;
    assert_eq!(photos_of(&app, trip).await[0]["location_source"], "none");

    let response = send(
        &app,
        place_request(trip, photo, r#"{"lat":59.93,"lon":10.72}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // US-66's "Unplaced photos" filter stops listing it.
    let listed = get(&app, "/api/trips?unplaced=true").await;
    let listed: serde_json::Value = serde_json::from_str(&body_string(listed).await).unwrap();
    assert_eq!(listed, serde_json::json!([]), "got: {listed}");
}

#[tokio::test]
async fn us30_a_photo_is_only_placed_through_its_own_trip() {
    let (app, _dir) = test_app().await;
    let geo = geotagged_bytes(45.5, 10.26);
    let trip = import_sample_with_photos(&app, &[("geo.jpg", &geo)]).await;
    let other = import_sample_with_photos(&app, &[]).await;
    let photo = only_photo(&app, trip).await;

    for (trip_id, photo_id) in [(other, photo), (trip, photo + 1_000), (9_999, photo)] {
        let response = send(
            &app,
            place_request(trip_id, photo_id, r#"{"lat":59.95,"lon":10.8}"#),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "trip {trip_id}, photo {photo_id}"
        );
    }
    assert_eq!(photos_of(&app, trip).await[0]["location_source"], "exif");
}

#[tokio::test]
async fn us30_a_position_off_the_globe_is_refused() {
    let (app, _dir) = test_app().await;
    let geo = geotagged_bytes(45.5, 10.26);
    let trip = import_sample_with_photos(&app, &[("geo.jpg", &geo)]).await;
    let photo = only_photo(&app, trip).await;

    for body in [
        r#"{"lat":90.5,"lon":10.0}"#,
        r#"{"lat":-91,"lon":10.0}"#,
        r#"{"lat":45.0,"lon":180.5}"#,
        r#"{"lat":45.0,"lon":-181}"#,
    ] {
        let response = send(&app, place_request(trip, photo, body)).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }
    // A body missing a coordinate is refused by the extractor, before any of
    // the handler runs.
    let response = send(&app, place_request(trip, photo, r#"{"lat":45.0}"#)).await;
    assert!(response.status().is_client_error());
    assert_eq!(photos_of(&app, trip).await[0]["location_source"], "exif");
}

#[tokio::test]
async fn us30_placing_needs_a_session() {
    let (app, _dir) = test_app().await;
    let geo = geotagged_bytes(45.5, 10.26);
    let trip = import_sample_with_photos(&app, &[("geo.jpg", &geo)]).await;
    let photo = only_photo(&app, trip).await;

    let response = send_unauthenticated(
        &app,
        place_request(trip, photo, r#"{"lat":59.95,"lon":10.8}"#),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// US-26: every `PATCH` waits for a running sync.
#[tokio::test]
async fn us30_placing_is_refused_while_a_sync_is_in_flight() {
    let (app, state, _dir) = test_app_with_state(None).await;
    let geo = geotagged_bytes(45.5, 10.26);
    let trip = import_sample_with_photos(&app, &[("geo.jpg", &geo)]).await;
    let photo = only_photo(&app, trip).await;

    state.set_sync_in_progress_for_test(true);
    let response = send(
        &app,
        place_request(trip, photo, r#"{"lat":59.95,"lon":10.8}"#),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(photos_of(&app, trip).await[0]["location_source"], "exif");
}
