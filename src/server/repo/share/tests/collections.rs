//! US-94 — a share of collections or collection tracks: it reaches what
//! they hold whenever it is asked, and dies with the last thing it names.

use super::*;
use crate::server::repo::{
    delete_collection, delete_collection_track, insert_collection, NewCollection,
    NewCollectionFile, NewCollectionTrack, NewWaypoint,
};

/// A collection named `name` with the tracks `tracks` and a waypoint per
/// track, belonging to it, plus one belonging to the collection alone.
async fn a_collection(pool: &SqlitePool, name: &str, tracks: &[&str]) -> i64 {
    let track = parse_gpx(SAMPLE_GPX).unwrap();
    let stats = compute_stats(&track.points);
    let geojson = build_track_geojson(&track.points);
    let new_tracks: Vec<NewCollectionTrack> = tracks
        .iter()
        .map(|name| NewCollectionTrack {
            name,
            stats: &stats,
            geojson: &geojson,
            gpx: b"<gpx/>",
        })
        .collect();
    let names: Vec<String> = tracks.iter().map(|t| format!("{t} hut")).collect();
    let loose = format!("{name} pass");
    let mut waypoints: Vec<NewWaypoint> = names
        .iter()
        .enumerate()
        .map(|(index, name)| a_waypoint(name, Some(index)))
        .collect();
    waypoints.push(a_waypoint(&loose, None));
    insert_collection(
        pool,
        &NewCollection {
            name,
            description: "A guide book",
            activity_type: Some(ActivityType::Hiking),
            files: &[NewCollectionFile {
                file_name: "book.gpx",
                gpx: SAMPLE_GPX,
            }],
            tracks: &new_tracks,
            waypoints: &waypoints,
        },
    )
    .await
    .expect("insert_collection")
}

fn a_waypoint(name: &str, track: Option<usize>) -> NewWaypoint<'_> {
    NewWaypoint {
        name: Some(name),
        description: None,
        lat: 61.0,
        lon: 9.0,
        ele: None,
        symbol: None,
        track,
    }
}

