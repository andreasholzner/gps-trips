//! US-7 acceptance tests — "a trip detail page lets me relive a trip".
//!
//! Acceptance criteria (docs/requirements.md):
//!   "Shows the track on an OSM map, an elevation profile, and a photo gallery
//!    with map markers."
//!
//! The track map and elevation profile are driven from a single track-GeoJSON fetch
//! (ADR-0005/0006). The photo gallery (US-2) fetches `/api/trips/:id/photos` and
//! renders images served at `/media/*path`.
//!
//! What this file covers is the data the screen is built from. The screen
//! itself is the SPA's (US-42) and is tested in `crates/ui-dioxus`; the
//! server-rendered page that used to carry these assertions is gone, its
//! coverage having moved first (ADR-0012's migration rule).
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

mod common;

use axum::http::StatusCode;
use common::{body_string, get, import_sample, import_sample_with_photos, test_app};

// ── The track GeoJSON endpoint: data for the map + elevation chart ───────────

#[tokio::test]
async fn us7_track_geojson_endpoint_returns_feature_geometry_and_elevation() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = get(&app, &format!("/api/trips/{id}/track.geojson")).await;
    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.contains("application/geo+json"),
        "track endpoint should serve GeoJSON; got content-type: {content_type}"
    );

    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    // Track on a map: a GeoJSON LineString with coordinates.
    assert_eq!(json["type"], "Feature");
    assert_eq!(json["geometry"]["type"], "LineString");
    let coords = json["geometry"]["coordinates"].as_array().unwrap();
    assert!(!coords.is_empty(), "the track must have coordinates");

    // Elevation profile: parallel distance/elevation arrays, one entry per point.
    let elevation = json["properties"]["elevation_m"].as_array().unwrap();
    let distance = json["properties"]["cumulative_distance_m"]
        .as_array()
        .unwrap();
    assert_eq!(
        elevation.len(),
        coords.len(),
        "one elevation per coordinate"
    );
    assert_eq!(distance.len(), coords.len(), "one distance per coordinate");
}

#[tokio::test]
async fn us7_track_geojson_endpoint_404_for_unknown_trip() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/api/trips/999/track.geojson").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ── The detail metadata endpoint: what the screen shows around the map ──────

#[tokio::test]
async fn us7_detail_api_returns_the_trips_metadata() {
    // ADR-0008's v1 surface names `GET /api/trips/:id`; until US-42 only the
    // server-rendered page existed, so the SPA had no way to read a single
    // trip. The stats here are US-8's, computed at import — the endpoint
    // reports them, it does not recompute them.
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = get(&app, &format!("/api/trips/{id}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    assert_eq!(json["id"], id);
    assert_eq!(json["name"], "Oslo Hills Walk");
    assert_eq!(json["activity_type"], "unknown");
    assert!(
        json["distance_m"].as_f64().is_some_and(|m| m > 0.0),
        "the track's computed distance must travel with the trip; got: {json}"
    );
    assert!(
        json["start_time"].is_string(),
        "the start time must travel with the trip; got: {json}"
    );
    // A trip that never came from Komoot has no link, and so no privacy the
    // detail screen could offer to change (US-35).
    assert!(json["komoot"].is_null(), "got: {json}");
}

#[tokio::test]
async fn us7_detail_api_404_for_unknown_trip() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/api/trips/999").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ── Photo gallery: images served + gallery container ─────────────────────────

#[tokio::test]
async fn us7_photos_json_includes_a_serving_url() {
    let (app, _dir) = test_app().await;
    let id =
        import_sample_with_photos(&app, &[("photo.jpg", b"\xFF\xD8\xFF-fake-jpeg".as_slice())])
            .await;

    let response = get(&app, &format!("/api/trips/{id}/photos")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    let url = json[0]["url"]
        .as_str()
        .expect("photo must have a url field");
    assert!(!url.is_empty(), "url must not be empty");
    assert!(
        url.starts_with('/'),
        "url must be an absolute path; got: {url}"
    );
}

#[tokio::test]
async fn us7_photo_blob_is_served_at_its_url() {
    const FAKE_JPEG: &[u8] = b"\xFF\xD8\xFF-fake-jpeg-blob";
    let (app, _dir) = test_app().await;
    let id = import_sample_with_photos(&app, &[("photo.jpg", FAKE_JPEG)]).await;

    let response = get(&app, &format!("/api/trips/{id}/photos")).await;
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    let url = json[0]["url"].as_str().unwrap().to_string();

    let photo_response = get(&app, &url).await;
    assert_eq!(photo_response.status(), StatusCode::OK);
    assert_eq!(common::body_bytes(photo_response).await, FAKE_JPEG);
}

#[tokio::test]
async fn us7_media_endpoint_returns_404_for_missing_blob() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/media/trips/999/0000-nope.jpg").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
