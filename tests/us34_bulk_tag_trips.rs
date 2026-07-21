//! US-34 — select multiple trips on the list page and assign tags to them.
//!
//! Acceptance criteria: the owner can bulk-apply one or more tags to several
//! selected trips in a single request. All-or-nothing on trip existence — an
//! unknown trip id in the batch 404s the whole request and nothing is
//! created or linked. Tag names are validated the same way US-33 validates
//! them (normalized, rejected if they contain whitespace).

mod common;

use axum::http::{Method, StatusCode};
use common::{body_string, get, import_sample, json_request, send, test_app};

fn bulk_tag_request(body: &str) -> axum::http::Request<axum::body::Body> {
    json_request(Method::POST, "/api/trips/tags", body)
}

#[tokio::test]
async fn us34_bulk_tagging_applies_every_tag_to_every_selected_trip() {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;
    let b = import_sample(&app).await;

    let response = send(
        &app,
        bulk_tag_request(&format!(
            r#"{{"trip_ids":[{a},{b}],"names":["alps","hiking"]}}"#
        )),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    for id in [a, b] {
        let tags = body_string(get(&app, &format!("/api/trips/{id}/tags")).await).await;
        assert!(tags.contains("\"name\":\"alps\""));
        assert!(tags.contains("\"name\":\"hiking\""));
    }
}

#[tokio::test]
async fn us34_an_unknown_trip_id_404s_and_tags_nothing() {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;

    let response = send(
        &app,
        bulk_tag_request(&format!(r#"{{"trip_ids":[{a},999],"names":["hiking"]}}"#)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let tags = body_string(get(&app, &format!("/api/trips/{a}/tags")).await).await;
    assert_eq!(tags, "[]");
    let all_tags = body_string(get(&app, "/api/tags").await).await;
    assert_eq!(all_tags, "[]");
}

#[tokio::test]
async fn us34_a_tag_name_with_a_space_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;

    let response = send(
        &app,
        bulk_tag_request(&format!(r#"{{"trip_ids":[{a}],"names":["day trip"]}}"#)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let tags = body_string(get(&app, &format!("/api/trips/{a}/tags")).await).await;
    assert_eq!(tags, "[]");
}

#[tokio::test]
async fn us34_an_empty_trip_selection_is_rejected_with_400() {
    let (app, _dir) = test_app().await;

    let response = send(&app, bulk_tag_request(r#"{"trip_ids":[],"names":["hiking"]}"#)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us34_an_empty_tag_list_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;

    let response = send(
        &app,
        bulk_tag_request(&format!(r#"{{"trip_ids":[{a}],"names":[]}}"#)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us34_the_list_page_renders_row_checkboxes_and_the_bulk_tag_panel() {
    let (app, _dir) = test_app().await;
    import_sample(&app).await;

    let list_html = body_string(get(&app, "/").await).await;
    assert!(list_html.contains("class=\"trip-select\""));
    assert!(list_html.contains("id=\"select-all\""));
    assert!(list_html.contains("id=\"bulk-tag-panel\""));
    assert!(list_html.contains("id=\"bulk-tag-input\""));
    assert!(list_html.contains("id=\"bulk-tag-suggestions\""));
    assert!(list_html.contains("id=\"bulk-tag-apply\""));
}

#[tokio::test]
async fn us34_bulk_tagging_reuses_an_existing_tag_rather_than_duplicating_it() {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;
    let b = import_sample(&app).await;

    send(
        &app,
        json_request(
            Method::POST,
            &format!("/api/trips/{a}/tags"),
            r#"{"name":"hiking"}"#,
        ),
    )
    .await;
    send(
        &app,
        bulk_tag_request(&format!(r#"{{"trip_ids":[{b}],"names":["hiking"]}}"#)),
    )
    .await;

    let all_tags = body_string(get(&app, "/api/tags").await).await;
    assert_eq!(all_tags.matches("\"name\":\"hiking\"").count(), 1);
}
