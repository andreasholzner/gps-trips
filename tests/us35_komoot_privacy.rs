//! US-35 — control a linked Komoot tour's privacy_status from the archive.
//!
//! The list screen's Privacy column is the SPA's (US-41):
//! `crates/ui-dioxus/src/trip_table.rs` asserts the dash, the value and the
//! "Unknown" case. What stays here is where each privacy comes from and where
//! it goes — asserted through `GET /api/trips`.
//!
//! Acceptance criteria: the privacy_status is visible on the list page, and
//! it can be edited from the details page.
//!
//! Driven through the real router, end to end: pull a tour from a mocked
//! Komoot, read its privacy off the list page, change it via the detail
//! page's `PATCH`, and prove the next "Sync now" pushes that choice back to
//! Komoot (ADR-0021's deferred push, the same path name/activity edits take).

mod common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{body_string, get, import_sample, json_request, send, test_app, test_app_with_komoot};
use trip_archive::server::komoot::{
    testing::{a_tour_with_status, MockKomootClient, RecordedCall},
    KomootClient,
};

fn sync_request(tour_ids: &[&str]) -> axum::http::Request<axum::body::Body> {
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

fn patch_request(id: i64, body: &str) -> axum::http::Request<axum::body::Body> {
    json_request(Method::PATCH, &format!("/api/trips/{id}"), body)
}

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

/// Sync one private tour in and return the router, the mock (to inspect what
/// was pushed later) and the imported trip's id.
async fn app_with_a_synced_tour(
    status: &str,
) -> (axum::Router, Arc<MockKomootClient>, i64, tempfile::TempDir) {
    let mock = Arc::new(MockKomootClient {
        tours: vec![a_tour_with_status("111", "Fjord Loop", "hike", status)],
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = mock.clone();
    let (app, dir) = test_app_with_komoot(client).await;

    let response = send(&app, sync_request(&["111"])).await;
    assert_eq!(response.status(), StatusCode::OK);
    let trip_id = trip_id_by_name(&app, "Fjord Loop").await;
    (app, mock, trip_id, dir)
}

#[tokio::test]
async fn us35_a_linked_trips_privacy_is_carried_by_the_list() {
    let (app, _mock, _trip_id, _dir) = app_with_a_synced_tour("private").await;

    let json = body_string(get(&app, "/api/trips").await).await;

    assert!(
        json.contains(r#""privacy_status":"private""#),
        "got: {json}"
    );
}

#[tokio::test]
async fn us35_a_trip_that_never_came_from_komoot_has_no_privacy() {
    let (app, _dir) = test_app().await;
    import_sample(&app).await;

    let json = body_string(get(&app, "/api/trips").await).await;

    assert!(json.contains(r#""privacy_status":null"#), "got: {json}");
}

#[tokio::test]
async fn us35_editing_the_privacy_shows_up_on_the_list() {
    let (app, _mock, trip_id, _dir) = app_with_a_synced_tour("private").await;

    let response = send(
        &app,
        patch_request(trip_id, r#"{"privacy_status":"public"}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let json = body_string(get(&app, "/api/trips").await).await;
    assert!(json.contains(r#""privacy_status":"public""#), "got: {json}");
}

#[tokio::test]
async fn us35_the_trip_carries_its_linked_tours_privacy() {
    // What the picker opens on: the screen that renders it is the SPA's, and
    // is tested there (`ui-dioxus`'s `edit` module). What has to be true here
    // is that the link and its privacy reach the client at all.
    let (app, _mock, trip_id, _dir) = app_with_a_synced_tour("public").await;

    let trip = body_string(get(&app, &format!("/api/trips/{trip_id}")).await).await;

    assert!(trip.contains(r#""privacy":"public""#), "got: {trip}");
    assert!(trip.contains(r#""tour_id":"111""#), "got: {trip}");
}

#[tokio::test]
async fn us35_the_next_sync_pushes_the_chosen_privacy_to_komoot() {
    let (app, mock, trip_id, _dir) = app_with_a_synced_tour("private").await;

    send(
        &app,
        patch_request(trip_id, r#"{"privacy_status":"public"}"#),
    )
    .await;
    let response = send(&app, sync_request(&[])).await;
    assert_eq!(response.status(), StatusCode::OK);

    let calls = mock.calls.lock().unwrap();
    assert!(
        calls.contains(&RecordedCall::UpdateTour(
            "111".to_string(),
            Some("public".to_string())
        )),
        "the queued privacy must reach Komoot on the next sync, got {calls:?}"
    );
}

#[tokio::test]
async fn us35_a_privacy_komoot_reports_unmappably_is_shown_as_unknown_and_can_be_replaced() {
    // The archive must not pretend to know a privacy it couldn't map — and
    // the owner must still be able to set *either* value from that state,
    // including the one a bare picker would already be showing.
    let (app, mock, trip_id, _dir) = app_with_a_synced_tour("friends_only").await;

    let list = body_string(get(&app, "/api/trips").await).await;
    assert!(
        list.contains(r#""privacy_status":"unknown""#),
        "got: {list}"
    );

    // The trip carries the unmappable privacy as such, so the screen can
    // show it without claiming the owner chose it (the picker's placeholder
    // is `ui-dioxus`'s `edit` module's business, and tested there).
    let detail = body_string(get(&app, &format!("/api/trips/{trip_id}")).await).await;
    assert!(detail.contains(r#""privacy":"unknown""#), "got: {detail}");

    let response = send(
        &app,
        patch_request(trip_id, r#"{"privacy_status":"private"}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    assert!(body_string(get(&app, "/api/trips").await)
        .await
        .contains(r#""privacy_status":"private""#));
    send(&app, sync_request(&[])).await;
    assert!(
        mock.calls
            .lock()
            .unwrap()
            .contains(&RecordedCall::UpdateTour(
                "111".to_string(),
                Some("private".to_string())
            )),
        "the replacement privacy must reach Komoot"
    );
}

#[tokio::test]
async fn us35_a_caught_up_archive_still_picks_up_a_privacy_changed_inside_komoot() {
    // Nothing left to import, so a sync imports nothing and its pull lists
    // nothing. Opening the review screen lists the account anyway — that is
    // what keeps the archive's copy fresh, at no extra API cost.
    let (app, mock, _trip_id, _dir) = app_with_a_synced_tour("private").await;

    mock.set_tour_status("111", "public");
    let review: serde_json::Value =
        serde_json::from_str(&body_string(get(&app, "/api/komoot/sync").await).await).unwrap();
    assert_eq!(
        review["candidates"].as_array().unwrap().len(),
        0,
        "{review}"
    );

    let list = body_string(get(&app, "/api/trips").await).await;
    assert!(list.contains(r#""privacy_status":"public""#), "{list}");
}

#[tokio::test]
async fn us35_a_privacy_edit_on_a_trip_that_is_not_linked_to_komoot_is_rejected() {
    // There is no Komoot tour whose privacy this could possibly change.
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send(&app, patch_request(id, r#"{"privacy_status":"public"}"#)).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn us35_an_unrecognized_privacy_is_rejected_and_nothing_changes() {
    let (app, _mock, trip_id, _dir) = app_with_a_synced_tour("private").await;

    let response = send(
        &app,
        patch_request(trip_id, r#"{"privacy_status":"friends_only"}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = body_string(get(&app, "/api/trips").await).await;
    assert!(
        json.contains(r#""privacy_status":"private""#),
        "got: {json}"
    );
}

#[tokio::test]
async fn us35_the_unknown_privacy_cannot_be_set_by_hand() {
    // `unknown` is a display-only state for a Komoot value this app can't
    // map — never something the owner can push back.
    let (app, _mock, trip_id, _dir) = app_with_a_synced_tour("private").await;

    let response = send(
        &app,
        patch_request(trip_id, r#"{"privacy_status":"unknown"}"#),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
