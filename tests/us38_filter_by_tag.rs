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

    let html = body_string(get(&app, "/?tags=alps,hiking").await).await;
    assert!(html.contains(&format!("/trips/{both}")), "got: {html}");
    assert!(!html.contains(&format!("/trips/{one}")), "got: {html}");
    assert!(!html.contains(&format!("/trips/{neither}")), "got: {html}");
}

#[tokio::test]
async fn us38_a_single_selected_tag_narrows_the_html_list() {
    let (app, _dir) = test_app().await;
    let tagged = import_sample(&app).await;
    let untagged = import_sample(&app).await;
    tag_trip(&app, tagged, "alps").await;

    let html = body_string(get(&app, "/?tags=alps").await).await;
    assert!(html.contains(&format!("/trips/{tagged}")), "got: {html}");
    assert!(!html.contains(&format!("/trips/{untagged}")), "got: {html}");
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

    let html = body_string(get(&app, "/").await).await;
    assert!(html.contains(&format!("/trips/{a}")), "got: {html}");
    assert!(html.contains(&format!("/trips/{b}")), "got: {html}");
}

#[tokio::test]
async fn us38_tag_filter_combines_with_other_filters_as_and() {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;
    tag_trip(&app, a, "alps").await;

    let html = body_string(get(&app, "/?tags=alps&activity=cycling").await).await;
    assert!(
        html.to_lowercase().contains("no trips match"),
        "got: {html}"
    );
}

#[tokio::test]
async fn us38_a_tag_value_containing_whitespace_is_rejected_with_400() {
    let (app, _dir) = test_app().await;

    let html_response = get(&app, "/?tags=day%20trip").await;
    assert_eq!(html_response.status(), StatusCode::BAD_REQUEST);

    let api_response = get(&app, "/api/trips?tags=day%20trip").await;
    assert_eq!(api_response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us38_a_well_formed_but_unknown_tag_matches_nothing_without_erroring() {
    let (app, _dir) = test_app().await;
    import_sample(&app).await;

    let response = get(&app, "/?tags=nonexistent").await;
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_string(response).await;
    assert!(
        html.to_lowercase().contains("no trips match"),
        "got: {html}"
    );
}

#[tokio::test]
async fn us38_the_filter_form_renders_a_tag_multiselect_populated_with_known_tags_and_echoes_the_selection(
) {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;
    tag_trip(&app, a, "alps").await;

    let html = body_string(get(&app, "/?tags=alps").await).await;
    assert!(html.contains(r#"id="tags-select""#), "got: {html}");
    assert!(
        html.contains(r#"<option value="alps" selected>"#),
        "got: {html}"
    );
    assert!(
        html.contains(r#"id="tags-input" name="tags" value="alps""#),
        "got: {html}"
    );
}

/// Regression test: filtering by a differently-cased tag value (e.g. from a
/// hand-edited or shared URL) must both filter correctly *and* show the
/// matching option as selected in the multi-select — not just filter
/// correctly while leaving the dropdown looking like nothing is selected.
#[tokio::test]
async fn us38_a_differently_cased_tag_value_still_shows_the_option_as_selected() {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;
    tag_trip(&app, a, "alps").await;

    let html = body_string(get(&app, "/?tags=Alps").await).await;
    assert!(html.contains(&format!("/trips/{a}")), "got: {html}");
    assert!(
        html.contains(r#"<option value="alps" selected>"#),
        "got: {html}"
    );
}
