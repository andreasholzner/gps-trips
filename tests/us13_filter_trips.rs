//! US-13 — filter the trip list by activity type, date interval, distance,
//! and free search of the name.
//!
//! Acceptance criteria: the list shows only trips matching the selected
//! filter criteria, asserted here through the JSON list (`GET /api/trips`,
//! ADR-0008) — the contract the SPA calls. The screen's half (the filter
//! panel, narrowing as you type, and the filtered-vs-empty archive states)
//! moved to the SPA with US-41/US-52; see `crates/ui-dioxus/src/list.rs`
//! and `tests/browser/trip_list.spec.mjs`.
//! Date/distance boundary semantics are covered at the repo level
//! (`src/server/repo/trip/tests.rs`) where distinct `start_time`/`distance_m`
//! values are easy to construct directly; this file exercises the HTTP
//! wiring and the dimensions that are naturally distinct via import fields
//! (activity type, name).

mod common;

use axum::http::StatusCode;
use common::{
    body_string, get, import_request_with_fields, send, test_app, trip_id_from_redirect, SAMPLE_GPX,
};

async fn import_with(app: &axum::Router, name: &str, activity_type: &str) -> i64 {
    let request = import_request_with_fields(
        SAMPLE_GPX,
        &[("name", name), ("activity_type", activity_type)],
        &[],
    );
    let redirect = send(app, request).await;
    assert_eq!(redirect.status(), StatusCode::SEE_OTHER);
    trip_id_from_redirect(&redirect)
}

#[tokio::test]
async fn us13_name_search_narrows_the_list() {
    let (app, _dir) = test_app().await;
    import_with(&app, "Oslo Loop", "hiking").await;
    import_with(&app, "Bergen Ride", "cycling").await;

    let json = body_string(get(&app, "/api/trips?q=oslo").await).await;
    assert!(json.contains("Oslo Loop"), "got: {json}");
    assert!(!json.contains("Bergen Ride"), "got: {json}");
}

#[tokio::test]
async fn us13_api_trips_returns_only_matching_trips_as_json() {
    let (app, _dir) = test_app().await;
    let bike_id = import_with(&app, "Bike Trip", "cycling").await;
    import_with(&app, "Hike Trip", "hiking").await;

    let response = get(&app, "/api/trips?activity=cycling").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    let trips: serde_json::Value = serde_json::from_str(&body).unwrap();
    let trips = trips.as_array().expect("JSON array");
    assert_eq!(trips.len(), 1, "got: {body}");
    assert_eq!(trips[0]["id"], bike_id);
    assert_eq!(trips[0]["name"], "Bike Trip");
}

#[tokio::test]
async fn us13_an_unrecognized_activity_filter_is_rejected_with_400() {
    let (app, _dir) = test_app().await;

    let response = get(&app, "/api/trips?activity=unicycling").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us13_a_malformed_from_date_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/api/trips?from=not-a-date").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Regression test: a client that always sends every filter field, leaving
/// the untouched ones blank, must not 400 — either because `from`/`to`
/// reached `parse_date("")`, or because `min_dist`/`max_dist` being typed
/// `Option<f64>` made axum's own `Query` extractor reject an empty numeric
/// field before the handler ever ran.
#[tokio::test]
async fn us13_submitting_the_full_form_with_only_one_field_filled_in_succeeds() {
    let (app, _dir) = test_app().await;
    import_with(&app, "Bike Trip", "cycling").await;

    let response = get(
        &app,
        "/api/trips?q=Bike&activity=&from=&to=&min_dist=&max_dist=",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_string(response).await;
    assert!(json.contains("Bike Trip"), "got: {json}");
}

#[tokio::test]
async fn us13_a_backwards_date_range_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/api/trips?from=2024-06-10&to=2024-06-01").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us13_min_dist_greater_than_max_dist_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/api/trips?min_dist=50&max_dist=5").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us13_a_nonsense_distance_value_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/api/trips?min_dist=nan").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us13_an_activity_value_with_surrounding_whitespace_is_trimmed_like_import_does() {
    let (app, _dir) = test_app().await;
    import_with(&app, "Bike Trip", "cycling").await;

    let json = body_string(get(&app, "/api/trips?activity=%20cycling%20").await).await;
    assert!(json.contains("Bike Trip"), "got: {json}");
}

#[tokio::test]
async fn us13_name_search_matches_non_ascii_case_insensitively() {
    let (app, _dir) = test_app().await;
    import_with(&app, "Tromsø Fjelltur", "hiking").await;

    let json = body_string(get(&app, "/api/trips?q=TROMS%C3%98").await).await;
    assert!(json.contains("Tromsø Fjelltur"), "got: {json}");
}
