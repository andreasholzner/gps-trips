//! US-53 — as the owner, I give someone read-only access to a trip or a few,
//! with the GPX downloadable, without giving them my password.
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *a share is an unguessable link scoped to the trips it names* —
//!   `us53_the_owner_creates_a_share_and_its_link_reads_the_trip`,
//!   `us53_a_share_reaches_none_of_the_trips_it_does_not_name`.
//! * *read-only, no account* — `us53_a_share_reaches_no_owner_route`,
//!   `us53_a_share_changes_nothing`.
//! * *it reaches those trips' photos, tracks and GPX and nothing else* —
//!   the first two, plus `us53_a_share_serves_only_its_trips_photos` and
//!   `us53_the_recipient_sees_no_tags_and_no_komoot_link`.
//! * *optionally expiring; an unknown, expired or emptied share answers like
//!   a route that does not exist* — `us53_an_ended_share_answers_like_no_share`.
//! * *only the owner creates shares* — `us53_only_the_owner_creates_a_share`.
//!
//! The repository's half (expiry as instants, cascades) is unit-tested in
//! `src/server/repo/share/tests.rs`; the screens in the UI crate.

use crate::common;

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    response::Response,
    Router,
};
use trip_archive::config;
use trip_archive::models::{CreatedShare, PhotoResponse, ShareOverview, SharedTrip, TripDetail};

const PHOTO: &[u8] = include_bytes!("../fixtures/geotagged.jpg");

// ── Helpers ──────────────────────────────────────────────────────────────────

fn create_request(body: serde_json::Value) -> Request<Body> {
    common::json_request(Method::POST, "/api/shares", &body.to_string())
}

/// Share `trip_ids` as the owner and return the token.
async fn share(app: &Router, trip_ids: &[i64]) -> String {
    let response = common::send(
        app,
        create_request(serde_json::json!({ "trip_ids": trip_ids, "label": "For Kari" })),
    )
    .await;
    let status = response.status();
    let body = common::body_string(response).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "creating a share failed: {body}"
    );
    let created: CreatedShare = serde_json::from_str(&body).expect("CreatedShare JSON");
    created.token
}

/// A request as the recipient makes it: no session, the link alone.
async fn visit(app: &Router, uri: &str) -> Response {
    common::send_unauthenticated(
        app,
        Request::builder().uri(uri).body(Body::empty()).unwrap(),
    )
    .await
}

async fn json<T: serde::de::DeserializeOwned>(response: Response) -> T {
    let status = response.status();
    let body = common::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "got {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{e}; got {body}"))
}

/// Everything the recipient's screens read for one trip.
fn trip_uris(token: &str, id: i64) -> [String; 4] {
    [
        format!("/s/{token}/api/trips/{id}"),
        format!("/s/{token}/api/trips/{id}/track.geojson"),
        format!("/s/{token}/api/trips/{id}/photos"),
        format!("/s/{token}/api/trips/{id}/gpx"),
    ]
}

/// What a route that does not exist answers — the answer every ended or
/// unknown share must give too.
async fn assert_like_no_route(response: Response, what: &str) {
    assert_eq!(response.status(), StatusCode::NOT_FOUND, "{what}");
    let body = common::body_string(response).await;
    assert!(
        !body.contains("Oslo"),
        "{what} must carry no trip data; got {body}"
    );
    let error: trip_archive::models::ErrorResponse =
        serde_json::from_str(&body).unwrap_or_else(|e| panic!("{what}: {e}; got {body}"));
    assert_eq!(error.error, "Not found", "{what}");
}

// ── Creating a share ─────────────────────────────────────────────────────────

