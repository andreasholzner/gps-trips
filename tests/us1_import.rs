//! US-1 acceptance tests — "import a GPX file".
//!
//! Acceptance criteria (docs/requirements.md):
//!   "Uploading a valid GPX creates a trip and redirects to its detail page.
//!    Invalid/empty GPX is rejected with a clear error."
//!
//! These drive the real Axum router in-process (ADR-0012), against a real
//! temporary SQLite database (the only mocked thing would be externals — there
//! are none in US-1).

mod common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use common::{body_string, get, import, import_sample, send, test_app, NO_TRACKS_GPX, SAMPLE_GPX};

// ── Acceptance: a valid GPX creates a trip and redirects to its detail page ──

#[tokio::test]
async fn us1_valid_gpx_redirects_to_detail_page() {
    let (app, _dir) = test_app().await;

    let response = import(&app, SAMPLE_GPX).await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .expect("Location header")
        .to_str()
        .unwrap();
    let id: i64 = location
        .strip_prefix("/trips/")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("expected /trips/<id>, got {location}"));
    assert!(id > 0, "trip id must be positive");
}

#[tokio::test]
async fn us1_detail_page_shows_the_imported_trip() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let detail = get(&app, &format!("/trips/{id}")).await;
    assert_eq!(detail.status(), StatusCode::OK);
    let html = body_string(detail).await;
    assert!(
        html.contains("Oslo Hills Walk"),
        "detail page should show the trip name; got: {html}"
    );
}

// ── Acceptance: invalid/empty GPX is rejected with a clear error ─────────────

#[tokio::test]
async fn us1_missing_gpx_field_is_rejected_with_400() {
    let (app, _dir) = test_app().await;

    // Valid multipart, but no `gpx` field.
    let boundary = "TripArchiveTestBoundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nMy Trip\r\n--{boundary}--\r\n"
    );
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/import")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();

    let response = send(&app, request).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(body_string(response).await.contains("gpx"));
}

#[tokio::test]
async fn us1_gpx_without_tracks_is_rejected_with_422() {
    let (app, _dir) = test_app().await;
    let response = import(&app, NO_TRACKS_GPX).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body_string(response).await.to_lowercase().contains("track"));
}

#[tokio::test]
async fn us1_invalid_xml_is_rejected_with_422() {
    let (app, _dir) = test_app().await;
    let response = import(&app, b"not xml at all").await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
