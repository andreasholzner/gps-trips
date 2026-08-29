//! US-14 — filter the trip list by geographic region by selecting an area on
//! a map.
//!
//! Acceptance criterion: the list shows only trips matching the selected
//! region. The region is a rectangle, submitted as
//! `bbox=minLon,minLat,maxLon,maxLat` (ADR-0008) and matched by bbox overlap
//! against the trip's stored bounding box (ADR-0011).
//!
//! **What is asserted where.** The screen's half of this story now lives in
//! the SPA (US-52): that a chosen region narrows the list, that another
//! region shows the other trip, that a region holding nothing shows the
//! *filtered* empty state, that it combines with the other filters as AND,
//! that the map is there to drag on, and that the region survives a tab
//! switch and a reload — see `crates/ui-dioxus/src/list.rs`,
//! `crates/ui-dioxus/src/region.rs` and `tests/browser/trip_list.spec.mjs`.
//! What stays here is the server's half: the same filter parsing and
//! bbox-overlap query, asserted through `GET /api/trips`, which is the
//! contract the SPA actually calls (ADR-0012's migration rule).

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
async fn us14_the_region_returns_only_trips_whose_bbox_overlaps_it() {
    let (app, _dir) = test_app().await;
    let oslo = import_sample(&app).await;
    let alps = import_alps(&app).await;

    let json = body_string(get(&app, &format!("/api/trips?{OSLO}")).await).await;
    assert!(json.contains(&format!("\"id\":{oslo}")), "got: {json}");
    assert!(!json.contains(&format!("\"id\":{alps}")), "got: {json}");
}

#[tokio::test]
async fn us14_a_different_region_returns_the_other_trip() {
    let (app, _dir) = test_app().await;
    let oslo = import_sample(&app).await;
    let alps = import_alps(&app).await;

    let json = body_string(get(&app, &format!("/api/trips?{ALPS}")).await).await;
    assert!(json.contains(&format!("\"id\":{alps}")), "got: {json}");
    assert!(!json.contains(&format!("\"id\":{oslo}")), "got: {json}");
}

#[tokio::test]
async fn us14_a_region_containing_no_trips_returns_nothing() {
    let (app, _dir) = test_app().await;
    import_sample(&app).await;

    let json = body_string(get(&app, &format!("/api/trips?{ATLANTIC}")).await).await;
    assert_eq!(json, "[]", "got: {json}");
}

#[tokio::test]
async fn us14_the_region_combines_with_the_other_filters_as_and() {
    let (app, _dir) = test_app().await;
    let oslo = import_sample(&app).await;
    import_alps(&app).await;

    // In the region, but excluded by a name search that only the Alps trip
    // matches — a region filter must narrow, never widen, the other filters.
    let json = body_string(get(&app, &format!("/api/trips?{OSLO}&q=inn+valley")).await).await;
    assert!(!json.contains(&format!("\"id\":{oslo}")), "got: {json}");
    assert_eq!(json, "[]", "got: {json}");
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
        let response = get(&app, &format!("/api/trips?bbox={bbox}")).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "expected 400 for bbox={bbox:?}"
        );
    }
}

#[tokio::test]
async fn us14_a_blank_bbox_is_not_a_filter_at_all() {
    // What an untouched region control sends along with the rest of the
    // filters — it must behave like "no region selected", not 400.
    let (app, _dir) = test_app().await;
    let oslo = import_sample(&app).await;

    let json = body_string(get(&app, "/api/trips?bbox=").await).await;
    assert!(json.contains(&format!("\"id\":{oslo}")), "got: {json}");
}
