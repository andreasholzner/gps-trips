//! US-31 — choose recorded vs. planned when importing a GPX file.
//!
//! Acceptance criteria: the import form offers a Recorded/Planned choice
//! (defaulting to Recorded); the chosen `trip_kind` determines which list
//! tab (US-32) the trip appears under; an unrecognized value is rejected
//! with 400.

mod common;

use axum::http::StatusCode;
use common::{
    body_string, get, import_request_with_fields, send, test_app, trip_id_from_redirect, SAMPLE_GPX,
};

#[tokio::test]
async fn us31_a_trip_imported_as_planned_appears_under_the_planned_tab_only() {
    let (app, _dir) = test_app().await;
    let request = import_request_with_fields(SAMPLE_GPX, &[("kind", "planned")], &[]);
    let redirect = send(&app, request).await;
    assert_eq!(redirect.status(), StatusCode::SEE_OTHER);
    let id = trip_id_from_redirect(&redirect);

    let planned_html = body_string(get(&app, "/?kind=planned").await).await;
    assert!(
        planned_html.contains(&format!("/trips/{id}")),
        "planned tab should list the imported trip; got: {planned_html}"
    );

    let recorded_html = body_string(get(&app, "/").await).await;
    assert!(
        !recorded_html.contains(&format!("/trips/{id}")),
        "recorded tab (the default) should not list a trip imported as planned; got: {recorded_html}"
    );
}

#[tokio::test]
async fn us31_omitted_kind_defaults_to_recorded() {
    let (app, _dir) = test_app().await;
    let id = common::import_sample(&app).await;

    let recorded_html = body_string(get(&app, "/").await).await;
    assert!(
        recorded_html.contains(&format!("/trips/{id}")),
        "default (recorded) tab should list a trip imported without a kind; got: {recorded_html}"
    );

    let planned_html = body_string(get(&app, "/?kind=planned").await).await;
    assert!(
        !planned_html.contains(&format!("/trips/{id}")),
        "planned tab should not list a trip that defaulted to recorded; got: {planned_html}"
    );
}

#[tokio::test]
async fn us31_an_unrecognized_kind_is_rejected_with_400() {
    let (app, _dir) = test_app().await;
    let request = import_request_with_fields(SAMPLE_GPX, &[("kind", "scheduled")], &[]);
    let response = send(&app, request).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_string(response).await;
    assert!(
        body.contains("unknown trip kind"),
        "400 should be the trip-kind-specific rejection, not some other bad request; got: {body}"
    );
}
