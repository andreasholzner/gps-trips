//! US-93 — as the owner, I tag collections and their tracks, so I organize
//! collections the way I organize my trips.
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *a collection's page and a collection track's page tag it as a trip's
//!   page tags a trip: the same normalization, a new tag created on demand* —
//!   `us93_a_collection_and_a_track_are_tagged_and_untagged_as_a_trip_is`,
//!   `us93_a_name_us33_refuses_is_refused_and_an_unknown_target_is_not_found`.
//! * *a collection's tags are its own and do not pass to its tracks, nor a
//!   track's to its collection* — `us93_tags_do_not_pass_between_a_collection_and_its_tracks`.
//! * *the Tags screen counts the collections carrying a tag, and the
//!   collection tracks carrying it whose collection does not* —
//!   `us93_the_tags_screen_counts_collections_and_their_tracks`.
//! * *renaming a tag renames it on them too, deleting one takes it off them*
//!   — `us93_renaming_and_deleting_a_tag_reach_collections_and_tracks`.
//! * *the trip list's tag filter, the tag summaries and summary shares reach
//!   trips only* — `us93_what_is_about_trips_does_not_see_collections`.
//!
//! The repository's half is unit-tested in
//! `src/server/repo/tag/tests/collections.rs`; that a track replaced by a
//! re-import keeps its tags in `us88_add_to_collection.rs`; the screens in
//! the UI crate.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use trip_archive::models::{CollectionDetail, CreatedShare, ShareOverview, Tag, TagOverview};

use crate::common::{
    body_string, delete, get, import_collection, import_sample, json_request, send,
    send_unauthenticated, test_app, COLLECTION_GPX,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn read_json<T: serde::de::DeserializeOwned>(app: &Router, uri: &str) -> T {
    let response = get(app, uri).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{uri}: {e}: {body}"))
}

/// A collection named `name`: its id and its tracks' ids.
async fn a_collection(app: &Router, name: &str) -> (i64, Vec<i64>) {
    let id = import_collection(app, COLLECTION_GPX, &[("name", name)]).await;
    let detail: CollectionDetail = read_json(app, &format!("/api/collections/{id}")).await;
    (id, detail.tracks.iter().map(|track| track.id).collect())
}

async fn tag_response(app: &Router, uri: &str, name: &str) -> axum::response::Response {
    let body = serde_json::json!({ "name": name });
    send(app, json_request(Method::POST, uri, &body.to_string())).await
}

/// Tag what `uri` names (`/api/…/:id/tags`) and return the tag.
async fn tag(app: &Router, uri: &str, name: &str) -> Tag {
    let response = tag_response(app, uri, name).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::CREATED, "{uri}: {body}");
    serde_json::from_str(&body).unwrap()
}

async fn names(app: &Router, uri: &str) -> Vec<String> {
    let tags: Vec<Tag> = read_json(app, uri).await;
    tags.into_iter().map(|tag| tag.name).collect()
}

fn on_collection(id: i64) -> String {
    format!("/api/collections/{id}/tags")
}

fn on_track(id: i64) -> String {
    format!("/api/collection-tracks/{id}/tags")
}

// ── Tagging ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us93_a_collection_and_a_track_are_tagged_and_untagged_as_a_trip_is() {
    let (app, _dir) = test_app().await;
    let (id, tracks) = a_collection(&app, "Lakes").await;

    // Normalized as a trip's tag is, and created on demand.
    let alps = tag(&app, &on_collection(id), "  Alps ").await;
    assert_eq!(alps.name, "alps");
    assert_eq!(tag(&app, &on_track(tracks[0]), "ALPS").await, alps);
    // Tagging twice is no change.
    tag(&app, &on_collection(id), "alps").await;
    assert_eq!(names(&app, &on_collection(id)).await, ["alps"]);
    assert_eq!(names(&app, &on_track(tracks[0])).await, ["alps"]);
    // The suggestions are every tag the archive knows.
    assert_eq!(names(&app, "/api/tags").await, ["alps"]);

    for uri in [on_collection(id), on_track(tracks[0])] {
        let response = delete(&app, &format!("{uri}/{}", alps.id)).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT, "{uri}");
        assert!(names(&app, &uri).await.is_empty(), "{uri}");
    }
    // Untagging keeps the tag, suggestible later.
    assert_eq!(names(&app, "/api/tags").await, ["alps"]);
}

