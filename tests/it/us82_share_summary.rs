//! US-82 — as the owner, I share a tag summary by link, so someone I
//! travelled with can look back on the whole vacation.
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *created for the tags the Summary screen shows, with the optional label
//!   and expiry a trip share has* — `us82_the_owner_shares_tags_and_the_link_reads_their_summary`,
//!   `us82_a_share_names_trips_or_tags_never_both_nor_neither`,
//!   `us82_only_known_tags_are_shared`, `us82_only_the_owner_shares_a_summary`.
//! * *the share names the tags, not trips: a trip tagged later appears, one
//!   untagged disappears; planned trips never appear* —
//!   `us82_the_share_follows_the_tags`, `us82_a_planned_trip_never_appears`.
//! * *everything else is US-53's: an ended share answers like a route that
//!   does not exist* — `us82_a_stopped_summary_answers_like_no_share`.
//! * *a share whose tags hold no recorded trips stays alive and says so* —
//!   `us82_a_share_whose_tags_hold_no_trips_stays_alive`.
//! * *a trip's other tags, its Komoot link and every trip outside the shared
//!   tags stay out of reach* — `us82_nothing_outside_the_shared_tags_is_reached`.
//! * *the Shares screen lists it with its tags; the access log counts it as
//!   the share* — `us82_the_owner_sees_the_shares_tags_and_stops_it`,
//!   `us82_the_link_is_logged_as_the_share`.
//!
//! The repository's half is unit-tested in `src/server/repo/share/tests/tags.rs`;
//! the screens in the UI crate.

use crate::common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    response::Response,
    Router,
};
use trip_archive::models::{ActiveShare, CreatedShare, ShareOverview};
use trip_archive::server::auth::Caller;

const PHOTO: &[u8] = include_bytes!("../fixtures/geotagged.jpg");

// ── Helpers ──────────────────────────────────────────────────────────────────

fn create_request(body: serde_json::Value) -> Request<Body> {
    common::json_request(Method::POST, "/api/shares", &body.to_string())
}

/// Share the summary of `tags` as the owner and return the token.
async fn share_tags(app: &Router, tags: &[&str], label: Option<&str>) -> String {
    let response = common::send(
        app,
        create_request(serde_json::json!({ "tags": tags, "label": label })),
    )
    .await;
    let status = response.status();
    let body = common::body_string(response).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "creating a share failed: {body}"
    );
    serde_json::from_str::<CreatedShare>(&body).unwrap().token
}

async fn tag_trip(app: &Router, trip_id: i64, name: &str) {
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
}

async fn untag_trip(app: &Router, trip_id: i64, name: &str) {
    let tags: Vec<serde_json::Value> = serde_json::from_str(
        &common::body_string(common::get(app, &format!("/api/trips/{trip_id}/tags")).await).await,
    )
    .unwrap();
    let tag_id = tags.iter().find(|tag| tag["name"] == name).unwrap()["id"]
        .as_i64()
        .unwrap();
    let response = common::delete(app, &format!("/api/trips/{trip_id}/tags/{tag_id}")).await;
    assert!(response.status().is_success(), "{}", response.status());
}

async fn import_planned(app: &Router) -> i64 {
    let response = common::send(
        app,
        common::import_request_with_fields(common::SAMPLE_GPX, &[("kind", "planned")], &[]),
    )
    .await;
    common::trip_id_from_redirect(&response)
}

/// A request as the recipient makes it: no session, the link alone.
async fn visit(app: &Router, uri: &str) -> Response {
    common::send_unauthenticated(
        app,
        Request::builder().uri(uri).body(Body::empty()).unwrap(),
    )
    .await
}

async fn overview(app: &Router, token: &str) -> ShareOverview {
    let response = visit(app, &format!("/s/{token}/api/share")).await;
    let status = response.status();
    let body = common::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "got {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{e}; got {body}"))
}

fn trip_ids(overview: &ShareOverview) -> Vec<i64> {
    overview.trips.iter().map(|trip| trip.id).collect()
}

async fn active_shares(app: &Router) -> Vec<ActiveShare> {
    serde_json::from_str(&common::body_string(common::get(app, "/api/shares").await).await).unwrap()
}

/// The blob key of the trip's first photo, as the owner's photo URL names it.
async fn photo_key(app: &Router, trip_id: i64) -> String {
    let listed: Vec<serde_json::Value> = serde_json::from_str(
        &common::body_string(common::get(app, &format!("/api/trips/{trip_id}/photos")).await).await,
    )
    .unwrap();
    let url = listed[0]["url"].as_str().unwrap();
    url.strip_prefix("/media/").unwrap().to_string()
}

