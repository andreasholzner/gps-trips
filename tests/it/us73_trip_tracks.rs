//! US-73 acceptance tests — the tracks of the trips in the list map's view,
//! in one request.
//!
//! Which trips are in view, and drawing them, is the SPA's
//! (`crates/ui-dioxus`, `trip_lines`). What stays here is the server's half:
//! `GET /api/trips/tracks?ids=…` answers every requested trip's stored
//! positions as `[lon, lat]` — the geometry alone, without the elevation,
//! times and chart series `track.geojson` carries for the detail screen.
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

use axum::body::Body;
use axum::http::{Request, StatusCode};

use crate::common::{
    body_string, error_message, get, import_sample, send_unauthenticated, test_app,
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
