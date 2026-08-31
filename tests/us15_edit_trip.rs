//! US-15 — edit a trip's name and activity type from the detail page.
//!
//! Acceptance criteria: the new values for name and activity type are saved
//! to the database.

mod common;

use axum::http::{Method, StatusCode};
use common::{body_string, get, import_sample, json_request, send, test_app};

fn patch_request(id: i64, body: &str) -> axum::http::Request<axum::body::Body> {
    json_request(Method::PATCH, &format!("/api/trips/{id}"), body)
}

#[tokio::test]
async fn us15_editing_both_fields_updates_the_list_and_detail_page() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send(
        &app,
        patch_request(id, r#"{"name":"Renamed Trip","activity_type":"cycling"}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let trip = body_string(get(&app, &format!("/api/trips/{id}")).await).await;
    assert!(trip.contains(r#""name":"Renamed Trip""#), "{trip}");
    assert!(trip.contains(r#""activity_type":"cycling""#), "{trip}");
}

#[tokio::test]
async fn us15_editing_only_activity_type_leaves_the_name_unchanged() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send(&app, patch_request(id, r#"{"activity_type":"kayaking"}"#)).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let trip = body_string(get(&app, &format!("/api/trips/{id}")).await).await;
    assert!(trip.contains(r#""activity_type":"kayaking""#), "{trip}");
    // SAMPLE_GPX's own <name> ("Oslo Hills Walk") is what import_sample resolves to;
    // it must survive an edit that only touches activity_type.
    assert!(trip.contains(r#""name":"Oslo Hills Walk""#), "{trip}");
}

#[tokio::test]
async fn us15_editing_only_name_leaves_the_activity_type_unchanged() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send(&app, patch_request(id, r#"{"name":"Only Name Changed"}"#)).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let trip = body_string(get(&app, &format!("/api/trips/{id}")).await).await;
    assert!(trip.contains(r#""name":"Only Name Changed""#), "{trip}");
    assert!(trip.contains(r#""activity_type":"unknown""#), "{trip}");
}

#[tokio::test]
async fn us15_a_blank_name_is_rejected_with_400_and_nothing_changes() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send(&app, patch_request(id, r#"{"name":"   "}"#)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let trip = body_string(get(&app, &format!("/api/trips/{id}")).await).await;
    assert!(trip.contains(r#""name":"Oslo Hills Walk""#), "{trip}");
}

#[tokio::test]
async fn us15_an_unrecognized_activity_type_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send(&app, patch_request(id, r#"{"activity_type":"unicycling"}"#)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_string(response).await;
    assert!(
        body.contains("unknown activity type"),
        "400 should be the activity-type-specific rejection; got: {body}"
    );
}

#[tokio::test]
async fn us15_editing_an_unknown_trip_returns_404() {
    let (app, _dir) = test_app().await;
    let response = send(&app, patch_request(999, r#"{"name":"Whatever"}"#)).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
