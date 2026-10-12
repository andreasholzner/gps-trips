//! US-98 — one waypoint read, changed and deleted, against a real SQLite
//! file (ADR-0012).

use super::*;
use crate::models::ActivityType;
use crate::server::db::testing::TestDb;
use crate::server::geojson::build_track_geojson;
use crate::server::gpx::{compute_stats, parse_gpx};
use crate::server::repo::{
    get_collection, insert_collection, list_collection_waypoints, NewCollection, NewCollectionFile,
    NewCollectionTrack, NewWaypoint,
};

const SAMPLE_GPX: &[u8] = include_bytes!("../../../../tests/fixtures/sample.gpx");

/// A hiking collection of one track, with a hut belonging to it and a pass
/// belonging to none. Returns the collection's id and the two waypoints'.
async fn a_hut_and_a_pass(pool: &SqlitePool) -> (i64, i64, i64) {
    let track = parse_gpx(SAMPLE_GPX).unwrap();
    let stats = compute_stats(&track.points);
    let geojson = build_track_geojson(&track.points);
    let waypoint = |name, track| NewWaypoint {
        name: Some(name),
        description: Some("From the book"),
        lat: 46.5,
        lon: 9.2,
        ele: Some(2065.0),
        symbol: Some("Lodge"),
        track,
    };
    let id = insert_collection(
        pool,
        &NewCollection {
            name: "Alpencross",
            description: "",
            activity_type: Some(ActivityType::Hiking),
            files: &[NewCollectionFile {
                file_name: "sample.gpx",
                gpx: SAMPLE_GPX,
            }],
            tracks: &[NewCollectionTrack {
                name: "Day 1",
                stats: &stats,
                geojson: &geojson,
                gpx: b"<gpx/>",
            }],
            waypoints: &[waypoint("Hut", Some(0)), waypoint("Pass", None)],
        },
    )
    .await
    .unwrap();
    let waypoints = list_collection_waypoints(pool, id).await.unwrap();
    (id, waypoints[0].id, waypoints[1].id)
}

#[tokio::test]
async fn us98_a_waypoint_is_read_with_its_collection_and_its_track() {
    let db = TestDb::new().await;
    let (collection, hut, pass) = a_hut_and_a_pass(&db.pool).await;
    let track = get_collection(&db.pool, collection)
        .await
        .unwrap()
        .unwrap()
        .tracks[0]
        .id;

    let read = get_waypoint(&db.pool, hut).await.unwrap().expect("stored");

    assert_eq!(read.waypoint.name.as_deref(), Some("Hut"));
    assert_eq!(read.collection_id, collection);
    assert_eq!(read.collection_name, "Alpencross");
    assert_eq!(
        read.track,
        Some(WaypointTrack {
            id: track,
            name: "Day 1".to_string(),
            activity_type: ActivityType::Hiking,
        })
    );
    let read = get_waypoint(&db.pool, pass).await.unwrap().expect("stored");
    assert_eq!(read.track, None);
}

#[tokio::test]
async fn us98_a_change_alters_only_what_it_names() {
    let db = TestDb::new().await;
    let (_, hut, pass) = a_hut_and_a_pass(&db.pool).await;

    let change = WaypointChange {
        name: Some("Chamanna"),
        ele: Some(None),
        ..Default::default()
    };
    assert!(update_waypoint(&db.pool, hut, &change).await.unwrap());

    let hut = get_waypoint(&db.pool, hut).await.unwrap().unwrap().waypoint;
    assert_eq!(hut.name.as_deref(), Some("Chamanna"));
    assert_eq!(hut.ele, None);
    assert_eq!(hut.description.as_deref(), Some("From the book"));
    assert_eq!(hut.symbol.as_deref(), Some("Lodge"));
    assert_eq!((hut.lat, hut.lon), (46.5, 9.2));
    let pass = get_waypoint(&db.pool, pass)
        .await
        .unwrap()
        .unwrap()
        .waypoint;
    assert_eq!(
        pass.name.as_deref(),
        Some("Pass"),
        "no other waypoint changes"
    );
}

#[tokio::test]
async fn us98_a_change_clears_the_description_and_symbol_and_moves_it() {
    let db = TestDb::new().await;
    let (_, hut, _) = a_hut_and_a_pass(&db.pool).await;

    let change = WaypointChange {
        description: Some(None),
        symbol: Some(Some("Summit")),
        position: Some((46.6, 9.3)),
        ..Default::default()
    };
    assert!(update_waypoint(&db.pool, hut, &change).await.unwrap());

    let hut = get_waypoint(&db.pool, hut).await.unwrap().unwrap().waypoint;
    assert_eq!(hut.description, None);
    assert_eq!(hut.symbol.as_deref(), Some("Summit"));
    assert_eq!((hut.lat, hut.lon), (46.6, 9.3));
    assert_eq!(hut.ele, Some(2065.0));
}

#[tokio::test]
async fn us98_deleting_a_waypoint_leaves_the_rest() {
    let db = TestDb::new().await;
    let (collection, hut, pass) = a_hut_and_a_pass(&db.pool).await;

    assert!(delete_waypoint(&db.pool, hut).await.unwrap());

    assert_eq!(get_waypoint(&db.pool, hut).await.unwrap(), None);
    let left = list_collection_waypoints(&db.pool, collection)
        .await
        .unwrap();
    assert_eq!(left.iter().map(|w| w.id).collect::<Vec<_>>(), [pass]);
    let collection = get_collection(&db.pool, collection).await.unwrap().unwrap();
    assert_eq!(collection.tracks.len(), 1);
}

#[tokio::test]
async fn us98_what_does_not_exist_is_said_so() {
    let db = TestDb::new().await;

    assert_eq!(get_waypoint(&db.pool, 9).await.unwrap(), None);
    let change = WaypointChange {
        name: Some("x"),
        ..Default::default()
    };
    assert!(!update_waypoint(&db.pool, 9, &change).await.unwrap());
    assert!(!delete_waypoint(&db.pool, 9).await.unwrap());
}
