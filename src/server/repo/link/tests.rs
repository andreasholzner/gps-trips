//! US-99 — links from what the owner did to what they planned, against a
//! real SQLite file (ADR-0012).

use super::*;
use crate::models::{ActivityType, TripKind};
use crate::server::db::testing::TestDb;
use crate::server::geojson::build_track_geojson;
use crate::server::gpx::{compute_stats, parse_gpx};
use crate::server::repo::{
    delete_collection, delete_collection_track, delete_trip, delete_waypoint, get_collection,
    insert_collection, insert_trip, list_collection_waypoints, NewCollection, NewCollectionFile,
    NewCollectionTrack, NewTrip, NewWaypoint,
};

const SAMPLE_GPX: &[u8] = include_bytes!("../../../../tests/fixtures/sample.gpx");

async fn a_trip(pool: &SqlitePool, name: &str, trip_kind: TripKind) -> i64 {
    let track = parse_gpx(SAMPLE_GPX).unwrap();
    let stats = compute_stats(&track.points);
    let geojson = build_track_geojson(&track.points);
    insert_trip(
        pool,
        &NewTrip {
            name,
            activity_type: ActivityType::Hiking,
            tz_name: "Europe/Oslo",
            stats: &stats,
            geojson: &geojson,
            gpx: SAMPLE_GPX,
            trip_kind,
        },
    )
    .await
    .expect("insert_trip")
}

async fn recorded(pool: &SqlitePool, name: &str) -> i64 {
    a_trip(pool, name, TripKind::Recorded).await
}

async fn planned(pool: &SqlitePool, name: &str) -> i64 {
    a_trip(pool, name, TripKind::Planned).await
}

/// When the trip started, so recorded trips list newest first.
async fn started(pool: &SqlitePool, trip: i64, start_time: &str) {
    sqlx::query("UPDATE trip SET start_time = ? WHERE id = ?")
        .bind(start_time)
        .bind(trip)
        .execute(pool)
        .await
        .unwrap();
}

/// A collection named `name` with the tracks named, and two waypoints; its
/// id, its tracks' ids and its waypoints' ids.
async fn a_collection(pool: &SqlitePool, name: &str, tracks: &[&str]) -> (i64, Vec<i64>, Vec<i64>) {
    let track = parse_gpx(SAMPLE_GPX).unwrap();
    let stats = compute_stats(&track.points);
    let geojson = build_track_geojson(&track.points);
    let tracks: Vec<NewCollectionTrack> = tracks
        .iter()
        .map(|name| NewCollectionTrack {
            name,
            stats: &stats,
            geojson: &geojson,
            gpx: b"<gpx/>",
        })
        .collect();
    let waypoints: Vec<NewWaypoint> = ["Pass", "Hut"]
        .iter()
        .map(|name| NewWaypoint {
            name: Some(name),
            description: None,
            lat: 60.0,
            lon: 10.0,
            ele: None,
            symbol: Some("Summit"),
            track: None,
        })
        .collect();
    let id = insert_collection(
        pool,
        &NewCollection {
            name,
            description: "",
            activity_type: Some(ActivityType::Cycling),
            files: &[NewCollectionFile {
                file_name: "book.gpx",
                gpx: SAMPLE_GPX,
            }],
            tracks: &tracks,
            waypoints: &waypoints,
        },
    )
    .await
    .expect("insert_collection");
    let detail = get_collection(pool, id).await.unwrap().unwrap();
    let waypoints = list_collection_waypoints(pool, id).await.unwrap();
    (
        id,
        detail.tracks.iter().map(|track| track.id).collect(),
        waypoints.iter().map(|waypoint| waypoint.id).collect(),
    )
}

async fn links(pool: &SqlitePool, end: LinkEnd) -> Links {
    list_links(pool, end)
        .await
        .unwrap()
        .expect("the end exists")
}

