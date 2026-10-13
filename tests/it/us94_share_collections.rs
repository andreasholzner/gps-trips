//! US-94 — as the owner, I share whole collections or some of their tracks
//! by link, so someone I plan to travel with can look at the guide books'
//! suggestions.
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *the owner shares collections, or collection tracks from one collection
//!   or several, with the label and expiry a trip share has; a share names
//!   one kind of thing* — `us94_a_share_names_one_kind_of_thing`,
//!   `us94_only_existing_collections_and_tracks_are_shared`,
//!   `us94_only_the_owner_shares_collections`.
//! * *shared collections reach their pages, every track and every waypoint
//!   they hold whenever the link is opened* —
//!   `us94_shared_collections_reach_everything_they_hold`,
//!   `us94_a_shared_collection_follows_its_tracks`.
//! * *shared tracks reach those tracks and their waypoints, but neither their
//!   collections' pages nor their other tracks* —
//!   `us94_shared_tracks_reach_themselves_and_their_waypoints_only`.
//! * *an unknown, expired, stopped or emptied share answers like a route that
//!   does not exist* — `us94_an_emptied_or_stopped_share_answers_like_no_share`,
//!   `us94_an_expired_share_answers_like_no_share`.
//! * *never its tags; a summary share never reaches a collection* —
//!   `us94_no_tag_reaches_the_recipient`,
//!   `us94_other_shares_reach_no_collection`.
//! * *the Shares screen lists the collections or tracks it names; the access
//!   log counts its link as that share's* —
//!   `us94_the_owner_sees_what_the_share_names_and_stops_it`,
//!   `us94_the_link_is_logged_as_the_share`.
//!
//! The repository's half is unit-tested in
//! `src/server/repo/share/tests/collections.rs`; the screens in the UI crate.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    response::Response,
    Router,
};
use trip_archive::models::{
    ActiveShare, CollectionDetail, CreatedShare, ShareOverview, SharedCollectionDetail,
    SharedCollectionTrackDetail, SharedWaypoint, TripTrack,
};
use trip_archive::server::auth::Caller;

