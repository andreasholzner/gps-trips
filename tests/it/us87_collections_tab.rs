//! US-87 acceptance tests — the trip list's Collections tab, the server's
//! half.
//!
//! The tab itself is the SPA's (`crates/ui-dioxus`). What it reads is here:
//! `GET /api/collections` lists every collection alphabetically with its
//! activity, its counts and where its tracks and waypoints lie, narrowed by
//! the name search and the region as the trip list is; and
//! `GET /api/collection-tracks?ids=…` answers the tracks the map draws as
//! lines, as `/api/trips/tracks` does for trips (US-73).
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use trip_archive::models::{
    ActivityType, CollectionCount, CollectionDetail, CollectionSummary, TripTrack,
};

use crate::common::{
    body_string, error_message, get, import_collection, send_unauthenticated, test_app,
    COLLECTION_GPX, COLLECTION_SINGLE_GPX, WAYPOINTS_ONLY_GPX,
};

async fn read_json<T: serde::de::DeserializeOwned>(app: &axum::Router, uri: &str) -> T {
    let response = get(app, uri).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("{uri}: {e}: {body}"))
}

async fn collections(app: &axum::Router, query: &str) -> Vec<CollectionSummary> {
    read_json(app, &format!("/api/collections{query}")).await
}

fn names(listed: &[CollectionSummary]) -> Vec<&str> {
    listed.iter().map(|c| c.name.as_str()).collect()
}

// ── The list ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn us87_every_collection_is_listed_alphabetically_whatever_its_case() {
    let (app, _dir) = test_app().await;
    for name in ["bergtouren", "Comer See", "Alpencross"] {
        import_collection(&app, COLLECTION_SINGLE_GPX, &[("name", name)]).await;
    }

    let listed = collections(&app, "").await;

    assert_eq!(names(&listed), ["Alpencross", "bergtouren", "Comer See"]);
}

#[tokio::test]
async fn us87_an_archive_without_collections_lists_none() {
    let (app, _dir) = test_app().await;

    assert!(collections(&app, "").await.is_empty());
}

#[tokio::test]
async fn us87_a_collection_comes_with_its_activity_and_its_counts() {
    let (app, _dir) = test_app().await;
    let id = import_collection(
        &app,
        COLLECTION_GPX,
        &[("name", "Lakes"), ("activity_type", "cycling")],
    )
    .await;
    import_collection(&app, WAYPOINTS_ONLY_GPX, &[("name", "Passes")]).await;

    let listed = collections(&app, "").await;

    let lakes = &listed[0];
    assert_eq!(lakes.id, id);
    assert_eq!(lakes.activity_type, Some(ActivityType::Cycling));
    assert_eq!(lakes.tracks.len(), 3);
    assert_eq!(lakes.waypoint_count, 2);
    let passes = &listed[1];
    assert_eq!(passes.activity_type, None, "no activity type, no icon");
    assert!(passes.tracks.is_empty());
    assert_eq!(passes.waypoint_count, 2);
}

#[tokio::test]
async fn us87_each_track_comes_with_its_name_activity_and_bounding_box() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, COLLECTION_GPX, &[("activity_type", "cycling")]).await;
    let detail: CollectionDetail = read_json(&app, &format!("/api/collections/{id}")).await;

    let listed = collections(&app, "").await;

    let tracks = &listed[0].tracks;
    let ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();
    let expected: Vec<i64> = detail.tracks.iter().map(|t| t.id).collect();
    assert_eq!(ids, expected, "in file order");
    assert_eq!(tracks[0].name, "Stage 1: Splügen");
    assert!(tracks
        .iter()
        .all(|t| t.activity_type == ActivityType::Cycling));
    // Stage 3 runs from 46.64/9.78 to the Albula Pass at 46.5822/9.8377.
    let stage_3 = &tracks[2];
    assert_eq!(
        [
            stage_3.min_lat,
            stage_3.min_lon,
            stage_3.max_lat,
            stage_3.max_lon
        ],
        [46.5822, 9.78, 46.64, 9.8377]
    );
}

#[tokio::test]
async fn us87_a_track_without_an_activity_of_its_own_goes_by_unknown() {
    let (app, _dir) = test_app().await;
    import_collection(&app, COLLECTION_SINGLE_GPX, &[]).await;

    let listed = collections(&app, "").await;

    assert_eq!(listed[0].tracks[0].activity_type, ActivityType::Unknown);
}

#[tokio::test]
async fn us87_the_waypoints_bounds_span_every_waypoint() {
    let (app, _dir) = test_app().await;
    import_collection(&app, WAYPOINTS_ONLY_GPX, &[("name", "A")]).await;
    let mut without = COLLECTION_SINGLE_GPX.to_vec();
    // The same tour, its waypoints cut out.
    let text = String::from_utf8(without.clone()).unwrap();
    let start = text.find("<wpt").unwrap();
    let end = text.rfind("</wpt>").unwrap() + "</wpt>".len();
    without = format!("{}{}", &text[..start], &text[end..]).into_bytes();
    import_collection(&app, &without, &[("name", "B")]).await;

    let listed = collections(&app, "").await;

    // San Bernardino Pass (46.4958/9.1705) and Albula Pass (46.5822/9.8377).
    assert_eq!(
        listed[0].waypoint_bounds,
        Some([9.1705, 46.4958, 9.8377, 46.5822])
    );
    assert_eq!(listed[1].waypoint_count, 0);
    assert_eq!(listed[1].waypoint_bounds, None);
}

// ── Narrowing it ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn us87_the_name_search_narrows_the_list_whatever_the_case() {
    let (app, _dir) = test_app().await;
    for name in ["Rund um Tromsø", "Lofoten", "Tromsø Nord"] {
        import_collection(&app, COLLECTION_SINGLE_GPX, &[("name", name)]).await;
    }

    let listed = collections(&app, "?q=TROMSØ").await;

    assert_eq!(names(&listed), ["Rund um Tromsø", "Tromsø Nord"]);
}

