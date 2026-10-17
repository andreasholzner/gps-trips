//! US-108 — a trip's page tells whether the trip was recorded or only
//! planned. The kind comes with the trip from the JSON API (ADR-0008), and
//! with the recipient's own copy of it (ADR-0015); the page itself is the UI
//! crate's.

use crate::common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use trip_archive::models::{CreatedShare, SharedTrip, TripDetail, TripKind};

async fn import(app: &Router, kind: &str) -> i64 {
    let request = common::import_request_with_fields(common::SAMPLE_GPX, &[("kind", kind)], &[]);
    let redirect = common::send(app, request).await;
    assert_eq!(redirect.status(), StatusCode::SEE_OTHER);
    common::trip_id_from_redirect(&redirect)
}

async fn share(app: &Router, trip_ids: &[i64]) -> String {
    let body = serde_json::json!({ "trip_ids": trip_ids }).to_string();
    let response = common::send(
        app,
        common::json_request(Method::POST, "/api/shares", &body),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created: CreatedShare = serde_json::from_str(&common::body_string(response).await).unwrap();
    created.token
}

#[tokio::test]
async fn us108_the_trip_comes_with_its_kind() {
    let (app, _dir) = common::test_app().await;
    let recorded = import(&app, "recorded").await;
    let planned = import(&app, "planned").await;

    for (id, kind) in [(recorded, TripKind::Recorded), (planned, TripKind::Planned)] {
        let body = common::body_string(common::get(&app, &format!("/api/trips/{id}")).await).await;
        let trip: TripDetail = serde_json::from_str(&body).unwrap();

        assert_eq!(trip.trip_kind, kind, "{body}");
    }
}

#[tokio::test]
async fn us108_a_shared_trip_comes_with_its_kind() {
    let (app, _dir) = common::test_app().await;
    let recorded = import(&app, "recorded").await;
    let planned = import(&app, "planned").await;
    let token = share(&app, &[recorded, planned]).await;

    for (id, kind) in [(recorded, TripKind::Recorded), (planned, TripKind::Planned)] {
        let response = common::send_unauthenticated(
            &app,
            Request::builder()
                .uri(format!("/s/{token}/api/trips/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        let body = common::body_string(response).await;
        let trip: SharedTrip = serde_json::from_str(&body).unwrap();

        assert_eq!(trip.trip_kind, kind, "{body}");
    }
}
