//! US-86 acceptance tests, "kept apart": a collection's tracks are not the
//! owner's trips (ADR-0028). They are counted in no Statistics (US-77),
//! listed in neither the Recorded nor the Planned tab (US-32), and exported
//! by neither export until US-89 — the QMapShack export reads
//! `/api/export/trips` (US-51). The tag summaries (US-78) and every Komoot
//! sync path start from a trip, its tags and its link, which a collection
//! track has none of.

use axum::http::StatusCode;

use crate::common::{body_string, get, import_collection, import_sample, test_app, COLLECTION_GPX};

async fn json(app: &axum::Router, uri: &str) -> serde_json::Value {
    let response = get(app, uri).await;
    assert_eq!(response.status(), StatusCode::OK, "{uri}");
    serde_json::from_str(&body_string(response).await).unwrap()
}

/// An archive holding one trip of the owner's and one collection of three
/// tracks; the trip's id and the collection's first track's.
async fn a_trip_and_a_collection() -> (axum::Router, tempfile::TempDir, i64, i64) {
    let (app, dir) = test_app().await;
    let trip = import_sample(&app).await;
    let collection = import_collection(&app, COLLECTION_GPX, &[("activity_type", "cycling")]).await;
    let first = json(&app, &format!("/api/collections/{collection}")).await["tracks"][0]["id"]
        .as_i64()
        .unwrap();
    (app, dir, trip, first)
}

fn ids(list: &serde_json::Value) -> Vec<i64> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn us86_no_tab_and_no_filter_lists_a_collection_track() {
    let (app, _dir, trip, _) = a_trip_and_a_collection().await;

    for uri in [
        "/api/trips",
        "/api/trips?kind=recorded",
        "/api/trips?activity=cycling",
    ] {
        let listed = ids(&json(&app, uri).await);
        assert!(listed.iter().all(|id| *id == trip), "{uri}: {listed:?}");
    }
    assert!(ids(&json(&app, "/api/trips?kind=planned").await).is_empty());
    assert!(ids(&json(&app, "/api/trips?q=Stage").await).is_empty());
}

#[tokio::test]
async fn us86_the_statistics_count_the_owners_trips_only() {
    let (app, _dir, trip, _) = a_trip_and_a_collection().await;

    let stats = json(&app, "/api/stats/trips").await;

    assert_eq!(ids(&stats["trips"]), [trip], "{stats}");
}

#[tokio::test]
async fn us86_neither_export_writes_a_collection_track() {
    let (app, _dir, trip, _) = a_trip_and_a_collection().await;

    let exported = json(&app, "/api/export/trips").await;

    assert_eq!(ids(&exported), [trip], "{exported}");
}

#[tokio::test]
async fn us86_a_collection_track_is_no_trip_by_any_trip_route() {
    let (app, _dir, trip, first) = a_trip_and_a_collection().await;
    // The two are counted apart: the collection's second track has an id no trip
    // has, and asked for as a trip it is nothing.
    let second = first + 1;
    assert_ne!(second, trip);

    for uri in [
        format!("/api/trips/{second}"),
        format!("/api/trips/{second}/gpx"),
        format!("/api/trips/{second}/track.geojson"),
    ] {
        assert_eq!(
            get(&app, &uri).await.status(),
            StatusCode::NOT_FOUND,
            "{uri}"
        );
    }
    let tracks = json(&app, &format!("/api/trips/tracks?ids={second}")).await;
    assert!(ids(&tracks).is_empty(), "{tracks}");
}
