//! US-69 — as the owner, I see which shares are active and can stop one, so
//! a link I handed out stops working when I want it to.
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *a list shows every active share: the trips it reaches, when it was
//!   created, and when it expires if it does* —
//!   `us69_the_owner_lists_the_active_shares`.
//! * *stopping takes effect on the next request; the link then answers
//!   exactly as an unknown link does* —
//!   `us69_a_stopped_share_answers_like_an_unknown_link`.
//! * *a stopped or expired share leaves the list; stopping cannot be
//!   undone* — `us69_a_stopped_or_expired_share_leaves_the_list`,
//!   `us69_a_stopped_share_cannot_be_stopped_again`.
//! * *only the owner's session can list or stop shares; a share's own link
//!   can do neither* — `us69_only_the_owner_lists_or_stops_shares`.
//!
//! Which shares count as active, and the order, is unit-tested in
//! `src/server/repo/share/tests.rs`; the screen in the UI crate.

use crate::common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    response::Response,
    Router,
};
use trip_archive::models::{ActiveShare, CreatedShare};

const PHOTO: &[u8] = include_bytes!("../fixtures/geotagged.jpg");

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Share `trip_ids` as the owner, with `expiry`, and return what came back.
async fn share(app: &Router, trip_ids: &[i64], expiry: &str) -> CreatedShare {
    let body = serde_json::json!({ "trip_ids": trip_ids, "label": "For Kari", "expiry": expiry });
    let response = common::send(
        app,
        common::json_request(Method::POST, "/api/shares", &body.to_string()),
    )
    .await;
    let status = response.status();
    let body = common::body_string(response).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "creating a share failed: {body}"
    );
    serde_json::from_str(&body).expect("CreatedShare JSON")
}

async fn list(app: &Router) -> Vec<ActiveShare> {
    let response = common::get(app, "/api/shares").await;
    let status = response.status();
    let body = common::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "got {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{e}; got {body}"))
}

/// The id the list gives the share `token` opens.
async fn id_of(app: &Router, token: &str) -> i64 {
    list(app)
        .await
        .into_iter()
        .find(|share| share.token == token)
        .expect("the share is listed")
        .id
}

async fn stop(app: &Router, id: i64) -> Response {
    common::delete(app, &format!("/api/shares/{id}")).await
}

/// A request as the recipient makes it: no session, the link alone.
async fn visit(app: &Router, uri: &str) -> Response {
    common::send_unauthenticated(
        app,
        Request::builder().uri(uri).body(Body::empty()).unwrap(),
    )
    .await
}

// ── Listing ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us69_the_owner_lists_the_active_shares() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let name = common::body_string(common::get(&app, &format!("/api/trips/{id}")).await).await;
    let name = serde_json::from_str::<serde_json::Value>(&name).unwrap()["name"]
        .as_str()
        .unwrap()
        .to_string();
    let never = share(&app, &[id], "never").await;
    let month = share(&app, &[id], "one_month").await;

    let shares = list(&app).await;
    assert_eq!(shares.len(), 2);
    let [newest, oldest] = [&shares[0], &shares[1]];
    assert_eq!(newest.token, month.token, "newest first");
    assert_eq!(newest.expires_at, month.expires_at);
    assert_eq!(oldest.token, never.token);
    assert_eq!(oldest.expires_at, None);
    for listed in &shares {
        assert_eq!(listed.trip_names, [name.as_str()]);
        assert_eq!(listed.label.as_deref(), Some("For Kari"));
        time::OffsetDateTime::parse(
            &listed.created_at,
            &time::format_description::well_known::Rfc3339,
        )
        .expect("created_at is RFC-3339");
    }
}

#[tokio::test]
async fn us69_a_stopped_or_expired_share_leaves_the_list() {
    let (app, state, _dir) = common::test_app_with_state(None).await;
    let id = common::import_sample(&app).await;
    let kept = share(&app, &[id], "never").await;
    let stopped = share(&app, &[id], "never").await;
    let expired = share(&app, &[id], "one_month").await;
    sqlx::query("UPDATE share SET expires_at = '2000-01-01T00:00:00Z' WHERE token = ?")
        .bind(&expired.token)
        .execute(&state.pool)
        .await
        .unwrap();

    let stopped_id = id_of(&app, &stopped.token).await;
    assert_eq!(
        stop(&app, stopped_id).await.status(),
        StatusCode::NO_CONTENT
    );

    let tokens: Vec<String> = list(&app).await.into_iter().map(|s| s.token).collect();
    assert_eq!(tokens, [kept.token]);
}

// ── Stopping ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us69_a_stopped_share_answers_like_an_unknown_link() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample_with_photos(&app, &[("summit.jpg", PHOTO)]).await;
    let token = share(&app, &[id], "never").await.token;

    // Everything the link reached, read while it still works.
    let photos: Vec<serde_json::Value> = serde_json::from_str(
        &common::body_string(visit(&app, &format!("/s/{token}/api/trips/{id}/photos")).await).await,
    )
    .unwrap();
    let mut uris = vec![
        format!("/s/{token}/api/share"),
        format!("/s/{token}/api/trips/{id}"),
        format!("/s/{token}/api/trips/{id}/track.geojson"),
        format!("/s/{token}/api/trips/{id}/photos"),
        format!("/s/{token}/api/trips/{id}/gpx"),
    ];
    for key in ["url", "thumbnail_url"] {
        uris.push(photos[0][key].as_str().unwrap().to_string());
    }
    for uri in &uris {
        assert_eq!(visit(&app, uri).await.status(), StatusCode::OK, "{uri}");
    }

    let unknown = visit(&app, "/s/not-a-token/api/share").await;
    let unknown_status = unknown.status();
    let unknown_body = common::body_bytes(unknown).await;

    let share_id = id_of(&app, &token).await;
    assert_eq!(stop(&app, share_id).await.status(), StatusCode::NO_CONTENT);

    for uri in &uris {
        let response = visit(&app, uri).await;
        assert_eq!(response.status(), unknown_status, "{uri}");
        assert_eq!(common::body_bytes(response).await, unknown_body, "{uri}");
    }
}

#[tokio::test]
async fn us69_a_stopped_share_cannot_be_stopped_again() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let token = share(&app, &[id], "never").await.token;
    let share_id = id_of(&app, &token).await;

    assert_eq!(stop(&app, share_id).await.status(), StatusCode::NO_CONTENT);
    assert_eq!(stop(&app, share_id).await.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        stop(&app, share_id + 100).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn us69_only_the_owner_lists_or_stops_shares() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let token = share(&app, &[id], "never").await.token;
    let share_id = id_of(&app, &token).await;

    let request = |method: Method, uri: String| {
        Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    };
    // Without a session.
    for (method, uri) in [
        (Method::GET, "/api/shares".to_string()),
        (Method::DELETE, format!("/api/shares/{share_id}")),
    ] {
        let response = common::send_unauthenticated(&app, request(method.clone(), uri)).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{method}");
    }
    // Through the share's own link.
    for (method, uri) in [
        (Method::GET, format!("/s/{token}/api/shares")),
        (Method::DELETE, format!("/s/{token}/api/shares/{share_id}")),
    ] {
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

    assert_eq!(list(&app).await.len(), 1, "the share is still there");
}
