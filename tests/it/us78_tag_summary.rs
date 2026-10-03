//! US-78 acceptance tests — the server's half of the tag summary.
//!
//! The figures are added up in the SPA (`crates/ui-dioxus`, `summary`). What
//! stays here is what it adds them up from: `GET /api/stats/tags?tags=…`
//! answers, per chosen tag in the order asked, the dated recorded trips under
//! it and how many undated ones it has; each trip comes once, however many
//! of the chosen tags it is under.
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

use axum::http::{Method, StatusCode};

use crate::common::{
    body_string, get, import, import_request_with_fields, json_request, send, test_app,
    trip_id_from_redirect, SAMPLE_GPX,
};

const UNTIMED_GPX: &[u8] = include_bytes!("../fixtures/untimed.gpx");

async fn summary(app: &axum::Router, tags: &str) -> serde_json::Value {
    let response = get(app, &format!("/api/stats/tags?tags={tags}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_str(&body_string(response).await).unwrap()
}

async fn import_with(app: &axum::Router, gpx: &[u8], fields: &[(&str, &str)]) -> i64 {
    let response = send(app, import_request_with_fields(gpx, fields, &[])).await;
    trip_id_from_redirect(&response)
}

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

fn ids(value: &serde_json::Value) -> Vec<i64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_i64().unwrap())
        .collect()
}

fn trip_ids(summary: &serde_json::Value) -> Vec<i64> {
    summary["trips"]
        .as_array()
        .unwrap()
        .iter()
        .map(|trip| trip["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn us78_each_chosen_tag_names_its_recorded_trips_in_the_order_asked() {
    let (app, _dir) = test_app().await;
    let both = import_with(&app, SAMPLE_GPX, &[]).await;
    let alps_only = import_with(&app, SAMPLE_GPX, &[]).await;
    let planned = import_with(&app, SAMPLE_GPX, &[("kind", "planned")]).await;
    let untagged = import_with(&app, SAMPLE_GPX, &[]).await;
    tag_trip(&app, both, "alps").await;
    tag_trip(&app, both, "norway").await;
    tag_trip(&app, alps_only, "alps").await;
    tag_trip(&app, planned, "norway").await;

    let summary = summary(&app, "norway,alps").await;

    assert_eq!(summary["tags"][0]["name"], "norway");
    assert_eq!(ids(&summary["tags"][0]["trip_ids"]), [both]);
    assert_eq!(summary["tags"][1]["name"], "alps");
    assert_eq!(ids(&summary["tags"][1]["trip_ids"]), [both, alps_only]);
    // A trip under both tags comes once; planned and untagged ones not at all.
    assert_eq!(trip_ids(&summary), [both, alps_only]);
    assert!(!trip_ids(&summary).contains(&untagged));
}

#[tokio::test]
async fn us78_a_trip_comes_with_its_descent_as_well() {
    let (app, _dir) = test_app().await;
    let id = import_with(&app, SAMPLE_GPX, &[("activity_type", "hiking")]).await;
    tag_trip(&app, id, "alps").await;

    let summary = summary(&app, "alps").await;

    let trip = &summary["trips"][0];
    assert_eq!(trip["activity_type"], "hiking");
    assert_eq!(trip["start_date"], "2024-06-01");
    assert!(trip["descent_m"].as_f64().is_some(), "{trip}");
    assert_eq!(trip["moving_secs"], 3600);
}

#[tokio::test]
async fn us78_undated_trips_are_only_counted_and_per_tag() {
    let (app, _dir) = test_app().await;
    let dated = import_with(&app, SAMPLE_GPX, &[]).await;
    let response = import(&app, UNTIMED_GPX).await;
    let undated = trip_id_from_redirect(&response);
    tag_trip(&app, dated, "alps").await;
    tag_trip(&app, undated, "alps").await;
    tag_trip(&app, dated, "norway").await;

    let summary = summary(&app, "alps,norway").await;

    assert_eq!(ids(&summary["tags"][0]["trip_ids"]), [dated]);
    assert_eq!(summary["tags"][0]["undated"], 1);
    assert_eq!(summary["tags"][1]["undated"], 0);
    assert_eq!(trip_ids(&summary), [dated]);
}

#[tokio::test]
async fn us78_tag_names_are_normalized_and_an_unknown_one_has_no_trips() {
    let (app, _dir) = test_app().await;
    let id = import_with(&app, SAMPLE_GPX, &[]).await;
    tag_trip(&app, id, "alps").await;

    let summary = summary(&app, "Alps,nowhere").await;

    assert_eq!(summary["tags"][0]["name"], "alps");
    assert_eq!(ids(&summary["tags"][0]["trip_ids"]), [id]);
    assert_eq!(summary["tags"][1]["name"], "nowhere");
    assert_eq!(ids(&summary["tags"][1]["trip_ids"]), Vec::<i64>::new());
}

#[tokio::test]
async fn us78_no_tags_ask_for_nothing() {
    let (app, _dir) = test_app().await;
    import_with(&app, SAMPLE_GPX, &[]).await;

    let response = get(&app, "/api/stats/tags").await;
    assert_eq!(response.status(), StatusCode::OK);
    let summary: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    assert_eq!(summary["tags"], serde_json::json!([]));
    assert_eq!(summary["trips"], serde_json::json!([]));
}

#[tokio::test]
async fn us78_a_malformed_tag_name_is_refused() {
    let (app, _dir) = test_app().await;

    let response = get(&app, "/api/stats/tags?tags=day%20trip").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
