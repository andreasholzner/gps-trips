//! US-99 acceptance tests, server half — "link what I did to what I
//! planned".
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *a selection links every pair in it that can be linked — recorded to
//!   planned trips and collection tracks, planned trips to collection
//!   tracks; six recorded trips and one planned trip become six links* —
//!   `us99_a_selection_links_every_pair_and_each_end_lists_the_other`,
//!   `us99_six_days_and_one_plan_become_six_links`.
//! * *no other pair is ever linked* —
//!   `us99_a_selection_without_a_pair_to_link_is_refused`.
//! * *each end's page lists what it is linked to, waypoints included* —
//!   `us99_a_selection_links_every_pair_and_each_end_lists_the_other`,
//!   `us99_a_waypoints_links_are_listed_and_removed`.
//! * *a link is removed from either end's page* —
//!   `us99_a_link_is_removed_from_either_end`,
//!   `us99_a_waypoints_links_are_listed_and_removed`.
//! * *a link goes when either end is deleted* —
//!   `us99_deleting_an_end_through_the_api_takes_its_links`.
//!
//! The repository's half, ordering included, is unit-tested in
//! `src/server/repo/link/tests.rs`; that the routes need a session is part of
//! US-19's table of routes.

use axum::{
    http::{Method, StatusCode},
    Router,
};
use trip_archive::models::{CollectionDetail, CreatedLinks, Links, Waypoint};
use trip_archive::server::{db, repo};