use crate::common::{
    self, body_string, get, import_collection, json_request, send, send_unauthenticated, test_app,
    COLLECTION_GPX, COLLECTION_SINGLE_GPX,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn read_json<T: serde::de::DeserializeOwned>(response: Response, what: &str) -> T {
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{what}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{what}: {e}: {body}"))
}

/// A collection named `name` from `gpx`: its id and its tracks' ids.
async fn a_collection(app: &Router, gpx: &[u8], name: &str) -> (i64, Vec<i64>) {
    let id = import_collection(app, gpx, &[("name", name)]).await;
    let uri = format!("/api/collections/{id}");
    let detail: CollectionDetail = read_json(get(app, &uri).await, &uri).await;
    (id, detail.tracks.iter().map(|track| track.id).collect())
}

async fn create(app: &Router, body: serde_json::Value) -> Response {
    send(
        app,
        json_request(Method::POST, "/api/shares", &body.to_string()),
    )
    .await
}

/// Share what `body` names as the owner and return the token.
async fn share(app: &Router, body: serde_json::Value) -> String {
    let response = create(app, body).await;
    let status = response.status();
    let text = body_string(response).await;
    assert_eq!(status, StatusCode::CREATED, "creating a share: {text}");
    serde_json::from_str::<CreatedShare>(&text).unwrap().token
}

/// A request as the recipient makes it: no session, the link alone.
async fn visit(app: &Router, uri: &str) -> Response {
    send_unauthenticated(
        app,
        Request::builder().uri(uri).body(Body::empty()).unwrap(),
    )
    .await
}

async fn read<T: serde::de::DeserializeOwned>(app: &Router, token: &str, path: &str) -> T {
    let uri = format!("/s/{token}{path}");
    read_json(visit(app, &uri).await, &uri).await
}

async fn status_of(app: &Router, token: &str, path: &str) -> StatusCode {
    visit(app, &format!("/s/{token}{path}")).await.status()
}

async fn active_shares(app: &Router) -> Vec<ActiveShare> {
    read_json(get(app, "/api/shares").await, "/api/shares").await
}

fn waypoint_names(waypoints: &[SharedWaypoint]) -> Vec<&str> {
    waypoints
        .iter()
        .map(|waypoint| waypoint.name.as_deref().unwrap_or_default())
        .collect()
}

async fn tag(app: &Router, uri: &str, name: &str) {
    let body = serde_json::json!({ "name": name });
    let response = send(app, json_request(Method::POST, uri, &body.to_string())).await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

// ── Creating a share ─────────────────────────────────────────────────────────

#[tokio::test]
async fn us94_a_share_names_one_kind_of_thing() {
    let (app, _dir) = test_app().await;
    let trip = common::import_sample(&app).await;
    let (collection, tracks) = a_collection(&app, COLLECTION_GPX, "Alps").await;

    for body in [
        serde_json::json!({}),
        serde_json::json!({ "collection_ids": [], "collection_track_ids": [] }),
        serde_json::json!({ "collection_ids": [collection], "collection_track_ids": [tracks[0]] }),
        serde_json::json!({ "collection_ids": [collection], "trip_ids": [trip] }),
        serde_json::json!({ "collection_track_ids": [tracks[0]], "tags": ["alps"] }),
    ] {
        let response = create(&app, body.clone()).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }
    assert!(active_shares(&app).await.is_empty());
}

#[tokio::test]
async fn us94_only_existing_collections_and_tracks_are_shared() {
    let (app, _dir) = test_app().await;
    let (collection, tracks) = a_collection(&app, COLLECTION_GPX, "Alps").await;

    let response = create(
        &app,
        serde_json::json!({ "collection_ids": [collection, collection + 100] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = create(
        &app,
        serde_json::json!({ "collection_track_ids": [tracks[0], tracks[2] + 100] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(active_shares(&app).await.is_empty());
}

#[tokio::test]
async fn us94_only_the_owner_shares_collections() {
    let (app, _dir) = test_app().await;
    let (collection, _) = a_collection(&app, COLLECTION_GPX, "Alps").await;
    let body = serde_json::json!({ "collection_ids": [collection] }).to_string();

    let response =
        send_unauthenticated(&app, json_request(Method::POST, "/api/shares", &body)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ── What the recipient reaches ───────────────────────────────────────────────

#[tokio::test]
async fn us94_shared_collections_reach_everything_they_hold() {
    let (app, _dir) = test_app().await;
    let (alps, alps_tracks) = a_collection(&app, COLLECTION_GPX, "Alps").await;
    let (tyrol, tyrol_tracks) = a_collection(&app, COLLECTION_SINGLE_GPX, "Tyrol").await;
    let (other, other_tracks) = a_collection(&app, COLLECTION_SINGLE_GPX, "Other").await;
    let token = share(
        &app,
        serde_json::json!({ "collection_ids": [tyrol, alps], "label": "For Kari", "expiry": "one_month" }),
    )
    .await;

    let overview: ShareOverview = read(&app, &token, "/api/share").await;
    assert_eq!(overview.label.as_deref(), Some("For Kari"));
    assert!(overview.trips.is_empty());
    let names: Vec<&str> = overview
        .collections
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["Alps", "Tyrol"]);
    assert_eq!(overview.collections[0].track_count, 3);
    assert_eq!(overview.collections[0].waypoint_count, 2);
    let reached: Vec<i64> = overview.collection_tracks.iter().map(|t| t.id).collect();
    let expected: Vec<i64> = alps_tracks.iter().chain(&tyrol_tracks).copied().collect();
    assert_eq!(reached, expected);

    let detail: SharedCollectionDetail =
        read(&app, &token, &format!("/api/collections/{alps}")).await;
    assert_eq!(detail.name, "Alps");
    assert_eq!(detail.tracks.len(), 3);
    let waypoints: Vec<SharedWaypoint> =
        read(&app, &token, &format!("/api/collections/{alps}/waypoints")).await;
    assert_eq!(waypoints.len(), 2);
    let all: Vec<SharedWaypoint> = read(&app, &token, "/api/share/waypoints").await;
    assert_eq!(all.len(), 4);

    let track = alps_tracks[1];
    let detail: SharedCollectionTrackDetail =
        read(&app, &token, &format!("/api/collection-tracks/{track}")).await;
    assert_eq!(detail.track.id, track);
    assert_eq!(detail.collection.map(|c| c.name).as_deref(), Some("Alps"));
    for path in ["track.geojson", "gpx", "waypoints"] {
        let uri = format!("/api/collection-tracks/{track}/{path}");
        assert_eq!(status_of(&app, &token, &uri).await, StatusCode::OK, "{uri}");
    }
    let ids = format!(
        "/api/collection-tracks?ids={},{},{}",
        alps_tracks[0], other_tracks[0], tyrol_tracks[0]
    );
    let positions: Vec<TripTrack> = read(&app, &token, &ids).await;
    let drawn: Vec<i64> = positions.iter().map(|p| p.id).collect();
    assert_eq!(drawn, [alps_tracks[0], tyrol_tracks[0]]);

    for path in [
        format!("/api/collections/{other}"),
        format!("/api/collections/{other}/waypoints"),
        format!("/api/collection-tracks/{}", other_tracks[0]),
        format!("/api/collection-tracks/{}/track.geojson", other_tracks[0]),
        format!("/api/collection-tracks/{}/gpx", other_tracks[0]),
        format!("/api/collection-tracks/{}/waypoints", other_tracks[0]),
    ] {
        assert_eq!(
            status_of(&app, &token, &path).await,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
}

#[tokio::test]
async fn us94_a_shared_collection_follows_its_tracks() {
    let (app, _dir) = test_app().await;
    let (alps, tracks) = a_collection(&app, COLLECTION_GPX, "Alps").await;
    let token = share(&app, serde_json::json!({ "collection_ids": [alps] })).await;

    let response = common::delete(&app, &format!("/api/collection-tracks/{}", tracks[0])).await;
    assert!(response.status().is_success());
    let overview: ShareOverview = read(&app, &token, "/api/share").await;
    let reached: Vec<i64> = overview.collection_tracks.iter().map(|t| t.id).collect();
    assert_eq!(reached, tracks[1..]);
    assert_eq!(
        status_of(
            &app,
            &token,
            &format!("/api/collection-tracks/{}", tracks[0])
        )
        .await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn us94_shared_tracks_reach_themselves_and_their_waypoints_only() {
    let (app, _dir) = test_app().await;
    let (alps, alps_tracks) = a_collection(&app, COLLECTION_GPX, "Alps").await;
    let (tyrol, tyrol_tracks) = a_collection(&app, COLLECTION_SINGLE_GPX, "Tyrol").await;
    let token = share(
        &app,
        serde_json::json!({ "collection_track_ids": [tyrol_tracks[0], alps_tracks[0]] }),
    )
    .await;

    let overview: ShareOverview = read(&app, &token, "/api/share").await;
    assert!(overview.collections.is_empty());
    let reached: Vec<i64> = overview.collection_tracks.iter().map(|t| t.id).collect();
    assert_eq!(reached, [alps_tracks[0], tyrol_tracks[0]]);

    let detail: SharedCollectionTrackDetail = read(
        &app,
        &token,
        &format!("/api/collection-tracks/{}", tyrol_tracks[0]),
    )
    .await;
    assert_eq!(detail.track.name, "Nordkette");
    assert_eq!(detail.collection, None, "its collection is not shared");
    let waypoints: Vec<SharedWaypoint> = read(
        &app,
        &token,
        &format!("/api/collection-tracks/{}/waypoints", tyrol_tracks[0]),
    )
    .await;
    assert_eq!(waypoint_names(&waypoints).len(), 2);
    // The Alps' waypoints belong to the collection alone, with three tracks.
    let all: Vec<SharedWaypoint> = read(&app, &token, "/api/share/waypoints").await;
    assert_eq!(waypoint_names(&all), waypoint_names(&waypoints));

    for path in [
        format!("/api/collections/{alps}"),
        format!("/api/collections/{tyrol}"),
        format!("/api/collections/{alps}/waypoints"),
        format!("/api/collection-tracks/{}", alps_tracks[1]),
        format!("/api/collection-tracks/{}/gpx", alps_tracks[2]),
    ] {
        assert_eq!(
            status_of(&app, &token, &path).await,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
}

#[tokio::test]
async fn us94_no_tag_reaches_the_recipient() {
    let (app, _dir) = test_app().await;
    let (alps, tracks) = a_collection(&app, COLLECTION_GPX, "Alps").await;
    tag(&app, &format!("/api/collections/{alps}/tags"), "secretbook").await;
    tag(
        &app,
        &format!("/api/collection-tracks/{}/tags", tracks[0]),
        "secrettrack",
    )
    .await;
    let token = share(&app, serde_json::json!({ "collection_ids": [alps] })).await;

    for path in [
        "/api/share".to_string(),
        format!("/api/collections/{alps}"),
        format!("/api/collection-tracks/{}", tracks[0]),
    ] {
        let body = body_string(visit(&app, &format!("/s/{token}{path}")).await).await;
        assert!(!body.contains("secret"), "{path}: {body}");
    }
    assert_eq!(
        status_of(&app, &token, &format!("/api/collections/{alps}/tags")).await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn us94_other_shares_reach_no_collection() {
    let (app, _dir) = test_app().await;
    let trip = common::import_sample(&app).await;
    tag(&app, &format!("/api/trips/{trip}/tags"), "alps").await;
    let (alps, tracks) = a_collection(&app, COLLECTION_GPX, "Alps").await;
    tag(&app, &format!("/api/collections/{alps}/tags"), "alps").await;
    let summary = share(&app, serde_json::json!({ "tags": ["alps"] })).await;
    let trips = share(&app, serde_json::json!({ "trip_ids": [trip] })).await;
    let collections = share(&app, serde_json::json!({ "collection_ids": [alps] })).await;

    for token in [&summary, &trips] {
        let overview: ShareOverview = read(&app, token, "/api/share").await;
        assert!(overview.collections.is_empty() && overview.collection_tracks.is_empty());
        for path in [
            format!("/api/collections/{alps}"),
            format!("/api/collection-tracks/{}", tracks[0]),
        ] {
            assert_eq!(status_of(&app, token, &path).await, StatusCode::NOT_FOUND);
        }
        let all: Vec<SharedWaypoint> = read(&app, token, "/api/share/waypoints").await;
        assert!(all.is_empty());
    }
    assert_eq!(
        status_of(&app, &collections, &format!("/api/trips/{trip}")).await,
        StatusCode::NOT_FOUND
    );
}

// ── An ended share ───────────────────────────────────────────────────────────

#[tokio::test]
async fn us94_an_emptied_or_stopped_share_answers_like_no_share() {
    let (app, _dir) = test_app().await;
    let (alps, _) = a_collection(&app, COLLECTION_GPX, "Alps").await;
    let (tyrol, tyrol_tracks) = a_collection(&app, COLLECTION_SINGLE_GPX, "Tyrol").await;
    let collections = share(&app, serde_json::json!({ "collection_ids": [alps, tyrol] })).await;
    let tracks = share(
        &app,
        serde_json::json!({ "collection_track_ids": [tyrol_tracks[0]] }),
    )
    .await;

    common::delete(&app, &format!("/api/collections/{tyrol}")).await;
    assert_eq!(
        status_of(&app, &collections, "/api/share").await,
        StatusCode::OK,
        "one collection is left"
    );
    assert_eq!(
        status_of(&app, &tracks, "/api/share").await,
        StatusCode::NOT_FOUND
    );

    let id = active_shares(&app).await[0].id;
    let response = common::delete(&app, &format!("/api/shares/{id}")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        status_of(&app, &collections, &format!("/api/collections/{alps}")).await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn us94_an_expired_share_answers_like_no_share() {
    let (app, state, _dir) = common::test_app_with_state(None).await;
    let (alps, _) = a_collection(&app, COLLECTION_GPX, "Alps").await;
    let token = share(
        &app,
        serde_json::json!({ "collection_ids": [alps], "expiry": "one_month" }),
    )
    .await;
    sqlx::query("UPDATE share SET expires_at = '2000-01-01T00:00:00Z'")
        .execute(&state.pool)
        .await
        .unwrap();

    assert_eq!(
        status_of(&app, &token, &format!("/api/collections/{alps}")).await,
        StatusCode::NOT_FOUND
    );
}

// ── Managing it ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn us94_the_owner_sees_what_the_share_names_and_stops_it() {
    let (app, _dir) = test_app().await;
    let (alps, alps_tracks) = a_collection(&app, COLLECTION_GPX, "Alps").await;
    let (tyrol, tyrol_tracks) = a_collection(&app, COLLECTION_SINGLE_GPX, "Tyrol").await;
    share(&app, serde_json::json!({ "collection_ids": [tyrol, alps] })).await;
    share(
        &app,
        serde_json::json!({ "collection_track_ids": [tyrol_tracks[0], alps_tracks[2]] }),
    )
    .await;

    let shares = active_shares(&app).await;
    assert_eq!(shares.len(), 2);
    assert_eq!(
        shares[0].collection_track_names,
        ["Stage 3: Albula", "Nordkette"]
    );
    assert!(shares[0].collection_names.is_empty());
    assert_eq!(shares[1].collection_names, ["Alps", "Tyrol"]);
    assert!(shares[1].trip_names.is_empty() && shares[1].tags.is_empty());

    for share in &shares {
        let response = common::delete(&app, &format!("/api/shares/{}", share.id)).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
    assert!(active_shares(&app).await.is_empty());
}

#[tokio::test]
async fn us94_the_link_is_logged_as_the_share() {
    let (app, _dir) = test_app().await;
    let (alps, _) = a_collection(&app, COLLECTION_GPX, "Alps").await;
    let token = share(
        &app,
        serde_json::json!({ "collection_ids": [alps], "label": "Summer" }),
    )
    .await;
    let share_id = active_shares(&app).await[0].id;

    let response = visit(&app, &format!("/s/{token}/api/collections/{alps}")).await;
    assert_eq!(
        response.extensions().get::<Caller>(),
        Some(&Caller::Share {
            id: share_id,
            label: Some("Summer".to_string()),
        })
    );
}