async fn assert_not_found(response: Response, what: &str) {
    assert_eq!(response.status(), StatusCode::NOT_FOUND, "{what}");
    let error: trip_archive::models::ErrorResponse =
        serde_json::from_str(&common::body_string(response).await).unwrap();
    assert_eq!(error.error, "Not found", "{what}");
}

// ── Creating a share of tags ─────────────────────────────────────────────────

#[tokio::test]
async fn us82_the_owner_shares_tags_and_the_link_reads_their_summary() {
    let (app, _dir) = common::test_app().await;
    let alps = common::import_sample(&app).await;
    let norway = common::import_sample(&app).await;
    tag_trip(&app, alps, "alps").await;
    tag_trip(&app, norway, "norway").await;

    let token = share_tags(&app, &["Norway", "alps"], Some("Summer")).await;
    let overview = overview(&app, &token).await;

    assert_eq!(overview.label.as_deref(), Some("Summer"));
    assert_eq!(trip_ids(&overview), [alps, norway]);
    let summary = overview.summary.expect("a share of tags has a summary");
    let tags: Vec<(&str, &[i64])> = summary
        .tags
        .iter()
        .map(|tag| (tag.name.as_str(), &tag.trip_ids[..]))
        .collect();
    assert_eq!(tags, [("norway", &[norway][..]), ("alps", &[alps][..])]);
    let summed: Vec<i64> = summary.trips.iter().map(|trip| trip.id).collect();
    assert_eq!(summed, [alps, norway]);
    let trip = &summary.trips[0];
    assert!(trip.descent_m.is_some(), "{trip:?}");
    assert!(!trip.start_date.is_empty());
}

#[tokio::test]
async fn us82_a_share_of_trips_has_no_summary() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let response = common::send(
        &app,
        create_request(serde_json::json!({ "trip_ids": [id] })),
    )
    .await;
    let created: CreatedShare = serde_json::from_str(&common::body_string(response).await).unwrap();

    assert_eq!(overview(&app, &created.token).await.summary, None);
}

#[tokio::test]
async fn us82_a_share_names_trips_or_tags_never_both_nor_neither() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    tag_trip(&app, id, "alps").await;

    for body in [
        serde_json::json!({ "trip_ids": [id], "tags": ["alps"] }),
        serde_json::json!({}),
        serde_json::json!({ "tags": [] }),
        serde_json::json!({ "tags": ["has space"] }),
        serde_json::json!({ "tags": ["alps"], "label": "x".repeat(101) }),
    ] {
        let response = common::send(&app, create_request(body.clone())).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }
}