#[tokio::test]
async fn us53_only_the_owner_creates_a_share() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;

    let response = common::send_unauthenticated(
        &app,
        create_request(serde_json::json!({ "trip_ids": [id] })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Nor does a share's own link reach the create route under its prefix.
    let token = share(&app, &[id]).await;
    let response = common::send_unauthenticated(
        &app,
        common::json_request(
            Method::POST,
            &format!("/s/{token}/api/shares"),
            &serde_json::json!({ "trip_ids": [id] }).to_string(),
        ),
    )
    .await;
    assert!(
        response.status().is_client_error(),
        "got {}",
        response.status()
    );
}

#[tokio::test]
async fn us53_a_share_request_is_validated() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;

    for (body, status) in [
        (
            serde_json::json!({ "trip_ids": [] }),
            StatusCode::BAD_REQUEST,
        ),
        (
            serde_json::json!({ "trip_ids": [id + 100] }),
            StatusCode::NOT_FOUND,
        ),
        (
            serde_json::json!({ "trip_ids": [id], "label": "x".repeat(101) }),
            StatusCode::BAD_REQUEST,
        ),
        (
            serde_json::json!({ "trip_ids": [id], "expiry": "forever" }),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        let response = common::send(&app, create_request(body.clone())).await;
        assert_eq!(response.status(), status, "{body}");
    }
}

#[tokio::test]
async fn us53_a_share_expires_when_the_owner_chose() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;

    let created =
        |expiry: &str| create_request(serde_json::json!({ "trip_ids": [id], "expiry": expiry }));
    let never: CreatedShare = serde_json::from_str(
        &common::body_string(common::send(&app, created("never")).await).await,
    )
    .unwrap();
    assert_eq!(never.expires_at, None);

    let month: CreatedShare = serde_json::from_str(
        &common::body_string(common::send(&app, created("one_month")).await).await,
    )
    .unwrap();
    let expires_at = time::OffsetDateTime::parse(
        month
            .expires_at
            .as_deref()
            .expect("a month's share expires"),
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap();
    let from_now = expires_at - time::OffsetDateTime::now_utc();
    assert!(
        (config::share::ONE_MONTH - time::Duration::minutes(1)..=config::share::ONE_MONTH)
            .contains(&from_now),
        "expires {from_now} from now"
    );
    assert_ne!(month.token, never.token, "every share has its own token");
    assert!(
        month.token.len() >= 43,
        "256 bits, base64url: {}",
        month.token.len()
    );
}

// ── What a share reaches ─────────────────────────────────────────────────────

#[tokio::test]
async fn us53_the_owner_creates_a_share_and_its_link_reads_the_trip() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample_with_photos(&app, &[("summit.jpg", PHOTO)]).await;
    let token = share(&app, &[id]).await;

    let overview: ShareOverview = json(visit(&app, &format!("/s/{token}/api/share")).await).await;
    assert_eq!(overview.label.as_deref(), Some("For Kari"));
    assert_eq!(overview.trips.len(), 1);
    assert_eq!(overview.trips[0].id, id);

    let trip: SharedTrip = json(visit(&app, &format!("/s/{token}/api/trips/{id}")).await).await;
    assert_eq!(trip.id, id);

    let track = visit(&app, &format!("/s/{token}/api/trips/{id}/track.geojson")).await;
    assert_eq!(track.status(), StatusCode::OK);
    assert_eq!(
        track.headers()[header::CONTENT_TYPE],
        "application/geo+json"
    );

    let gpx = visit(&app, &format!("/s/{token}/api/trips/{id}/gpx")).await;
    assert_eq!(gpx.status(), StatusCode::OK);
    assert!(gpx.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .starts_with("attachment"));
    assert_eq!(common::body_bytes(gpx).await, common::SAMPLE_GPX);

    let photos: Vec<PhotoResponse> =
        json(visit(&app, &format!("/s/{token}/api/trips/{id}/photos")).await).await;
    assert_eq!(photos.len(), 1);
    // The photos come back addressed through the share, so an `<img src>`
    // carries the link's own credential.
    for url in [&photos[0].url, &photos[0].thumbnail_url] {
        assert!(url.starts_with(&format!("/s/{token}/media/")), "{url}");
        let image = visit(&app, url).await;
        assert_eq!(image.status(), StatusCode::OK, "{url}");
        assert_eq!(image.headers()[header::CONTENT_TYPE], "image/jpeg");
    }
}