use crate::common::{
    body_string, delete, error_message, get, import_collection, import_request_with_fields,
    import_sample, json_request, router_over, send, trip_id_from_redirect, COLLECTION_GPX,
    SAMPLE_GPX,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// A router over a pool the test also reaches, to link waypoints — which
/// only US-103's proposals do.
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

/// A collection's tracks' ids and its waypoints' ids.
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

async fn link(app: &Router, trip_ids: &[i64], track_ids: &[i64]) -> axum::response::Response {
    let body = serde_json::json!({ "trip_ids": trip_ids, "collection_track_ids": track_ids });
    send(
        app,
        json_request(Method::POST, "/api/links", &body.to_string()),
    )
    .await
}

async fn links(app: &Router, uri: &str) -> Links {
    read_json(app, &format!("{uri}/links")).await
}

fn ids<T>(items: &[T], id: impl Fn(&T) -> i64) -> Vec<i64> {
    let mut ids: Vec<i64> = items.iter().map(id).collect();
    ids.sort();
    ids
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us99_a_selection_links_every_pair_and_each_end_lists_the_other() {
    let (app, _pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let plan = planned(&app).await;
    let (_, tracks, _) = a_collection(&app).await;

    let response = link(&app, &[monday, plan], &tracks[..2]).await;

    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let created: CreatedLinks = serde_json::from_str(&body).unwrap();
    // 1 × 1 + 1 × 2 + 1 × 2.
    assert_eq!(created.pairs, 5);
    let on_monday = links(&app, &format!("/api/trips/{monday}")).await;
    assert_eq!(ids(&on_monday.planned_trips, |trip| trip.id), vec![plan]);
    assert_eq!(
        ids(&on_monday.collection_tracks, |track| track.id),
        tracks[..2]
    );
    let on_plan = links(&app, &format!("/api/trips/{plan}")).await;
    assert_eq!(ids(&on_plan.recorded_trips, |trip| trip.id), vec![monday]);
    assert_eq!(on_plan.collection_tracks.len(), 2);
    let on_stage = links(&app, &format!("/api/collection-tracks/{}", tracks[0])).await;
    assert_eq!(ids(&on_stage.recorded_trips, |trip| trip.id), vec![monday]);
    assert_eq!(ids(&on_stage.planned_trips, |trip| trip.id), vec![plan]);
    let unlinked = links(&app, &format!("/api/collection-tracks/{}", tracks[2])).await;
    assert_eq!(unlinked, Links::default());
}

#[tokio::test]
async fn us99_six_days_and_one_plan_become_six_links() {
    let (app, _pool, _dir) = app().await;
    let plan = planned(&app).await;
    let mut selection = vec![plan];
    for _ in 0..6 {
        selection.push(import_sample(&app).await);
    }

    let response = link(&app, &selection, &[]).await;

    let created: CreatedLinks = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(created.pairs, 6);
    let on_plan = links(&app, &format!("/api/trips/{plan}")).await;
    assert_eq!(
        ids(&on_plan.recorded_trips, |trip| trip.id),
        ids(&selection[1..], |id| *id)
    );
}

#[tokio::test]
async fn us99_a_selection_without_a_pair_to_link_is_refused() {
    let (app, _pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let tuesday = import_sample(&app).await;
    let plan = planned(&app).await;
    let (_, tracks, _) = a_collection(&app).await;

    for (trips, tracks) in [
        (vec![monday, tuesday], vec![]),
        (vec![], tracks.clone()),
        (vec![plan], vec![]),
    ] {
        let response = link(&app, &trips, &tracks).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(!error_message(response).await.is_empty());
    }
    assert_eq!(
        links(&app, &format!("/api/trips/{monday}")).await,
        Links::default()
    );
}

#[tokio::test]
async fn us99_a_selection_naming_something_missing_links_nothing() {
    let (app, _pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let plan = planned(&app).await;
    let (_, tracks, _) = a_collection(&app).await;

    assert_eq!(
        link(&app, &[monday, plan, 9999], &tracks).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        link(&app, &[monday, plan], &[9999]).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        links(&app, &format!("/api/trips/{monday}")).await,
        Links::default()
    );
}

#[tokio::test]
async fn us99_a_link_is_removed_from_either_end() {
    let (app, _pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let tuesday = import_sample(&app).await;
    let plan = planned(&app).await;
    let (_, tracks, _) = a_collection(&app).await;
    link(&app, &[monday, tuesday, plan], &tracks[..1]).await;

    // From the plan's page, from the recorded trip's, and from the track's
    // — which names the trip at the other end.
    for uri in [
        format!("/api/trips/{plan}/links/trips/{monday}"),
        format!("/api/trips/{tuesday}/links/trips/{plan}"),
        format!("/api/trips/{monday}/links/collection-tracks/{}", tracks[0]),
        // Removing it again is no error.
        format!("/api/trips/{monday}/links/collection-tracks/{}", tracks[0]),
    ] {
        assert_eq!(
            delete(&app, &uri).await.status(),
            StatusCode::NO_CONTENT,
            "{uri}"
        );
    }

    assert_eq!(
        links(&app, &format!("/api/trips/{monday}")).await,
        Links::default()
    );
    let on_plan = links(&app, &format!("/api/trips/{plan}")).await;
    assert!(on_plan.recorded_trips.is_empty());
    assert_eq!(on_plan.collection_tracks.len(), 1);
    assert_eq!(
        delete(&app, &format!("/api/trips/9999/links/trips/{plan}"))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn us99_a_waypoints_links_are_listed_and_removed() {
    let (app, pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let plan = planned(&app).await;
    let (collection, _, waypoints) = a_collection(&app).await;
    for trip in [monday, plan] {
        assert!(repo::link_waypoint(&pool, trip, waypoints[0])
            .await
            .unwrap());
    }

    let on_monday = links(&app, &format!("/api/trips/{monday}")).await;
    assert_eq!(
        ids(&on_monday.waypoints, |waypoint| waypoint.id),
        vec![waypoints[0]]
    );
    assert_eq!(on_monday.waypoints[0].collection_id, collection);
    let on_waypoint = links(&app, &format!("/api/waypoints/{}", waypoints[0])).await;
    assert_eq!(
        ids(&on_waypoint.recorded_trips, |trip| trip.id),
        vec![monday]
    );
    assert_eq!(ids(&on_waypoint.planned_trips, |trip| trip.id), vec![plan]);

    let uri = format!("/api/trips/{monday}/links/waypoints/{}", waypoints[0]);
    assert_eq!(delete(&app, &uri).await.status(), StatusCode::NO_CONTENT);
    let on_waypoint = links(&app, &format!("/api/waypoints/{}", waypoints[0])).await;
    assert!(on_waypoint.recorded_trips.is_empty());
    assert_eq!(on_waypoint.planned_trips.len(), 1);
}

#[tokio::test]
async fn us99_deleting_an_end_through_the_api_takes_its_links() {
    let (app, pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let plan = planned(&app).await;
    let (collection, tracks, waypoints) = a_collection(&app).await;
    link(&app, &[monday, plan], &tracks[..2]).await;
    repo::link_waypoint(&pool, monday, waypoints[0])
        .await
        .unwrap();

    delete(&app, &format!("/api/collection-tracks/{}", tracks[0])).await;
    delete(&app, &format!("/api/waypoints/{}", waypoints[0])).await;
    let on_monday = links(&app, &format!("/api/trips/{monday}")).await;
    assert_eq!(
        ids(&on_monday.collection_tracks, |track| track.id),
        vec![tracks[1]]
    );
    assert!(on_monday.waypoints.is_empty());

    delete(&app, &format!("/api/trips/{plan}")).await;
    delete(&app, &format!("/api/collections/{collection}")).await;
    assert_eq!(
        links(&app, &format!("/api/trips/{monday}")).await,
        Links::default()
    );
}

#[tokio::test]
async fn us99_the_links_of_something_missing_are_not_found() {
    let (app, _pool, _dir) = app().await;
    for uri in [
        "/api/trips/9999/links",
        "/api/collection-tracks/9999/links",
        "/api/waypoints/9999/links",
    ] {
        assert_eq!(
            get(&app, uri).await.status(),
            StatusCode::NOT_FOUND,
            "{uri}"
        );
    }
}
