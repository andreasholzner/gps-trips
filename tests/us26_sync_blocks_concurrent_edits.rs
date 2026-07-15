//! US-26 — editing or deleting a trip is blocked while a sync is running.
//!
//! Acceptance criteria: `PATCH`/`DELETE` requests made while a sync is in
//! flight are rejected with `409`. Only one sync runs at a time (single
//! in-process flag, ADR-0021).
//!
//! `state.set_sync_in_progress_for_test` sets the flag directly here to
//! simulate "a sync is in flight" rather than racing a real background sync
//! against these requests — the flag itself is the thing under test (see
//! `src/server/state.rs`'s own unit tests for `SyncGuard`/`try_start_sync`),
//! so this is deterministic instead of timing-dependent.

mod common;

use std::collections::HashSet;
use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{
    body_string, delete, import_sample, json_request, send, test_app_with_komoot,
    test_app_with_state,
};
use trip_archive::server::komoot::{
    testing::{a_tour, MockKomootClient},
    KomootClient,
};

fn patch_request(id: i64, body: &str) -> axum::http::Request<axum::body::Body> {
    json_request(Method::PATCH, &format!("/api/trips/{id}"), body)
}

fn sync_request(tour_ids: &[&str]) -> axum::http::Request<axum::body::Body> {
    let ids: Vec<String> = tour_ids.iter().map(|s| s.to_string()).collect();
    json_request(
        Method::POST,
        "/api/komoot/sync",
        &serde_json::json!({ "tour_ids": ids }).to_string(),
    )
}

#[tokio::test]
async fn us26_edit_is_rejected_409_while_a_sync_is_in_flight_then_succeeds_once_cleared() {
    let (app, state, _dir) = test_app_with_state(None).await;
    let id = import_sample(&app).await;

    state.set_sync_in_progress_for_test(true);
    let response = send(&app, patch_request(id, r#"{"name":"Renamed"}"#)).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    state.set_sync_in_progress_for_test(false);
    let response = send(&app, patch_request(id, r#"{"name":"Renamed"}"#)).await;
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "editing must succeed again once the sync flag is cleared"
    );
}

#[tokio::test]
async fn us26_delete_is_rejected_409_while_a_sync_is_in_flight_then_succeeds_once_cleared() {
    let (app, state, _dir) = test_app_with_state(None).await;
    let id = import_sample(&app).await;

    state.set_sync_in_progress_for_test(true);
    let response = delete(&app, &format!("/api/trips/{id}")).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    state.set_sync_in_progress_for_test(false);
    let response = delete(&app, &format!("/api/trips/{id}")).await;
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "deleting must succeed again once the sync flag is cleared"
    );
}

#[tokio::test]
async fn us26_a_second_sync_is_rejected_409_while_one_is_already_in_flight() {
    let mock = Arc::new(MockKomootClient::default());
    let client: Arc<dyn KomootClient> = mock;
    let (app, state, _dir) = test_app_with_state(Some(client)).await;

    state.set_sync_in_progress_for_test(true);
    let response = send(&app, sync_request(&[])).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    state.set_sync_in_progress_for_test(false);
    let response = send(&app, sync_request(&[])).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a sync must succeed again once the flag is cleared"
    );
}

#[tokio::test]
async fn us26_guard_releases_after_a_failed_sync_so_edit_delete_and_sync_still_work() {
    let mock = Arc::new(MockKomootClient {
        tours: vec![a_tour("111", "Fails to pull", "hike")],
        fail_get_tour_gpx_for: HashSet::from(["111".to_string()]),
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = mock;
    let (app, _dir) = test_app_with_komoot(client).await;
    let id = import_sample(&app).await;

    // This sync halts on its first (and only) failure (US-25) — the guard
    // must still release, not stay claimed forever.
    let response = send(&app, sync_request(&["111"])).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(body["failed_tour"], "111");

    let response = send(&app, patch_request(id, r#"{"name":"Still Editable"}"#)).await;
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "a halted sync must not leave edits permanently locked out"
    );

    let response = send(&app, sync_request(&[])).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a halted sync must not leave later syncs permanently locked out"
    );
}
