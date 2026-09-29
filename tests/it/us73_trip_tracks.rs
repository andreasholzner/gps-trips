//! US-73 acceptance tests — the tracks of the trips in the list map's view,
//! in one request.
//!
//! Which trips are in view, and drawing them, is the SPA's
//! (`crates/ui-dioxus`, `trip_lines`). What stays here is the server's half:
//! `GET /api/trips/tracks?ids=…` answers every requested trip's stored
//! positions as `[lon, lat]` — the geometry alone, without the elevation,
//! times and chart series `track.geojson` carries for the detail screen.
//!
//! A share's overview map (US-53) reads the same through its own route,
//! `/s/:token/api/trips/tracks`, which answers only the trips the share
//! names.
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use trip_archive::models::CreatedShare;

use crate::common::{
    body_string, error_message, get, import_sample, json_request, send, send_unauthenticated,
    test_app, test_app_with_state,
};

async fn tracks(app: &axum::Router, query: &str) -> serde_json::Value {
    let response = get(app, &format!("/api/trips/tracks{query}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_str(&body_string(response).await).unwrap()
}

/// The stored positions, as `track.geojson` serves them.
async fn stored_positions(app: &axum::Router, id: i64) -> Vec<Vec<f64>> {
    let geojson: serde_json::Value = serde_json::from_str(
        &body_string(get(app, &format!("/api/trips/{id}/track.geojson")).await).await,
    )
    .unwrap();
    serde_json::from_value(geojson["geometry"]["coordinates"].clone()).unwrap()
}

#[tokio::test]
async fn us73_each_requested_trip_comes_with_its_positions_without_elevation() {
    let (app, _dir) = test_app().await;
    let a = import_sample(&app).await;
    let b = import_sample(&app).await;

    let answer = tracks(&app, &format!("?ids={a},{b}")).await;

    let answer = answer.as_array().unwrap();
    assert_eq!(answer.len(), 2, "{answer:?}");
    for (track, id) in answer.iter().zip([a, b]) {
        assert_eq!(track["id"], id);
        let expected: Vec<Vec<f64>> = stored_positions(&app, id)
            .await
            .into_iter()
            .map(|position| position[..2].to_vec())
            .collect();
        assert!(!expected.is_empty());
        let coordinates: Vec<Vec<f64>> =
            serde_json::from_value(track["coordinates"].clone()).unwrap();
        assert_eq!(coordinates, expected);
    }
}

#[tokio::test]
async fn us73_a_trip_that_does_not_exist_is_left_out() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let answer = tracks(&app, &format!("?ids={id},{}", id + 1000)).await;

    let ids: Vec<i64> = answer
        .as_array()
        .unwrap()
        .iter()
        .map(|track| track["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [id]);
}

#[tokio::test]
async fn us73_no_ids_ask_for_nothing() {
    let (app, _dir) = test_app().await;
    import_sample(&app).await;

    assert_eq!(tracks(&app, "").await, serde_json::json!([]));
    assert_eq!(tracks(&app, "?ids=").await, serde_json::json!([]));
}

#[tokio::test]
async fn us73_malformed_ids_are_refused() {
    let (app, _dir) = test_app().await;

    for query in ["?ids=1,x", "?ids=1,,2", "?ids=1.5"] {
        let response = get(&app, &format!("/api/trips/tracks{query}")).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
        assert!(error_message(response).await.contains("ids"), "{query}");
    }
}

#[tokio::test]
async fn us73_the_tracks_are_the_owners_alone() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = send_unauthenticated(
        &app,
        Request::builder()
            .uri(format!("/api/trips/tracks?ids={id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ── Through a share ──────────────────────────────────────────────────────────

/// Share `trip_ids` as the owner and return the token.
async fn share(app: &axum::Router, trip_ids: &[i64]) -> String {
    let body = serde_json::json!({ "trip_ids": trip_ids }).to_string();
    let response = send(app, json_request(Method::POST, "/api/shares", &body)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created: CreatedShare = serde_json::from_str(&body_string(response).await).unwrap();
    created.token
}

/// A request as the recipient makes it: no session, the link alone.
async fn visit(app: &axum::Router, uri: &str) -> axum::response::Response {
    send_unauthenticated(
        app,
        Request::builder().uri(uri).body(Body::empty()).unwrap(),
    )
    .await
}

#[tokio::test]
async fn us73_a_share_answers_the_tracks_of_its_own_trips_only() {
    let (app, _dir) = test_app().await;
    let shared = import_sample(&app).await;
    let other = import_sample(&app).await;
    let token = share(&app, &[shared]).await;

    let response = visit(
        &app,
        &format!("/s/{token}/api/trips/tracks?ids={shared},{other}"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let answer: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    let ids: Vec<i64> = answer
        .as_array()
        .unwrap()
        .iter()
        .map(|track| track["id"].as_i64().unwrap())
        .collect();
    // A trip the share does not name is absent, exactly as one that does not
    // exist: the answer says nothing about it.
    assert_eq!(ids, [shared]);
    assert!(!answer[0]["coordinates"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn us73_an_ended_share_reaches_no_tracks() {
    let (app, state, _dir) = test_app_with_state(None).await;
    let id = import_sample(&app).await;
    let expired = share(&app, &[id]).await;
    sqlx::query("UPDATE share SET expires_at = '2000-01-01T00:00:00Z' WHERE token = ?")
        .bind(&expired)
        .execute(&state.pool)
        .await
        .unwrap();

    for token in [expired.as_str(), "not-a-token"] {
        let response = visit(&app, &format!("/s/{token}/api/trips/tracks?ids={id}")).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{token}");
    }
}