async fn track_ids(pool: &SqlitePool, collection_id: i64) -> Vec<i64> {
    sqlx::query_scalar("SELECT id FROM collection_track WHERE collection_id = ? ORDER BY position")
        .bind(collection_id)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn share_of(pool: &SqlitePool, token: &str, target: ShareTarget<'_>) -> i64 {
    insert_share(
        pool,
        &NewShare {
            token,
            label: None,
            target,
            created_at: noon(),
            expires_at: None,
        },
    )
    .await
    .unwrap()
}

fn names<T>(items: &[T], name: impl Fn(&T) -> Option<&str>) -> Vec<String> {
    items
        .iter()
        .map(|item| name(item).unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn us94_a_share_of_collections_reaches_every_track_and_waypoint_they_hold() {
    let db = TestDb::new().await;
    let rondane = a_collection(&db.pool, "Rondane", &["R1", "R2"]).await;
    let jotunheimen = a_collection(&db.pool, "Jotunheimen", &["J1"]).await;
    let other = a_collection(&db.pool, "Other", &["O1"]).await;
    let share = share_of(
        &db.pool,
        "t",
        ShareTarget::Collections(&[rondane, jotunheimen]),
    )
    .await;

    let collections = list_shared_collections(&db.pool, share).await.unwrap();
    assert_eq!(
        names(&collections, |c| Some(&c.name)),
        ["Jotunheimen", "Rondane"]
    );
    assert_eq!(collections[1].track_count, 2);
    assert_eq!(collections[1].waypoint_count, 3);
    let tracks = list_shared_collection_tracks(&db.pool, share)
        .await
        .unwrap();
    assert_eq!(names(&tracks, |t| Some(&t.name)), ["J1", "R1", "R2"]);
    let waypoints = list_shared_waypoints(&db.pool, share).await.unwrap();
    assert_eq!(
        names(&waypoints, |w| w.name.as_deref()),
        [
            "J1 hut",
            "Jotunheimen pass",
            "R1 hut",
            "R2 hut",
            "Rondane pass"
        ]
    );

    assert!(share_covers_collection(&db.pool, share, rondane)
        .await
        .unwrap());
    assert!(!share_covers_collection(&db.pool, share, other)
        .await
        .unwrap());
    let other_track = track_ids(&db.pool, other).await[0];
    assert!(!share_covers_collection_track(&db.pool, share, other_track)
        .await
        .unwrap());
    assert!(shared_collection_track(&db.pool, share, other_track)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        list_shared_collection_waypoints(&db.pool, share, other)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        list_shared_collection_waypoints(&db.pool, share, rondane)
            .await
            .unwrap()
            .map(|waypoints| waypoints.len()),
        Some(3)
    );
    // Reached without waypoints is not the same as not reached.
    sqlx::query("DELETE FROM waypoint WHERE collection_id = ?")
        .bind(jotunheimen)
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        list_shared_collection_waypoints(&db.pool, share, jotunheimen)
            .await
            .unwrap(),
        Some(Vec::new())
    );
}

#[tokio::test]
async fn us94_a_shared_collection_reaches_a_track_added_later_and_not_a_deleted_one() {
    let db = TestDb::new().await;
    let rondane = a_collection(&db.pool, "Rondane", &["R1", "R2"]).await;
    let share = share_of(&db.pool, "t", ShareTarget::Collections(&[rondane])).await;
    let [r1, _] = track_ids(&db.pool, rondane).await[..] else {
        panic!("two tracks");
    };

    delete_collection_track(&db.pool, r1).await.unwrap();
    sqlx::query(
        "INSERT INTO collection_track (collection_id, position, name, distance_m, ascent_m, \
         descent_m, min_lat, min_lon, max_lat, max_lon, geojson, gpx) \
         SELECT collection_id, 3, 'R3', distance_m, ascent_m, descent_m, min_lat, min_lon, \
                max_lat, max_lon, geojson, gpx FROM collection_track WHERE collection_id = ?",
    )
    .bind(rondane)
    .execute(&db.pool)
    .await
    .unwrap();

    let tracks = list_shared_collection_tracks(&db.pool, share)
        .await
        .unwrap();
    assert_eq!(names(&tracks, |t| Some(&t.name)), ["R2", "R3"]);
    assert!(!share_covers_collection_track(&db.pool, share, r1)
        .await
        .unwrap());
}

#[tokio::test]
async fn us94_shared_tracks_from_several_collections_reach_their_own_waypoints_only() {
    let db = TestDb::new().await;
    let rondane = a_collection(&db.pool, "Rondane", &["R1", "R2"]).await;
    let jotunheimen = a_collection(&db.pool, "Jotunheimen", &["J1"]).await;
    let r1 = track_ids(&db.pool, rondane).await[0];
    let j1 = track_ids(&db.pool, jotunheimen).await[0];
    let share = share_of(&db.pool, "t", ShareTarget::CollectionTracks(&[r1, j1])).await;

    assert_eq!(list_shared_collections(&db.pool, share).await.unwrap(), []);
    let tracks = list_shared_collection_tracks(&db.pool, share)
        .await
        .unwrap();
    assert_eq!(names(&tracks, |t| Some(&t.name)), ["J1", "R1"]);
    let waypoints = list_shared_waypoints(&db.pool, share).await.unwrap();
    assert_eq!(
        names(&waypoints, |w| w.name.as_deref()),
        ["J1 hut", "R1 hut"]
    );
    assert!(!share_covers_collection(&db.pool, share, rondane)
        .await
        .unwrap());
    assert_eq!(
        list_shared_collection_waypoints(&db.pool, share, rondane)
            .await
            .unwrap(),
        None
    );

    let detail = shared_collection_track(&db.pool, share, r1)
        .await
        .unwrap()
        .expect("reached");
    assert_eq!(detail.track.name, "R1");
    assert_eq!(detail.track.activity_type, ActivityType::Hiking);
    assert_eq!(detail.collection, None, "its collection is not shared");
    let track_waypoints = list_shared_track_waypoints(&db.pool, share, r1)
        .await
        .unwrap()
        .expect("reached");
    assert_eq!(names(&track_waypoints, |w| w.name.as_deref()), ["R1 hut"]);
    let r2 = track_ids(&db.pool, rondane).await[1];
    assert_eq!(
        list_shared_track_waypoints(&db.pool, share, r2)
            .await
            .unwrap(),
        None
    );
    // Reached without waypoints is not the same as not reached.
    sqlx::query("DELETE FROM waypoint WHERE collection_track_id = ?")
        .bind(j1)
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        list_shared_track_waypoints(&db.pool, share, j1)
            .await
            .unwrap(),
        Some(Vec::new())
    );
}

#[tokio::test]
async fn us94_a_track_of_a_shared_collection_names_its_collection() {
    let db = TestDb::new().await;
    let rondane = a_collection(&db.pool, "Rondane", &["R1"]).await;
    let share = share_of(&db.pool, "t", ShareTarget::Collections(&[rondane])).await;
    let r1 = track_ids(&db.pool, rondane).await[0];

    let detail = shared_collection_track(&db.pool, share, r1)
        .await
        .unwrap()
        .expect("reached");
    assert_eq!(
        detail.collection,
        Some(crate::models::SharedCollectionName {
            id: rondane,
            name: "Rondane".to_string(),
        })
    );
}

#[tokio::test]
async fn us94_a_share_lives_until_the_last_collection_or_track_it_names_is_deleted() {
    let db = TestDb::new().await;
    let rondane = a_collection(&db.pool, "Rondane", &["R1"]).await;
    let jotunheimen = a_collection(&db.pool, "Jotunheimen", &["J1"]).await;
    let r1 = track_ids(&db.pool, rondane).await[0];
    share_of(
        &db.pool,
        "collections",
        ShareTarget::Collections(&[rondane, jotunheimen]),
    )
    .await;
    share_of(&db.pool, "tracks", ShareTarget::CollectionTracks(&[r1])).await;

    delete_collection(&db.pool, jotunheimen).await.unwrap();
    assert!(resolve_share(&db.pool, "collections", noon())
        .await
        .unwrap()
        .is_some());
    delete_collection_track(&db.pool, r1).await.unwrap();
    assert!(resolve_share(&db.pool, "tracks", noon())
        .await
        .unwrap()
        .is_none());
    delete_collection(&db.pool, rondane).await.unwrap();
    assert!(resolve_share(&db.pool, "collections", noon())
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn us94_the_owner_lists_what_a_share_of_collections_or_tracks_names_and_stops_it() {
    let db = TestDb::new().await;
    let rondane = a_collection(&db.pool, "Rondane", &["R1", "R2"]).await;
    let jotunheimen = a_collection(&db.pool, "Jotunheimen", &["J1"]).await;
    let [r1, r2] = track_ids(&db.pool, rondane).await[..] else {
        panic!("two tracks");
    };
    let j1 = track_ids(&db.pool, jotunheimen).await[0];
    let collections = share_of(
        &db.pool,
        "c",
        ShareTarget::Collections(&[rondane, jotunheimen]),
    )
    .await;
    let tracks = share_of(&db.pool, "k", ShareTarget::CollectionTracks(&[r2, r1, j1])).await;

    let listed = list_active_shares(&db.pool, noon()).await.unwrap();
    let of = |id: i64| listed.iter().find(|share| share.id == id).unwrap();
    assert_eq!(of(collections).collection_names, ["Jotunheimen", "Rondane"]);
    assert!(of(collections).collection_track_names.is_empty());
    assert_eq!(of(tracks).collection_track_names, ["J1", "R1", "R2"]);
    assert!(of(tracks).trip_names.is_empty());

    assert!(stop_share(&db.pool, collections, noon()).await.unwrap());
    assert!(resolve_share(&db.pool, "c", noon())
        .await
        .unwrap()
        .is_none());
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM share_collection")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn us94_a_share_of_collections_reaches_no_trip() {
    let db = TestDb::new().await;
    let trip = a_trip(&db.pool, "Walk").await;
    let rondane = a_collection(&db.pool, "Rondane", &["R1"]).await;
    let share = share_of(&db.pool, "t", ShareTarget::Collections(&[rondane])).await;

    assert!(list_shared_trips(&db.pool, share).await.unwrap().is_empty());
    assert!(!share_covers_trip(&db.pool, share, trip).await.unwrap());
}
