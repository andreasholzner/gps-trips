//! US-95 acceptance tests — "waypoint symbols".
//!
//! A guide book's waypoints keep the symbol their file gives them, for the
//! maps to draw (ADR-0025: the UI decides which icon). Driven through the
//! real router against a real temporary database (ADR-0012).

use axum::http::StatusCode;
use trip_archive::models::{CollectionDetail, Waypoint};

use crate::common::{body_string, get, import_collection, GUIDE_BOOK_GPX, GUIDE_SINGLE_GPX};

async fn read_json<T: serde::de::DeserializeOwned>(app: &axum::Router, uri: &str) -> T {
    let response = get(app, uri).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{uri}: {e}: {body}"))
}

#[tokio::test]
async fn us95_a_collections_waypoints_keep_their_symbols_as_the_file_names_them() {
    let (app, _dir) = crate::common::test_app().await;
    let id = import_collection(&app, GUIDE_BOOK_GPX, &[]).await;

    let waypoints: Vec<Waypoint> =
        read_json(&app, &format!("/api/collections/{id}/waypoints")).await;

    assert_eq!(waypoints[0].symbol.as_deref(), Some("Triangle, Blue"));
    // A waypoint whose file names no symbol has none.
    assert_eq!(waypoints[1].symbol, None);
}

#[tokio::test]
async fn us95_a_collection_tracks_waypoints_keep_their_symbols() {
    let (app, _dir) = crate::common::test_app().await;
    let id = import_collection(&app, GUIDE_SINGLE_GPX, &[]).await;
    let collection: CollectionDetail = read_json(&app, &format!("/api/collections/{id}")).await;
    let only = collection.tracks[0].id;

    let on_track: Vec<Waypoint> =
        read_json(&app, &format!("/api/collection-tracks/{only}/waypoints")).await;

    assert_eq!(on_track[0].symbol.as_deref(), Some("Parking Area"));
    assert_eq!(on_track[1].symbol, None);
}
