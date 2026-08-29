//! US-6 acceptance tests — "browse a list of all trips".
//!
//! Acceptance criteria (docs/requirements.md):
//!   "List shows each trip's name, date, distance, ascent, and duration; loads
//!    without reading track geometry."
//!
//! The list *screen* is the SPA's (US-41): that the five fields are shown,
//! formatted, and linked is asserted in `crates/ui-dioxus/src/trip_table.rs`
//! and `crates/ui-dioxus/src/list.rs`. What stays here is the server's half —
//! that `GET /api/trips` carries those fields and no track geometry, which is
//! what makes the list cheap (ADR-0003).
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

mod common;

use axum::http::StatusCode;
use common::{body_string, get, import_sample, test_app};

#[tokio::test]
async fn us6_the_list_carries_every_summary_field_and_no_geometry() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = get(&app, "/api/trips").await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_string(response).await;
    let trips: serde_json::Value = serde_json::from_str(&body).unwrap();
    let trip = &trips.as_array().expect("JSON array")[0];

    // The five required fields, as data — the screen does the formatting.
    assert_eq!(trip["id"], id);
    assert_eq!(trip["name"], "Oslo Hills Walk");
    assert_eq!(trip["start_time"], "2024-06-01T08:00:00Z");
    assert!(trip["distance_m"].is_number(), "distance; got: {body}");
    assert_eq!(trip["ascent_m"], 40.0);
    assert_eq!(trip["duration_secs"], 3600);

    // "Loads without reading track geometry": the row carries none, so the
    // list stays cheap however long the track is (ADR-0003).
    assert!(trip.get("track").is_none(), "got: {body}");
    assert!(trip.get("geojson").is_none(), "got: {body}");
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
