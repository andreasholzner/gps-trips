//! US-38 — filter trips by tag on the list page.
//!
//! Acceptance criteria: the list page offers a multi-select of known tags;
//! only trips carrying every selected tag are shown. Covers both the HTML
//! list (`GET /`) and the JSON list (`GET /api/trips`, ADR-0008), since both
//! share the same filter parsing (US-13/ADR-0011).

mod common;

use axum::http::{Method, StatusCode};
use common::{body_string, get, import_sample, json_request, send, test_app};

async fn tag_trip(app: &axum::Router, trip_id: i64, name: &str) {
    let response = send(
        app,
        json_request(
            Method::POST,
            &format!("/api/trips/{trip_id}/tags"),
            &format!(r#"{{"name":"{name}"}}"#),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn us38_tag_filter_narrows_the_html_list_to_trips_with_every_selected_tag() {
    let (app, _dir) = test_app().await;
    let both = import_sample(&app).await;
    let one = import_sample(&app).await;
    let neither = import_sample(&app).await;
    tag_trip(&app, both, "alps").await;
    tag_trip(&app, both, "hiking").await;
    tag_trip(&app, one, "alps").await;

    let json = body_string(get(&app, "/api/trips?tags=alps,hiking").await).await;
    assert!(json.contains(&format!("\"id\":{both}")), "got: {json}");
    assert!(!json.contains(&format!("\"id\":{one}")), "got: {json}");
    assert!(!json.contains(&format!("\"id\":{neither}")), "got: {json}");
}

#[tokio::test]
async fn us38_a_single_selected_tag_narrows_the_list() {
    let (app, _dir) = test_app().await;
    let tagged = import_sample(&app).await;
    let untagged = import_sample(&app).await;
    tag_trip(&app, tagged, "alps").await;

    let json = body_string(get(&app, "/api/trips?tags=alps").await).await;
    assert!(json.contains(&format!("\"id\":{tagged}")), "got: {json}");
    assert!(!json.contains(&format!("\"id\":{untagged}")), "got: {json}");
}

#[tokio::test]
async fn us38_api_trips_returns_only_trips_matching_every_selected_tag() {
    let (app, _dir) = test_app().await;
    let both = import_sample(&app).await;
    let one = import_sample(&app).await;
    tag_trip(&app, both, "alps").await;
    tag_trip(&app, both, "hiking").await;
    tag_trip(&app, one, "alps").await;

    let response = get(&app, "/api/trips?tags=alps,hiking").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    let trips: serde_json::Value = serde_json::from_str(&body).unwrap();
    let trips = trips.as_array().expect("JSON array");
    assert_eq!(trips.len(), 1, "got: {body}");
    assert_eq!(trips[0]["id"], both);
}

#[tokio::test]
async fn us38_no_tags_selected_shows_every_trip() {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;
    let b = import_sample(&app).await;
    tag_trip(&app, a, "alps").await;

    let json = body_string(get(&app, "/api/trips").await).await;
    assert!(json.contains(&format!("\"id\":{a}")), "got: {json}");
    assert!(json.contains(&format!("\"id\":{b}")), "got: {json}");
}

#[tokio::test]
async fn us38_tag_filter_combines_with_other_filters_as_and() {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;
    tag_trip(&app, a, "alps").await;

    let json = body_string(get(&app, "/api/trips?tags=alps&activity=cycling").await).await;
    assert_eq!(json, "[]", "got: {json}");
}

#[tokio::test]
async fn us38_a_tag_value_containing_whitespace_is_rejected_with_400() {
    let (app, _dir) = test_app().await;

    let response = get(&app, "/api/trips?tags=day%20trip").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us38_a_well_formed_but_unknown_tag_matches_nothing_without_erroring() {
    let (app, _dir) = test_app().await;
    import_sample(&app).await;

    let response = get(&app, "/api/trips?tags=nonexistent").await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_string(response).await;
    assert_eq!(json, "[]", "got: {json}");
}

/// Filtering by a differently-cased tag value — from a hand-edited or shared
/// URL — still matches, since stored names are normalized (US-33).
#[tokio::test]
async fn us38_a_differently_cased_tag_value_still_matches() {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;
    tag_trip(&app, a, "alps").await;

    let json = body_string(get(&app, "/api/trips?tags=Alps").await).await;
    assert!(json.contains(&format!("\"id\":{a}")), "got: {json}");
}
