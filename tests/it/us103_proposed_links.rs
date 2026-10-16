//! US-103 acceptance tests, server half — "proposed links".
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *a proposed link is not a link yet: it does not make a planned trip,
//!   collection track or waypoint one with a recorded trip, and it counts
//!   toward no mark, count or filter* —
//!   `us103_a_proposed_link_marks_nothing_and_counts_nowhere`.
//! * *a proposal shows in the lists of links on both ends' pages, marked as
//!   proposed* — `us103_a_proposal_is_listed_on_both_ends_as_proposed`.
//! * *proposals are the owner's alone* —
//!   `us103_a_recipient_never_sees_a_proposal`.
//! * *a confirmed proposal becomes a link* —
//!   `us103_linking_a_proposed_pair_by_hand_confirms_it`.
//!
//! Whether a trip followed a plan is unit-tested on synthetic tracks in
//! `src/server/link_match/tests.rs`.

use axum::{
    http::{Method, StatusCode},
    Router,
};
use trip_archive::models::{
    CollectionDetail, CollectionSummary, CreatedShare, Links, Proposal, SharedLinks, TripSummary,
    Waypoint,
};
use trip_archive::server::db;

use crate::common::{
    body_string, get, import_collection, import_request_with_fields, import_sample, json_request,
    router_over, send, send_unauthenticated, trip_id_from_redirect, COLLECTION_GPX, SAMPLE_GPX,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn app() -> (Router, sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let pool = db::create_pool(&dir.path().join("test.db"))
        .await
        .expect("create pool");
    (router_over(pool.clone(), dir.path()), pool, dir)
}

async fn read_json<T: serde::de::DeserializeOwned>(app: &Router, uri: &str) -> T {
    let response = get(app, uri).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{uri}: {e}: {body}"))
}

async fn planned(app: &Router) -> i64 {
    let request = import_request_with_fields(SAMPLE_GPX, &[("kind", "planned")], &[]);
    trip_id_from_redirect(&send(app, request).await)
}

/// A collection's id, its tracks' ids and its waypoints' ids.
async fn a_collection(app: &Router) -> (i64, Vec<i64>, Vec<i64>) {
    let id = import_collection(app, COLLECTION_GPX, &[("name", "Book")]).await;
    let detail: CollectionDetail = read_json(app, &format!("/api/collections/{id}")).await;
    let waypoints: Vec<Waypoint> =
        read_json(app, &format!("/api/collections/{id}/waypoints")).await;
    (
        id,
        detail.tracks.iter().map(|track| track.id).collect(),
        waypoints.iter().map(|waypoint| waypoint.id).collect(),
    )
}

/// Propose a link as the archive would, whatever the tracks' geometry.
async fn propose(pool: &sqlx::SqlitePool, table: &str, columns: (&str, &str), ends: (i64, i64)) {
    sqlx::query(&format!(
        "INSERT OR REPLACE INTO {table} ({}, {}, proposed) VALUES (?, ?, 1)",
        columns.0, columns.1
    ))
    .bind(ends.0)
    .bind(ends.1)
    .execute(pool)
    .await
    .unwrap();
}

async fn propose_plan(pool: &sqlx::SqlitePool, recorded: i64, plan: i64) {
    propose(
        pool,
        "trip_trip_link",
        ("recorded_trip_id", "planned_trip_id"),
        (recorded, plan),
    )
    .await;
}

async fn propose_track(pool: &sqlx::SqlitePool, trip: i64, track: i64) {
    propose(
        pool,
        "trip_track_link",
        ("trip_id", "collection_track_id"),
        (trip, track),
    )
    .await;
}

async fn propose_waypoint(pool: &sqlx::SqlitePool, trip: i64, waypoint: i64) {
    propose(
        pool,
        "trip_waypoint_link",
        ("trip_id", "waypoint_id"),
        (trip, waypoint),
    )
    .await;
}

async fn link(app: &Router, trip_ids: &[i64], track_ids: &[i64]) {
    let body = serde_json::json!({ "trip_ids": trip_ids, "collection_track_ids": track_ids });
    let request = json_request(Method::POST, "/api/links", &body.to_string());
    assert_eq!(send(app, request).await.status(), StatusCode::OK);
}

fn has_recorded_trip(trips: &[TripSummary], id: i64) -> bool {
    trips
        .iter()
        .find(|trip| trip.id == id)
        .expect("listed")
        .has_recorded_trip
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us103_a_proposed_link_marks_nothing_and_counts_nowhere() {
    let (app, pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let plan = planned(&app).await;
    let (book, tracks, waypoints) = a_collection(&app).await;
    propose_plan(&pool, monday, plan).await;
    propose_track(&pool, monday, tracks[0]).await;
    propose_waypoint(&pool, monday, waypoints[0]).await;

    let trips: Vec<TripSummary> = read_json(&app, "/api/trips").await;
    assert!(!has_recorded_trip(&trips, plan));
    let with: Vec<TripSummary> =
        read_json(&app, "/api/trips?kind=planned&recorded_trip=with").await;
    assert!(with.is_empty());
    let collections: Vec<CollectionSummary> = read_json(&app, "/api/collections").await;
    assert_eq!(collections[0].tracks_with_recorded_trip, 0);
    assert!(collections[0].tracks.iter().all(|t| !t.has_recorded_trip));
    let narrowed: Vec<CollectionSummary> =
        read_json(&app, "/api/collections?recorded_trip=with").await;
    assert!(narrowed.is_empty());
    let detail: CollectionDetail = read_json(&app, &format!("/api/collections/{book}")).await;
    assert!(detail.tracks.iter().all(|t| !t.has_recorded_trip));
    assert!(detail.waypoints_with_recorded_trip.is_empty());
}

#[tokio::test]
async fn us103_a_proposal_is_listed_on_both_ends_as_proposed() {
    let (app, pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let plan = planned(&app).await;
    let (_, tracks, waypoints) = a_collection(&app).await;
    propose_plan(&pool, monday, plan).await;
    propose_track(&pool, monday, tracks[0]).await;
    propose_waypoint(&pool, monday, waypoints[0]).await;
    link(&app, &[monday], &tracks[1..2]).await;

    let on_monday: Links = read_json(&app, &format!("/api/trips/{monday}/links")).await;
    assert_eq!(
        on_monday.planned_trips[0].proposal,
        Some(Proposal::default())
    );
    let by_track: Vec<(i64, bool)> = on_monday
        .collection_tracks
        .iter()
        .map(|track| (track.id, track.proposal.is_some()))
        .collect();
    assert!(by_track.contains(&(tracks[0], true)));
    assert!(by_track.contains(&(tracks[1], false)));
    assert!(on_monday.waypoints[0].proposal.is_some());
    let on_plan: Links = read_json(&app, &format!("/api/trips/{plan}/links")).await;
    assert!(on_plan.recorded_trips[0].proposal.is_some());
    let on_track: Links =
        read_json(&app, &format!("/api/collection-tracks/{}/links", tracks[0])).await;
    assert!(on_track.recorded_trips[0].proposal.is_some());
    let on_waypoint: Links =
        read_json(&app, &format!("/api/waypoints/{}/links", waypoints[0])).await;
    assert!(on_waypoint.recorded_trips[0].proposal.is_some());
}

#[tokio::test]
async fn us103_a_recipient_never_sees_a_proposal() {
    let (app, pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let plan = planned(&app).await;
    propose_plan(&pool, monday, plan).await;
    let body = serde_json::json!({ "trip_ids": [monday, plan] });
    let response = send(
        &app,
        json_request(Method::POST, "/api/shares", &body.to_string()),
    )
    .await;
    let created: CreatedShare = serde_json::from_str(&body_string(response).await).unwrap();

    for (id, uri) in [(monday, "monday"), (plan, "plan")] {
        let request = axum::http::Request::builder()
            .uri(format!("/s/{}/api/trips/{id}/links", created.token))
            .body(axum::body::Body::empty())
            .unwrap();
        let response = send_unauthenticated(&app, request).await;
        let links: SharedLinks = serde_json::from_str(&body_string(response).await).unwrap();
        assert_eq!(links, SharedLinks::default(), "{uri}");
    }
}

#[tokio::test]
async fn us103_linking_a_proposed_pair_by_hand_confirms_it() {
    let (app, pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let plan = planned(&app).await;
    propose_plan(&pool, monday, plan).await;

    link(&app, &[monday, plan], &[]).await;

    let trips: Vec<TripSummary> = read_json(&app, "/api/trips").await;
    assert!(has_recorded_trip(&trips, plan));
    let proposed: i64 = sqlx::query_scalar("SELECT proposed FROM trip_trip_link")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(proposed, 0);
}
