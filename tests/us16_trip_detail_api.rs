//! US-16 — the trip detail as JSON, for a non-HTML client (ADR-0008).
//!
//! `GET /api/trips` already serves the list a client needs (US-13); this is
//! its detail counterpart, so a client that opened a trip from that list can
//! render the trip's own page without scraping the server-rendered HTML.

mod common;

use axum::http::StatusCode;
use common::{body_string, get, import_sample, test_app};
use serde_json::Value;

#[tokio::test]
async fn us16_trip_detail_is_available_as_json() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = get(&app, &format!("/api/trips/{id}")).await;

    assert_eq!(response.status(), StatusCode::OK);
    let trip: Value = serde_json::from_str(&body_string(response).await).expect("JSON body");
    assert_eq!(trip["id"], id);
    // SAMPLE_GPX's own <name>, resolved at import.
    assert_eq!(trip["name"], "Oslo Hills Walk");
    assert_eq!(trip["activity_type"], "unknown");
    assert!(
        trip["distance_m"].as_f64().expect("distance_m") > 0.0,
        "the stats the detail page shows must be present"
    );
    // The bounding box a client needs to frame the track on a map (ADR-0005).
    for field in ["min_lat", "min_lon", "max_lat", "max_lon"] {
        assert!(trip[field].is_number(), "{field} must be present");
    }
    // A GPX-imported trip has no linked Komoot tour, so no privacy to show.
    assert!(trip["komoot"].is_null());
}

#[tokio::test]
async fn us16_an_unknown_trip_is_not_found() {
    let (app, _dir) = test_app().await;

    let response = get(&app, "/api/trips/999").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
