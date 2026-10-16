//! US-103 — proposals kept as links marked as proposed, against a real
//! SQLite file (ADR-0012).

use super::*;
use crate::models::{LinkOther, LinkPair};
use crate::server::db::testing::TestDb;
use crate::server::repo::link::candidates::*;
use crate::server::repo::link::tests::{a_collection, planned, recorded, started};
use crate::server::repo::{link_selection, list_links, LinkEnd};

const COVERED: Proposal = Proposal {
    plan_covered: Some(0.92),
    trip_on_plan: Some(0.85),
    points_reached: None,
};

fn pair(trip_id: i64, other: LinkOther) -> LinkPair {
    LinkPair { trip_id, other }
}

/// Around the sample track every trip and collection track here follows.
fn around_the_sample() -> Extent {
    Extent::around([(59.9139, 10.7522), (59.925, 10.765)]).unwrap()
}

#[tokio::test]
async fn us103_a_proposal_waits_with_its_coverage_until_confirmed() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let plan = planned(&db.pool, "Plan").await;

    let new = propose(
        &db.pool,
        &[(
            ProposalPair::Plan {
                recorded: monday,
                planned: plan,
            },
            COVERED,
        )],
    )
    .await
    .unwrap();

    assert_eq!(new, 1);
    assert_eq!(count_proposals(&db.pool).await.unwrap(), 1);
    let listed = list_proposals(&db.pool).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].trip.id, monday);
    assert_eq!(listed[0].trip_kind, TripKind::Recorded);
    assert!(matches!(&listed[0].other, ProposedEnd::PlannedTrip(trip) if trip.id == plan));
    assert_eq!(listed[0].proposal, COVERED);
    assert_eq!(listed[0].pair(), pair(monday, LinkOther::Trip(plan)));

    assert!(confirm_proposals(&db.pool, &[listed[0].pair()])
        .await
        .unwrap());

    assert_eq!(count_proposals(&db.pool).await.unwrap(), 0);
    let links = list_links(&db.pool, LinkEnd::Trip(monday))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(links.planned_trips[0].proposal, None);
}

#[tokio::test]
async fn us103_proposing_never_undoes_a_link_nor_proposes_a_pair_twice() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let (_, tracks, waypoints) = a_collection(&db.pool, "Book", &["Stage 1", "Stage 2"]).await;
    link_selection(&db.pool, &[monday], &tracks[..1])
        .await
        .unwrap();
    let proposals = [
        (
            ProposalPair::Track {
                trip: monday,
                track: tracks[0],
            },
            COVERED,
        ),
        (
            ProposalPair::Track {
                trip: monday,
                track: tracks[1],
            },
            COVERED,
        ),
        (
            ProposalPair::Waypoint {
                trip: monday,
                waypoint: waypoints[0],
            },
            Proposal::default(),
        ),
    ];

    assert_eq!(propose(&db.pool, &proposals).await.unwrap(), 2);
    assert_eq!(propose(&db.pool, &proposals).await.unwrap(), 0);

    assert_eq!(count_proposals(&db.pool).await.unwrap(), 2);
    let links = list_links(&db.pool, LinkEnd::Trip(monday))
        .await
        .unwrap()
        .unwrap();
    let linked: Vec<(i64, bool)> = links
        .collection_tracks
        .iter()
        .map(|track| (track.id, track.proposal.is_some()))
        .collect();
    assert!(linked.contains(&(tracks[0], false)), "{linked:?}");
}