#[tokio::test]
async fn us53_a_share_reaches_none_of_the_trips_it_does_not_name() {
    let (app, _dir) = common::test_app().await;
    let shared = common::import_sample(&app).await;
    let private = common::import_sample(&app).await;
    let token = share(&app, &[shared]).await;

    for uri in trip_uris(&token, private) {
        assert_like_no_route(visit(&app, &uri).await, &uri).await;
    }
    let overview: ShareOverview = json(visit(&app, &format!("/s/{token}/api/share")).await).await;
    assert_eq!(
        overview.trips.iter().map(|t| t.id).collect::<Vec<_>>(),
        [shared]
    );
}

#[tokio::test]
async fn us53_a_share_serves_only_its_trips_photos() {
    let (app, _dir) = common::test_app().await;
    let shared = common::import_sample_with_photos(&app, &[("a.jpg", PHOTO)]).await;
    let private = common::import_sample_with_photos(&app, &[("b.jpg", PHOTO)]).await;
    let token = share(&app, &[shared]).await;

    let private_photos: Vec<PhotoResponse> =
        json(common::get(&app, &format!("/api/trips/{private}/photos")).await).await;
    let private_key = private_photos[0].url.strip_prefix("/media/").unwrap();
    let shared_photos: Vec<PhotoResponse> =
        json(common::get(&app, &format!("/api/trips/{shared}/photos")).await).await;
    let shared_key = shared_photos[0].url.strip_prefix("/media/").unwrap();
    let shared_dir = shared_key.rsplit_once('/').unwrap().0;
    let private_file = private_key.rsplit_once('/').unwrap().1;

    for uri in [
        format!("/s/{token}/media/{private_key}"),
        // Climbing out of a shared trip's directory into another's.
        format!("/s/{token}/media/{shared_dir}/../../{private}/{private_file}"),
        format!("/s/{token}/media/{shared_dir}/..%2F..%2F{private}%2F{private_file}"),
        format!("/s/{token}/media/{shared_dir}"),
    ] {
        assert_like_no_route(visit(&app, &uri).await, &uri).await;
    }
}

#[tokio::test]
async fn us53_the_recipient_sees_no_tags_and_no_komoot_link() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    common::send(
        &app,
        common::json_request(
            Method::POST,
            &format!("/api/trips/{id}/tags"),
            r#"{"name":"secret-tag"}"#,
        ),
    )
    .await;
    let token = share(&app, &[id]).await;

    for uri in [
        format!("/s/{token}/api/share"),
        format!("/s/{token}/api/trips/{id}"),
    ] {
        let body = common::body_string(visit(&app, &uri).await).await;
        assert!(!body.contains("secret-tag"), "{uri}: {body}");
        assert!(!body.contains("komoot"), "{uri}: {body}");
        assert!(!body.contains("trip_kind"), "{uri}: {body}");
    }
    assert_like_no_route(
        visit(&app, &format!("/s/{token}/api/trips/{id}/tags")).await,
        "a trip's tags",
    )
    .await;
}

/// Every owner route, as it would be reached through a share's prefix.
/// None of them is a share route, so none may answer — whichever way the
/// share's own routes grow.
const OWNER_ROUTES: &[(Method, &str)] = &[
    (Method::GET, "/api/session"),
    (Method::DELETE, "/api/session"),
    (Method::POST, "/api/import"),
    (Method::POST, "/api/import/staged"),
    (Method::GET, "/api/trips"),
    (Method::PATCH, "/api/trips/{id}"),
    (Method::DELETE, "/api/trips/{id}"),
    (Method::POST, "/api/trips/{id}/photos"),
    (Method::PATCH, "/api/trips/{id}/photos/1"),
    (Method::GET, "/api/trips/{id}/tags"),
    (Method::POST, "/api/trips/{id}/tags"),
    (Method::POST, "/api/trips/tags"),
    (Method::POST, "/api/trips/activity_type"),
    (Method::GET, "/api/tags"),
    (Method::POST, "/api/shares"),
    (Method::GET, "/api/shares"),
    (Method::DELETE, "/api/shares/{id}"),
    (Method::GET, "/api/komoot/sync"),
    (Method::POST, "/api/komoot/sync"),
    (Method::GET, "/api/backup/database"),
    (Method::GET, "/api/export/trips"),
    (Method::GET, "/api/version"),
];

