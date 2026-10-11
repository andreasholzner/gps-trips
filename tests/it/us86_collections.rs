//! US-86 acceptance tests — "import a guide-book collection".
//!
//! One GPX file creates a collection holding every track and waypoint in it,
//! kept apart from the owner's trips (ADR-0028). Driven through the real
//! router against a real temporary database (ADR-0012).

use axum::http::{Method, StatusCode};
use trip_archive::models::{
    ActivityType, CollectionDetail, CollectionPreview, CollectionTrackDetail, TripTrack, Waypoint,
};

use crate::common::{
    body_bytes, body_string, collection_request, delete, error_message, get, import_collection,
    json_request, send, test_app, GUIDE_BOOK_GPX, GUIDE_SINGLE_GPX, NO_TRACKS_GPX,
    WAYPOINTS_ONLY_GPX,
};

async fn read_json<T: serde::de::DeserializeOwned>(app: &axum::Router, uri: &str) -> T {
    let response = get(app, uri).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{uri}: {e}: {body}"))
}

async fn collection(app: &axum::Router, id: i64) -> CollectionDetail {
    read_json(app, &format!("/api/collections/{id}")).await
}

async fn track(app: &axum::Router, id: i64) -> CollectionTrackDetail {
    read_json(app, &format!("/api/collection-tracks/{id}")).await
}

async fn patch(app: &axum::Router, uri: &str, body: &str) -> StatusCode {
    send(app, json_request(Method::PATCH, uri, body))
        .await
        .status()
}

// ── Before importing: what the file says about itself ────────────────────────