fn trip_ids(trips: &[LinkedTrip]) -> Vec<i64> {
    trips.iter().map(|trip| trip.id).collect()
}

fn track_ids(tracks: &[LinkedTrack]) -> Vec<i64> {
    tracks.iter().map(|track| track.id).collect()
}

fn waypoint_ids(waypoints: &[LinkedWaypoint]) -> Vec<i64> {
    waypoints.iter().map(|waypoint| waypoint.id).collect()
}

#[tokio::test]
async fn us99_a_selection_links_every_pair_that_can_be_linked() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let tuesday = recorded(&db.pool, "Tuesday").await;
    let plan = planned(&db.pool, "Plan").await;
    let (_, tracks, _) = a_collection(&db.pool, "Book", &["Stage 1", "Stage 2"]).await;

    let outcome = link_selection(&db.pool, &[monday, tuesday, plan], &tracks)
        .await
        .unwrap();

    // 2 recorded × 1 planned + 2 recorded × 2 tracks + 1 planned × 2 tracks.
    assert_eq!(outcome, LinkOutcome::Linked { pairs: 8 });
    let on_monday = links(&db.pool, LinkEnd::Trip(monday)).await;
    assert_eq!(trip_ids(&on_monday.planned_trips), vec![plan]);
    assert_eq!(track_ids(&on_monday.collection_tracks), tracks);
    assert!(on_monday.recorded_trips.is_empty());
    let on_plan = links(&db.pool, LinkEnd::Trip(plan)).await;
    assert_eq!(track_ids(&on_plan.collection_tracks), tracks);
    let mut done_by = trip_ids(&on_plan.recorded_trips);
    done_by.sort();
    assert_eq!(done_by, vec![monday, tuesday]);
    let on_stage = links(&db.pool, LinkEnd::CollectionTrack(tracks[0])).await;
    assert_eq!(on_stage.planned_trips.len(), 1);
    assert_eq!(on_stage.recorded_trips.len(), 2);
    assert!(on_stage.collection_tracks.is_empty());
}

#[tokio::test]
async fn us99_trips_of_one_kind_or_tracks_alone_are_never_linked() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let tuesday = recorded(&db.pool, "Tuesday").await;
    let plan = planned(&db.pool, "Plan").await;
    let other_plan = planned(&db.pool, "Other plan").await;
    let (_, tracks, _) = a_collection(&db.pool, "Book", &["Stage 1", "Stage 2"]).await;

    for (trips, tracks) in [
        (vec![monday, tuesday], vec![]),
        (vec![plan, other_plan], vec![]),
        (vec![], tracks.clone()),
        (vec![monday], vec![]),
    ] {
        assert_eq!(
            link_selection(&db.pool, &trips, &tracks).await.unwrap(),
            LinkOutcome::NothingToLink
        );
    }
    assert_eq!(
        links(&db.pool, LinkEnd::Trip(monday)).await,
        Links::default()
    );
    assert_eq!(links(&db.pool, LinkEnd::Trip(plan)).await, Links::default());
}

#[tokio::test]
async fn us99_a_long_plan_is_linked_to_every_day_that_followed_it_newest_first() {
    let db = TestDb::new().await;
    let plan = planned(&db.pool, "Six days").await;
    let mut days = Vec::new();
    for day in 1..=6 {
        let trip = recorded(&db.pool, &format!("Day {day}")).await;
        started(&db.pool, trip, &format!("2026-07-0{day}T08:00:00Z")).await;
        days.push(trip);
    }
    let mut selection = days.clone();
    selection.push(plan);

    let outcome = link_selection(&db.pool, &selection, &[]).await.unwrap();

    assert_eq!(outcome, LinkOutcome::Linked { pairs: 6 });
    let on_plan = links(&db.pool, LinkEnd::Trip(plan)).await;
    days.reverse();
    assert_eq!(trip_ids(&on_plan.recorded_trips), days);
    assert_eq!(
        on_plan.recorded_trips[0].start_date.as_deref(),
        Some("2026-07-06")
    );
}

