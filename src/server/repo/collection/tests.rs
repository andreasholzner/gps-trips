//! US-86 — the collection repository, against a real SQLite file (ADR-0012).

use super::*;
use crate::server::db::testing::TestDb;
use crate::server::geojson::build_track_geojson;
use crate::server::gpx::{compute_stats, parse_gpx};

const SAMPLE_GPX: &[u8] = include_bytes!("../../../../tests/fixtures/sample.gpx");

fn sample_stats() -> (TrackStats, String) {
    let track = parse_gpx(SAMPLE_GPX).unwrap();
    (
        compute_stats(&track.points),
        build_track_geojson(&track.points),
    )
}

fn a_waypoint(name: &str, track: Option<usize>) -> NewWaypoint<'_> {
    NewWaypoint {
        name: Some(name),
        description: Some("A pass"),
        lat: 46.5,
        lon: 9.2,
        ele: Some(2065.0),
        symbol: None,
        track,
    }
}

/// A collection of `names.len()` tracks and the given waypoints.
async fn insert(
    pool: &SqlitePool,
    activity: Option<ActivityType>,
    names: &[&str],
    waypoints: &[NewWaypoint<'_>],
) -> i64 {
    let (stats, geojson) = sample_stats();
    let tracks: Vec<NewCollectionTrack> = names
        .iter()
        .map(|name| NewCollectionTrack {
            name,
            stats: &stats,
            geojson: &geojson,
            gpx: b"<gpx/>",
        })
        .collect();
    insert_collection(
        pool,
        &NewCollection {
            name: "Alpencross",
            description: "Bruckmann",
            activity_type: activity,
            files: &[NewCollectionFile {
                file_name: "sample.gpx",
                gpx: SAMPLE_GPX,
            }],
            tracks: &tracks,
            waypoints,
        },
    )
    .await
    .expect("insert_collection")
}

#[tokio::test]
async fn a_collection_holds_its_tracks_in_file_order_and_counts_its_waypoints() {
    let db = TestDb::new().await;
    let waypoints = [
        a_waypoint("San Bernardino", None),
        a_waypoint("Albula", None),
    ];
    let id = insert(&db.pool, None, &["Day 2", "Day 1"], &waypoints).await;

    let collection = get_collection(&db.pool, id).await.unwrap().expect("stored");

    assert_eq!(collection.name, "Alpencross");
    assert_eq!(collection.description, "Bruckmann");
    assert_eq!(collection.activity_type, None);
    assert_eq!(collection.waypoint_count, 2);
    let names: Vec<&str> = collection.tracks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Day 2", "Day 1"]);
    assert!(collection.tracks[0].distance_m > 0.0);
}

#[tokio::test]
async fn a_track_without_an_activity_takes_its_collections_and_else_none() {
    let db = TestDb::new().await;
    let id = insert(&db.pool, Some(ActivityType::Cycling), &["Day 1"], &[]).await;
    let track_id = get_collection(&db.pool, id).await.unwrap().unwrap().tracks[0].id;

    let track = get_collection_track(&db.pool, track_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(track.activity_type, ActivityType::Cycling);
    assert_eq!(track.own_activity, None);

    let change = CollectionChange {
        activity_type: Some(None),
        ..Default::default()
    };
    assert!(update_collection(&db.pool, id, &change).await.unwrap());
    let track = get_collection_track(&db.pool, track_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(track.activity_type, ActivityType::Unknown);
}

#[tokio::test]
async fn changing_the_collections_activity_spares_a_track_with_its_own() {
    let db = TestDb::new().await;
    let id = insert(
        &db.pool,
        Some(ActivityType::Cycling),
        &["Own", "Taken"],
        &[],
    )
    .await;
    let tracks = get_collection(&db.pool, id).await.unwrap().unwrap().tracks;
    let own = CollectionTrackChange {
        activity_type: Some(Some(ActivityType::Hiking)),
        ..Default::default()
    };
    assert!(update_collection_track(&db.pool, tracks[0].id, &own)
        .await
        .unwrap());

    let change = CollectionChange {
        activity_type: Some(Some(ActivityType::Bikepacking)),
        ..Default::default()
    };
    update_collection(&db.pool, id, &change).await.unwrap();

    let tracks = get_collection(&db.pool, id).await.unwrap().unwrap().tracks;
    assert_eq!(tracks[0].activity_type, ActivityType::Hiking);
    assert_eq!(tracks[1].activity_type, ActivityType::Bikepacking);
}

#[tokio::test]
async fn a_track_given_back_to_its_collection_takes_its_activity_again() {
    let db = TestDb::new().await;
    let id = insert(&db.pool, Some(ActivityType::Cycling), &["Day 1"], &[]).await;
    let track_id = get_collection(&db.pool, id).await.unwrap().unwrap().tracks[0].id;
    let own = CollectionTrackChange {
        activity_type: Some(Some(ActivityType::Hiking)),
        ..Default::default()
    };
    update_collection_track(&db.pool, track_id, &own)
        .await
        .unwrap();

    let back = CollectionTrackChange {
        activity_type: Some(None),
        ..Default::default()
    };
    update_collection_track(&db.pool, track_id, &back)
        .await
        .unwrap();

    let track = get_collection_track(&db.pool, track_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(track.activity_type, ActivityType::Cycling);
}

#[tokio::test]
async fn an_edit_changes_only_what_it_names() {
    let db = TestDb::new().await;
    let id = insert(&db.pool, Some(ActivityType::Cycling), &["Day 1"], &[]).await;
    let track_id = get_collection(&db.pool, id).await.unwrap().unwrap().tracks[0].id;

    let rename = CollectionChange {
        name: Some("Transalp"),
        ..Default::default()
    };
    update_collection(&db.pool, id, &rename).await.unwrap();
    let rename = CollectionTrackChange {
        name: Some("First day"),
        ..Default::default()
    };
    update_collection_track(&db.pool, track_id, &rename)
        .await
        .unwrap();

    let collection = get_collection(&db.pool, id).await.unwrap().unwrap();
    assert_eq!(collection.name, "Transalp");
    assert_eq!(collection.description, "Bruckmann");
    assert_eq!(collection.activity_type, Some(ActivityType::Cycling));
    let track = get_collection_track(&db.pool, track_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(track.name, "First day");
    assert_eq!(track.collection_name, "Transalp");
}

#[tokio::test]
async fn whether_a_collection_exists_is_answered_without_reading_it() {
    let db = TestDb::new().await;
    let id = insert(&db.pool, None, &["Tour"], &[]).await;

    assert!(collection_exists(&db.pool, id).await.unwrap());
    assert!(!collection_exists(&db.pool, id + 1).await.unwrap());
}

#[tokio::test]
async fn us88_a_collections_name_is_read_without_its_tracks() {
    let db = TestDb::new().await;
    let id = insert(&db.pool, None, &["Day 1"], &[]).await;

    assert_eq!(
        collection_name(&db.pool, id).await.unwrap().as_deref(),
        Some("Alpencross")
    );
    assert_eq!(collection_name(&db.pool, id + 1).await.unwrap(), None);
}

#[tokio::test]
async fn editing_or_deleting_what_does_not_exist_says_so() {
    let db = TestDb::new().await;
    let change = CollectionChange {
        name: Some("x"),
        ..Default::default()
    };
    assert!(!update_collection(&db.pool, 99, &change).await.unwrap());
    assert!(
        !update_collection_track(&db.pool, 99, &CollectionTrackChange::default())
            .await
            .unwrap()
    );
    assert!(!delete_collection(&db.pool, 99).await.unwrap());
    assert!(!delete_collection_track(&db.pool, 99).await.unwrap());
    assert!(get_collection(&db.pool, 99).await.unwrap().is_none());
    assert!(get_collection_track(&db.pool, 99).await.unwrap().is_none());
}

#[tokio::test]
async fn a_waypoint_may_belong_to_a_track_as_well() {
    let db = TestDb::new().await;
    let waypoints = [a_waypoint("Summit", Some(0)), a_waypoint("Hut", None)];
    let id = insert(&db.pool, None, &["Tour"], &waypoints).await;
    let track_id = get_collection(&db.pool, id).await.unwrap().unwrap().tracks[0].id;

    let all = list_collection_waypoints(&db.pool, id).await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].name.as_deref(), Some("Summit"));
    assert_eq!(all[0].collection_track_id, Some(track_id));
    assert_eq!(all[0].ele, Some(2065.0));
    assert_eq!(all[0].description.as_deref(), Some("A pass"));
    assert_eq!(all[1].collection_track_id, None);

    let on_track = list_collection_track_waypoints(&db.pool, track_id)
        .await
        .unwrap();
    assert_eq!(on_track.len(), 1);
    assert_eq!(on_track[0].name.as_deref(), Some("Summit"));
}

#[tokio::test]
async fn us95_a_waypoint_keeps_its_symbol_on_both_lists() {
    let db = TestDb::new().await;
    let waypoints = [
        NewWaypoint {
            symbol: Some("Triangle, Blue"),
            ..a_waypoint("Summit", Some(0))
        },
        a_waypoint("Hut", None),
    ];
    let id = insert(&db.pool, None, &["Tour"], &waypoints).await;
    let track_id = get_collection(&db.pool, id).await.unwrap().unwrap().tracks[0].id;

    let all = list_collection_waypoints(&db.pool, id).await.unwrap();
    assert_eq!(all[0].symbol.as_deref(), Some("Triangle, Blue"));
    assert_eq!(all[1].symbol, None);
    let on_track = list_collection_track_waypoints(&db.pool, track_id)
        .await
        .unwrap();
    assert_eq!(on_track[0].symbol.as_deref(), Some("Triangle, Blue"));
}

#[tokio::test]
async fn deleting_a_track_leaves_its_waypoints_to_the_collection() {
    let db = TestDb::new().await;
    let waypoints = [a_waypoint("Summit", Some(0))];
    let id = insert(&db.pool, None, &["Tour"], &waypoints).await;
    let track_id = get_collection(&db.pool, id).await.unwrap().unwrap().tracks[0].id;

    assert!(delete_collection_track(&db.pool, track_id).await.unwrap());

    let collection = get_collection(&db.pool, id).await.unwrap().unwrap();
    assert!(collection.tracks.is_empty());
    let all = list_collection_waypoints(&db.pool, id).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].collection_track_id, None);
}

#[tokio::test]
async fn deleting_a_collection_deletes_everything_it_holds() {
    let db = TestDb::new().await;
    let waypoints = [a_waypoint("Summit", Some(0))];
    let id = insert(&db.pool, None, &["Tour"], &waypoints).await;
    let track_id = get_collection(&db.pool, id).await.unwrap().unwrap().tracks[0].id;

    assert!(delete_collection(&db.pool, id).await.unwrap());

    assert!(get_collection(&db.pool, id).await.unwrap().is_none());
    assert!(get_collection_track(&db.pool, track_id)
        .await
        .unwrap()
        .is_none());
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM waypoint")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn a_tracks_geometry_and_gpx_are_kept_with_it() {
    let db = TestDb::new().await;
    let id = insert(&db.pool, None, &["Tour"], &[]).await;
    let track_id = get_collection(&db.pool, id).await.unwrap().unwrap().tracks[0].id;

    let geojson = get_collection_track_geojson(&db.pool, track_id)
        .await
        .unwrap()
        .expect("geometry");
    assert!(geojson.contains("LineString"), "{geojson}");
    let gpx = get_collection_track_gpx(&db.pool, track_id)
        .await
        .unwrap()
        .expect("gpx");
    assert_eq!(gpx.name, "Tour");
    assert_eq!(gpx.bytes, b"<gpx/>");

    let positions = list_collection_track_positions(&db.pool, id).await.unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].id, track_id);
    assert!(!positions[0].coordinates.is_empty());
}

#[tokio::test]
async fn collections_stay_out_of_the_trips() {
    // ADR-0028: nothing that reads trips sees a collection track.
    let db = TestDb::new().await;
    insert(&db.pool, None, &["Tour"], &[]).await;

    let trips: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trip")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(trips, 0);
}