#[tokio::test]
async fn us86_the_preview_suggests_the_details_and_counts_what_the_file_holds() {
    let (app, _dir) = test_app().await;

    let request = collection_request("/api/collections/preview", GUIDE_BOOK_GPX, "x.gpx", &[]);
    let response = send(&app, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let preview: CollectionPreview = serde_json::from_slice(&body_bytes(response).await).unwrap();

    assert_eq!(
        preview,
        CollectionPreview {
            suggested_name: "Lakes & Passes".to_string(),
            suggested_description: "Ten tours around the lakes.\n\nBruckmann, Baukasten"
                .to_string(),
            track_count: 3,
            waypoint_count: 2,
        }
    );
}

#[tokio::test]
async fn us86_without_metadata_the_file_name_is_suggested() {
    let (app, _dir) = test_app().await;

    let request = collection_request(
        "/api/collections/preview",
        GUIDE_SINGLE_GPX,
        "Nordkette-Touren.gpx",
        &[],
    );
    let preview: CollectionPreview =
        serde_json::from_slice(&body_bytes(send(&app, request).await).await).unwrap();

    assert_eq!(preview.suggested_name, "Nordkette-Touren");
}

#[tokio::test]
async fn us86_the_preview_creates_nothing() {
    let (app, _dir) = test_app().await;

    let request = collection_request("/api/collections/preview", GUIDE_BOOK_GPX, "x.gpx", &[]);
    send(&app, request).await;

    let response = get(&app, "/api/collections/1").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ── Importing ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us86_one_file_creates_a_collection_of_all_its_tracks_and_waypoints() {
    let (app, _dir) = test_app().await;

    let id = import_collection(
        &app,
        GUIDE_BOOK_GPX,
        &[
            ("name", "Alpencross"),
            ("description", "Bruckmann's book"),
            ("activity_type", "cycling"),
        ],
    )
    .await;

    let collection = collection(&app, id).await;
    assert_eq!(collection.name, "Alpencross");
    assert_eq!(collection.description, "Bruckmann's book");
    assert_eq!(collection.activity_type, Some(ActivityType::Cycling));
    assert_eq!(collection.waypoint_count, 2);
    let names: Vec<&str> = collection.tracks.iter().map(|t| t.name.as_str()).collect();
    // The unnamed track takes the collection's name and its place in the file.
    assert_eq!(
        names,
        ["Stage 1: Splügen", "Alpencross 2", "Stage 3: Albula"]
    );
    assert!(collection
        .tracks
        .iter()
        .all(|t| t.activity_type == ActivityType::Cycling));
}

#[tokio::test]
async fn us86_details_left_out_take_what_the_file_suggests() {
    let (app, _dir) = test_app().await;

    let id = import_collection(&app, GUIDE_BOOK_GPX, &[("name", "  ")]).await;

    let collection = collection(&app, id).await;
    assert_eq!(collection.name, "Lakes & Passes");
    assert_eq!(
        collection.description,
        "Ten tours around the lakes.\n\nBruckmann, Baukasten"
    );
    // A book with no activity focus: its tracks are unknown.
    assert_eq!(collection.activity_type, None);
    assert_eq!(collection.tracks[0].activity_type, ActivityType::Unknown);
}

#[tokio::test]
async fn us86_a_whole_book_imports_in_one_go() {
    // The size of `data/collections/Alpencross-light-Baukasten.gpx`: 125
    // tracks and 46 waypoints.
    let (app, _dir) = test_app().await;
    let book = a_book(125, 46);

    let id = import_collection(&app, book.as_bytes(), &[("name", "Alpencross")]).await;

    let collection = collection(&app, id).await;
    assert_eq!(collection.tracks.len(), 125);
    assert_eq!(collection.waypoint_count, 46);
}

/// A generated guide book of `tracks` short tracks and `waypoints` waypoints.
fn a_book(tracks: usize, waypoints: usize) -> String {
    let mut gpx = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1">"#,
    );
    for i in 0..waypoints {
        let lat = 46.0 + i as f64 * 0.01;
        gpx.push_str(&format!(
            r#"<wpt lat="{lat}" lon="9.0"><name>Pass {i}</name></wpt>"#
        ));
    }
    for i in 0..tracks {
        let lat = 46.0 + i as f64 * 0.01;
        gpx.push_str(&format!(
            r#"<trk><name>{i}</name><trkseg><trkpt lat="{lat}" lon="9.0"><ele>500</ele></trkpt><trkpt lat="{lat}" lon="9.01"><ele>550</ele></trkpt></trkseg></trk>"#
        ));
    }
    gpx.push_str("</gpx>");
    gpx
}

#[tokio::test]
async fn us86_a_file_of_waypoints_alone_imports_as_a_collection() {
    let (app, _dir) = test_app().await;

    let id = import_collection(&app, WAYPOINTS_ONLY_GPX, &[]).await;

    let collection = collection(&app, id).await;
    assert!(collection.tracks.is_empty());
    assert_eq!(collection.waypoint_count, 2);
}

#[tokio::test]
async fn us86_a_file_with_nothing_to_collect_is_refused_and_nothing_is_created() {
    let (app, _dir) = test_app().await;

    for (gpx, uri) in [
        (NO_TRACKS_GPX, "/api/collections"),
        (b"not gpx".as_slice(), "/api/collections"),
        (NO_TRACKS_GPX, "/api/collections/preview"),
    ] {
        let response = send(&app, collection_request(uri, gpx, "x.gpx", &[])).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
        assert!(!error_message(response).await.is_empty());
    }
    let response = get(&app, "/api/collections/1").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us86_an_unknown_activity_is_refused() {
    let (app, _dir) = test_app().await;

    let request = collection_request(
        "/api/collections",
        GUIDE_BOOK_GPX,
        "x.gpx",
        &[("activity_type", "flying")],
    );
    let response = send(&app, request).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ── Tracks ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us86_a_tracks_stats_are_distance_ascent_and_descent() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, GUIDE_BOOK_GPX, &[("name", "Lakes")]).await;
    let first = collection(&app, id).await.tracks[0].id;

    let track = track(&app, first).await;

    assert_eq!(track.collection_id, id);
    assert_eq!(track.collection_name, "Lakes");
    assert_eq!(track.name, "Stage 1: Splügen");
    assert!(track.distance_m > 0.0);
    assert!((track.ascent_m - 615.0).abs() < 1.0, "{}", track.ascent_m);
    assert_eq!(track.descent_m, 0.0);
    // Nothing about when: the file's timestamps are not when anyone went.
    let raw = body_string(get(&app, &format!("/api/collection-tracks/{first}")).await).await;
    for field in ["start", "duration", "moving", "speed"] {
        assert!(!raw.contains(field), "{field}: {raw}");
    }
    let geojson = body_string(
        get(
            &app,
            &format!("/api/collection-tracks/{first}/track.geojson"),
        )
        .await,
    )
    .await;
    assert!(!geojson.contains("2013-08-05"), "{geojson}");
}

#[tokio::test]
async fn us86_a_tracks_gpx_downloads_under_its_name() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, GUIDE_BOOK_GPX, &[]).await;
    let last = collection(&app, id).await.tracks[2].id;

    let response = get(&app, &format!("/api/collection-tracks/{last}/gpx")).await;

    assert_eq!(response.status(), StatusCode::OK);
    let disposition = response.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(disposition.contains("Stage 3: Albula.gpx"), "{disposition}");
    let gpx = body_string(response).await;
    assert!(gpx.contains("Stage 3: Albula"), "{gpx}");
    assert!(!gpx.contains("Stage 1"), "only this track: {gpx}");
}

