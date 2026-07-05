//! US-11 — choose an activity type when importing a GPX file.
//!
//! Acceptance criteria: the activity type is stored in the database and shown
//! on the list over all trips and on the trip detail page.

mod common;

use axum::http::StatusCode;
use common::{
    body_string, detail_activity_fragment, get, import_request_with_fields, list_activity_fragment,
    send, test_app, trip_id_from_redirect, SAMPLE_GPX,
};

#[tokio::test]
async fn us11_chosen_activity_type_appears_on_the_list_and_detail_page() {
    let (app, _dir) = test_app().await;
    let request = import_request_with_fields(SAMPLE_GPX, &[("activity_type", "cycling")], &[]);
    let redirect = send(&app, request).await;
    assert_eq!(redirect.status(), StatusCode::SEE_OTHER);
    let id = trip_id_from_redirect(&redirect);

    let list_html = body_string(get(&app, "/").await).await;
    assert!(
        list_html.contains(&list_activity_fragment("cycling")),
        "list page should show the chosen activity type; got: {list_html}"
    );

    let detail_html = body_string(get(&app, &format!("/trips/{id}")).await).await;
    assert!(
        detail_html.contains(&detail_activity_fragment("cycling")),
        "detail page should show the chosen activity type; got: {detail_html}"
    );
}

#[tokio::test]
async fn us11_omitted_activity_type_defaults_to_unknown_on_both_pages() {
    let (app, _dir) = test_app().await;
    let id = common::import_sample(&app).await;

    let list_html = body_string(get(&app, "/").await).await;
    assert!(
        list_html.contains(&list_activity_fragment("unknown")),
        "list page should show the default activity type; got: {list_html}"
    );

    let detail_html = body_string(get(&app, &format!("/trips/{id}")).await).await;
    assert!(
        detail_html.contains(&detail_activity_fragment("unknown")),
        "detail page should show the default activity type; got: {detail_html}"
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
