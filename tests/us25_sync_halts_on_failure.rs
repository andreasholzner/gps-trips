//! US-25 — a failed Komoot call halts the whole "Sync now" run immediately,
//! across phases, not just within the phase it happened in.
//!
//! Acceptance criteria: the first failed Komoot call in either the push or
//! pull phase halts the sync without attempting further items; a visible
//! error names the specific trip/tour that failed.
//!
//! `komoot_sync.rs`'s own unit tests already cover halt-on-first-failure
//! *within* a single phase (`push_pending_edits`, `push_pending_deletes`,
//! `sync_selected_tours`). What's missing there is proof that `http.rs`'s
//! `handle_sync` — which sequences push-edits -> push-deletes -> pull —
//! actually stops the *whole run* on an earlier phase's failure, exercised
//! through the real router the owner's browser talks to.

mod common;

use std::collections::HashSet;
use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{body_string, delete, get, json_request, send, test_app_with_komoot};
use trip_archive::server::komoot::{
    testing::{a_tour, MockKomootClient, RecordedCall},
    KomootClient,
};

fn sync_request(tour_ids: &[&str]) -> axum::http::Request<axum::body::Body> {
    // These tests sync recorded tours; the review page tags each selection
    // with its kind (US-29).
    let tours: Vec<_> = tour_ids
        .iter()
        .map(|s| serde_json::json!({ "tour_id": s, "kind": "recorded" }))
        .collect();
    json_request(
        Method::POST,
        "/api/komoot/sync",
        &serde_json::json!({ "tours": tours }).to_string(),
    )
}

/// Find the trip id list_trips assigned to a tour imported by name (the
/// import pipeline names the trip after the tour, US-22).
async fn trip_id_by_name(app: &axum::Router, name: &str) -> i64 {
    let body = body_string(get(app, "/api/trips").await).await;
    let trips: serde_json::Value = serde_json::from_str(&body).unwrap();
    trips
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == name)
        .unwrap_or_else(|| panic!("no trip named {name} in {trips}"))["id"]
        .as_i64()
        .unwrap()
}

#[tokio::test]
async fn us25_happy_path_reports_no_failure() {
    let mock = Arc::new(MockKomootClient {
        tours: vec![a_tour("111", "Fjord Loop", "hike")],
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = mock;
    let (app, _dir) = test_app_with_komoot(client).await;

    let response = send(&app, sync_request(&["111"])).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(body["imported"], 1);
    assert!(body["failed_tour"].is_null());
}

#[tokio::test]
async fn us25_pull_phase_halts_and_never_attempts_the_next_selected_tour() {
    let mock = Arc::new(MockKomootClient {
        tours: vec![
            a_tour("111", "Fails to pull", "hike"),
            a_tour("222", "Never attempted", "hike"),
        ],
        fail_get_tour_gpx_for: HashSet::from(["111".to_string()]),
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = Arc::clone(&mock) as Arc<dyn KomootClient>;
    let (app, _dir) = test_app_with_komoot(client).await;

    let response = send(&app, sync_request(&["111", "222"])).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(body["imported"], 0);
    assert_eq!(body["failed_tour"], "111");
    assert_eq!(body["failed_phase"], "pull");

    let calls = mock.calls.lock().unwrap();
    assert!(
        !calls.contains(&RecordedCall::GetTourGpx("222".to_string())),
        "tour 222 must never have been attempted after 111 failed: {calls:?}"
    );
}

#[tokio::test]
async fn us25_push_edit_failure_halts_before_the_pull_phase_even_starts() {
    let mock = Arc::new(MockKomootClient {
        tours: vec![
            a_tour("111", "Edit Push Fails", "hike"),
            a_tour("222", "Never attempted", "hike"),
        ],
        fail_update_tour_for: HashSet::from(["111".to_string()]),
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = Arc::clone(&mock) as Arc<dyn KomootClient>;
    let (app, _dir) = test_app_with_komoot(client).await;

    // First run: pull tour 111 only, to create a Komoot-linked trip.
    let response = send(&app, sync_request(&["111"])).await;
    assert_eq!(response.status(), StatusCode::OK);
    let trip_id = trip_id_by_name(&app, "Edit Push Fails").await;

    // Edit it, so its link row becomes edit_pending (US-20).
    let response = send(
        &app,
        json_request(
            Method::PATCH,
            &format!("/api/trips/{trip_id}"),
            r#"{"name":"Renamed","activity_type":"hiking"}"#,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Second run: the pending edit's push fails (tour 111 is in
    // fail_update_tour_for) — tour 222's pull must never be attempted.
    let response = send(&app, sync_request(&["222"])).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(body["imported"], 0);
    assert_eq!(body["failed_tour"], "111");
    assert_eq!(body["failed_phase"], "push");

    let calls = mock.calls.lock().unwrap();
    assert!(
        !calls.contains(&RecordedCall::GetTourGpx("222".to_string())),
        "the pull phase must never have started: {calls:?}"
    );
}

#[tokio::test]
async fn us25_push_delete_failure_halts_before_the_pull_phase_even_starts() {
    let mock = Arc::new(MockKomootClient {
        tours: vec![
            a_tour("111", "Delete Push Fails", "hike"),
            a_tour("222", "Never attempted", "hike"),
        ],
        fail_delete_tour_for: HashSet::from(["111".to_string()]),
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = Arc::clone(&mock) as Arc<dyn KomootClient>;
    let (app, _dir) = test_app_with_komoot(client).await;

    // First run: pull tour 111 only, to create a Komoot-linked trip.
    let response = send(&app, sync_request(&["111"])).await;
    assert_eq!(response.status(), StatusCode::OK);
    let trip_id = trip_id_by_name(&app, "Delete Push Fails").await;

    // Delete it, so its link row becomes delete_pending (US-24).
    let response = delete(&app, &format!("/api/trips/{trip_id}")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Second run: the pending delete's push fails (tour 111 is in
    // fail_delete_tour_for) — tour 222's pull must never be attempted.
    let response = send(&app, sync_request(&["222"])).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(body["imported"], 0);
    assert_eq!(body["failed_tour"], "111");
    assert_eq!(body["failed_phase"], "push");

    let calls = mock.calls.lock().unwrap();
    assert!(
        !calls.contains(&RecordedCall::GetTourGpx("222".to_string())),
        "the pull phase must never have started: {calls:?}"
    );
}

#[tokio::test]
async fn us25_failure_banner_names_the_specific_failed_tour() {
    let mock = Arc::new(MockKomootClient {
        tours: vec![a_tour("111", "Fails to pull", "hike")],
        fail_get_tour_gpx_for: HashSet::from(["111".to_string()]),
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = mock;
    let (app, _dir) = test_app_with_komoot(client).await;

    let response = send(&app, sync_request(&["111"])).await;
    let body: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    let params = format!(
        "failed_tour={}&failed_msg={}&failed_phase={}",
        body["failed_tour"].as_str().unwrap(),
        urlencoding_stub(body["failed_msg"].as_str().unwrap()),
        body["failed_phase"].as_str().unwrap(),
    );
    let page = body_string(get(&app, &format!("/komoot/sync?{params}")).await).await;
    assert!(
        page.contains("111"),
        "the review page banner must name the failed tour: {page}"
    );
}

/// Minimal query-string escaping for the one banner test above — no need
/// for a real percent-encoding crate dependency for a handful of ASCII
/// words in a test-only fixture message.
fn urlencoding_stub(s: &str) -> String {
    s.replace(' ', "%20").replace(':', "%3A")
}
