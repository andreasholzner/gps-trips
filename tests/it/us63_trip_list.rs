//! US-63 acceptance tests — "the trip list shows me where the trips are, a
//! page at a time, and works on a phone".
//!
//! The map, the paging and the phone layout are the SPA's
//! (`crates/ui-dioxus`, and the browser layer for what only a rendered page
//! shows). What stays here is the server's half: the bounding box the list's
//! rows now carry, so the map has somewhere to put each trip; and setting one
//! activity type on many trips in one request — all or nothing, refused
//! during a sync (US-26), and queued for Komoot per linked trip (US-22).
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

use std::sync::Arc;

use crate::common::{
    body_string, error_message, get, import_sample, json_request, send, test_app,
    test_app_with_komoot, test_app_with_state,
};
use axum::http::{Method, StatusCode};
use trip_archive::server::komoot::{
    testing::{a_tour, MockKomootClient, RecordedCall},
    KomootClient,
};

fn bulk_activity_request(body: &str) -> axum::http::Request<axum::body::Body> {
    json_request(Method::POST, "/api/trips/activity_type", body)
}

async fn activity_of(app: &axum::Router, id: i64) -> String {
    let trip: serde_json::Value =
        serde_json::from_str(&body_string(get(app, &format!("/api/trips/{id}")).await).await)
            .unwrap();
    trip["activity_type"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn us63_each_listed_trip_carries_its_bounding_box() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let list: serde_json::Value =
        serde_json::from_str(&body_string(get(&app, "/api/trips").await).await).unwrap();
    let detail: serde_json::Value =
        serde_json::from_str(&body_string(get(&app, &format!("/api/trips/{id}")).await).await)
            .unwrap();
    let row = &list[0];

    // The same stored box the detail screen reads, not a second computation.
    for corner in ["min_lat", "min_lon", "max_lat", "max_lon"] {
        assert!(row[corner].is_number(), "{corner}: {row}");
        assert_eq!(row[corner], detail[corner], "{corner}");
    }
}

#[tokio::test]
async fn us63_one_request_sets_the_activity_of_every_selected_trip() {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;
    let b = import_sample(&app).await;
    let untouched = import_sample(&app).await;

    let response = send(
        &app,
        bulk_activity_request(&format!(
            r#"{{"trip_ids":[{a},{b}],"activity_type":"cycling"}}"#
        )),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(activity_of(&app, a).await, "cycling");
    assert_eq!(activity_of(&app, b).await, "cycling");
    assert_eq!(activity_of(&app, untouched).await, "unknown");
}

#[tokio::test]
async fn us63_a_blank_activity_resets_the_trips_to_unspecified() {
    // The same closed set the edit form offers, blank included (ADR-0018).
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;
    send(
        &app,
        bulk_activity_request(&format!(
            r#"{{"trip_ids":[{id}],"activity_type":"hiking"}}"#
        )),
    )
    .await;

    let response = send(
        &app,
        bulk_activity_request(&format!(r#"{{"trip_ids":[{id}],"activity_type":""}}"#)),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(activity_of(&app, id).await, "unknown");
}

#[tokio::test]
async fn us63_an_unrecognised_activity_is_rejected_and_nothing_changes() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send(
        &app,
        bulk_activity_request(&format!(
            r#"{{"trip_ids":[{id}],"activity_type":"unicycling"}}"#
        )),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(activity_of(&app, id).await, "unknown");
}

#[tokio::test]
async fn us63_no_selected_trips_is_rejected() {
    let (app, _dir) = test_app().await;

    let response = send(
        &app,
        bulk_activity_request(r#"{"trip_ids":[],"activity_type":"hiking"}"#),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(error_message(response).await.contains("no trips selected"));
}

#[tokio::test]
async fn us63_an_unknown_trip_404s_and_changes_none_of_the_others() {
    // All or nothing: a failure leaves the selection as it was rather than
    // half-changed.
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send(
        &app,
        bulk_activity_request(&format!(
            r#"{{"trip_ids":[{id},9999],"activity_type":"hiking"}}"#
        )),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(activity_of(&app, id).await, "unknown");
}

#[tokio::test]
async fn us63_a_bulk_activity_change_is_refused_while_a_sync_is_in_flight() {
    // US-26, exactly as a single trip's `PATCH`.
    let (app, state, _dir) = test_app_with_state(None).await;
    let id = import_sample(&app).await;

    state.set_sync_in_progress_for_test(true);
    let response = send(
        &app,
        bulk_activity_request(&format!(
            r#"{{"trip_ids":[{id}],"activity_type":"hiking"}}"#
        )),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(activity_of(&app, id).await, "unknown");
}

#[tokio::test]
async fn us63_the_next_sync_pushes_the_change_for_every_linked_trip() {
    // US-22: a linked trip's edit is queued for Komoot — here, once for each
    // linked trip in the selection.
    let mock = Arc::new(MockKomootClient {
        tours: vec![
            a_tour("111", "Fjord Loop", "hike"),
            a_tour("222", "Ridge Walk", "hike"),
        ],
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = mock.clone();
    let (app, _dir) = test_app_with_komoot(client).await;
    let pull = serde_json::json!({ "tours": [
        { "tour_id": "111", "kind": "recorded" },
        { "tour_id": "222", "kind": "recorded" },
    ] });
    let response = send(
        &app,
        json_request(Method::POST, "/api/komoot/sync", &pull.to_string()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let local = import_sample(&app).await;
    let trips: serde_json::Value =
        serde_json::from_str(&body_string(get(&app, "/api/trips").await).await).unwrap();
    let ids: Vec<i64> = trips
        .as_array()
        .unwrap()
        .iter()
        .map(|trip| trip["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids.len(), 3, "two pulled and one local: {trips}");

    let response = send(
        &app,
        bulk_activity_request(
            &serde_json::json!({ "trip_ids": ids, "activity_type": "cycling" }).to_string(),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(activity_of(&app, local).await, "cycling");

    let push = serde_json::json!({ "tours": [] });
    let response = send(
        &app,
        json_request(Method::POST, "/api/komoot/sync", &push.to_string()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let calls = mock.calls.lock().unwrap();
    for tour in ["111", "222"] {
        assert!(
            calls
                .iter()
                .any(|call| matches!(call, RecordedCall::UpdateTour(id, _) if id == tour)),
            "tour {tour} must be pushed on the next sync, got {calls:?}"
        );
    }
}
