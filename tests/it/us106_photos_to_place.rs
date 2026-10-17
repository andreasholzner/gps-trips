//! US-106 — the trip's page marks the photos still to place. Whether a photo
//! needs placing comes with it from the JSON API (ADR-0008), by the
//! "Unplaced photos" filter's own rule; the rule's cases are the repo's
//! tests, the badge is the UI crate's. A share's recipient is never told.

use crate::common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use trip_archive::models::{CreatedShare, LocationSource, PhotoResponse};
use trip_archive::server::location::fixtures::{capture_time_bytes, geotagged_bytes};

/// `SAMPLE_GPX`'s first point.
const START: (f64, f64) = (59.9139, 10.7522);

async fn photos_of(app: &Router, trip: i64) -> Vec<PhotoResponse> {
    let response = common::get(app, &format!("/api/trips/{trip}/photos")).await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_str(&common::body_string(response).await).unwrap()
}

fn named<'a>(photos: &'a [PhotoResponse], name: &str) -> &'a PhotoResponse {
    photos
        .iter()
        .find(|p| p.original_name == name)
        .unwrap_or_else(|| panic!("no {name} in {photos:?}"))
}

/// Taken long after the sample track ended, so US-4 leaves it unplaced.
fn late() -> Vec<u8> {
    capture_time_bytes("2024:06:01 23:00:00", None)
}

#[tokio::test]
async fn us106_each_photo_says_whether_it_needs_placing() {
    let (app, _dir) = common::test_app().await;
    let late = late();
    let at_start = geotagged_bytes(START.0, START.1);
    // 10:00 in Oslo is the track's first point's 08:00 UTC.
    let timed = capture_time_bytes("2024:06:01 10:00:00", None);
    let trip = common::import_sample_with_photos(
        &app,
        &[
            ("late.jpg", &late),
            ("exif.jpg", &at_start),
            ("timed.jpg", &timed),
        ],
    )
    .await;

    let photos = photos_of(&app, trip).await;
    assert!(named(&photos, "late.jpg").needs_placing, "{photos:?}");
    // Near the start, but placed from EXIF or by timestamp: meant there.
    assert!(!named(&photos, "exif.jpg").needs_placing, "{photos:?}");
    let timed = named(&photos, "timed.jpg");
    assert_eq!(timed.location_source, LocationSource::Interpolated);
    assert!(!timed.needs_placing, "{photos:?}");
}

#[tokio::test]
async fn us106_a_photo_placed_by_hand_no_longer_needs_placing() {
    let (app, _dir) = common::test_app().await;
    let late = late();
    let trip = common::import_sample_with_photos(&app, &[("late.jpg", &late)]).await;
    let photo = photos_of(&app, trip).await[0].id;

    let response = common::send(
        &app,
        common::json_request(
            Method::PATCH,
            &format!("/api/trips/{trip}/photos/{photo}"),
            &format!(r#"{{"lat":{},"lon":{}}}"#, START.0, START.1),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // The answer is what the screen shows next, and so is the list.
    let placed: PhotoResponse = serde_json::from_str(&common::body_string(response).await).unwrap();
    assert!(!placed.needs_placing, "{placed:?}");
    assert!(!photos_of(&app, trip).await[0].needs_placing);
}

#[tokio::test]
async fn us106_a_shares_recipient_is_never_told_a_photo_needs_placing() {
    let (app, _dir) = common::test_app().await;
    let late = late();
    let trip = common::import_sample_with_photos(&app, &[("late.jpg", &late)]).await;
    assert!(photos_of(&app, trip).await[0].needs_placing);
    let body = serde_json::json!({ "trip_ids": [trip] }).to_string();
    let response = common::send(
        &app,
        common::json_request(Method::POST, "/api/shares", &body),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created: CreatedShare = serde_json::from_str(&common::body_string(response).await).unwrap();

    let response = common::send_unauthenticated(
        &app,
        Request::builder()
            .uri(format!("/s/{}/api/trips/{trip}/photos", created.token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let photos: Vec<PhotoResponse> =
        serde_json::from_str(&common::body_string(response).await).unwrap();

    assert_eq!(photos.len(), 1);
    assert!(!photos[0].needs_placing, "{photos:?}");
}
