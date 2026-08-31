//! US-11 — choose an activity type when importing a GPX file.
//!
//! Acceptance criteria: the activity type is stored in the database and shown
//! on the list over all trips and on the trip detail page.

mod common;

use axum::http::StatusCode;
use common::{
    body_string, get, import_request_with_fields, send, test_app, trip_id_from_redirect, SAMPLE_GPX,
};

#[tokio::test]
async fn us11_chosen_activity_type_is_stored_and_served() {
    let (app, _dir) = test_app().await;
    let request = import_request_with_fields(SAMPLE_GPX, &[("activity_type", "cycling")], &[]);
    let redirect = send(&app, request).await;
    assert_eq!(redirect.status(), StatusCode::SEE_OTHER);
    let id = trip_id_from_redirect(&redirect);

    let detail = body_string(get(&app, &format!("/api/trips/{id}")).await).await;
    assert!(
        detail.contains(r#""activity_type":"cycling""#),
        "the trip must carry the chosen activity type; got: {detail}"
    );
}

#[tokio::test]
async fn us11_omitted_activity_type_defaults_to_unknown() {
    let (app, _dir) = test_app().await;
    let id = common::import_sample(&app).await;

    let detail = body_string(get(&app, &format!("/api/trips/{id}")).await).await;
    assert!(
        detail.contains(r#""activity_type":"unknown""#),
        "an import that named no activity must default to unknown; got: {detail}"
    );
}

#[tokio::test]
async fn us11_an_unrecognized_activity_type_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let request = import_request_with_fields(SAMPLE_GPX, &[("activity_type", "unicycling")], &[]);
    let response = send(&app, request).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_string(response).await;
    assert!(
        body.contains("unknown activity type"),
        "400 should be the activity-type-specific rejection, not some other bad request; got: {body}"
    );
}
