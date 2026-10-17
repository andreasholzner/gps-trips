//! US-107 acceptance tests — the server's half of a tag summary that shows
//! everything a tag holds.
//!
//! `GET /api/stats/tags?tags=…` answers, beside the dated recorded trips the
//! figures count (US-78), the trips it lists without counting — planned
//! ones and recorded ones without timestamps — the collections carrying a
//! tag with all their tracks, and the collection tracks the tag reaches on
//! their own. A summary share (US-82) still reaches the counted trips alone.
//! What the screen lists, draws and notes is asserted in the UI crate
//! (`summary`).
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use trip_archive::models::{CollectionDetail, CreatedShare, TagSummaries};

use crate::common::{
    body_string, get, import_collection, import_request_with_fields, json_request, send,
    send_unauthenticated, test_app, trip_id_from_redirect, COLLECTION_GPX, COLLECTION_SINGLE_GPX,
    SAMPLE_GPX,
};

const UNTIMED_GPX: &[u8] = include_bytes!("../fixtures/untimed.gpx");

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn import_with(app: &Router, gpx: &[u8], fields: &[(&str, &str)]) -> i64 {
    let response = send(app, import_request_with_fields(gpx, fields, &[])).await;
    trip_id_from_redirect(&response)
}

async fn tag(app: &Router, uri: &str, name: &str) {
    let body = serde_json::json!({ "name": name });
    let response = send(app, json_request(Method::POST, uri, &body.to_string())).await;
    assert_eq!(response.status(), StatusCode::CREATED, "{uri}");
}

