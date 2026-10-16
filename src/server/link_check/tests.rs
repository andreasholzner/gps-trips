//! US-103 — checks against a real SQLite file, over synthetic tracks laid
//! out in metres east and north of 8° E, 60° N (ADR-0012).

use super::*;
use crate::models::{ActivityType, LinkOther, LinkPair, ProposedLink};
use crate::server::db::testing::TestDb;
use crate::server::geojson::build_track_geojson;
use crate::server::gpx::{compute_stats, TrackPoint};
use crate::server::repo::{
    dismiss_proposals, get_collection, insert_collection, insert_trip, link_selection,
    list_collection_waypoints, list_proposals, NewCollection, NewCollectionFile,
    NewCollectionTrack, NewTrip, NewWaypoint,
};

const METRES_PER_DEGREE: f64 = 111_195.08;

/// `x` metres east and `y` north.
fn at(x: f64, y: f64) -> (f64, f64) {
    (
        60.0 + y / METRES_PER_DEGREE,
        8.0 + x / (METRES_PER_DEGREE * 60f64.to_radians().cos()),
    )
}

/// A line through `corners`, a point every 20 m.
fn path(corners: &[(f64, f64)]) -> Vec<TrackPoint> {
    let point = |x: f64, y: f64| {
        let (lat, lon) = at(x, y);
        TrackPoint {
            lat,
            lon,
            ele: None,
            time: None,
        }
    };
    let mut points = vec![point(corners[0].0, corners[0].1)];
    for pair in corners.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        let pieces = ((x1 - x0).hypot(y1 - y0) / 20.0).ceil().max(1.0) as usize;
        for i in 1..=pieces {
            let t = i as f64 / pieces as f64;
            points.push(point(x0 + t * (x1 - x0), y0 + t * (y1 - y0)));
        }
    }
    points
}

/// The stage everything here follows, or not.
fn along_the_stage(y: f64) -> Vec<TrackPoint> {
    path(&[(0.0, y), (10_000.0, y)])
}

/// Far from the stage.
fn elsewhere() -> Vec<TrackPoint> {
    path(&[(0.0, 50_000.0), (10_000.0, 50_000.0)])
}

async fn a_trip(pool: &SqlitePool, kind: TripKind, points: Vec<TrackPoint>) -> i64 {
    insert_trip(
        pool,
        &NewTrip {
            name: "Trip",
            activity_type: ActivityType::Hiking,
            tz_name: "Europe/Oslo",
            stats: &compute_stats(&points),
            geojson: &build_track_geojson(&points),
            gpx: b"<gpx/>",
            trip_kind: kind,
        },
    )
    .await
    .unwrap()
}

/// A collection with the stage and a track elsewhere, a waypoint beside the
/// stage and one away from it; its tracks' and waypoints' ids.
async fn a_book(pool: &SqlitePool) -> (i64, Vec<i64>, Vec<i64>) {
    let (stage, far) = (along_the_stage(0.0), elsewhere());
    let (stage_stats, far_stats) = (compute_stats(&stage), compute_stats(&far));
    let (stage_json, far_json) = (build_track_geojson(&stage), build_track_geojson(&far));
    let tracks = [
        NewCollectionTrack {
            name: "Stage",
            stats: &stage_stats,
            geojson: &stage_json,
            gpx: b"<gpx/>",
        },
        NewCollectionTrack {
            name: "Elsewhere",
            stats: &far_stats,
            geojson: &far_json,
            gpx: b"<gpx/>",
        },
    ];
    let waypoint = |name, (lat, lon): (f64, f64)| NewWaypoint {
        name: Some(name),
        description: None,
        lat,
        lon,
        ele: None,
        symbol: None,
        track: None,
    };
    let waypoints = [
        waypoint("Beside", at(5_000.0, 30.0)),
        waypoint("Away", at(5_000.0, 5_000.0)),
    ];
    let id = insert_collection(
        pool,
        &NewCollection {
            name: "Book",
            description: "",
            activity_type: None,
            files: &[NewCollectionFile {
                file_name: "book.gpx",
                gpx: b"<gpx/>",
            }],
            tracks: &tracks,
            waypoints: &waypoints,
        },
    )
    .await
    .unwrap();
    let detail = get_collection(pool, id).await.unwrap().unwrap();
    let waypoints = list_collection_waypoints(pool, id).await.unwrap();
    (
        id,
        detail.tracks.iter().map(|track| track.id).collect(),
        waypoints.iter().map(|waypoint| waypoint.id).collect(),
    )
}

async fn proposed(pool: &SqlitePool) -> Vec<LinkPair> {
    let mut pairs: Vec<LinkPair> = list_proposals(pool)
        .await
        .unwrap()
        .iter()
        .map(ProposedLink::pair)
        .collect();
    pairs.sort();
    pairs
}

fn pair(trip_id: i64, other: LinkOther) -> LinkPair {
    LinkPair { trip_id, other }
}

fn sorted(mut pairs: Vec<LinkPair>) -> Vec<LinkPair> {
    pairs.sort();
    pairs
}

