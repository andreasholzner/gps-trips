//! US-83 — as the owner, I see every tag in one place, so I can reach a
//! tag's summary, clear out tags I no longer use and set up a tag before any
//! trip carries it.
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *every tag alphabetically, those no trip carries included, with the
//!   number of trips carrying it; whether it has a recorded trip; the active
//!   summary shares naming it* — `us83_every_tag_is_listed_with_its_trips_and_shares`.
//! * *deleting a tag takes it off every trip and changes no trip; once
//!   deleted it is no longer suggested, offered as a filter or choosable on
//!   the Summary screen* — `us83_a_deleted_tag_is_gone_and_its_trips_stay`,
//!   `us83_deleting_an_unknown_tag_is_not_found`.
//! * *a shared tag leaves its share: narrowed, or stopped as the Shares
//!   screen would* — `us83_a_deleted_tag_leaves_its_shares`.
//! * *creating a tag by name, normalized and validated as US-33's, not
//!   twice, and the screen says why* — `us83_the_owner_creates_a_tag_carrying_no_trips`,
//!   `us83_an_invalid_name_is_refused_with_the_reason`,
//!   `us83_an_existing_name_is_refused_as_existing`.
//! * *the Tags screen is the owner's* — `us83_only_the_owner_reads_or_changes_tags`.
//!
//! The repository's half is unit-tested in `src/server/repo/tag/tests.rs`;
//! the screen, its filter, paging and confirmation in the UI crate.