#[tokio::test]
async fn us103_confirming_or_dismissing_is_all_or_nothing() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let (_, tracks, waypoints) = a_collection(&db.pool, "Book", &["Stage 1", "Stage 2"]).await;
    propose(
        &db.pool,
        &[
            (
                ProposalPair::Track {
                    trip: monday,
                    track: tracks[0],
                },
                COVERED,
            ),
            (
                ProposalPair::Waypoint {
                    trip: monday,
                    waypoint: waypoints[0],
                },
                Proposal::default(),
            ),
        ],
    )
    .await
    .unwrap();
    let proposed = [
        pair(monday, LinkOther::CollectionTrack(tracks[0])),
        pair(monday, LinkOther::Waypoint(waypoints[0])),
    ];
    let not_proposed = pair(monday, LinkOther::CollectionTrack(tracks[1]));

    assert!(!confirm_proposals(&db.pool, &[proposed[0], not_proposed])
        .await
        .unwrap());
    assert!(!dismiss_proposals(&db.pool, &[proposed[1], not_proposed])
        .await
        .unwrap());
    assert_eq!(count_proposals(&db.pool).await.unwrap(), 2);

    assert!(dismiss_proposals(&db.pool, &proposed).await.unwrap());

    assert_eq!(count_proposals(&db.pool).await.unwrap(), 0);
    let links = list_links(&db.pool, LinkEnd::Trip(monday))
        .await
        .unwrap()
        .unwrap();
    assert!(links.collection_tracks.is_empty() && links.waypoints.is_empty());
    // Not remembered: proposed again, it is new.
    let again = [(
        ProposalPair::Track {
            trip: monday,
            track: tracks[0],
        },
        COVERED,
    )];
    assert_eq!(propose(&db.pool, &again).await.unwrap(), 1);
}

#[tokio::test]
async fn us103_a_proposal_between_trips_is_named_from_either_end() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let plan = planned(&db.pool, "Plan").await;
    let proposal = [(
        ProposalPair::Plan {
            recorded: monday,
            planned: plan,
        },
        COVERED,
    )];
    propose(&db.pool, &proposal).await.unwrap();

    assert!(
        confirm_proposals(&db.pool, &[pair(plan, LinkOther::Trip(monday))])
            .await
            .unwrap()
    );

    assert_eq!(count_proposals(&db.pool).await.unwrap(), 0);
}

#[tokio::test]
async fn us103_what_may_have_been_followed_is_found_by_its_extent() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let plan = planned(&db.pool, "Plan").await;
    let (_, tracks, waypoints) = a_collection(&db.pool, "Book", &["Stage 1", "Stage 2"]).await;
    let ids = |lines: Vec<Line>| lines.into_iter().map(|line| line.id).collect::<Vec<_>>();
    let far = Extent::around([(46.0, 9.0)]).unwrap().widened(1_000.0);

    let near = around_the_sample();
    assert_eq!(
        ids(trips_near(&db.pool, TripKind::Planned, near).await.unwrap()),
        vec![plan]
    );
    assert_eq!(
        ids(trips_near(&db.pool, TripKind::Recorded, near)
            .await
            .unwrap()),
        vec![monday]
    );
    assert_eq!(ids(tracks_near(&db.pool, near).await.unwrap()), tracks);
    assert!(waypoints_near(&db.pool, near).await.unwrap().is_empty());
    // The collection's waypoints lie at 60° N, 10° E.
    let at_waypoints = Extent::around([(60.0, 10.0)]).unwrap().widened(100.0);
    let places = waypoints_near(&db.pool, at_waypoints).await.unwrap();
    assert_eq!(places.iter().map(|p| p.id).collect::<Vec<_>>(), waypoints);
    assert!(trips_near(&db.pool, TripKind::Recorded, far)
        .await
        .unwrap()
        .is_empty());
    assert!(tracks_near(&db.pool, far).await.unwrap().is_empty());
}

#[tokio::test]
async fn us103_a_trip_and_collection_items_are_read_with_their_geometry() {
    let db = TestDb::new().await;
    let plan = planned(&db.pool, "Plan").await;
    let (collection, tracks, waypoints) = a_collection(&db.pool, "Book", &["Stage 1"]).await;

    let (kind, line) = trip_line(&db.pool, plan).await.unwrap().expect("exists");
    assert_eq!(kind, TripKind::Planned);
    assert_eq!(line.line.len(), 3);
    assert_eq!(line.line[0], (59.9139, 10.7522));
    assert!(trip_line(&db.pool, plan + 100).await.unwrap().is_none());

    let (track_ids, waypoint_ids) = collection_item_ids(&db.pool, collection)
        .await
        .unwrap()
        .expect("exists");
    assert_eq!(
        (track_ids.clone(), waypoint_ids.clone()),
        (tracks.clone(), waypoints.clone())
    );
    assert!(collection_item_ids(&db.pool, collection + 100)
        .await
        .unwrap()
        .is_none());
    let (lines, places) = collection_items(&db.pool, &[tracks[0], 999], &[waypoints[1], 999])
        .await
        .unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].line.len(), 3);
    assert_eq!(places.len(), 1);
    assert_eq!(places[0].at, (60.0, 10.0));
}

