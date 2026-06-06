//! US-7 acceptance tests — "a trip detail page lets me relive a trip".
//!
//! Acceptance criteria (docs/requirements.md):
//!   "Shows the track on an OSM map, an elevation profile, and a photo gallery
//!    with map markers."
//!
//! The photo gallery depends on the photo stories (US-2…US-5), which are not yet
//! implemented; this milestone covers the **track map** and the **elevation
//! profile**. Both are driven from a single track-GeoJSON fetch (ADR-0005/0006),
//! so the tests assert (a) that endpoint and (b) the page wiring that consumes it.
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

mod common;

use axum::http::StatusCode;
use common::{body_string, get, import_sample, test_app};

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

// ── The detail page: wires up the map and the elevation chart ────────────────

#[tokio::test]
async fn us7_detail_page_renders_map_and_elevation_consuming_the_track() {
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = get(&app, &format!("/trips/{id}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_string(response).await;

    // Containers the client script renders the map and elevation chart into.
    assert!(html.contains(r#"id="map""#), "map container; got: {html}");
    assert!(
        html.contains(r#"id="elevation""#),
        "elevation chart container; got: {html}"
    );
    // Both are fed from the single track-GeoJSON fetch (ADR-0005/0006).
    assert!(
        html.contains(&format!("/api/trips/{id}/track.geojson")),
        "page must point the client at its track data; got: {html}"
    );
    // The vendored, self-hosted map + chart libraries (ADR-0005/0006, US-10).
    assert!(
        html.contains("leaflet"),
        "Leaflet must be loaded; got: {html}"
    );
    assert!(
        html.to_lowercase().contains("uplot"),
        "uPlot must be loaded; got: {html}"
    );
}

#[tokio::test]
async fn us7_detail_page_404_for_unknown_trip() {
    let (app, _dir) = test_app().await;
    let response = get(&app, "/trips/999").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// The map and chart are useless if their assets 404; assert they are served
// (self-hosted vendored files, ADR-0005/0006).
#[tokio::test]
async fn us7_vendored_map_and_chart_assets_are_served() {
    let (app, _dir) = test_app().await;

    for asset in [
        "/static/vendor/leaflet.js",
        "/static/vendor/leaflet.css",
        "/static/vendor/uPlot.iife.min.js",
        "/static/vendor/uPlot.min.css",
        "/static/js/trip_detail.js",
    ] {
        let response = get(&app, asset).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "vendored asset must be served: {asset}"
        );
    }
}