use crate::common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use trip_archive::models::{ActiveShare, CreatedShare, Tag, TagOverview};

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn tag_trip(app: &Router, trip_id: i64, name: &str) -> i64 {
    let response = common::send(
        app,
        common::json_request(
            Method::POST,
            &format!("/api/trips/{trip_id}/tags"),
            &format!(r#"{{"name":"{name}"}}"#),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    serde_json::from_str::<Tag>(&common::body_string(response).await)
        .unwrap()
        .id
}

async fn share_tags(app: &Router, tags: &[&str], label: Option<&str>) -> String {
    let body = serde_json::json!({ "tags": tags, "label": label });
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

async fn tags_overview(app: &Router) -> Vec<TagOverview> {
    let response = common::get(app, "/api/tags/overview").await;
    let status = response.status();
    let body = common::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "got {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{e}; got {body}"))
}

async fn create_tag(app: &Router, name: &str) -> axum::response::Response {
    let body = serde_json::json!({ "name": name });
    common::send(
        app,
        common::json_request(Method::POST, "/api/tags", &body.to_string()),
    )
    .await
}

async fn tag_names(app: &Router, uri: &str) -> Vec<String> {
    let tags: Vec<Tag> =
        serde_json::from_str(&common::body_string(common::get(app, uri).await).await).unwrap();
    tags.into_iter().map(|tag| tag.name).collect()
}

async fn active_shares(app: &Router) -> Vec<ActiveShare> {
    serde_json::from_str(&common::body_string(common::get(app, "/api/shares").await).await).unwrap()
}

async fn share_status(app: &Router, token: &str) -> StatusCode {
    common::send_unauthenticated(
        app,
        Request::builder()
            .uri(format!("/s/{token}/api/share"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .status()
}

// ── Listing ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us83_every_tag_is_listed_with_its_trips_and_shares() {
    let (app, _dir) = common::test_app().await;
    let recorded = common::import_sample(&app).await;
    let planned = common::send(
        &app,
        common::import_request_with_fields(common::SAMPLE_GPX, &[("kind", "planned")], &[]),
    )
    .await;
    let planned = common::trip_id_from_redirect(&planned);
    tag_trip(&app, recorded, "norway").await;
    tag_trip(&app, planned, "norway").await;
    tag_trip(&app, planned, "plans").await;
    assert_eq!(create_tag(&app, "alps").await.status(), StatusCode::CREATED);
    share_tags(&app, &["norway"], Some("Summer")).await;

    let tags = tags_overview(&app).await;

    let rows: Vec<(&str, i64, i64, usize)> = tags
        .iter()
        .map(|tag| {
            (
                tag.name.as_str(),
                tag.trip_count,
                tag.recorded_trip_count,
                tag.shares.len(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![("alps", 0, 0, 0), ("norway", 2, 1, 1), ("plans", 1, 0, 0)]
    );
    assert_eq!(tags[1].shares[0].label.as_deref(), Some("Summer"));
}

// ── Deleting ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us83_a_deleted_tag_is_gone_and_its_trips_stay() {
    let (app, _dir) = common::test_app().await;
    let trip = common::import_sample(&app).await;
    let alps = tag_trip(&app, trip, "alps").await;
    tag_trip(&app, trip, "norway").await;

    let response = common::delete(&app, &format!("/api/tags/{alps}")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // No longer suggested, offered as a filter or choosable: all three read
    // `/api/tags`.
    assert_eq!(tag_names(&app, "/api/tags").await, vec!["norway"]);
    assert_eq!(
        tag_names(&app, &format!("/api/trips/{trip}/tags")).await,
        vec!["norway"]
    );
    assert_eq!(
        common::get(&app, &format!("/api/trips/{trip}"))
            .await
            .status(),
        StatusCode::OK,
        "the trip stays"
    );
}

#[tokio::test]
async fn us83_deleting_an_unknown_tag_is_not_found() {
    let (app, _dir) = common::test_app().await;

    let response = common::delete(&app, "/api/tags/999").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us83_a_deleted_tag_leaves_its_shares() {
    let (app, _dir) = common::test_app().await;
    let trip = common::import_sample(&app).await;
    let alps = tag_trip(&app, trip, "alps").await;
    tag_trip(&app, trip, "norway").await;
    let narrowed = share_tags(&app, &["alps", "norway"], None).await;
    let stopped = share_tags(&app, &["alps"], Some("Alps only")).await;

    common::delete(&app, &format!("/api/tags/{alps}")).await;

    assert_eq!(share_status(&app, &narrowed).await, StatusCode::OK);
    assert_eq!(share_status(&app, &stopped).await, StatusCode::NOT_FOUND);
    let shares = active_shares(&app).await;
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].tags, vec!["norway"]);
}

// ── Creating ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us83_the_owner_creates_a_tag_carrying_no_trips() {
    let (app, _dir) = common::test_app().await;

    let response = create_tag(&app, "  Alps ").await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let created: Tag = serde_json::from_str(&common::body_string(response).await).unwrap();
    assert_eq!(created.name, "alps");
    assert_eq!(tag_names(&app, "/api/tags").await, vec!["alps"]);
}

#[tokio::test]
async fn us83_an_invalid_name_is_refused_with_the_reason() {
    let (app, _dir) = common::test_app().await;

    let response = create_tag(&app, "day trip").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        common::error_message(response).await,
        "tag name cannot contain spaces"
    );
    assert!(tag_names(&app, "/api/tags").await.is_empty());
}

#[tokio::test]
async fn us83_an_existing_name_is_refused_as_existing() {
    let (app, _dir) = common::test_app().await;
    create_tag(&app, "alps").await;

    let response = create_tag(&app, "ALPS").await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        common::error_message(response).await,
        "tag \"alps\" already exists"
    );
    assert_eq!(tag_names(&app, "/api/tags").await, vec!["alps"]);
}

// ── Access ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us83_only_the_owner_reads_or_changes_tags() {
    let (app, _dir) = common::test_app().await;
    let trip = common::import_sample(&app).await;
    let alps = tag_trip(&app, trip, "alps").await;
    let token = share_tags(&app, &["alps"], None).await;

    let request = |method: Method, uri: String| {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":"new"}"#))
            .unwrap()
    };
    let routes = |prefix: &str| {
        [
            (Method::GET, format!("{prefix}/api/tags/overview")),
            (Method::POST, format!("{prefix}/api/tags")),
            (Method::DELETE, format!("{prefix}/api/tags/{alps}")),
        ]
    };
    for (method, uri) in routes("") {
        let response = common::send_unauthenticated(&app, request(method.clone(), uri)).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{method}");
    }
    for (method, uri) in routes(&format!("/s/{token}")) {
        let response = common::send_unauthenticated(&app, request(method.clone(), uri)).await;
        assert!(
            matches!(
                response.status(),
                StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
            ),
            "{method} answered {}",
            response.status()
        );
    }

    assert_eq!(tag_names(&app, "/api/tags").await, vec!["alps"]);
}