#[tokio::test]
async fn us82_only_known_tags_are_shared() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    tag_trip(&app, id, "alps").await;

    let response = common::send(
        &app,
        create_request(serde_json::json!({ "tags": ["alps", "nowhere"] })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(active_shares(&app).await.is_empty());
}

#[tokio::test]
async fn us82_only_the_owner_shares_a_summary() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    tag_trip(&app, id, "alps").await;

    let response = common::send_unauthenticated(
        &app,
        create_request(serde_json::json!({ "tags": ["alps"] })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ── What the link reaches ────────────────────────────────────────────────────

#[tokio::test]
async fn us82_the_share_follows_the_tags() {
    let (app, _dir) = common::test_app().await;
    let first = common::import_sample(&app).await;
    let later = common::import_sample(&app).await;
    tag_trip(&app, first, "alps").await;
    let token = share_tags(&app, &["alps"], None).await;
    assert_eq!(trip_ids(&overview(&app, &token).await), [first]);

    tag_trip(&app, later, "alps").await;
    untag_trip(&app, first, "alps").await;

    let overview = overview(&app, &token).await;
    assert_eq!(trip_ids(&overview), [later]);
    assert_eq!(overview.summary.unwrap().tags[0].trip_ids, [later]);
    let status = visit(&app, &format!("/s/{token}/api/trips/{later}"))
        .await
        .status();
    assert_eq!(status, StatusCode::OK);
    assert_not_found(
        visit(&app, &format!("/s/{token}/api/trips/{first}")).await,
        "an untagged trip",
    )
    .await;
}

#[tokio::test]
async fn us82_a_planned_trip_never_appears() {
    let (app, _dir) = common::test_app().await;
    let recorded = common::import_sample(&app).await;
    let planned = import_planned(&app).await;
    tag_trip(&app, recorded, "alps").await;
    tag_trip(&app, planned, "alps").await;
    let token = share_tags(&app, &["alps"], None).await;

    assert_eq!(trip_ids(&overview(&app, &token).await), [recorded]);
    assert_not_found(
        visit(&app, &format!("/s/{token}/api/trips/{planned}")).await,
        "a planned trip",
    )
    .await;
}

#[tokio::test]
async fn us82_nothing_outside_the_shared_tags_is_reached() {
    let (app, _dir) = common::test_app().await;
    let shared = common::import_sample_with_photos(&app, &[("a.jpg", PHOTO)]).await;
    let private = common::import_sample_with_photos(&app, &[("b.jpg", PHOTO)]).await;
    tag_trip(&app, shared, "alps").await;
    tag_trip(&app, shared, "secret-tag").await;
    tag_trip(&app, private, "home").await;
    let token = share_tags(&app, &["alps"], None).await;

    for uri in [
        format!("/s/{token}/api/share"),
        format!("/s/{token}/api/trips/{shared}"),
    ] {
        let body = common::body_string(visit(&app, &uri).await).await;
        assert!(!body.contains("secret-tag"), "{uri}: {body}");
        assert!(!body.contains("home"), "{uri}: {body}");
        assert!(!body.contains("komoot"), "{uri}: {body}");
    }
    for uri in [
        format!("/s/{token}/api/trips/{private}"),
        format!("/s/{token}/api/trips/{private}/track.geojson"),
        format!("/s/{token}/api/trips/{private}/photos"),
        format!("/s/{token}/api/trips/{private}/gpx"),
        format!("/s/{token}/api/stats/tags?tags=home"),
    ] {
        assert_not_found(visit(&app, &uri).await, &uri).await;
    }
    let tracks = common::body_string(
        visit(
            &app,
            &format!("/s/{token}/api/trips/tracks?ids={shared},{private}"),
        )
        .await,
    )
    .await;
    let tracks: Vec<serde_json::Value> = serde_json::from_str(&tracks).unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0]["id"], shared);

    // The shared trip's photos are served, the other's are not.
    let shared_key = photo_key(&app, shared).await;
    let private_key = photo_key(&app, private).await;
    let status = visit(&app, &format!("/s/{token}/media/{shared_key}"))
        .await
        .status();
    assert_eq!(status, StatusCode::OK);
    assert_not_found(
        visit(&app, &format!("/s/{token}/media/{private_key}")).await,
        "another trip's photo",
    )
    .await;
}

#[tokio::test]
async fn us82_a_share_whose_tags_hold_no_trips_stays_alive() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    tag_trip(&app, id, "alps").await;
    let token = share_tags(&app, &["alps"], None).await;
    untag_trip(&app, id, "alps").await;

    let overview = overview(&app, &token).await;
    assert!(overview.trips.is_empty());
    let summary = overview.summary.unwrap();
    assert_eq!(summary.tags[0].name, "alps");
    assert!(summary.tags[0].trip_ids.is_empty());
    assert_eq!(active_shares(&app).await.len(), 1);
}

#[tokio::test]
async fn us82_a_stopped_summary_answers_like_no_share() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    tag_trip(&app, id, "alps").await;
    let token = share_tags(&app, &["alps"], None).await;
    let share_id = active_shares(&app).await[0].id;

    let response = common::delete(&app, &format!("/api/shares/{share_id}")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_not_found(
        visit(&app, &format!("/s/{token}/api/share")).await,
        "a stopped share",
    )
    .await;
    assert_not_found(
        visit(&app, &format!("/s/{token}/api/trips/{id}")).await,
        "a stopped share's trip",
    )
    .await;
}

// ── Managing it ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn us82_the_owner_sees_the_shares_tags_and_stops_it() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    tag_trip(&app, id, "alps").await;
    tag_trip(&app, id, "norway").await;
    share_tags(&app, &["norway", "alps"], Some("Summer")).await;

    let shares = active_shares(&app).await;
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].tags, ["norway", "alps"]);
    assert!(shares[0].trip_names.is_empty());

    let response = common::delete(&app, &format!("/api/shares/{}", shares[0].id)).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(active_shares(&app).await.is_empty());
}

#[tokio::test]
async fn us82_the_link_is_logged_as_the_share() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    tag_trip(&app, id, "alps").await;
    let token = share_tags(&app, &["alps"], Some("Summer")).await;
    let share_id = active_shares(&app).await[0].id;

    let response = visit(&app, &format!("/s/{token}/api/share")).await;
    assert_eq!(
        response.extensions().get::<Caller>(),
        Some(&Caller::Share {
            id: share_id,
            label: Some("Summer".to_string()),
        })
    );
}
