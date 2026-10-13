//! US-100 — as the owner, I select several collections and tag them
//! together, so organizing a shelf of guide books does not take a visit to
//! each one's page.
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *the staged tag names are applied to every selected collection in one
//!   request, a new tag created on demand* —
//!   `us100_every_selected_collection_carries_every_tag`.
//! * *a collection that no longer exists 404s the whole request, and nothing
//!   is created or linked; an invalid tag name 400s* —
//!   `us100_an_unknown_collection_tags_nothing`,
//!   `us100_an_invalid_or_empty_request_is_refused`.
//! * *only the collections are tagged, never their tracks* —
//!   `us100_every_selected_collection_carries_every_tag`.
//! * *afterwards the tag suggestions offer the new tags* — the tag list in
//!   `us100_every_selected_collection_carries_every_tag`.
//!
//! The repository's half is unit-tested in
//! `src/server/repo/tag/tests/collections.rs`; the screens in the UI crate
//! and `tests/browser/collections_page.spec.mjs`.

use axum::{
    http::{Method, StatusCode},
    Router,
};
use trip_archive::models::{CollectionDetail, Tag, TagOverview};

use crate::common::{
    body_string, get, import_collection, import_sample, json_request, send, send_unauthenticated,
    test_app, COLLECTION_GPX,
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

async fn bulk_tag(app: &Router, body: serde_json::Value) -> axum::response::Response {
    send(
        app,
        json_request(Method::POST, "/api/collections/tags", &body.to_string()),
    )
    .await
}

async fn names_on(app: &Router, uri: &str) -> Vec<String> {
    let tags: Vec<Tag> = read_json(app, uri).await;
    tags.into_iter().map(|tag| tag.name).collect()
}

async fn all_tags(app: &Router) -> Vec<String> {
    names_on(app, "/api/tags").await
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us100_every_selected_collection_carries_every_tag() {
    let (app, _dir) = test_app().await;
    let trip = import_sample(&app).await;
    let (alps, alps_tracks) = a_collection(&app, "Alps").await;
    let (tyrol, _) = a_collection(&app, "Tyrol").await;
    let (other, _) = a_collection(&app, "Other").await;

    let response = bulk_tag(
        &app,
        serde_json::json!({ "collection_ids": [alps, tyrol], "names": ["Shelf", "alpine"] }),
    )
    .await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let applied: Vec<Tag> = serde_json::from_str(&body).unwrap();
    assert_eq!(
        applied
            .iter()
            .map(|tag| tag.name.as_str())
            .collect::<Vec<_>>(),
        ["shelf", "alpine"],
        "normalized as a single tag is"
    );

    for collection in [alps, tyrol] {
        assert_eq!(
            names_on(&app, &format!("/api/collections/{collection}/tags")).await,
            ["alpine", "shelf"]
        );
    }
    assert!(names_on(&app, &format!("/api/collections/{other}/tags"))
        .await
        .is_empty());
    for track in alps_tracks {
        assert!(
            names_on(&app, &format!("/api/collection-tracks/{track}/tags"))
                .await
                .is_empty(),
            "a collection's tags do not pass to its tracks"
        );
    }
    assert!(names_on(&app, &format!("/api/trips/{trip}/tags"))
        .await
        .is_empty());
    assert_eq!(all_tags(&app).await, ["alpine", "shelf"]);
    let overview: Vec<TagOverview> = read_json(&app, "/api/tags/overview").await;
    assert!(overview.iter().all(|tag| tag.collection_count == 2));
}

#[tokio::test]
async fn us100_an_unknown_collection_tags_nothing() {
    let (app, _dir) = test_app().await;
    let (alps, _) = a_collection(&app, "Alps").await;

    let response = bulk_tag(
        &app,
        serde_json::json!({ "collection_ids": [alps, alps + 100], "names": ["shelf"] }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(all_tags(&app).await.is_empty(), "no tag created");
    assert!(names_on(&app, &format!("/api/collections/{alps}/tags"))
        .await
        .is_empty());
}

#[tokio::test]
async fn us100_an_invalid_or_empty_request_is_refused() {
    let (app, _dir) = test_app().await;
    let (alps, _) = a_collection(&app, "Alps").await;

    for body in [
        serde_json::json!({ "collection_ids": [alps], "names": ["two words"] }),
        serde_json::json!({ "collection_ids": [alps], "names": ["shelf", ""] }),
        serde_json::json!({ "collection_ids": [], "names": ["shelf"] }),
        serde_json::json!({ "collection_ids": [alps], "names": [] }),
    ] {
        let response = bulk_tag(&app, body.clone()).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }
    assert!(all_tags(&app).await.is_empty(), "no tag created");
}

#[tokio::test]
async fn us100_only_the_owner_tags_collections() {
    let (app, _dir) = test_app().await;
    let (alps, _) = a_collection(&app, "Alps").await;
    let body = serde_json::json!({ "collection_ids": [alps], "names": ["shelf"] }).to_string();

    let response = send_unauthenticated(
        &app,
        json_request(Method::POST, "/api/collections/tags", &body),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(all_tags(&app).await.is_empty());
}
