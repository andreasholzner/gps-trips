//! US-14 — filter the trip list by geographic region by selecting an area on
//! a map.
//!
//! Acceptance criterion: the list shows only trips matching the selected
//! region. The region is a rectangle, submitted as
//! `bbox=minLon,minLat,maxLon,maxLat` (ADR-0008) and matched by bbox overlap
//! against the trip's stored bounding box (ADR-0011). Covers both the HTML
//! list (`GET /`) and the JSON list (`GET /api/trips`), since both share the
//! same filter parsing (US-13).

mod common;

use axum::http::StatusCode;
use common::{
    body_string, get, import, import_sample, test_app, trip_id_from_redirect, REGION_ALPS_GPX,
};

/// A rectangle around `SAMPLE_GPX`'s Oslo track (59.91–59.93 N, 10.75–10.77 E).
const OSLO: &str = "bbox=10.5,59.8,11.0,60.0";
/// A rectangle around `REGION_ALPS_GPX`'s Inn valley track (47.26–47.30 N,
/// 11.38–11.42 E).
const ALPS: &str = "bbox=11.2,47.1,11.6,47.4";
/// A rectangle over the Atlantic — no fixture track is anywhere near it.
const ATLANTIC: &str = "bbox=-30.0,30.0,-20.0,40.0";

async fn import_alps(app: &axum::Router) -> i64 {
    trip_id_from_redirect(&import(app, REGION_ALPS_GPX).await)
}

#[tokio::test]
async fn us14_region_filter_narrows_the_html_list_to_trips_in_the_selected_area() {
    let (app, _dir) = test_app().await;
    let oslo = import_sample(&app).await;
    let alps = import_alps(&app).await;

    let html = body_string(get(&app, &format!("/?{OSLO}")).await).await;
    assert!(html.contains(&format!("/trips/{oslo}")), "got: {html}");
    assert!(!html.contains(&format!("/trips/{alps}")), "got: {html}");
}

#[tokio::test]
async fn us14_selecting_a_different_area_shows_the_other_trip() {
    let (app, _dir) = test_app().await;
    let oslo = import_sample(&app).await;
    let alps = import_alps(&app).await;

    let html = body_string(get(&app, &format!("/?{ALPS}")).await).await;
    assert!(html.contains(&format!("/trips/{alps}")), "got: {html}");
    assert!(!html.contains(&format!("/trips/{oslo}")), "got: {html}");
}

#[tokio::test]
async fn us14_api_trips_returns_only_trips_in_the_selected_area() {
    let (app, _dir) = test_app().await;
    let oslo = import_sample(&app).await;
    let alps = import_alps(&app).await;

    let json = body_string(get(&app, &format!("/api/trips?{OSLO}")).await).await;
    assert!(json.contains(&format!("\"id\":{oslo}")), "got: {json}");
    assert!(!json.contains(&format!("\"id\":{alps}")), "got: {json}");
}

#[tokio::test]
async fn us14_a_region_containing_no_trips_shows_the_filtered_empty_state() {
    let (app, _dir) = test_app().await;
    import_sample(&app).await;

    let html = body_string(get(&app, &format!("/?{ATLANTIC}")).await).await;
    assert!(html.contains("No trips match your filters"), "got: {html}");
}

#[tokio::test]
async fn us14_the_region_combines_with_the_other_filters_as_and() {
    let (app, _dir) = test_app().await;
    let oslo = import_sample(&app).await;
    import_alps(&app).await;

    // In the region, but excluded by a name search that only the Alps trip
    // matches — a region filter must narrow, never widen, the other filters.
    let html = body_string(get(&app, &format!("/?{OSLO}&q=inn+valley")).await).await;
    assert!(!html.contains(&format!("/trips/{oslo}")), "got: {html}");
    assert!(html.contains("No trips match your filters"), "got: {html}");
}

#[tokio::test]
async fn us14_the_list_page_offers_a_map_to_select_the_region_on() {
    let (app, _dir) = test_app().await;
    import_sample(&app).await;

    let html = body_string(get(&app, "/").await).await;
    assert!(html.contains("id=\"region-map\""), "got: {html}");
    assert!(html.contains("name=\"bbox\""), "got: {html}");
}

#[tokio::test]
async fn us14_an_active_region_survives_a_recorded_planned_tab_switch() {
    let (app, _dir) = test_app().await;
    import_sample(&app).await;

    let html = body_string(get(&app, &format!("/?{OSLO}")).await).await;
    assert!(
        html.contains("<input type=\"hidden\" name=\"bbox\" value=\"10.5,59.8,11.0,60.0\">"),
        "got: {html}"
    );
}

#[tokio::test]
async fn us14_a_malformed_bbox_is_rejected_with_400() {
    let (app, _dir) = test_app().await;

    for bbox in [
        "10.5,59.8,11.0",        // too few values
        "10.5,59.8,11.0,60.0,1", // too many
        "10.5,59.8,east,60.0",   // not a number
        "10.5,-91.0,11.0,60.0",  // latitude out of range
        "10.5,59.8,181.0,60.0",  // longitude out of range
        "11.0,59.8,10.5,60.0",   // backwards longitudes (no antimeridian wrap in v1)
        "10.5,60.0,11.0,59.8",   // backwards latitudes
    ] {
        let response = get(&app, &format!("/?bbox={bbox}")).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "expected 400 for bbox={bbox:?}"
        );
    }
}

#[tokio::test]
async fn us14_a_blank_bbox_is_not_a_filter_at_all() {
    // Exactly what an untouched region control submits with the rest of the
    // filter form — it must behave like "no region selected", not 400.
    let (app, _dir) = test_app().await;
    let oslo = import_sample(&app).await;

    let html = body_string(get(&app, "/?bbox=").await).await;
    assert!(html.contains(&format!("/trips/{oslo}")), "got: {html}");
}
