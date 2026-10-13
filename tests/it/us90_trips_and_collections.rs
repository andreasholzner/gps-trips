//! US-90 acceptance tests — trips and collections in one list, the server's
//! half.
//!
//! The list itself is the SPA's (`crates/ui-dioxus`). What it reads of the
//! collections is here: `GET /api/collections` narrows each listed
//! collection to the tracks the filters match — by the region and by the
//! activity a track goes by — and says how many tracks it holds in all, so a
//! collection's group row reads "7 of 125". Each track comes with what its
//! row in the table shows: its distance and its ascent.
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

use axum::http::{Method, StatusCode};
use trip_archive::models::{CollectionDetail, CollectionSummary};

use crate::common::{
    body_string, error_message, get, import_collection, json_request, send, test_app,
    COLLECTION_GPX, WAYPOINTS_ONLY_GPX,
};

async fn read_json<T: serde::de::DeserializeOwned>(app: &axum::Router, uri: &str) -> T {
    let response = get(app, uri).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{uri}: {e}: {body}"))
}

async fn collections(app: &axum::Router, query: &str) -> Vec<CollectionSummary> {
    read_json(app, &format!("/api/collections{query}")).await
}

fn track_names(collection: &CollectionSummary) -> Vec<&str> {
    collection.tracks.iter().map(|t| t.name.as_str()).collect()
}

/// The three stages of `COLLECTION_GPX`, the second given an activity of its
/// own: hiking, while the collection's is cycling. Their names, in file
/// order.
async fn lakes_with_a_hiking_stage(app: &axum::Router) -> [String; 3] {
    let id = import_collection(
        app,
        COLLECTION_GPX,
        &[("name", "Lakes"), ("activity_type", "cycling")],
    )
    .await;
    let detail: CollectionDetail = read_json(app, &format!("/api/collections/{id}")).await;
    let uri = format!("/api/collection-tracks/{}", detail.tracks[1].id);
    let response = send(
        app,
        json_request(Method::PATCH, &uri, r#"{"activity_type": "hiking"}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let names: Vec<String> = detail.tracks.into_iter().map(|t| t.name).collect();
    names.try_into().unwrap()
}

// ── What a track's row shows ─────────────────────────────────────────────────

#[tokio::test]
async fn us90_each_track_comes_with_its_distance_and_ascent() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, COLLECTION_GPX, &[]).await;
    let detail: CollectionDetail = read_json(&app, &format!("/api/collections/{id}")).await;

    let listed = collections(&app, "").await;

    let listed: Vec<(i64, f64, f64)> = listed[0]
        .tracks
        .iter()
        .map(|t| (t.id, t.distance_m, t.ascent_m))
        .collect();
    let expected: Vec<(i64, f64, f64)> = detail
        .tracks
        .iter()
        .map(|t| (t.id, t.distance_m, t.ascent_m))
        .collect();
    assert_eq!(listed, expected);
    assert!(listed.iter().all(|(_, distance, _)| *distance > 0.0));
}

#[tokio::test]
async fn us90_a_collection_says_how_many_tracks_it_holds_in_all() {
    let (app, _dir) = test_app().await;
    import_collection(&app, COLLECTION_GPX, &[]).await;
    import_collection(&app, WAYPOINTS_ONLY_GPX, &[("name", "Passes")]).await;

    let listed = collections(&app, "").await;

    assert_eq!(listed[0].track_count, 3);
    assert_eq!(listed[1].track_count, 0);
}

// ── Narrowed to the tracks that match ────────────────────────────────────────

#[tokio::test]
async fn us90_a_region_narrows_a_collection_to_its_tracks_in_it() {
    let (app, _dir) = test_app().await;
    import_collection(&app, COLLECTION_GPX, &[]).await;

    // Around the Albula Pass: only stage 3 reaches it.
    let listed = collections(&app, "?bbox=9.83,46.58,9.84,46.59").await;

    assert_eq!(listed.len(), 1);
    assert_eq!(track_names(&listed[0]), ["Stage 3: Albula"]);
    assert_eq!(listed[0].track_count, 3, "out of all its tracks");
}

#[tokio::test]
async fn us90_the_activity_narrows_a_collection_to_its_tracks_of_that_activity() {
    let (app, _dir) = test_app().await;
    let [first, second, third] = lakes_with_a_hiking_stage(&app).await;

    let hiking = collections(&app, "?activity=hiking").await;
    let cycling = collections(&app, "?activity=cycling").await;

    assert_eq!(track_names(&hiking[0]), [second.as_str()]);
    assert_eq!(hiking[0].track_count, 3);
    assert_eq!(
        track_names(&cycling[0]),
        [first.as_str(), third.as_str()],
        "the others go by the collection's activity"
    );
}

#[tokio::test]
async fn us90_the_activity_leaves_out_a_collection_without_such_a_track() {
    let (app, _dir) = test_app().await;
    lakes_with_a_hiking_stage(&app).await;
    import_collection(&app, WAYPOINTS_ONLY_GPX, &[("name", "Passes")]).await;

    let listed = collections(&app, "?activity=kayaking").await;
    let hiking = collections(&app, "?activity=hiking").await;

    assert!(listed.is_empty());
    let names: Vec<&str> = hiking.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Lakes"], "waypoints alone have no activity");
}

#[tokio::test]
async fn us90_the_activity_and_the_region_narrow_the_tracks_together() {
    let (app, _dir) = test_app().await;
    let [first, _, third] = lakes_with_a_hiking_stage(&app).await;

    // The whole route, from Splügen to the Albula Pass.
    let region = "bbox=9.0,46.3,10.0,46.8";
    let cycling = collections(&app, &format!("?activity=cycling&{region}")).await;
    // Around the Albula Pass, where only the cycling stage 3 is.
    let hiking = collections(&app, "?activity=hiking&bbox=9.83,46.58,9.84,46.59").await;

    assert_eq!(track_names(&cycling[0]), [first.as_str(), third.as_str()]);
    assert!(hiking.is_empty());
}

#[tokio::test]
async fn us90_a_blank_activity_narrows_nothing() {
    let (app, _dir) = test_app().await;
    import_collection(&app, COLLECTION_GPX, &[]).await;

    assert_eq!(collections(&app, "?activity=").await[0].tracks.len(), 3);
}

#[tokio::test]
async fn us90_an_unknown_activity_is_refused_with_the_reason() {
    let (app, _dir) = test_app().await;

    let response = get(&app, "/api/collections?activity=sailing").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        error_message(response).await.contains("sailing"),
        "the reason names the value"
    );
}
