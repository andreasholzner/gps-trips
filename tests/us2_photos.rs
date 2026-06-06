//! US-2 acceptance tests — "attach photos to a trip".
//!
//! Acceptance criteria (docs/requirements.md):
//!   "Photos uploaded with the import are stored and associated with the trip.
//!    Photos can be added to a trip both during the gpx import and at a later
//!    time."
//!
//! Drives the real Axum router in-process against a real temp SQLite DB and a
//! real `LocalDisk` blob store (ADR-0012); there are no externals to mock.

mod common;

use axum::http::StatusCode;
use common::{
    add_photos_request, body_string, get, import_sample, import_sample_with_photos, send, test_app,
};
use serde_json::Value;

// Arbitrary bytes — US-2 only stores and associates photos; it does not decode
// them (that is US-3/US-5), so any distinct payloads will do.
const PHOTO_A: &[u8] = b"\xFF\xD8\xFF-pretend-jpeg-A";
const PHOTO_B: &[u8] = b"\xFF\xD8\xFF-pretend-jpeg-bytes-B";

/// GET the trip's photos JSON, asserting 200, and return the array.
async fn photos_json(app: &axum::Router, trip_id: i64) -> Vec<Value> {
    let response = get(app, &format!("/api/trips/{trip_id}/photos")).await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_str(&body_string(response).await).expect("photos JSON array")
}

// ── Acceptance: photos uploaded with the import are stored and associated ─────

#[tokio::test]
async fn us2_import_with_photos_stores_and_associates_them() {
    let (app, _dir) = test_app().await;
    let id = import_sample_with_photos(&app, &[("a.jpg", PHOTO_A), ("b.jpg", PHOTO_B)]).await;

    let photos = photos_json(&app, id).await;
    assert_eq!(photos.len(), 2, "both uploaded photos should be associated");

    let names: Vec<&str> = photos
        .iter()
        .map(|p| p["original_name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"a.jpg") && names.contains(&"b.jpg"));

    // Stored faithfully: the recorded size matches the uploaded bytes.
    let a = photos
        .iter()
        .find(|p| p["original_name"] == "a.jpg")
        .unwrap();
    assert_eq!(a["byte_len"].as_i64().unwrap(), PHOTO_A.len() as i64);
    assert_eq!(a["trip_id"].as_i64().unwrap(), id);
}

#[tokio::test]
async fn us2_import_without_photos_creates_a_trip_with_no_photos() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;
    assert!(
        photos_json(&app, id).await.is_empty(),
        "an import with no photos leaves the trip with none"
    );
}

// ── Acceptance: photos can be added at a later time ──────────────────────────

#[tokio::test]
async fn us2_photos_can_be_added_after_import() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send(&app, add_photos_request(id, &[("later.jpg", PHOTO_A)])).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers().get("location").unwrap(),
        &format!("/trips/{id}")
    );

    let photos = photos_json(&app, id).await;
    assert_eq!(photos.len(), 1);
    assert_eq!(photos[0]["original_name"], "later.jpg");
}

#[tokio::test]
async fn us2_photos_added_later_accumulate_with_imported_ones() {
    let (app, _dir) = test_app().await;
    let id = import_sample_with_photos(&app, &[("a.jpg", PHOTO_A)]).await;

    send(&app, add_photos_request(id, &[("b.jpg", PHOTO_B)])).await;

    assert_eq!(photos_json(&app, id).await.len(), 2);
}

// ── Edge cases ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn us2_adding_photos_to_an_unknown_trip_returns_404() {
    let (app, _dir) = test_app().await;
    let response = send(&app, add_photos_request(999, &[("a.jpg", PHOTO_A)])).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us2_listing_photos_for_an_unknown_trip_returns_404() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/api/trips/999/photos").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us2_detail_page_offers_an_add_photos_form() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let html = body_string(get(&app, &format!("/trips/{id}")).await).await;
    assert!(
        html.contains(&format!("action=\"/api/trips/{id}/photos\"")),
        "detail page should offer an add-photos form; got: {html}"
    );
}
