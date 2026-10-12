//! US-98 acceptance tests, server half — "waypoint pages".
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *the waypoint page shows its name, symbol, description, elevation and
//!   position, the track it belongs to, and its collection* —
//!   `us98_a_waypoint_is_read_with_its_collection_and_its_track`,
//!   `us98_a_waypoint_of_no_track_is_read_without_one`.
//! * *name, description, symbol and elevation are edited; the name is
//!   required, the description and elevation may be emptied* —
//!   `us98_the_details_are_edited`, `us98_the_description_symbol_and_elevation_may_be_emptied`,
//!   `us98_a_blank_name_or_a_wrong_elevation_is_refused`.
//! * *the position is changed and saved explicitly* —
//!   `us98_the_position_is_changed`, `us98_a_position_needs_both_halves_in_range`.
//! * *an edit shows on the collection, the trip-list map and in the
//!   QMapShack export* — `us98_an_edit_shows_wherever_the_waypoint_does`,
//!   `us98_a_moved_waypoint_moves_the_collection_on_the_trip_list_map`.
//! * *re-adding a file treats an edited waypoint as any other* —
//!   `us98_re_adding_the_file_proposes_to_replace_a_moved_or_resymbolled_waypoint`,
//!   `us98_re_adding_the_file_proposes_to_add_a_renamed_waypoints_old_name`.
//! * *deleting a waypoint keeps its collection and tracks* —
//!   `us98_deleting_a_waypoint_keeps_its_collection_and_tracks`.
//!
//! What the pages show is the UI crate's; that the routes need a session is
//! part of US-19's table of routes.

use axum::http::{Method, StatusCode};
use trip_archive::models::{
    ActivityType, CollectionDetail, CollectionImportReview, CollectionSummary, ExportCollection,
    ImportAction, Waypoint, WaypointDetail,
};

use crate::common::{
    body_string, collection_files_request, delete, error_message, get, import_collection,
    json_request, send, test_app, COLLECTION_SINGLE_GPX, WAYPOINTS_ONLY_GPX,
};

async fn read_json<T: serde::de::DeserializeOwned>(app: &axum::Router, uri: &str) -> T {
    let response = get(app, uri).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{uri}: {e}: {body}"))
}

async fn waypoints(app: &axum::Router, collection: i64) -> Vec<Waypoint> {
    read_json(app, &format!("/api/collections/{collection}/waypoints")).await
}

async fn waypoint(app: &axum::Router, id: i64) -> WaypointDetail {
    read_json(app, &format!("/api/waypoints/{id}")).await
}

async fn patch(app: &axum::Router, id: i64, body: serde_json::Value) -> axum::response::Response {
    let uri = format!("/api/waypoints/{id}");
    send(app, json_request(Method::PATCH, &uri, &body.to_string())).await
}

async fn edit(app: &axum::Router, id: i64, body: serde_json::Value) {
    let response = patch(app, id, body).await;
    let status = response.status();
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "{}",
        body_string(response).await
    );
}

/// A collection of one tour near Innsbruck, and its "Start" waypoint —
/// which belongs to the tour.
async fn the_start() -> (axum::Router, tempfile::TempDir, i64, Waypoint) {
    let (app, dir) = test_app().await;
    let collection =
        import_collection(&app, COLLECTION_SINGLE_GPX, &[("activity_type", "hiking")]).await;
    let start = waypoints(&app, collection).await.remove(0);
    assert_eq!(start.name.as_deref(), Some("Start"));
    (app, dir, collection, start)
}

// ── Reading ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us98_a_waypoint_is_read_with_its_collection_and_its_track() {
    let (app, _dir, collection, start) = the_start().await;
    let detail: CollectionDetail = read_json(&app, &format!("/api/collections/{collection}")).await;

    let read = waypoint(&app, start.id).await;

    assert_eq!(read.waypoint, start);
    assert_eq!(read.collection_id, collection);
    assert_eq!(read.collection_name, detail.name);
    let track = read.track.expect("it belongs to the tour");
    assert_eq!(track.id, detail.tracks[0].id);
    assert_eq!(track.name, "Nordkette");
    assert_eq!(track.activity_type, ActivityType::Hiking);
}

