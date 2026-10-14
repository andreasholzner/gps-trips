//! US-99 acceptance tests, server half — what has a recorded trip linked to
//! it, however little of it the trip covered.
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *the collections table and a collection's page count the tracks with a
//!   recorded trip* — `us99_the_tracks_with_a_recorded_trip_are_counted`.
//! * *the trip list marks a planned trip or collection track with a recorded
//!   trip, and a collection's page its tracks and waypoints with one* —
//!   `us99_a_planned_trip_with_a_recorded_trip_is_marked`,
//!   `us99_the_tracks_with_a_recorded_trip_are_counted`,
//!   `us99_a_collections_page_marks_its_tracks_and_waypoints`.
//! * *a planned trip's link to a collection track does not mark it* —
//!   `us99_the_tracks_with_a_recorded_trip_are_counted`.
//! * *a filter narrows the planned trips and collection tracks on the trip
//!   list, leaving recorded trips as they are* —
//!   `us99_the_trip_list_narrows_planned_trips_by_their_recorded_trips`,
//!   `us99_the_collections_narrow_their_tracks_by_their_recorded_trips`,
//!   `us99_an_unknown_recorded_trip_filter_is_refused`.
//!
//! Narrowing a collection page's own lists is the UI crate's, from the marks.

use axum::{http::StatusCode, Router};
use trip_archive::models::{CollectionDetail, CollectionSummary, TripSummary, Waypoint};
use trip_archive::server::{db, repo};

use crate::common::{
    body_string, get, import_collection, import_request_with_fields, import_sample, json_request,
    router_over, send, trip_id_from_redirect, COLLECTION_GPX, SAMPLE_GPX,
};

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

async fn a_collection(app: &Router, name: &str) -> (i64, Vec<i64>) {
    let id = import_collection(app, COLLECTION_GPX, &[("name", name)]).await;
    let detail: CollectionDetail = read_json(app, &format!("/api/collections/{id}")).await;
    (id, detail.tracks.iter().map(|track| track.id).collect())
}

async fn link(app: &Router, trip_ids: &[i64], track_ids: &[i64]) {
    let body = serde_json::json!({ "trip_ids": trip_ids, "collection_track_ids": track_ids });
    let request = json_request(axum::http::Method::POST, "/api/links", &body.to_string());
    assert_eq!(send(app, request).await.status(), StatusCode::OK);
}

async fn trip_ids(app: &Router, query: &str) -> Vec<i64> {
    let trips: Vec<TripSummary> = read_json(app, &format!("/api/trips?{query}")).await;
    let mut ids: Vec<i64> = trips.iter().map(|trip| trip.id).collect();
    ids.sort();
    ids
}

/// Each listed collection's id and its listed tracks' ids.
async fn listed(app: &Router, query: &str) -> Vec<(i64, Vec<i64>)> {
    let collections: Vec<CollectionSummary> =
        read_json(app, &format!("/api/collections?{query}")).await;
    collections
        .iter()
        .map(|c| (c.id, c.tracks.iter().map(|track| track.id).collect()))
        .collect()
}

#[tokio::test]
async fn us99_a_planned_trip_with_a_recorded_trip_is_marked() {
    let (app, _pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let done = planned(&app).await;
    let open = planned(&app).await;
    link(&app, &[monday, done], &[]).await;

    let trips: Vec<TripSummary> = read_json(&app, "/api/trips").await;

    let marked = |id: i64| {
        trips
            .iter()
            .find(|trip| trip.id == id)
            .unwrap()
            .has_recorded_trip
    };
    assert!(marked(done));
    assert!(!marked(open));
    assert!(!marked(monday), "a recorded trip is never marked");
}

#[tokio::test]
async fn us99_the_trip_list_narrows_planned_trips_by_their_recorded_trips() {
    let (app, _pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let tuesday = import_sample(&app).await;
    let done = planned(&app).await;
    let open = planned(&app).await;
    link(&app, &[monday, done], &[]).await;

    assert_eq!(
        trip_ids(&app, "recorded_trip=with").await,
        vec![monday, tuesday, done]
    );
    assert_eq!(
        trip_ids(&app, "recorded_trip=without").await,
        vec![monday, tuesday, open]
    );
    assert_eq!(
        trip_ids(&app, "kind=planned&recorded_trip=without").await,
        vec![open]
    );
    assert_eq!(trip_ids(&app, "recorded_trip=").await.len(), 4);
}

#[tokio::test]
async fn us99_the_tracks_with_a_recorded_trip_are_counted() {
    let (app, _pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let plan = planned(&app).await;
    let (book, tracks) = a_collection(&app, "Book").await;
    link(&app, &[monday], &tracks[..2]).await;
    // Planned from a third stage, not been on it.
    link(&app, &[plan], &tracks[2..3]).await;

    let collections: Vec<CollectionSummary> = read_json(&app, "/api/collections").await;
    let detail: CollectionDetail = read_json(&app, &format!("/api/collections/{book}")).await;

    assert_eq!(collections[0].tracks_with_recorded_trip, 2);
    assert_eq!(collections[0].track_count, tracks.len() as i64);
    let marks: Vec<bool> = collections[0]
        .tracks
        .iter()
        .map(|track| track.has_recorded_trip)
        .collect();
    let mut expected = vec![false; tracks.len()];
    expected[0] = true;
    expected[1] = true;
    assert_eq!(marks, expected);
    let on_page: Vec<bool> = detail.tracks.iter().map(|t| t.has_recorded_trip).collect();
    assert_eq!(on_page, expected);
}

#[tokio::test]
async fn us99_the_collections_narrow_their_tracks_by_their_recorded_trips() {
    let (app, _pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let (book, tracks) = a_collection(&app, "Book").await;
    let (untouched, untouched_tracks) = a_collection(&app, "Untouched").await;
    link(&app, &[monday], &tracks[..2]).await;

    assert_eq!(
        listed(&app, "recorded_trip=with").await,
        vec![(book, tracks[..2].to_vec())]
    );
    assert_eq!(
        listed(&app, "recorded_trip=without").await,
        vec![(book, tracks[2..].to_vec()), (untouched, untouched_tracks)]
    );
    // Counted whatever the filter.
    let collections: Vec<CollectionSummary> =
        read_json(&app, "/api/collections?recorded_trip=with").await;
    assert_eq!(collections[0].track_count, tracks.len() as i64);
    assert_eq!(collections[0].tracks_with_recorded_trip, 2);
}

#[tokio::test]
async fn us99_a_collections_page_marks_its_tracks_and_waypoints() {
    let (app, pool, _dir) = app().await;
    let monday = import_sample(&app).await;
    let plan = planned(&app).await;
    let (book, _) = a_collection(&app, "Book").await;
    let waypoints: Vec<Waypoint> =
        read_json(&app, &format!("/api/collections/{book}/waypoints")).await;
    repo::link_waypoint(&pool, monday, waypoints[0].id)
        .await
        .unwrap();
    repo::link_waypoint(&pool, plan, waypoints[1].id)
        .await
        .unwrap();

    let detail: CollectionDetail = read_json(&app, &format!("/api/collections/{book}")).await;

    assert_eq!(detail.waypoints_with_recorded_trip, vec![waypoints[0].id]);
}

#[tokio::test]
async fn us99_an_unknown_recorded_trip_filter_is_refused() {
    let (app, _pool, _dir) = app().await;
    for uri in [
        "/api/trips?recorded_trip=maybe",
        "/api/collections?recorded_trip=maybe",
    ] {
        assert_eq!(
            get(&app, uri).await.status(),
            StatusCode::BAD_REQUEST,
            "{uri}"
        );
    }
}
