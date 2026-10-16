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
//! * *whatever enters the archive is checked against everything it can be
//!   linked to: a recorded or planned trip imported, a collection imported or
//!   added to, a collection item replaced on re-import* —
//!   `us103_an_imported_trip_is_checked_against_its_plans`,
//!   `us103_a_trip_confirmed_from_a_staged_import_is_checked`,
//!   `us103_an_imported_collection_is_checked_against_the_trips`,
//!   `us103_what_a_file_adds_to_a_collection_is_checked`,
//!   `us103_a_collection_item_replaced_on_re_import_is_checked_again`.
//!   A tour synced from Komoot is checked as
//!   `src/server/komoot_sync/tests/proposals.rs` asserts.
//!
//! * *the owner can start the same check by hand from a recorded trip's, a
//!   planned trip's or a collection's page* —
//!   `us103_the_owner_starts_a_check_from_a_trip_or_a_collection`,
//!   `us103_a_check_of_something_missing_is_not_found`.
//! * *the Proposed links page lists every proposed link with its coverage,
//!   and the navigation the number of them* —
//!   `us103_every_proposal_is_listed_and_counted`.
//! * *a selection confirmed or dismissed in one action; a confirmed proposal
//!   becomes a link, a dismissed one is deleted* —
//!   `us103_a_selection_of_proposals_is_confirmed_or_dismissed_at_once`,
//!   `us103_settling_something_not_proposed_changes_nothing`.
//!
//! Whether a trip followed a plan is unit-tested on synthetic tracks in
//! `src/server/link_match/tests.rs`, every allowed pair of a check in
//! `src/server/link_check/tests.rs`.

use axum::{
    http::{Method, StatusCode},
    Router,
};
use trip_archive::models::{
    CollectionDetail, CollectionSummary, CreatedShare, Links, Proposal, SharedLinks, TripSummary,
    Waypoint,
};
use trip_archive::models::{
    CollectionImportReview, LinkOther, LinkPair, ProposalCount, ProposedLink,
};
use trip_archive::server::{db, repo};

