//! US-33 — tag trips on the trip detail page.
//!
//! Acceptance criteria: tags are simple strings (no spaces); using a new tag
//! creates the tag on-demand after confirmation. The "confirmation" step is
//! client-side UI (`public/js/trip_detail.js`); at the HTTP boundary this
//! translates to: creating via a tag name that doesn't exist yet succeeds
//! and creates it, an existing name is reused, and a name containing
//! whitespace is rejected.

mod common;

use axum::http::{Method, StatusCode};
use common::{body_string, get, import_sample, json_request, send, test_app};

fn add_tag_request(trip_id: i64, body: &str) -> axum::http::Request<axum::body::Body> {
    json_request(Method::POST, &format!("/api/trips/{trip_id}/tags"), body)
}

fn remove_tag_uri(trip_id: i64, tag_id: i64) -> String {
    format!("/api/trips/{trip_id}/tags/{tag_id}")
}

#[tokio::test]
async fn us33_tagging_a_trip_creates_the_tag_and_links_it() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send(&app, add_tag_request(id, r#"{"name":"hiking"}"#)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_string(response).await;
    assert!(body.contains("\"name\":\"hiking\""));

    let tags = body_string(get(&app, &format!("/api/trips/{id}/tags")).await).await;
    assert!(tags.contains("\"name\":\"hiking\""));
}

#[tokio::test]
async fn us33_tag_names_are_normalized_case_insensitively() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    send(&app, add_tag_request(id, r#"{"name":"Hiking"}"#)).await;
    let response = send(&app, add_tag_request(id, r#"{"name":"HIKING"}"#)).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    // Same underlying tag reused, not a second one.
    let all_tags = body_string(get(&app, "/api/tags").await).await;
    assert_eq!(all_tags.matches("\"name\":\"hiking\"").count(), 1);

    let trip_tags = body_string(get(&app, &format!("/api/trips/{id}/tags")).await).await;
    assert_eq!(trip_tags.matches("\"name\":\"hiking\"").count(), 1);
}

#[tokio::test]
async fn us33_a_tag_name_containing_a_space_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send(&app, add_tag_request(id, r#"{"name":"day trip"}"#)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let tags = body_string(get(&app, &format!("/api/trips/{id}/tags")).await).await;
    assert_eq!(tags, "[]");
}

#[tokio::test]
async fn us33_a_tag_name_containing_a_comma_is_rejected_with_400() {
    // US-38: tag names can never contain a comma, so the trip-list filter's
    // comma-separated `?tags=` query param can unambiguously encode several
    // selected tags.
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send(&app, add_tag_request(id, r#"{"name":"day,trip"}"#)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let tags = body_string(get(&app, &format!("/api/trips/{id}/tags")).await).await;
    assert_eq!(tags, "[]");
}

#[tokio::test]
async fn us33_tagging_an_unknown_trip_returns_404() {
    let (app, _dir) = test_app().await;
    let response = send(&app, add_tag_request(999, r#"{"name":"hiking"}"#)).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us33_tagging_an_unknown_trip_with_an_invalid_name_still_returns_404_not_400() {
    // Trip existence is checked before the name is validated, so a request
    // that's wrong on both counts reports the trip as missing rather than
    // masking that behind a name-validation 400.
    let (app, _dir) = test_app().await;
    let response = send(&app, add_tag_request(999, r#"{"name":"day trip"}"#)).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us33_a_trip_with_no_tags_has_an_empty_tag_list() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let tags = body_string(get(&app, &format!("/api/trips/{id}/tags")).await).await;
    assert_eq!(tags, "[]");
}

#[tokio::test]
async fn us33_listing_tags_for_an_unknown_trip_returns_404() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/api/trips/999/tags").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us33_removing_a_tag_unlinks_it_from_the_trip_but_keeps_it_suggestible() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let created = body_string(send(&app, add_tag_request(id, r#"{"name":"hiking"}"#)).await).await;
    let tag_id: i64 = serde_json::from_str::<serde_json::Value>(&created).unwrap()["id"]
        .as_i64()
        .unwrap();

    let response = send(
        &app,
        axum::http::Request::builder()
            .method(Method::DELETE)
            .uri(remove_tag_uri(id, tag_id))
            .body(axum::body::Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let trip_tags = body_string(get(&app, &format!("/api/trips/{id}/tags")).await).await;
    assert_eq!(trip_tags, "[]");

    // Still suggestible via /api/tags — orphaned tags are kept (US-33).
    let all_tags = body_string(get(&app, "/api/tags").await).await;
    assert!(all_tags.contains("\"name\":\"hiking\""));
}

#[tokio::test]
async fn us33_removing_an_untagged_tag_is_idempotent() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send(
        &app,
        axum::http::Request::builder()
            .method(Method::DELETE)
            .uri(remove_tag_uri(id, 999))
            .body(axum::body::Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn us33_removing_a_tag_from_an_unknown_trip_returns_404() {
    let (app, _dir) = test_app().await;
    let response = send(
        &app,
        axum::http::Request::builder()
            .method(Method::DELETE)
            .uri(remove_tag_uri(999, 1))
            .body(axum::body::Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us33_api_tags_lists_every_tag_that_exists() {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;
    let b = import_sample(&app).await;

    send(&app, add_tag_request(a, r#"{"name":"alps"}"#)).await;
    send(&app, add_tag_request(b, r#"{"name":"coastal"}"#)).await;

    let all_tags = body_string(get(&app, "/api/tags").await).await;
    assert!(all_tags.contains("\"name\":\"alps\""));
    assert!(all_tags.contains("\"name\":\"coastal\""));
}

#[tokio::test]
async fn us33_the_trip_detail_page_renders_the_tags_section() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let detail_html = body_string(get(&app, &format!("/trips/{id}")).await).await;
    assert!(detail_html.contains("id=\"tags\""));
    assert!(detail_html.contains("id=\"tag-input\""));
    assert!(detail_html.contains("id=\"tag-suggestions\""));
}