#[tokio::test]
async fn us103_a_replaced_items_proposals_are_forgotten_and_its_links_kept() {
    let db = TestDb::new().await;
    let monday = recorded(&db.pool, "Monday").await;
    let tuesday = recorded(&db.pool, "Tuesday").await;
    let (_, tracks, waypoints) = a_collection(&db.pool, "Book", &["Stage 1"]).await;
    link_selection(&db.pool, &[monday], &tracks).await.unwrap();
    propose(
        &db.pool,
        &[
            (
                ProposalPair::Track {
                    trip: tuesday,
                    track: tracks[0],
                },
                COVERED,
            ),
            (
                ProposalPair::Waypoint {
                    trip: tuesday,
                    waypoint: waypoints[0],
                },
                Proposal::default(),
            ),
        ],
    )
    .await
    .unwrap();

    forget_proposals_of(&db.pool, &tracks, &waypoints)
        .await
        .unwrap();

    assert_eq!(count_proposals(&db.pool).await.unwrap(), 0);
    let links = list_links(&db.pool, LinkEnd::CollectionTrack(tracks[0]))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(links.recorded_trips.len(), 1);
    assert_eq!(links.recorded_trips[0].id, monday);
}

#[tokio::test]
async fn us103_proposals_are_listed_by_their_trips_newest_first() {
    let db = TestDb::new().await;
    let older = recorded(&db.pool, "Older").await;
    let newer = recorded(&db.pool, "Newer").await;
    started(&db.pool, older, "2024-06-01T08:00:00Z").await;
    started(&db.pool, newer, "2024-07-01T08:00:00Z").await;
    let plan = planned(&db.pool, "Plan").await;
    let (_, tracks, waypoints) = a_collection(&db.pool, "Book", &["Stage 1"]).await;
    propose(
        &db.pool,
        &[
            (
                ProposalPair::Track {
                    trip: plan,
                    track: tracks[0],
                },
                COVERED,
            ),
            (
                ProposalPair::Track {
                    trip: older,
                    track: tracks[0],
                },
                COVERED,
            ),
            (
                ProposalPair::Waypoint {
                    trip: newer,
                    waypoint: waypoints[0],
                },
                Proposal::default(),
            ),
            (
                ProposalPair::Plan {
                    recorded: newer,
                    planned: plan,
                },
                COVERED,
            ),
        ],
    )
    .await
    .unwrap();

    let listed: Vec<LinkPair> = list_proposals(&db.pool)
        .await
        .unwrap()
        .iter()
        .map(ProposedLink::pair)
        .collect();

    assert_eq!(
        listed,
        vec![
            pair(newer, LinkOther::Trip(plan)),
            pair(newer, LinkOther::Waypoint(waypoints[0])),
            pair(older, LinkOther::CollectionTrack(tracks[0])),
            pair(plan, LinkOther::CollectionTrack(tracks[0])),
        ]
    );
}

#[test]
fn us103_an_extent_is_widened_by_metres() {
    let extent = Extent::around([(60.0, 10.0)]).unwrap().widened(111_195.0);

    assert!((extent.max_lat - 61.0).abs() < 1e-6);
    assert!((extent.min_lat - 59.0).abs() < 1e-6);
    // A degree of longitude at 61° N is about half a degree of latitude.
    assert!((extent.max_lon - 10.0 - 1.0 / 61f64.to_radians().cos()).abs() < 1e-6);
}