use crate::common::{
    body_string, collection_files_request, confirm_import_request, get, import, import_collection,
    import_request_with_fields, import_sample, json_request, router_over, send,
    send_unauthenticated, stage_import, trip_id_from_redirect, COLLECTION_GPX, SAMPLE_GPX,
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

/// A GPX of one track through `points`, `(lat, lon)`, without times.
fn gpx_through(points: &[(f64, f64)]) -> Vec<u8> {
    let points: String = points
        .iter()
        .map(|(lat, lon)| format!(r#"<trkpt lat="{lat}" lon="{lon}"><ele>1000</ele></trkpt>"#))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1">
<trk><name>Walk</name><trkseg>{points}</trkseg></trk>
</gpx>
"#
    )
    .into_bytes()
}

/// Along the collection fixture's "Stage 1: Splügen", to San Bernardino Pass.
fn along_stage_1() -> Vec<u8> {
    gpx_through(&[(46.55, 9.32), (46.53, 9.30), (46.4958, 9.1705)])
}

async fn proposed(pool: &sqlx::SqlitePool) -> Vec<LinkPair> {
    let mut pairs: Vec<LinkPair> = repo::list_proposals(pool)
        .await
        .unwrap()
        .iter()
        .map(ProposedLink::pair)
        .collect();
    pairs.sort();
    pairs
}

fn pair(trip_id: i64, other: LinkOther) -> LinkPair {
    LinkPair { trip_id, other }
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

#[tokio::test]
async fn us103_an_imported_trip_is_checked_against_its_plans() {
    let (app, pool, _dir) = app().await;
    let plan = planned(&app).await;

    let monday = import_sample(&app).await;

    assert_eq!(
        proposed(&pool).await,
        vec![pair(monday, LinkOther::Trip(plan))]
    );
}

#[tokio::test]
async fn us103_a_trip_confirmed_from_a_staged_import_is_checked() {
    let (app, pool, _dir) = app().await;
    let plan = planned(&app).await;
    let staged = stage_import(&app, SAMPLE_GPX).await;

    let response = send(&app, confirm_import_request(staged.staging_id, "{}")).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let pairs = proposed(&pool).await;
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].other, LinkOther::Trip(plan));
}

#[tokio::test]
async fn us103_an_imported_collection_is_checked_against_the_trips() {
    let (app, pool, _dir) = app().await;
    let monday = trip_id_from_redirect(&import(&app, &along_stage_1()).await);

    let (_, tracks, waypoints) = a_collection(&app).await;

    // Stage 1 and the pass at its end; Stage 2 only starts there.
    assert_eq!(
        proposed(&pool).await,
        vec![
            pair(monday, LinkOther::CollectionTrack(tracks[0])),
            pair(monday, LinkOther::Waypoint(waypoints[0])),
        ]
    );
}

/// Add `gpx` to the collection `book` as the review proposes.
async fn add_file(app: &Router, book: i64, gpx: &[u8]) {
    let id = book.to_string();
    let response = send(
        app,
        collection_files_request(
            "/api/collection-imports",
            &[("more.gpx", gpx)],
            &[("collection_id", id.as_str())],
        ),
    )
    .await;
    let review: CollectionImportReview =
        serde_json::from_str(&body_string(response).await).unwrap();
    let body = serde_json::json!({ "decisions": {}, "held_digest": review.held_digest });
    let uri = format!("/api/collection-imports/{}/confirm", review.staging_id);
    let response = send(app, json_request(Method::POST, &uri, &body.to_string())).await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn us103_what_a_file_adds_to_a_collection_is_checked() {
    let (app, pool, _dir) = app().await;
    let book = import_collection(&app, &gpx_through(&[(46.0, 9.0), (46.01, 9.0)]), &[]).await;
    let monday = trip_id_from_redirect(&import(&app, &along_stage_1()).await);
    assert!(proposed(&pool).await.is_empty());

    add_file(&app, book, COLLECTION_GPX).await;

    let detail: CollectionDetail = read_json(&app, &format!("/api/collections/{book}")).await;
    let stage_1 = detail
        .tracks
        .iter()
        .find(|t| t.name.starts_with("Stage 1"))
        .unwrap();
    let pairs = proposed(&pool).await;
    assert!(
        pairs.contains(&pair(monday, LinkOther::CollectionTrack(stage_1.id))),
        "{pairs:?}"
    );
}

#[tokio::test]
async fn us103_a_collection_item_replaced_on_re_import_is_checked_again() {
    let (app, pool, _dir) = app().await;
    let monday = trip_id_from_redirect(&import(&app, &along_stage_1()).await);
    let (book, tracks, _) = a_collection(&app).await;
    let to_stage_1 = pair(monday, LinkOther::CollectionTrack(tracks[0]));
    assert!(repo::dismiss_proposals(&pool, &[to_stage_1]).await.unwrap());
    // A corrected file, a point of Stage 1 moved some 55 m — beyond the
    // tolerance of the same track (ADR-0029), so it is replaced.
    let corrected = String::from_utf8(COLLECTION_GPX.to_vec()).unwrap().replace(
        r#"lat="46.5300" lon="9.3000""#,
        r#"lat="46.5305" lon="9.3000""#,
    );

    add_file(&app, book, corrected.as_bytes()).await;

    assert!(proposed(&pool).await.contains(&to_stage_1));
}

// ── The API ──────────────────────────────────────────────────────────────────

async fn post_json(app: &Router, uri: &str, body: &serde_json::Value) -> axum::response::Response {
    send(app, json_request(Method::POST, uri, &body.to_string())).await
}

async fn proposal_count(app: &Router) -> i64 {
    read_json::<ProposalCount>(app, "/api/link-proposals/count")
        .await
        .count
}

#[tokio::test]
async fn us103_every_proposal_is_listed_and_counted() {
    let (app, _pool, _dir) = app().await;
    let plan = planned(&app).await;
    let monday = trip_id_from_redirect(&import(&app, &along_stage_1()).await);
    let tuesday = import_sample(&app).await;
    let (_, tracks, _) = a_collection(&app).await;

    let listed: Vec<ProposedLink> = read_json(&app, "/api/link-proposals").await;

    assert_eq!(proposal_count(&app).await, 3);
    let pairs: Vec<LinkPair> = listed.iter().map(ProposedLink::pair).collect();
    assert!(pairs.contains(&pair(tuesday, LinkOther::Trip(plan))));
    assert!(pairs.contains(&pair(monday, LinkOther::CollectionTrack(tracks[0]))));
    let to_stage = listed
        .iter()
        .find(|p| p.pair() == pair(monday, LinkOther::CollectionTrack(tracks[0])))
        .unwrap();
    assert!(to_stage.proposal.plan_covered.unwrap() > 0.9);
    assert!(to_stage.proposal.trip_on_plan.unwrap() > 0.9);
}

#[tokio::test]
async fn us103_a_selection_of_proposals_is_confirmed_or_dismissed_at_once() {
    let (app, _pool, _dir) = app().await;
    let monday = trip_id_from_redirect(&import(&app, &along_stage_1()).await);
    let (_, tracks, waypoints) = a_collection(&app).await;
    let to_stage = pair(monday, LinkOther::CollectionTrack(tracks[0]));
    let to_pass = pair(monday, LinkOther::Waypoint(waypoints[0]));

    let confirmed = post_json(
        &app,
        "/api/link-proposals/confirm",
        &serde_json::json!({ "pairs": [to_stage] }),
    )
    .await;
    let dismissed = post_json(
        &app,
        "/api/link-proposals/dismiss",
        &serde_json::json!({ "pairs": [to_pass] }),
    )
    .await;

    assert_eq!(confirmed.status(), StatusCode::NO_CONTENT);
    assert_eq!(dismissed.status(), StatusCode::NO_CONTENT);
    assert_eq!(proposal_count(&app).await, 0);
    let on_monday: Links = read_json(&app, &format!("/api/trips/{monday}/links")).await;
    assert_eq!(on_monday.collection_tracks.len(), 1);
    assert_eq!(on_monday.collection_tracks[0].proposal, None);
    assert!(on_monday.waypoints.is_empty());
}

#[tokio::test]
async fn us103_settling_something_not_proposed_changes_nothing() {
    let (app, _pool, _dir) = app().await;
    let monday = trip_id_from_redirect(&import(&app, &along_stage_1()).await);
    let (_, tracks, _) = a_collection(&app).await;
    let to_stage = pair(monday, LinkOther::CollectionTrack(tracks[0]));
    let to_stage_2 = pair(monday, LinkOther::CollectionTrack(tracks[1]));

    for action in ["confirm", "dismiss"] {
        let uri = format!("/api/link-proposals/{action}");
        let body = serde_json::json!({ "pairs": [to_stage, to_stage_2] });
        let response = post_json(&app, &uri, &body).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{action}");
        let empty = post_json(&app, &uri, &serde_json::json!({ "pairs": [] })).await;
        assert_eq!(empty.status(), StatusCode::BAD_REQUEST, "{action}");
    }
    assert_eq!(proposal_count(&app).await, 2);
}

#[tokio::test]
async fn us103_the_owner_starts_a_check_from_a_trip_or_a_collection() {
    let (app, pool, _dir) = app().await;
    let monday = trip_id_from_redirect(&import(&app, &along_stage_1()).await);
    let plan = planned(&app).await;
    let (book, tracks, waypoints) = a_collection(&app).await;
    let to_stage = pair(monday, LinkOther::CollectionTrack(tracks[0]));
    let to_pass = pair(monday, LinkOther::Waypoint(waypoints[0]));
    assert!(repo::dismiss_proposals(&pool, &[to_stage, to_pass])
        .await
        .unwrap());

    let response = post_json(
        &app,
        &format!("/api/collections/{book}/link-check"),
        &serde_json::json!({}),
    )
    .await;
    let status = response.status();
    let found: ProposalCount = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(found.count, 2);
    assert!(repo::dismiss_proposals(&pool, &[to_stage, to_pass])
        .await
        .unwrap());

    let response = post_json(
        &app,
        &format!("/api/trips/{monday}/link-check"),
        &serde_json::json!({}),
    )
    .await;
    let found: ProposalCount = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(found.count, 2);
    let response = post_json(
        &app,
        &format!("/api/trips/{plan}/link-check"),
        &serde_json::json!({}),
    )
    .await;
    let found: ProposalCount = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(found.count, 0, "the planned trip follows nothing here");
}

#[tokio::test]
async fn us103_a_check_of_something_missing_is_not_found() {
    let (app, _pool, _dir) = app().await;

    for uri in [
        "/api/trips/9999/link-check",
        "/api/collections/9999/link-check",
    ] {
        let response = post_json(&app, uri, &serde_json::json!({})).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}