#[tokio::test]
async fn us87_a_blank_search_narrows_nothing() {
    let (app, _dir) = test_app().await;
    import_collection(&app, COLLECTION_SINGLE_GPX, &[]).await;

    assert_eq!(collections(&app, "?q=&bbox=").await.len(), 1);
}

#[tokio::test]
async fn us87_a_region_lists_the_collections_with_a_track_in_it() {
    let (app, _dir) = test_app().await;
    import_collection(&app, COLLECTION_GPX, &[("name", "Lakes")]).await;
    import_collection(&app, COLLECTION_SINGLE_GPX, &[("name", "Nordkette")]).await;

    // Around Innsbruck: the Nordkette tour only.
    let listed = collections(&app, "?bbox=11.3,47.2,11.5,47.4").await;

    assert_eq!(names(&listed), ["Nordkette"]);
}

#[tokio::test]
async fn us87_a_region_lists_the_collections_with_a_waypoint_in_it() {
    let (app, _dir) = test_app().await;
    import_collection(&app, WAYPOINTS_ONLY_GPX, &[("name", "Passes")]).await;
    import_collection(&app, COLLECTION_SINGLE_GPX, &[("name", "Nordkette")]).await;

    // Around the San Bernardino Pass: no track, one waypoint.
    let listed = collections(&app, "?bbox=9.16,46.49,9.18,46.50").await;

    assert_eq!(names(&listed), ["Passes"]);
}

#[tokio::test]
async fn us87_a_region_with_nothing_in_it_lists_nothing() {
    let (app, _dir) = test_app().await;
    import_collection(&app, COLLECTION_GPX, &[]).await;

    assert!(collections(&app, "?bbox=0,0,1,1").await.is_empty());
}

#[tokio::test]
async fn us87_search_and_region_narrow_together() {
    let (app, _dir) = test_app().await;
    import_collection(&app, COLLECTION_SINGLE_GPX, &[("name", "Nordkette")]).await;
    import_collection(&app, COLLECTION_SINGLE_GPX, &[("name", "Patscherkofel")]).await;
    import_collection(&app, COLLECTION_GPX, &[("name", "Nord-Passes")]).await;

    let listed = collections(&app, "?q=nord&bbox=11.3,47.2,11.5,47.4").await;

    assert_eq!(names(&listed), ["Nordkette"]);
}

#[tokio::test]
async fn us87_a_malformed_region_is_refused_with_the_reason() {
    let (app, _dir) = test_app().await;

    let response = get(&app, "/api/collections?bbox=11,47,10,48").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        error_message(response).await.contains("longitude"),
        "the reason names what is wrong"
    );
}

// ── The tracks the map draws as lines ────────────────────────────────────────

async fn tracks(app: &axum::Router, query: &str) -> Vec<TripTrack> {
    read_json(app, &format!("/api/collection-tracks{query}")).await
}

#[tokio::test]
async fn us87_each_requested_track_comes_with_its_positions() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, COLLECTION_GPX, &[]).await;
    let all: Vec<TripTrack> = read_json(&app, &format!("/api/collections/{id}/tracks")).await;
    let (first, third) = (&all[0], &all[2]);

    let answer = tracks(&app, &format!("?ids={},{}", first.id, third.id)).await;

    assert_eq!(answer, [first.clone(), third.clone()]);
    assert_eq!(answer[1].coordinates[0], [9.78, 46.64], "[lon, lat]");
}

#[tokio::test]
async fn us87_an_unknown_track_is_left_out_and_no_ids_ask_for_nothing() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, COLLECTION_SINGLE_GPX, &[]).await;
    let all: Vec<TripTrack> = read_json(&app, &format!("/api/collections/{id}/tracks")).await;

    let answer = tracks(&app, &format!("?ids={},9999", all[0].id)).await;

    assert_eq!(answer.len(), 1);
    assert!(tracks(&app, "").await.is_empty());
    assert!(tracks(&app, "?ids=").await.is_empty());
}

#[tokio::test]
async fn us87_ids_that_are_not_integers_are_refused() {
    let (app, _dir) = test_app().await;

    let response = get(&app, "/api/collection-tracks?ids=1,x").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let reason = error_message(response).await;
    assert!(
        !reason.contains("trip"),
        "collection tracks are not trips: {reason}"
    );
}

// ── How many there are ───────────────────────────────────────────────────────

#[tokio::test]
async fn us87_the_collections_are_counted_without_being_listed() {
    let (app, _dir) = test_app().await;
    let empty: CollectionCount = read_json(&app, "/api/collections/count").await;
    for name in ["One", "Two", "Three"] {
        import_collection(&app, COLLECTION_SINGLE_GPX, &[("name", name)]).await;
    }

    let counted: CollectionCount = read_json(&app, "/api/collections/count").await;

    assert_eq!(empty.count, 0);
    assert_eq!(counted.count, 3);
}

// ── The owner's alone ────────────────────────────────────────────────────────

#[tokio::test]
async fn us87_the_collections_and_their_tracks_are_the_owners_alone() {
    let (app, _dir) = test_app().await;
    let id = import_collection(&app, COLLECTION_SINGLE_GPX, &[]).await;
    let all: Vec<TripTrack> = read_json(&app, &format!("/api/collections/{id}/tracks")).await;

    for uri in [
        "/api/collections".to_string(),
        "/api/collections/count".to_string(),
        format!("/api/collection-tracks?ids={}", all[0].id),
    ] {
        let response = send_unauthenticated(
            &app,
            Request::builder().uri(&uri).body(Body::empty()).unwrap(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
}