#[tokio::test]
async fn us99_linking_a_pair_again_leaves_one_link() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let plan = planned(&db.pool, "Plan").await;

    link_selection(&db.pool, &[monday, plan], &[])
        .await
        .unwrap();
    let again = link_selection(&db.pool, &[monday, plan, monday], &[])
        .await
        .unwrap();

    assert_eq!(again, LinkOutcome::Linked { pairs: 1 });
    assert_eq!(
        links(&db.pool, LinkEnd::Trip(plan))
            .await
            .recorded_trips
            .len(),
        1
    );
}

#[tokio::test]
async fn us99_a_selection_naming_something_missing_links_nothing() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let plan = planned(&db.pool, "Plan").await;
    let (_, tracks, _) = a_collection(&db.pool, "Book", &["Stage 1"]).await;

    assert_eq!(
        link_selection(&db.pool, &[monday, plan, 9999], &tracks)
            .await
            .unwrap(),
        LinkOutcome::Missing
    );
    assert_eq!(
        link_selection(&db.pool, &[monday, plan], &[tracks[0], 9999])
            .await
            .unwrap(),
        LinkOutcome::Missing
    );
    assert_eq!(
        links(&db.pool, LinkEnd::Trip(monday)).await,
        Links::default()
    );
}

#[tokio::test]
async fn us99_a_link_is_removed_from_either_end() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let tuesday = recorded(&db.pool, "Tuesday").await;
    let plan = planned(&db.pool, "Plan").await;
    let (_, tracks, waypoints) = a_collection(&db.pool, "Book", &["Stage 1"]).await;
    link_selection(&db.pool, &[monday, tuesday, plan], &tracks)
        .await
        .unwrap();
    assert!(link_waypoint(&db.pool, monday, waypoints[0]).await.unwrap());

    // From the planned trip's end, and from the recorded trip's.
    assert!(unlink(&db.pool, plan, LinkEnd::Trip(monday)).await.unwrap());
    assert!(unlink(&db.pool, tuesday, LinkEnd::Trip(plan))
        .await
        .unwrap());
    assert!(
        unlink(&db.pool, monday, LinkEnd::CollectionTrack(tracks[0]))
            .await
            .unwrap()
    );
    assert!(unlink(&db.pool, monday, LinkEnd::Waypoint(waypoints[0]))
        .await
        .unwrap());
    // Nothing left to remove.
    assert!(!unlink(&db.pool, plan, LinkEnd::Trip(monday)).await.unwrap());

    assert_eq!(
        links(&db.pool, LinkEnd::Trip(monday)).await,
        Links::default()
    );
    let on_plan = links(&db.pool, LinkEnd::Trip(plan)).await;
    assert!(on_plan.recorded_trips.is_empty());
    assert_eq!(track_ids(&on_plan.collection_tracks), tracks);
}

#[tokio::test]
async fn us99_waypoints_are_listed_on_the_trips_and_the_trips_on_the_waypoint() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let plan = planned(&db.pool, "Plan").await;
    let (collection, _, waypoints) = a_collection(&db.pool, "Book", &["Stage 1"]).await;
    let (pass, hut) = (waypoints[0], waypoints[1]);

    for (trip, waypoint) in [(monday, pass), (monday, hut), (plan, hut)] {
        assert!(link_waypoint(&db.pool, trip, waypoint).await.unwrap());
    }
    assert!(!link_waypoint(&db.pool, monday, 9999).await.unwrap());
    assert!(!link_waypoint(&db.pool, 9999, pass).await.unwrap());

    let on_monday = links(&db.pool, LinkEnd::Trip(monday)).await;
    // By name: Hut before Pass.
    assert_eq!(waypoint_ids(&on_monday.waypoints), vec![hut, pass]);
    assert_eq!(
        on_monday.waypoints[0],
        LinkedWaypoint {
            id: hut,
            name: Some("Hut".to_string()),
            symbol: Some("Summit".to_string()),
            collection_id: collection,
            collection_name: "Book".to_string(),
            proposal: None,
        }
    );
    let on_hut = links(&db.pool, LinkEnd::Waypoint(hut)).await;
    assert_eq!(trip_ids(&on_hut.recorded_trips), vec![monday]);
    assert_eq!(trip_ids(&on_hut.planned_trips), vec![plan]);
    assert!(on_hut.waypoints.is_empty());
}

