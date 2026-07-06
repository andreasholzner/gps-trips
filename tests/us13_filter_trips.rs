//! US-13 — filter the trip list by activity type, date interval, distance,
//! and free search of the name.
//!
//! Acceptance criteria: the list shows only trips matching the selected
//! filter criteria. Covers both the HTML list (`GET /`) and the JSON list
//! (`GET /api/trips`, ADR-0008) since both share the same filter parsing.
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
async fn us13_activity_filter_narrows_the_html_list() {
    let (app, _dir) = test_app().await;
    import_with(&app, "Bike Trip", "cycling").await;
    import_with(&app, "Hike Trip", "hiking").await;

    let html = body_string(get(&app, "/?activity=cycling").await).await;
    assert!(html.contains("Bike Trip"), "got: {html}");
    assert!(!html.contains("Hike Trip"), "got: {html}");
}

#[tokio::test]
async fn us13_name_search_narrows_the_html_list() {
    let (app, _dir) = test_app().await;
    import_with(&app, "Oslo Loop", "hiking").await;
    import_with(&app, "Bergen Ride", "cycling").await;

    let html = body_string(get(&app, "/?q=oslo").await).await;
    assert!(html.contains("Oslo Loop"), "got: {html}");
    assert!(!html.contains("Bergen Ride"), "got: {html}");
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

    let html_response = get(&app, "/?activity=unicycling").await;
    assert_eq!(html_response.status(), StatusCode::BAD_REQUEST);

    let api_response = get(&app, "/api/trips?activity=unicycling").await;
    assert_eq!(api_response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us13_a_malformed_from_date_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/?from=not-a-date").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us13_a_filter_matching_nothing_shows_a_distinct_empty_state() {
    let (app, _dir) = test_app().await;
    import_with(&app, "Hike Trip", "hiking").await;

    let html = body_string(get(&app, "/?activity=cycling").await).await;
    assert!(
        html.to_lowercase().contains("no trips match"),
        "got: {html}"
    );
    assert!(
        !html.to_lowercase().contains("no trips yet"),
        "a filtered empty result must not show the true-empty-archive message; got: {html}"
    );
}

#[tokio::test]
async fn us13_a_truly_empty_archive_still_shows_the_import_prompt() {
    let (app, _dir) = test_app().await;
    let html = body_string(get(&app, "/").await).await;
    assert!(html.to_lowercase().contains("no trips yet"), "got: {html}");
}

#[tokio::test]
async fn us13_the_filter_form_echoes_back_submitted_values() {
    let (app, _dir) = test_app().await;
    import_with(&app, "Bike Trip", "cycling").await;

    let html = body_string(get(&app, "/?q=Bike&activity=cycling").await).await;
    assert!(html.contains(r#"value="Bike""#), "got: {html}");
    assert!(
        html.contains(r#"<option value="cycling" selected>"#),
        "got: {html}"
    );
}

/// Regression test: the rendered filter form (render.rs's `render_filter_form`)
/// always submits every field in one GET request. Leaving all but one blank
/// used to 400 — either because `from`/`to` reached `parse_date("")`, or
/// because `min_dist`/`max_dist` being typed `Option<f64>` made axum's own
/// `Query` extractor reject an empty numeric field before the handler ever
/// ran. A real submission with only `q` filled in must succeed.
#[tokio::test]
async fn us13_submitting_the_full_form_with_only_one_field_filled_in_succeeds() {
    let (app, _dir) = test_app().await;
    import_with(&app, "Bike Trip", "cycling").await;

    let response = get(&app, "/?q=Bike&activity=&from=&to=&min_dist=&max_dist=").await;
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_string(response).await;
    assert!(html.contains("Bike Trip"), "got: {html}");

    let api_response = get(
        &app,
        "/api/trips?q=Bike&activity=&from=&to=&min_dist=&max_dist=",
    )
    .await;
    assert_eq!(api_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn us13_a_backwards_date_range_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/?from=2024-06-10&to=2024-06-01").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us13_min_dist_greater_than_max_dist_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/?min_dist=50&max_dist=5").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us13_a_nonsense_distance_value_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/?min_dist=nan").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us13_an_activity_value_with_surrounding_whitespace_is_trimmed_like_import_does() {
    let (app, _dir) = test_app().await;
    import_with(&app, "Bike Trip", "cycling").await;

    let html = body_string(get(&app, "/?activity=%20cycling%20").await).await;
    assert!(html.contains("Bike Trip"), "got: {html}");
}

#[tokio::test]
async fn us13_name_search_matches_non_ascii_case_insensitively() {
    let (app, _dir) = test_app().await;
    import_with(&app, "Tromsø Fjelltur", "hiking").await;

    let html = body_string(get(&app, "/?q=TROMS%C3%98").await).await;
    assert!(html.contains("Tromsø Fjelltur"), "got: {html}");
}
