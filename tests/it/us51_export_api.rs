//! US-51 acceptance tests, server half: `GET /api/export/trips`, the list the
//! QMapShack exporter reconciles against (ADR-0022's 2026-09-19 amendment).
//! Its removal pass treats a trip missing from this list as deleted, so the
//! list must hold every trip — never paged, never filtered. That it needs a
//! session is part of US-19's table of routes.

use crate::common::{
    body_string, get, import_request_with_fields, import_sample, json_request, send, test_app,
    trip_id_from_redirect, SAMPLE_GPX,
};
use axum::http::{Method, StatusCode};
use trip_archive::models::{ActivityType, ExportTrip, TripKind};

async fn export_list(app: &axum::Router, uri: &str) -> Vec<ExportTrip> {
    let response = get(app, uri).await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_str(&body_string(response).await).expect("a JSON list of export trips")
}

async fn tag(app: &axum::Router, trip_id: i64, name: &str) {
    let body = format!(r#"{{"name":"{name}"}}"#);
    let request = json_request(Method::POST, &format!("/api/trips/{trip_id}/tags"), &body);
    assert_eq!(send(app, request).await.status(), StatusCode::CREATED);
}

async fn import_planned_cycling(app: &axum::Router) -> i64 {
    let fields = [("activity_type", "cycling"), ("kind", "planned")];
    let response = send(app, import_request_with_fields(SAMPLE_GPX, &fields, &[])).await;
    trip_id_from_redirect(&response)
}

#[tokio::test]
async fn us51_the_export_list_holds_every_trip_with_its_tags() {
    let (app, _dir) = test_app().await;
    let hike = import_sample(&app).await;
    let ride = import_planned_cycling(&app).await;
    tag(&app, hike, "telt").await;
    tag(&app, hike, "fjell").await;

    let trips = export_list(&app, "/api/export/trips").await;

    assert_eq!(
        trips.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![hike, ride]
    );
    let names: Vec<&str> = trips[0].tags.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["fjell", "telt"], "tags in name order");
    assert!(trips[0].distance_m > 0.0);
    assert!(trips[0].start_time.is_some());
    assert!(trips[1].tags.is_empty());
    assert_eq!(trips[1].activity_type, ActivityType::Cycling);
    assert_eq!(trips[1].trip_kind, TripKind::Planned);
}

#[tokio::test]
async fn us51_the_export_list_ignores_list_filters() {
    let (app, _dir) = test_app().await;
    let hike = import_sample(&app).await;
    let ride = import_planned_cycling(&app).await;
    tag(&app, ride, "bike").await;

    // The trip list's own parameters: any of them applied here would make
    // the exporter trash every item the filter left out.
    let trips = export_list(
        &app,
        "/api/export/trips?activity=cycling&kind=planned&tags=bike&q=none",
    )
    .await;

    assert_eq!(
        trips.iter().map(|t| t.id).collect::<Vec<_>>(),
        vec![hike, ride]
    );
}

#[tokio::test]
async fn us51_an_empty_archive_is_an_empty_list() {
    let (app, _dir) = test_app().await;
    assert!(export_list(&app, "/api/export/trips").await.is_empty());
}
