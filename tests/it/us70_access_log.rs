//! US-70 — as the owner, I see how my archive is used, and above all how the
//! links I shared are used.
//!
//! Acceptance criteria, and where each is asserted below:
//!
//! * *who made each request — the owner, a share (its id and label), an
//!   anonymous caller, or an unknown link* — `us70_every_response_names_its_caller`
//!   and its siblings, read off the response the gate hands back: what the
//!   access log records is what the gate decided, not a second guess.

use crate::common;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    response::Response,
    Router,
};
use trip_archive::models::CreatedShare;
use trip_archive::server::auth::Caller;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Share `trip_ids` as the owner, labelled `label`, and return the share.
async fn share(app: &Router, trip_ids: &[i64], label: Option<&str>) -> CreatedShare {
    let body = serde_json::json!({ "trip_ids": trip_ids, "label": label });
    let response = common::send(
        app,
        common::json_request(Method::POST, "/api/shares", &body.to_string()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    serde_json::from_str(&common::body_string(response).await).expect("CreatedShare JSON")
}

/// The share's id, as the owner's list reports it.
async fn share_id(app: &Router, token: &str) -> i64 {
    let listed: Vec<serde_json::Value> =
        serde_json::from_str(&common::body_string(common::get(app, "/api/shares").await).await)
            .unwrap();
    listed
        .iter()
        .find(|share| share["token"] == token)
        .expect("the share is listed")["id"]
        .as_i64()
        .unwrap()
}

/// A request as a stranger makes it: no session.
async fn visit(app: &Router, uri: &str) -> Response {
    common::send_unauthenticated(
        app,
        Request::builder().uri(uri).body(Body::empty()).unwrap(),
    )
    .await
}

fn caller(response: &Response) -> Caller {
    response
        .extensions()
        .get::<Caller>()
        .cloned()
        .expect("the gate names the caller on every response")
}

// ── Who made the request ─────────────────────────────────────────────────────

#[tokio::test]
async fn us70_every_response_names_its_caller() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let created = share(&app, &[id], Some("For Kari")).await;
    let share_id = share_id(&app, &created.token).await;

    let owner = common::get(&app, "/api/trips").await;
    assert_eq!(caller(&owner), Caller::Owner);

    let recipient = visit(&app, &format!("/s/{}/api/share", created.token)).await;
    assert_eq!(recipient.status(), StatusCode::OK);
    assert_eq!(
        caller(&recipient),
        Caller::Share {
            id: share_id,
            label: Some("For Kari".to_string()),
        }
    );

    let refused = visit(&app, "/api/trips").await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(caller(&refused), Caller::Anonymous);

    let public = visit(&app, "/").await;
    assert_eq!(caller(&public), Caller::Anonymous);
}

#[tokio::test]
async fn us70_a_link_that_opens_nothing_is_named_as_such() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let created = share(&app, &[id], None).await;
    let share_id = share_id(&app, &created.token).await;
    common::delete(&app, &format!("/api/shares/{share_id}")).await;

    for uri in [
        format!("/s/{}/api/share", "0".repeat(64)),
        format!("/s/{}/api/share", created.token),
    ] {
        let response = visit(&app, &uri).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(caller(&response), Caller::UnknownLink, "{uri}");
    }
}

#[tokio::test]
async fn us70_a_share_without_a_label_is_named_by_its_id_alone() {
    let (app, _dir) = common::test_app().await;
    let id = common::import_sample(&app).await;
    let created = share(&app, &[id], None).await;
    let share_id = share_id(&app, &created.token).await;

    let response = visit(&app, &format!("/s/{}/api/share", created.token)).await;
    assert_eq!(
        caller(&response),
        Caller::Share {
            id: share_id,
            label: None,
        }
    );
}