#[tokio::test]
async fn us103_a_recorded_trip_is_checked_against_plans_collection_tracks_and_waypoints() {
    let db = TestDb::new().await;
    let plan = a_trip(&db.pool, TripKind::Planned, along_the_stage(0.0)).await;
    a_trip(&db.pool, TripKind::Planned, elsewhere()).await;
    let (_, tracks, waypoints) = a_book(&db.pool).await;
    let monday = a_trip(&db.pool, TripKind::Recorded, along_the_stage(10.0)).await;

    assert_eq!(check_trip(&db.pool, monday).await.unwrap(), Some(3));

    assert_eq!(
        proposed(&db.pool).await,
        sorted(vec![
            pair(monday, LinkOther::Trip(plan)),
            pair(monday, LinkOther::CollectionTrack(tracks[0])),
            pair(monday, LinkOther::Waypoint(waypoints[0])),
        ])
    );
    let listed = list_proposals(&db.pool).await.unwrap();
    let to_plan = listed
        .iter()
        .find(|p| p.pair().other == LinkOther::Trip(plan))
        .unwrap();
    assert!(to_plan.proposal.plan_covered.unwrap() > 0.95);
    assert!(to_plan.proposal.trip_on_plan.unwrap() > 0.95);
    assert_eq!(to_plan.proposal.points_reached, None);
}

#[tokio::test]
async fn us103_a_planned_trip_is_checked_against_recorded_trips_collection_tracks_and_waypoints() {
    let db = TestDb::new().await;
    let monday = a_trip(&db.pool, TripKind::Recorded, along_the_stage(10.0)).await;
    a_trip(&db.pool, TripKind::Recorded, elsewhere()).await;
    let (_, tracks, waypoints) = a_book(&db.pool).await;
    let plan = a_trip(&db.pool, TripKind::Planned, along_the_stage(0.0)).await;

    assert_eq!(check_trip(&db.pool, plan).await.unwrap(), Some(3));

    assert_eq!(
        proposed(&db.pool).await,
        sorted(vec![
            pair(monday, LinkOther::Trip(plan)),
            pair(plan, LinkOther::CollectionTrack(tracks[0])),
            pair(plan, LinkOther::Waypoint(waypoints[0])),
        ])
    );
}

#[tokio::test]
async fn us103_a_collections_items_are_checked_against_recorded_and_planned_trips() {
    let db = TestDb::new().await;
    let monday = a_trip(&db.pool, TripKind::Recorded, along_the_stage(10.0)).await;
    let plan = a_trip(&db.pool, TripKind::Planned, along_the_stage(-10.0)).await;
    let (book, tracks, waypoints) = a_book(&db.pool).await;

    assert_eq!(check_collection(&db.pool, book).await.unwrap(), Some(4));

    assert_eq!(
        proposed(&db.pool).await,
        sorted(vec![
            pair(monday, LinkOther::CollectionTrack(tracks[0])),
            pair(monday, LinkOther::Waypoint(waypoints[0])),
            pair(plan, LinkOther::CollectionTrack(tracks[0])),
            pair(plan, LinkOther::Waypoint(waypoints[0])),
        ])
    );
}

#[tokio::test]
async fn us103_a_pair_already_linked_or_proposed_is_not_proposed_again() {
    let db = TestDb::new().await;
    let plan = a_trip(&db.pool, TripKind::Planned, along_the_stage(0.0)).await;
    let (_, tracks, _) = a_book(&db.pool).await;
    let monday = a_trip(&db.pool, TripKind::Recorded, along_the_stage(10.0)).await;
    link_selection(&db.pool, &[monday, plan], &tracks[..1])
        .await
        .unwrap();

    assert_eq!(check_trip(&db.pool, monday).await.unwrap(), Some(1));
    assert_eq!(check_trip(&db.pool, monday).await.unwrap(), Some(0));
}

#[tokio::test]
async fn us103_a_dismissed_proposal_comes_back_from_a_check_the_owner_starts() {
    let db = TestDb::new().await;
    let plan = a_trip(&db.pool, TripKind::Planned, along_the_stage(0.0)).await;
    let monday = a_trip(&db.pool, TripKind::Recorded, along_the_stage(10.0)).await;
    check_trip(&db.pool, monday).await.unwrap();
    let to_plan = pair(monday, LinkOther::Trip(plan));
    assert!(dismiss_proposals(&db.pool, &[to_plan]).await.unwrap());

    assert_eq!(check_trip(&db.pool, plan).await.unwrap(), Some(1));

    assert_eq!(proposed(&db.pool).await, vec![to_plan]);
}

#[tokio::test]
async fn us103_an_off_path_plan_is_proposed_by_the_points_it_was_judged_by() {
    let db = TestDb::new().await;
    // Five points 2 km apart, the trip straying 400 m between them.
    let corners: Vec<(f64, f64)> = (0..5).map(|i| (f64::from(i) * 2_000.0, 0.0)).collect();
    let plan = a_trip(
        &db.pool,
        TripKind::Planned,
        path(&corners).into_iter().step_by(100).collect(),
    )
    .await;
    let tour: Vec<(f64, f64)> = (0..9)
        .map(|i| {
            (
                f64::from(i) * 1_000.0,
                if i % 2 == 0 { 60.0 } else { 400.0 },
            )
        })
        .collect();
    let monday = a_trip(&db.pool, TripKind::Recorded, path(&tour)).await;

    assert_eq!(check_trip(&db.pool, monday).await.unwrap(), Some(1));

    let listed = list_proposals(&db.pool).await.unwrap();
    assert_eq!(listed[0].pair(), pair(monday, LinkOther::Trip(plan)));
    assert_eq!(listed[0].proposal.plan_covered, None);
    assert_eq!(listed[0].proposal.points_reached, Some(1.0));
}

#[tokio::test]
async fn us103_nothing_is_checked_for_what_does_not_exist() {
    let db = TestDb::new().await;

    assert_eq!(check_trip(&db.pool, 1).await.unwrap(), None);
    assert_eq!(check_collection(&db.pool, 1).await.unwrap(), None);
}