#[tokio::test]
async fn us98_a_waypoint_of_no_track_is_read_without_one() {
    let (app, _dir) = test_app().await;
    let collection = import_collection(&app, WAYPOINTS_ONLY_GPX, &[]).await;
    let pass = waypoints(&app, collection).await.remove(0);

    assert_eq!(waypoint(&app, pass.id).await.track, None);
}

#[tokio::test]
async fn us98_an_unknown_waypoint_is_not_found() {
    let (app, _dir) = test_app().await;

    assert_eq!(
        get(&app, "/api/waypoints/999").await.status(),
        StatusCode::NOT_FOUND
    );
    let response = patch(&app, 999, serde_json::json!({ "name": "x" })).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        delete(&app, "/api/waypoints/999").await.status(),
        StatusCode::NOT_FOUND
    );
}

// ── Editing ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us98_the_details_are_edited() {
    let (app, _dir, _collection, start) = the_start().await;

    edit(
        &app,
        start.id,
        serde_json::json!({
            "name": "  Hungerburg  ",
            "description": "Funicular top station",
            "symbol": "Railway",
            "ele": "868",
        }),
    )
    .await;

    let edited = waypoint(&app, start.id).await.waypoint;
    assert_eq!(edited.name.as_deref(), Some("Hungerburg"));
    assert_eq!(edited.description.as_deref(), Some("Funicular top station"));
    assert_eq!(edited.symbol.as_deref(), Some("Railway"));
    assert_eq!(edited.ele, Some(868.0));
    // What was not sent stays.
    assert_eq!((edited.lat, edited.lon), (start.lat, start.lon));
    assert_eq!(edited.collection_track_id, start.collection_track_id);
}

#[tokio::test]
async fn us98_the_description_symbol_and_elevation_may_be_emptied() {
    let (app, _dir, _collection, start) = the_start().await;

    edit(
        &app,
        start.id,
        serde_json::json!({ "description": " ", "symbol": "", "ele": "" }),
    )
    .await;

    let edited = waypoint(&app, start.id).await.waypoint;
    assert_eq!(edited.description, None);
    assert_eq!(edited.symbol, None);
    assert_eq!(edited.ele, None);
    assert_eq!(edited.name.as_deref(), Some("Start"));
}

#[tokio::test]
async fn us98_a_blank_name_or_a_wrong_elevation_is_refused() {
    let (app, _dir, _collection, start) = the_start().await;

    for body in [
        serde_json::json!({ "name": "  " }),
        serde_json::json!({ "ele": "high" }),
        serde_json::json!({ "ele": "inf" }),
        serde_json::json!({ "symbol": "x".repeat(101) }),
    ] {
        let response = patch(&app, start.id, body.clone()).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
        assert!(!error_message(response).await.is_empty(), "{body}");
    }
    assert_eq!(
        waypoint(&app, start.id).await.waypoint,
        start,
        "nothing changed"
    );
}

#[tokio::test]
async fn us98_the_position_is_changed() {
    let (app, _dir, _collection, start) = the_start().await;

    edit(
        &app,
        start.id,
        serde_json::json!({ "lat": 47.2851, "lon": 11.3951 }),
    )
    .await;

    let moved = waypoint(&app, start.id).await.waypoint;
    assert_eq!((moved.lat, moved.lon), (47.2851, 11.3951));
    assert_eq!(moved.name, start.name);
}

#[tokio::test]
async fn us98_a_position_needs_both_halves_in_range() {
    let (app, _dir, _collection, start) = the_start().await;

    for body in [
        serde_json::json!({ "lat": 47.0 }),
        serde_json::json!({ "lon": 11.0 }),
        serde_json::json!({ "lat": 91.0, "lon": 11.0 }),
        serde_json::json!({ "lat": 47.0, "lon": -180.5 }),
    ] {
        let response = patch(&app, start.id, body.clone()).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }
    assert_eq!(
        waypoint(&app, start.id).await.waypoint,
        start,
        "nothing changed"
    );
}