async fn summary(app: &Router, tags: &str) -> TagSummaries {
    let response = get(app, &format!("/api/stats/tags?tags={tags}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_str(&body_string(response).await).unwrap()
}

/// A collection named `name`: its id and its tracks' ids, in file order.
async fn a_collection(app: &Router, gpx: &[u8], name: &str) -> (i64, Vec<i64>) {
    let id = import_collection(app, gpx, &[("name", name)]).await;
    let response = get(app, &format!("/api/collections/{id}")).await;
    let detail: CollectionDetail = serde_json::from_str(&body_string(response).await).unwrap();
    (id, detail.tracks.iter().map(|track| track.id).collect())
}

/// What the recipient of a share of `tags` is answered, as raw JSON.
async fn shared(app: &Router, tags: &[&str]) -> String {
    let body = serde_json::json!({ "tags": tags });
    let response = send(
        app,
        json_request(Method::POST, "/api/shares", &body.to_string()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let token = serde_json::from_str::<CreatedShare>(&body_string(response).await)
        .unwrap()
        .token;
    let response = send_unauthenticated(
        app,
        Request::builder()
            .uri(format!("/s/{token}/api/share"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    body_string(response).await
}

/// The tag holding one of everything (the story's "Tested" line).
struct Holdings {
    recorded: i64,
    untimed: i64,
    planned: i64,
    collection: i64,
    collection_tracks: Vec<i64>,
    lone_track: i64,
}

async fn tag_one_of_everything(app: &Router, name: &str) -> Holdings {
    let recorded = import_with(app, SAMPLE_GPX, &[("name", "Counted Walk")]).await;
    let untimed = import_with(app, UNTIMED_GPX, &[("name", "Untimed Walk")]).await;
    let planned = import_with(
        app,
        SAMPLE_GPX,
        &[("name", "Planned Walk"), ("kind", "planned")],
    )
    .await;
    let (collection, collection_tracks) = a_collection(app, COLLECTION_GPX, "Lakes").await;
    let (_, lone) = a_collection(app, COLLECTION_SINGLE_GPX, "Karwendel").await;
    for trip in [recorded, untimed, planned] {
        tag(app, &format!("/api/trips/{trip}/tags"), name).await;
    }
    tag(app, &format!("/api/collections/{collection}/tags"), name).await;
    tag(
        app,
        &format!("/api/collection-tracks/{}/tags", lone[0]),
        name,
    )
    .await;
    Holdings {
        recorded,
        untimed,
        planned,
        collection,
        collection_tracks,
        lone_track: lone[0],
    }
}

// ── Listed, not counted ──────────────────────────────────────────────────────

#[tokio::test]
async fn us107_a_tag_answers_everything_it_holds_and_counts_only_the_dated_recorded_trip() {
    let (app, _dir) = test_app().await;
    let held = tag_one_of_everything(&app, "alps").await;

    let summary = summary(&app, "alps").await;

    // The figures' part is US-78's, unchanged.
    let tag = &summary.tags[0];
    assert_eq!(tag.trip_ids, [held.recorded]);
    assert_eq!(tag.undated, 1);
    let counted: Vec<i64> = summary.trips.iter().map(|trip| trip.id).collect();
    assert_eq!(counted, [held.recorded]);
    // Listed beside it: the planned trip by its date, then the untimed one.
    assert_eq!(tag.uncounted_trip_ids, [held.planned, held.untimed]);
    let uncounted: Vec<(i64, Option<&str>)> = summary
        .uncounted
        .iter()
        .map(|trip| (trip.id, trip.start_date.as_deref()))
        .collect();
    assert_eq!(
        uncounted,
        [(held.planned, Some("2024-06-01")), (held.untimed, None)]
    );
    // The collection with all its tracks in file order, and the lone track
    // with its collection's name.
    assert_eq!(tag.collection_ids, [held.collection]);
    assert_eq!(summary.collections.len(), 1);
    assert_eq!(summary.collections[0].name, "Lakes");
    let tracks: Vec<i64> = summary.collections[0]
        .tracks
        .iter()
        .map(|track| track.id)
        .collect();
    assert_eq!(tracks, held.collection_tracks);
    assert_eq!(tag.collection_track_ids, [held.lone_track]);
    assert_eq!(summary.collection_tracks.len(), 1);
    let lone = &summary.collection_tracks[0];
    assert_eq!(lone.id, held.lone_track);
    assert_eq!(lone.collection_name, "Karwendel");
    assert!(lone.distance_m > 0.0, "{lone:?}");
}

#[tokio::test]
async fn us107_a_tagged_track_whose_collection_carries_the_tag_too_is_listed_with_it_alone() {
    let (app, _dir) = test_app().await;
    let (collection, tracks) = a_collection(&app, COLLECTION_GPX, "Lakes").await;
    tag(&app, &format!("/api/collections/{collection}/tags"), "alps").await;
    tag(
        &app,
        &format!("/api/collection-tracks/{}/tags", tracks[0]),
        "alps",
    )
    .await;
    tag(
        &app,
        &format!("/api/collection-tracks/{}/tags", tracks[0]),
        "norway",
    )
    .await;

    let summary = summary(&app, "alps,norway").await;

    assert!(summary.tags[0].collection_track_ids.is_empty());
    assert_eq!(summary.tags[0].collection_ids, [collection]);
    // Under the other tag, its collection is not: there it stands alone.
    assert_eq!(summary.tags[1].collection_track_ids, [tracks[0]]);
    assert!(summary.tags[1].collection_ids.is_empty());
    assert_eq!(summary.collections.len(), 1);
    assert_eq!(summary.collection_tracks.len(), 1);
}

#[tokio::test]
async fn us107_what_is_under_several_chosen_tags_comes_once() {
    let (app, _dir) = test_app().await;
    let planned = import_with(&app, SAMPLE_GPX, &[("kind", "planned")]).await;
    let (collection, _) = a_collection(&app, COLLECTION_GPX, "Lakes").await;
    for name in ["alps", "norway"] {
        tag(&app, &format!("/api/trips/{planned}/tags"), name).await;
        tag(&app, &format!("/api/collections/{collection}/tags"), name).await;
    }

    let summary = summary(&app, "alps,norway").await;

    assert_eq!(summary.uncounted.len(), 1);
    assert_eq!(summary.collections.len(), 1);
    for tag in &summary.tags {
        assert_eq!(tag.uncounted_trip_ids, [planned], "{}", tag.name);
        assert_eq!(tag.collection_ids, [collection], "{}", tag.name);
    }
}

// ── Shares unchanged ─────────────────────────────────────────────────────────

#[tokio::test]
async fn us107_a_summary_share_still_reaches_the_counted_trips_alone() {
    let (app, _dir) = test_app().await;
    let held = tag_one_of_everything(&app, "alps").await;

    let shared = shared(&app, &["alps"]).await;

    assert!(shared.contains("Counted Walk"), "{shared}");
    assert!(shared.contains(&format!(r#""trip_ids":[{}]"#, held.recorded)));
    // Recorded trips, untimed ones too, as before (US-82); nothing else.
    for unseen in ["Planned Walk", "Lakes", "Karwendel", "Nordkette", "Splügen"] {
        assert!(!shared.contains(unseen), "{unseen} in {shared}");
    }
}