#[tokio::test]
async fn us99_the_other_ends_are_named_as_their_pages_list_them() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let zermatt = planned(&db.pool, "zermatt loop").await;
    let alps = planned(&db.pool, "Alps").await;
    let (collection, tracks, _) = a_collection(&db.pool, "Book", &["Stage 2", "Stage 1"]).await;
    link_selection(&db.pool, &[monday, zermatt, alps], &tracks)
        .await
        .unwrap();

    let on_monday = links(&db.pool, LinkEnd::Trip(monday)).await;

    // By name, whatever the case of the letters.
    assert_eq!(trip_ids(&on_monday.planned_trips), vec![alps, zermatt]);
    assert_eq!(
        on_monday.planned_trips[0].activity_type,
        ActivityType::Hiking
    );
    assert_eq!(
        track_ids(&on_monday.collection_tracks),
        vec![tracks[1], tracks[0]]
    );
    assert_eq!(
        on_monday.collection_tracks[0],
        LinkedTrack {
            id: tracks[1],
            name: "Stage 1".to_string(),
            activity_type: ActivityType::Cycling,
            collection_id: collection,
            collection_name: "Book".to_string(),
            proposal: None,
        }
    );
}

#[tokio::test]
async fn us99_deleting_either_end_takes_its_links_with_it() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let plan = planned(&db.pool, "Plan").await;
    let doomed_plan = planned(&db.pool, "Doomed").await;
    let (_, tracks, waypoints) = a_collection(&db.pool, "Book", &["Stage 1", "Stage 2"]).await;
    let (gone, gone_tracks, gone_waypoints) = a_collection(&db.pool, "Gone", &["Stage 9"]).await;
    let every_track: Vec<i64> = tracks.iter().chain(&gone_tracks).copied().collect();
    link_selection(&db.pool, &[monday, plan, doomed_plan], &every_track)
        .await
        .unwrap();
    for waypoint in waypoints.iter().chain(&gone_waypoints) {
        link_waypoint(&db.pool, monday, *waypoint).await.unwrap();
    }

    delete_trip(&db.pool, doomed_plan).await.unwrap();
    delete_collection_track(&db.pool, tracks[0]).await.unwrap();
    delete_waypoint(&db.pool, waypoints[0]).await.unwrap();
    delete_collection(&db.pool, gone).await.unwrap();

    let on_monday = links(&db.pool, LinkEnd::Trip(monday)).await;
    assert_eq!(trip_ids(&on_monday.planned_trips), vec![plan]);
    assert_eq!(track_ids(&on_monday.collection_tracks), vec![tracks[1]]);
    assert_eq!(waypoint_ids(&on_monday.waypoints), vec![waypoints[1]]);
    delete_trip(&db.pool, monday).await.unwrap();
    assert!(links(&db.pool, LinkEnd::Trip(plan))
        .await
        .recorded_trips
        .is_empty());
    assert!(links(&db.pool, LinkEnd::CollectionTrack(tracks[1]))
        .await
        .recorded_trips
        .is_empty());
}

#[tokio::test]
async fn us99_an_end_that_does_not_exist_has_no_links_to_list() {
    let db = TestDb::new().await;
    for end in [
        LinkEnd::Trip(9999),
        LinkEnd::CollectionTrack(9999),
        LinkEnd::Waypoint(9999),
    ] {
        assert_eq!(list_links(&db.pool, end).await.unwrap(), None);
    }
}