#[tokio::test]
async fn us98_an_edit_shows_wherever_the_waypoint_does() {
    let (app, _dir, collection, start) = the_start().await;

    edit(
        &app,
        start.id,
        serde_json::json!({ "name": "Hungerburg", "symbol": "Railway", "lat": 47.29, "lon": 11.4 }),
    )
    .await;

    let listed = waypoints(&app, collection).await.remove(0);
    assert_eq!(listed.name.as_deref(), Some("Hungerburg"));
    assert_eq!((listed.lat, listed.lon), (47.29, 11.4));
    let exported: Vec<ExportCollection> = read_json(&app, "/api/export/collections").await;
    let exported = &exported[0].waypoints[0];
    assert_eq!(exported.name.as_deref(), Some("Hungerburg"));
    assert_eq!(exported.symbol.as_deref(), Some("Railway"));
    assert_eq!((exported.lat, exported.lon), (47.29, 11.4));
}

#[tokio::test]
async fn us98_a_moved_waypoint_moves_the_collection_on_the_trip_list_map() {
    let (app, _dir) = test_app().await;
    let collection = import_collection(&app, WAYPOINTS_ONLY_GPX, &[]).await;
    let albula = waypoints(&app, collection).await.remove(1);

    // Further east than the pass was: the rectangle the trip list's map
    // draws the collection by (US-87) grows to it.
    edit(
        &app,
        albula.id,
        serde_json::json!({ "lat": 46.6, "lon": 10.1 }),
    )
    .await;

    let listed: Vec<CollectionSummary> = read_json(&app, "/api/collections").await;
    assert_eq!(
        listed[0].waypoint_bounds,
        Some([9.1705, 46.4958, 10.1, 46.6])
    );
}

/// What re-adding the tour's own file proposes for each of its waypoints.
async fn proposed_on_re_adding(app: &axum::Router, collection: i64) -> Vec<ImportAction> {
    let id = collection.to_string();
    let request = collection_files_request(
        "/api/collection-imports",
        &[("again.gpx", COLLECTION_SINGLE_GPX)],
        &[("collection_id", id.as_str())],
    );
    let response = send(app, request).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let review: CollectionImportReview = serde_json::from_str(&body).unwrap();
    review.waypoints.iter().map(|item| item.proposed).collect()
}

#[tokio::test]
async fn us98_re_adding_the_file_proposes_to_replace_a_moved_or_resymbolled_waypoint() {
    let (app, _dir, collection, start) = the_start().await;
    // Moved a few hundred metres and given another symbol; its name is
    // what ADR-0029 matches it by, with its position.
    edit(
        &app,
        start.id,
        serde_json::json!({ "symbol": "Summit", "lat": 47.273, "lon": 11.39 }),
    )
    .await;

    assert_eq!(
        proposed_on_re_adding(&app, collection).await,
        [ImportAction::Replace, ImportAction::Skip]
    );
}

#[tokio::test]
async fn us98_re_adding_the_file_proposes_to_add_a_renamed_waypoints_old_name() {
    let (app, _dir, collection, start) = the_start().await;
    edit(&app, start.id, serde_json::json!({ "name": "Hungerburg" })).await;

    assert_eq!(
        proposed_on_re_adding(&app, collection).await,
        [ImportAction::Add, ImportAction::Skip]
    );
}

// ── Deleting ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us98_deleting_a_waypoint_keeps_its_collection_and_tracks() {
    let (app, _dir, collection, start) = the_start().await;

    let response = delete(&app, &format!("/api/waypoints/{}", start.id)).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let uri = format!("/api/waypoints/{}", start.id);
    assert_eq!(get(&app, &uri).await.status(), StatusCode::NOT_FOUND);
    let detail: CollectionDetail = read_json(&app, &format!("/api/collections/{collection}")).await;
    assert_eq!(detail.tracks.len(), 1);
    assert_eq!(detail.waypoint_count, 1);
    assert!(waypoints(&app, collection)
        .await
        .iter()
        .all(|w| w.id != start.id));
}