#[tokio::test]
async fn us93_a_name_us33_refuses_is_refused_and_an_unknown_target_is_not_found() {
    let (app, _dir) = test_app().await;
    let (id, tracks) = a_collection(&app, "Lakes").await;

    for uri in [on_collection(id), on_track(tracks[0])] {
        let response = tag_response(&app, &uri, "day trip").await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
    }
    assert!(names(&app, "/api/tags").await.is_empty());

    // Not found before the name is looked at, as for a trip.
    for uri in [on_collection(999), on_track(999)] {
        assert_eq!(get(&app, &uri).await.status(), StatusCode::NOT_FOUND);
        let response = tag_response(&app, &uri, "day trip").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        let response = delete(&app, &format!("{uri}/1")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn us93_tags_do_not_pass_between_a_collection_and_its_tracks() {
    let (app, _dir) = test_app().await;
    let (id, tracks) = a_collection(&app, "Lakes").await;

    tag(&app, &on_collection(id), "book").await;
    tag(&app, &on_track(tracks[1]), "summit").await;

    assert_eq!(names(&app, &on_collection(id)).await, ["book"]);
    assert!(names(&app, &on_track(tracks[0])).await.is_empty());
    assert_eq!(names(&app, &on_track(tracks[1])).await, ["summit"]);
}

// ── The Tags screen ──────────────────────────────────────────────────────────

#[tokio::test]
async fn us93_the_tags_screen_counts_collections_and_their_tracks() {
    let (app, _dir) = test_app().await;
    let trip = import_sample(&app).await;
    let (tagged, tagged_tracks) = a_collection(&app, "Lakes").await;
    let (_, other_tracks) = a_collection(&app, "Passes").await;
    tag(&app, &format!("/api/trips/{trip}/tags"), "alps").await;
    tag(&app, &on_collection(tagged), "alps").await;
    // Counted with its collection, not again.
    tag(&app, &on_track(tagged_tracks[0]), "alps").await;
    tag(&app, &on_track(other_tracks[0]), "alps").await;
    tag(&app, &on_track(other_tracks[2]), "alps").await;

    let overview: Vec<TagOverview> = read_json(&app, "/api/tags/overview").await;

    let alps = &overview[0];
    assert_eq!(
        (
            alps.trip_count,
            alps.recorded_trip_count,
            alps.collection_count,
            alps.collection_track_count
        ),
        (1, 1, 1, 2)
    );
}

#[tokio::test]
async fn us93_renaming_and_deleting_a_tag_reach_collections_and_tracks() {
    let (app, _dir) = test_app().await;
    let (id, tracks) = a_collection(&app, "Lakes").await;
    let alps = tag(&app, &on_collection(id), "alps").await;
    tag(&app, &on_track(tracks[0]), "alps").await;

    let response = send(
        &app,
        json_request(
            Method::PATCH,
            &format!("/api/tags/{}", alps.id),
            r#"{"name":"alpen"}"#,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(names(&app, &on_collection(id)).await, ["alpen"]);
    assert_eq!(names(&app, &on_track(tracks[0])).await, ["alpen"]);

    let response = delete(&app, &format!("/api/tags/{}", alps.id)).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(names(&app, &on_collection(id)).await.is_empty());
    assert!(names(&app, &on_track(tracks[0])).await.is_empty());
    // Only the tag goes.
    let detail: CollectionDetail = read_json(&app, &format!("/api/collections/{id}")).await;
    assert_eq!(detail.tracks.len(), tracks.len());
}

// ── Kept apart ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn us93_what_is_about_trips_does_not_see_collections() {
    let (app, _dir) = test_app().await;
    let trip = import_sample(&app).await;
    let (id, tracks) = a_collection(&app, "Lakes").await;
    tag(&app, &format!("/api/trips/{trip}/tags"), "norway").await;
    tag(&app, &on_collection(id), "alps").await;
    tag(&app, &on_track(tracks[0]), "alps").await;

    // The trip list's tag filter.
    let trips: serde_json::Value = read_json(&app, "/api/trips?tags=alps").await;
    assert_eq!(trips.as_array().map(Vec::len), Some(0), "{trips}");

    // The tag summaries.
    let summary: serde_json::Value = read_json(&app, "/api/stats/tags?tags=alps").await;
    assert_eq!(summary["tags"][0]["trip_ids"], serde_json::json!([]));
    assert_eq!(summary["trips"], serde_json::json!([]));

    // A summary share: alive, as one whose tags hold no trips is (US-82),
    // and showing none.
    let response = send(
        &app,
        json_request(Method::POST, "/api/shares", r#"{"tags":["alps"]}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let token = serde_json::from_str::<CreatedShare>(&body_string(response).await)
        .unwrap()
        .token;
    let response = send_unauthenticated(
        &app,
        Request::builder()
            .uri(format!("/s/{token}/api/share"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let shared: ShareOverview = serde_json::from_str(&body_string(response).await).unwrap();
    assert!(shared.trips.is_empty());

    // A trip's own tags.
    assert_eq!(
        names(&app, &format!("/api/trips/{trip}/tags")).await,
        ["norway"]
    );
}