#[tokio::test]
async fn us86_a_tracks_name_and_activity_can_be_edited_and_override_the_collections() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, GUIDE_BOOK_GPX, &[("activity_type", "cycling")]).await;
    let tracks = collection(&app, id).await.tracks;
    let uri = format!("/api/collection-tracks/{}", tracks[0].id);

    let status = patch(
        &app,
        &uri,
        r#"{"name": "Day one", "activity_type": "hiking"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let edited = track(&app, tracks[0].id).await;
    assert_eq!(edited.name, "Day one");
    assert_eq!(edited.activity_type, ActivityType::Hiking);
    assert_eq!(edited.own_activity, Some(ActivityType::Hiking));

    // The collection's activity changes every track without one of its own.
    let status = patch(
        &app,
        &format!("/api/collections/{id}"),
        r#"{"activity_type": "bikepacking"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let tracks = collection(&app, id).await.tracks;
    assert_eq!(tracks[0].activity_type, ActivityType::Hiking);
    assert_eq!(tracks[1].activity_type, ActivityType::Bikepacking);

    // A blank activity gives the track back to its collection's.
    patch(&app, &uri, r#"{"activity_type": ""}"#).await;
    assert_eq!(
        track(&app, tracks[0].id).await.activity_type,
        ActivityType::Bikepacking
    );
}

#[tokio::test]
async fn us86_an_edit_is_validated() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, GUIDE_BOOK_GPX, &[]).await;
    let first = collection(&app, id).await.tracks[0].id;

    let track_uri = format!("/api/collection-tracks/{first}");
    let collection_uri = format!("/api/collections/{id}");
    for (uri, body) in [
        (track_uri.as_str(), r#"{"name": " "}"#),
        (track_uri.as_str(), r#"{"activity_type": "flying"}"#),
        (collection_uri.as_str(), r#"{"name": ""}"#),
        (collection_uri.as_str(), r#"{"activity_type": "flying"}"#),
    ] {
        assert_eq!(
            patch(&app, uri, body).await,
            StatusCode::BAD_REQUEST,
            "{body}"
        );
    }
    assert_eq!(
        patch(&app, "/api/collection-tracks/999", r#"{"name": "x"}"#).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        patch(&app, "/api/collections/999", r#"{"name": "x"}"#).await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn us86_a_single_track_can_be_deleted() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, GUIDE_BOOK_GPX, &[]).await;
    let first = collection(&app, id).await.tracks[0].id;

    let response = delete(&app, &format!("/api/collection-tracks/{first}")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    assert_eq!(collection(&app, id).await.tracks.len(), 2);
    let response = get(&app, &format!("/api/collection-tracks/{first}")).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let response = delete(&app, &format!("/api/collection-tracks/{first}")).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ── Waypoints ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us86_waypoints_keep_their_name_position_elevation_and_description() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, GUIDE_BOOK_GPX, &[]).await;

    let waypoints: Vec<Waypoint> =
        read_json(&app, &format!("/api/collections/{id}/waypoints")).await;

    assert_eq!(waypoints.len(), 2);
    assert_eq!(waypoints[0].name.as_deref(), Some("San Bernardino Pass"));
    assert_eq!(
        waypoints[0].description.as_deref(),
        Some("Top of the first climb")
    );
    assert_eq!((waypoints[0].lat, waypoints[0].lon), (46.4958, 9.1705));
    assert_eq!(waypoints[0].ele, Some(2065.0));
    // From a file with several tracks, a waypoint is the collection's alone.
    assert!(waypoints.iter().all(|w| w.collection_track_id.is_none()));
}

#[tokio::test]
async fn us86_a_single_tours_waypoints_belong_to_its_track_too() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, GUIDE_SINGLE_GPX, &[]).await;
    let only = collection(&app, id).await.tracks[0].id;

    let on_track: Vec<Waypoint> =
        read_json(&app, &format!("/api/collection-tracks/{only}/waypoints")).await;

    assert_eq!(on_track.len(), 2);
    assert!(on_track.iter().all(|w| w.collection_track_id == Some(only)));
}

// ── The collection page's map ────────────────────────────────────────────────

#[tokio::test]
async fn us86_the_collections_tracks_come_as_positions_in_one_request() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, GUIDE_BOOK_GPX, &[]).await;
    let ids: Vec<i64> = collection(&app, id)
        .await
        .tracks
        .iter()
        .map(|t| t.id)
        .collect();

    let tracks: Vec<TripTrack> = read_json(&app, &format!("/api/collections/{id}/tracks")).await;

    assert_eq!(tracks.iter().map(|t| t.id).collect::<Vec<_>>(), ids);
    assert_eq!(tracks[0].coordinates[0], [9.32, 46.55]);
}

// ── Editing and deleting the collection ──────────────────────────────────────

#[tokio::test]
async fn us86_the_details_can_be_changed() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, GUIDE_BOOK_GPX, &[("activity_type", "cycling")]).await;

    let status = patch(
        &app,
        &format!("/api/collections/{id}"),
        r#"{"name": "Transalp", "description": "", "activity_type": ""}"#,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let collection = collection(&app, id).await;
    assert_eq!(collection.name, "Transalp");
    assert_eq!(collection.description, "");
    assert_eq!(collection.activity_type, None);
    assert_eq!(collection.tracks[0].activity_type, ActivityType::Unknown);
}

#[tokio::test]
async fn us86_deleting_the_collection_deletes_its_tracks_and_waypoints() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, GUIDE_SINGLE_GPX, &[]).await;
    let only = collection(&app, id).await.tracks[0].id;

    let response = delete(&app, &format!("/api/collections/{id}")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    for uri in [
        format!("/api/collections/{id}"),
        format!("/api/collection-tracks/{only}"),
        format!("/api/collection-tracks/{only}/gpx"),
        format!("/api/collection-tracks/{only}/track.geojson"),
    ] {
        assert_eq!(
            get(&app, &uri).await.status(),
            StatusCode::NOT_FOUND,
            "{uri}"
        );
    }
    let response = delete(&app, &format!("/api/collections/{id}")).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us86_collections_are_the_owners_alone() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, GUIDE_BOOK_GPX, &[]).await;

    let request = axum::http::Request::builder()
        .uri(format!("/api/collections/{id}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = crate::common::send_unauthenticated(&app, request).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
