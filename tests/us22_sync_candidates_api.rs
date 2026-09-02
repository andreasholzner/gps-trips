//! US-22/US-29 — what a "Sync now" run would do, as JSON.
//!
//! Acceptance criteria (US-22): "Sync now" lists Komoot tours not yet in
//! `trip_komoot_link` on a review page the owner can narrow before syncing;
//! already-linked tours are skipped (anti-join dedup). US-29 adds that each
//! row is labeled by kind, so the owner can tell a planned route from a
//! recorded tour before pulling it.
//!
//! The review page rendered all of this server-side. US-44 moves it to the
//! SPA, which needs the same facts over the API (ADR-0008) — so these are the
//! assertions that let the page go (ADR-0012's migration rule), stated
//! against the endpoint the screen reads rather than against HTML.
//!
//! `GET /api/komoot/sync` also reports what the *push* phases would send
//! (US-20's edits, US-24's deletes): those run before the pull whether or not
//! a single tour is ticked, so the screen can say what is about to leave the
//! archive as well as what is about to arrive.

mod common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{
    body_string, delete, error_message, get, json_request, send, test_app, test_app_with_komoot,
};
use trip_archive::server::komoot::testing::{a_tour, MockKomootClient};

/// The candidates endpoint's answer, parsed.
async fn sync_candidates(app: &axum::Router) -> serde_json::Value {
    let response = get(app, "/api/komoot/sync").await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_str(&body_string(response).await).expect("candidates JSON")
}

/// Pull the named tours, as the review screen's button does.
async fn sync(app: &axum::Router, tours: &[(&str, &str)]) {
    let tours: Vec<_> = tours
        .iter()
        .map(|(id, kind)| serde_json::json!({ "tour_id": id, "kind": kind }))
        .collect();
    let response = send(
        app,
        json_request(
            Method::POST,
            "/api/komoot/sync",
            &serde_json::json!({ "tours": tours }).to_string(),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// The id the import gave the trip it made for a tour (named after it, US-22).
async fn trip_id_by_name(app: &axum::Router, name: &str) -> i64 {
    let trips: serde_json::Value =
        serde_json::from_str(&body_string(get(app, "/api/trips").await).await).unwrap();
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
async fn us22_candidates_are_the_tours_not_yet_in_the_archive_each_labeled_by_kind() {
    let mock = Arc::new(MockKomootClient {
        tours: vec![a_tour("111", "Fjord Loop", "hike")],
        planned_tours: vec![a_tour("222", "Ridge Traverse", "touringbicycle")],
        ..Default::default()
    });
    let (app, _dir) = test_app_with_komoot(mock).await;

    let body = sync_candidates(&app).await;

    // US-29: both listings are offered, and each row says which it came from
    // — that label is what tells the owner which tab the trip will land on.
    let candidates = body["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2, "{body}");
    assert_eq!(candidates[0]["tour_id"], "111");
    assert_eq!(candidates[0]["name"], "Fjord Loop");
    assert_eq!(candidates[0]["sport"], "hike");
    assert_eq!(candidates[0]["kind"], "recorded");
    assert_eq!(candidates[1]["tour_id"], "222");
    assert_eq!(candidates[1]["kind"], "planned");
}

#[tokio::test]
async fn us22_a_tour_already_in_the_archive_is_not_offered_again() {
    let mock = Arc::new(MockKomootClient {
        tours: vec![
            a_tour("111", "Fjord Loop", "hike"),
            a_tour("222", "Ridge Traverse", "hike"),
        ],
        ..Default::default()
    });
    let (app, _dir) = test_app_with_komoot(mock).await;

    sync(&app, &[("111", "recorded")]).await;
    let body = sync_candidates(&app).await;

    // The anti-join dedup, seen from the screen: what is already linked is
    // not on offer, so a second run cannot import it twice.
    let ids: Vec<_> = body["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["tour_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["222"], "{body}");
}

#[tokio::test]
async fn us22_a_caught_up_archive_offers_nothing_rather_than_failing() {
    let mock = Arc::new(MockKomootClient {
        tours: vec![a_tour("111", "Fjord Loop", "hike")],
        ..Default::default()
    });
    let (app, _dir) = test_app_with_komoot(mock).await;

    sync(&app, &[("111", "recorded")]).await;
    let body = sync_candidates(&app).await;

    // An empty list is an ordinary answer — the screen still needs to render,
    // and a run with nothing to pull can still have edits to push.
    assert_eq!(body["candidates"].as_array().unwrap().len(), 0, "{body}");
    assert_eq!(body["pending_edits"], 0);
    assert_eq!(body["pending_deletes"], 0);
}

#[tokio::test]
async fn us22_the_counts_say_what_the_push_phases_would_send() {
    let mock = Arc::new(MockKomootClient {
        tours: vec![
            a_tour("111", "Fjord Loop", "hike"),
            a_tour("222", "Ridge Traverse", "hike"),
        ],
        ..Default::default()
    });
    let (app, _dir) = test_app_with_komoot(mock).await;
    sync(&app, &[("111", "recorded"), ("222", "recorded")]).await;

    // US-20: an edit to a Komoot-sourced trip queues a push.
    let edited = trip_id_by_name(&app, "Fjord Loop").await;
    let response = send(
        &app,
        json_request(
            Method::PATCH,
            &format!("/api/trips/{edited}"),
            r#"{"name":"Fjord Loop, the long way"}"#,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // US-24: deleting one queues a delete, and leaves the link row behind to
    // carry it — so the tour is not offered for import again either.
    let deleted = trip_id_by_name(&app, "Ridge Traverse").await;
    let response = delete(&app, &format!("/api/trips/{deleted}")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let body = sync_candidates(&app).await;
    assert_eq!(body["pending_edits"], 1, "{body}");
    assert_eq!(body["pending_deletes"], 1, "{body}");
    assert_eq!(body["candidates"].as_array().unwrap().len(), 0, "{body}");
}

#[tokio::test]
async fn us22_an_archive_without_komoot_credentials_says_so() {
    // `main.rs` treats Komoot as an optional integration, so the screen has
    // to be able to tell "nothing to sync" from "this archive cannot sync".
    let (app, _dir) = test_app().await;

    let response = get(&app, "/api/komoot/sync").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        error_message(response).await.contains("KOMOOT_EMAIL"),
        "the message should name what is missing"
    );
}