#[tokio::test]
async fn us53_a_share_reaches_no_owner_route() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let token = share(&app, &[id]).await;

    for (method, path) in OWNER_ROUTES {
        let uri = format!("/s/{token}{}", path.replace("{id}", &id.to_string()));
        let response = common::send_unauthenticated(
            &app,
            Request::builder()
                .method(method.clone())
                .uri(&uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
        assert!(
            matches!(
                response.status(),
                StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
            ),
            "{method} {uri} answered {}",
            response.status()
        );
    }

    // And the token is no credential outside its prefix, in either form a
    // session travels in.
    for request in [
        Request::builder()
            .uri(format!("/api/trips/{id}"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
        Request::builder()
            .uri(format!("/api/trips/{id}"))
            .header(
                header::COOKIE,
                format!("{}={token}", config::auth::COOKIE_NAME),
            )
            .body(Body::empty())
            .unwrap(),
    ] {
        let response = common::send_unauthenticated(&app, request).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn us53_a_share_changes_nothing() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let token = share(&app, &[id]).await;

    for (method, uri) in [
        (Method::PATCH, format!("/s/{token}/api/trips/{id}")),
        (Method::DELETE, format!("/s/{token}/api/trips/{id}")),
        (Method::POST, format!("/s/{token}/api/trips/{id}/photos")),
    ] {
        let response = common::send_unauthenticated(
            &app,
            common::json_request(method.clone(), &uri, r#"{"name":"Renamed"}"#),
        )
        .await;
        assert!(
            response.status().is_client_error(),
            "{method} {uri} answered {}",
            response.status()
        );
    }
    let trip: TripDetail = json(common::get(&app, &format!("/api/trips/{id}")).await).await;
    assert_ne!(trip.name, "Renamed");
}

#[tokio::test]
async fn us53_the_owners_session_adds_nothing_on_a_share_path() {
    // The share's routes answer to the link alone: a signed-in owner opening
    // a share sees what the recipient sees, and a wrong token stays wrong.
    let (app, _dir) = common::test_app().await;
    let shared = common::import_sample(&app).await;
    let private = common::import_sample(&app).await;
    let token = share(&app, &[shared]).await;

    for uri in [
        format!("/s/{token}/api/trips/{private}"),
        format!("/s/not-a-token/api/trips/{shared}"),
    ] {
        assert_like_no_route(common::get(&app, &uri).await, &uri).await;
    }
}

// ── When a share ends ────────────────────────────────────────────────────────

#[tokio::test]
async fn us53_an_ended_share_answers_like_no_share() {
    let (app, state, _dir) = common::test_app_with_state(None).await;
    let first = common::import_sample(&app).await;
    let second = common::import_sample(&app).await;
    let token = share(&app, &[first, second]).await;
    let expired = share(&app, &[first]).await;
    sqlx::query("UPDATE share SET expires_at = '2000-01-01T00:00:00Z' WHERE token = ?")
        .bind(&expired)
        .execute(&state.pool)
        .await
        .unwrap();

    // Unknown, and expired.
    for uri in [
        "/s/not-a-token/api/share".to_string(),
        format!("/s/{expired}/api/share"),
        format!("/s/{expired}/api/trips/{first}"),
    ] {
        assert_like_no_route(visit(&app, &uri).await, &uri).await;
    }

    // A deleted trip leaves the share; the last one ends it.
    assert_eq!(
        common::delete(&app, &format!("/api/trips/{first}"))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    let overview: ShareOverview = json(visit(&app, &format!("/s/{token}/api/share")).await).await;
    assert_eq!(
        overview.trips.iter().map(|t| t.id).collect::<Vec<_>>(),
        [second]
    );
    common::delete(&app, &format!("/api/trips/{second}")).await;
    assert_like_no_route(
        visit(&app, &format!("/s/{token}/api/share")).await,
        "an emptied share",
    )
    .await;

    // A share path nobody wrote a route for answers the same.
    let live = share(&app, &[common::import_sample(&app).await]).await;
    assert_like_no_route(
        visit(&app, &format!("/s/{live}/api/something/new")).await,
        "an unknown share route",
    )
    .await;
}
