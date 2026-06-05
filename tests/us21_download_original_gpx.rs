//! US-21 acceptance tests — "download the original GPX file".
//!
//! Acceptance criteria (docs/requirements.md):
//!   "The exact uploaded GPX bytes are stored on import; the detail page offers a
//!    download link; downloading returns the original file byte-for-byte with
//!    Content-Type: application/gpx+xml and a sensible filename."
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

mod common;

use axum::http::StatusCode;
use common::{body_bytes, get, import_sample, test_app, SAMPLE_GPX};

#[tokio::test]
async fn us21_download_returns_the_original_bytes_verbatim() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = get(&app, &format!("/api/trips/{id}/gpx")).await;
    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get("content-type")
        .expect("Content-Type header")
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(content_type, "application/gpx+xml");

    let body = body_bytes(response).await;
    assert_eq!(
        body, SAMPLE_GPX,
        "downloaded bytes must equal the uploaded GPX exactly"
    );
}

#[tokio::test]
async fn us21_download_is_served_as_attachment_named_after_the_trip() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = get(&app, &format!("/api/trips/{id}/gpx")).await;
    let disposition = response
        .headers()
        .get("content-disposition")
        .expect("Content-Disposition header")
        .to_str()
        .unwrap();

    assert!(disposition.contains("attachment"), "got: {disposition}");
    // Sample GPX track name is "Oslo Hills Walk" → sensible download filename.
    assert!(
        disposition.contains("Oslo Hills Walk.gpx"),
        "expected a trip-named .gpx filename; got: {disposition}"
    );
}

#[tokio::test]
async fn us21_detail_page_links_to_the_download() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let detail = get(&app, &format!("/trips/{id}")).await;
    let html = String::from_utf8(body_bytes(detail).await).unwrap();
    assert!(
        html.contains(&format!("/api/trips/{id}/gpx")),
        "detail page should link to the GPX download; got: {html}"
    );
}

#[tokio::test]
async fn us21_download_for_unknown_trip_returns_404() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/api/trips/999/gpx").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
