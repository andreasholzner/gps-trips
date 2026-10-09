//! US-85 — as the owner, I can rename a tag on the Tags page, so that I can
//! fix a typo or reword a tag without retagging every trip that carries it.
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *only the name changes: every trip that carried the tag still carries it
//!   under the new name — on the trip, in the list's tag filter, the
//!   suggestions and the Tags screen; a summary share naming it keeps
//!   working and shows the new name* — `us85_a_renamed_tag_shows_its_new_name_everywhere`.
//! * *the new name is normalized and validated as US-33's, and the screen
//!   says why a name is refused; a name another tag has is refused as
//!   existing* — `us85_an_invalid_name_is_refused_with_the_reason`,
//!   `us85_a_name_another_tag_has_is_refused_as_existing`.
//! * *renaming to the tag's own name changes nothing* —
//!   `us85_renaming_to_its_own_name_changes_nothing`.
//! * *an unknown tag, and renaming is the owner's* —
//!   `us85_renaming_an_unknown_tag_is_not_found`, `us85_only_the_owner_renames_a_tag`.
//!
//! The repository's half is unit-tested in `src/server/repo/tag/tests.rs`;
//! the screen's rename form in the UI crate.

use crate::common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    response::Response,
    Router,
};
use trip_archive::models::{CreatedShare, ShareOverview, Tag, TagOverview, TripSummary};

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn tag_trip(app: &Router, trip_id: i64, name: &str) -> i64 {
    let body = serde_json::json!({ "name": name });
    let response = common::send(
        app,
        common::json_request(
            Method::POST,
            &format!("/api/trips/{trip_id}/tags"),
            &body.to_string(),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    serde_json::from_str::<Tag>(&common::body_string(response).await)
        .unwrap()
        .id
}

async fn rename(app: &Router, tag_id: i64, name: &str) -> Response {
    let body = serde_json::json!({ "name": name });
    common::send(
        app,
        common::json_request(
            Method::PATCH,
            &format!("/api/tags/{tag_id}"),
            &body.to_string(),
        ),
    )
    .await
}

async fn tag_names(app: &Router, uri: &str) -> Vec<String> {
    let tags: Vec<Tag> =
        serde_json::from_str(&common::body_string(common::get(app, uri).await).await).unwrap();
    tags.into_iter().map(|tag| tag.name).collect()
}

async fn share_tag(app: &Router, name: &str) -> String {
    let body = serde_json::json!({ "tags": [name] });
    let response = common::send(
        app,
        common::json_request(Method::POST, "/api/shares", &body.to_string()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    serde_json::from_str::<CreatedShare>(&common::body_string(response).await)
        .unwrap()
        .token
}

async fn shared_overview(app: &Router, token: &str) -> ShareOverview {
    let response = common::send_unauthenticated(
        app,
        Request::builder()
            .uri(format!("/s/{token}/api/share"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = common::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "got {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{e}; got {body}"))
}

async fn listed_trip_ids(app: &Router, tags: &str) -> Vec<i64> {
    let body =
        common::body_string(common::get(app, &format!("/api/trips?tags={tags}")).await).await;
    let trips: Vec<TripSummary> =
        serde_json::from_str(&body).unwrap_or_else(|e| panic!("{e}; got {body}"));
    trips.into_iter().map(|trip| trip.id).collect()
}

// ── Renaming ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us85_a_renamed_tag_shows_its_new_name_everywhere() {
    let (app, _dir) = common::test_app().await;
    let trip = common::import_sample(&app).await;
    let alps = tag_trip(&app, trip, "alps").await;
    let token = share_tag(&app, "alps").await;

    let response = rename(&app, alps, " Alpen").await;

    assert_eq!(response.status(), StatusCode::OK);
    let renamed: Tag = serde_json::from_str(&common::body_string(response).await).unwrap();
    assert_eq!(
        renamed,
        Tag {
            id: alps,
            name: "alpen".to_string()
        }
    );
    assert_eq!(
        tag_names(&app, &format!("/api/trips/{trip}/tags")).await,
        vec!["alpen"]
    );
    assert_eq!(tag_names(&app, "/api/tags").await, vec!["alpen"]);
    assert_eq!(listed_trip_ids(&app, "alpen").await, vec![trip]);
    assert!(listed_trip_ids(&app, "alps").await.is_empty());
    let overview: Vec<TagOverview> = serde_json::from_str(
        &common::body_string(common::get(&app, "/api/tags/overview").await).await,
    )
    .unwrap();
    assert_eq!(overview[0].name, "alpen");
    assert_eq!(overview[0].trip_count, 1);
    assert_eq!(overview[0].shares.len(), 1, "the share still names it");
    let summary = shared_overview(&app, &token)
        .await
        .summary
        .expect("a share of tags has a summary");
    assert_eq!(summary.tags[0].name, "alpen");
    assert_eq!(summary.tags[0].trip_ids, vec![trip]);
}

#[tokio::test]
async fn us85_an_invalid_name_is_refused_with_the_reason() {
    let (app, _dir) = common::test_app().await;
    let trip = common::import_sample(&app).await;
    let alps = tag_trip(&app, trip, "alps").await;

    let response = rename(&app, alps, "day trip").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        common::error_message(response).await,
        "tag name cannot contain spaces"
    );
    assert_eq!(tag_names(&app, "/api/tags").await, vec!["alps"]);
}

#[tokio::test]
async fn us85_a_name_another_tag_has_is_refused_as_existing() {
    let (app, _dir) = common::test_app().await;
    let trip = common::import_sample(&app).await;
    let alps = tag_trip(&app, trip, "alps").await;
    tag_trip(&app, trip, "norway").await;

    let response = rename(&app, alps, "NORWAY").await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        common::error_message(response).await,
        "tag \"norway\" already exists"
    );
    assert_eq!(
        tag_names(&app, &format!("/api/trips/{trip}/tags")).await,
        vec!["alps", "norway"]
    );
}

#[tokio::test]
async fn us85_renaming_to_its_own_name_changes_nothing() {
    let (app, _dir) = common::test_app().await;
    let trip = common::import_sample(&app).await;
    let alps = tag_trip(&app, trip, "alps").await;

    let response = rename(&app, alps, "Alps").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        tag_names(&app, &format!("/api/trips/{trip}/tags")).await,
        vec!["alps"]
    );
}

#[tokio::test]
async fn us85_renaming_an_unknown_tag_is_not_found() {
    let (app, _dir) = common::test_app().await;

    let response = rename(&app, 999, "alpen").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(tag_names(&app, "/api/tags").await.is_empty());
}

#[tokio::test]
async fn us85_only_the_owner_renames_a_tag() {
    let (app, _dir) = common::test_app().await;
    let trip = common::import_sample(&app).await;
    let alps = tag_trip(&app, trip, "alps").await;
    let token = share_tag(&app, "alps").await;

    let request = |uri: String| {
        Request::builder()
            .method(Method::PATCH)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":"alpen"}"#))
            .unwrap()
    };
    let response = common::send_unauthenticated(&app, request(format!("/api/tags/{alps}"))).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response =
        common::send_unauthenticated(&app, request(format!("/s/{token}/api/tags/{alps}"))).await;
    assert!(
        matches!(
            response.status(),
            StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
        ),
        "answered {}",
        response.status()
    );

    assert_eq!(tag_names(&app, "/api/tags").await, vec!["alps"]);
}
