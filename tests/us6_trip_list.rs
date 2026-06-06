//! US-6 acceptance tests — "browse a list of all trips".
//!
//! Acceptance criteria (docs/requirements.md):
//!   "List shows each trip's name, date, distance, ascent, and duration; loads
//!    without reading track geometry."
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

mod common;

use axum::http::StatusCode;
use common::{body_string, get, import_sample, test_app};

#[tokio::test]
async fn us6_empty_list_shows_an_empty_state() {
    let (app, _dir) = test_app().await;

    let response = get(&app, "/").await;
    assert_eq!(response.status(), StatusCode::OK);

    let html = body_string(response).await;
    assert!(
        html.to_lowercase().contains("no trips"),
        "empty list should show an empty state; got: {html}"
    );
    assert!(
        html.contains("/import"),
        "empty state should link to import; got: {html}"
    );
}

#[tokio::test]
async fn us6_list_shows_imported_trip_summary_fields() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = get(&app, "/").await;
    assert_eq!(response.status(), StatusCode::OK);

    let html = body_string(response).await;
    // The five required fields: name, date, distance, ascent, duration.
    assert!(html.contains("Oslo Hills Walk"), "name; got: {html}");
    assert!(html.contains("2024-06-01"), "date; got: {html}");
    assert!(html.contains("km"), "distance; got: {html}");
    assert!(html.contains("40 m"), "ascent; got: {html}");
    assert!(html.contains("01:00:00"), "duration; got: {html}");
    // Each trip links to its detail page.
    assert!(
        html.contains(&format!("/trips/{id}")),
        "list should link to the trip detail; got: {html}"
    );
}

#[tokio::test]
async fn us6_import_form_is_available_at_import() {
    let (app, _dir) = test_app().await;

    let response = get(&app, "/import").await;
    assert_eq!(response.status(), StatusCode::OK);

    let html = body_string(response).await;
    assert!(
        html.contains(r#"action="/api/import""#),
        "import form should be served at /import; got: {html}"
    );
}
