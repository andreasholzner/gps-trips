//! US-1 acceptance tests — "import a GPX file".
//!
//! Acceptance criteria (docs/requirements.md):
//!   "Uploading a valid GPX creates a trip and redirects to its detail page.
//!    Invalid/empty GPX is rejected with a clear error."
//!
//! These drive the real Axum router in-process (ADR-0012), against a real
//! temporary SQLite database (the only mocked thing would be externals — there
//! are none in US-1).

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use tower::ServiceExt; // .oneshot()
use trip_archive::server::{db, http, state::AppState};

const SAMPLE_GPX: &[u8] = include_bytes!("fixtures/sample.gpx");
const NO_TRACKS_GPX: &[u8] = include_bytes!("fixtures/no_tracks.gpx");

/// A router backed by a fresh temp database. The returned `TempDir` must be kept
/// alive for the duration of the test (dropping it deletes the database).
async fn test_app() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = db::create_pool(&dir.path().join("test.db"))
        .await
        .expect("create pool");
    (http::router(AppState { pool }), dir)
}

/// Build a `multipart/form-data` POST to `/api/import` carrying a single `gpx` file.
fn import_request(gpx: &[u8]) -> Request<Body> {
    let boundary = "BoundaryUS1";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"gpx\"; filename=\"track.gpx\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: application/gpx+xml\r\n\r\n");
    body.extend_from_slice(gpx);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    Request::builder()
        .method(Method::POST)
        .uri("/api/import")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

async fn body_string(response: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

// ── Acceptance: a valid GPX creates a trip and redirects to its detail page ──

#[tokio::test]
async fn us1_valid_gpx_redirects_to_detail_page() {
    let (app, _dir) = test_app().await;

    let response = app.oneshot(import_request(SAMPLE_GPX)).await.unwrap();

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

    // Import, then follow the redirect to the detail page.
    let redirect = app
        .clone()
        .oneshot(import_request(SAMPLE_GPX))
        .await
        .unwrap();
    let location = redirect
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let detail = app
        .oneshot(
            Request::builder()
                .uri(&location)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

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
    let boundary = "BoundaryUS1";
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

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(body_string(response).await.contains("gpx"));
}

#[tokio::test]
async fn us1_gpx_without_tracks_is_rejected_with_422() {
    let (app, _dir) = test_app().await;
    let response = app.oneshot(import_request(NO_TRACKS_GPX)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body_string(response).await.to_lowercase().contains("track"));
}

#[tokio::test]
async fn us1_invalid_xml_is_rejected_with_422() {
    let (app, _dir) = test_app().await;
    let response = app
        .oneshot(import_request(b"not xml at all"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
