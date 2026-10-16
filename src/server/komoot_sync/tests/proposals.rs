//! A tour synced from Komoot enters the archive like an imported trip, and
//! is checked for links to propose (US-103) — by "Sync now" and by a
//! backfill alike, both of which import through `sync_one_tour`.

use super::*;
use crate::server::repo::{count_proposals, insert_trip, NewTrip};

#[tokio::test]
async fn us103_a_synced_tour_is_checked_for_links_to_propose() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    let track = gpx::parse_gpx(SAMPLE_GPX).unwrap();
    insert_trip(
        &db.pool,
        &NewTrip {
            name: "Plan",
            activity_type: ActivityType::Hiking,
            tz_name: "Europe/Oslo",
            stats: &gpx::compute_stats(&track.points),
            geojson: &crate::server::geojson::build_track_geojson(&track.points),
            gpx: SAMPLE_GPX,
            trip_kind: TripKind::Planned,
        },
    )
    .await
    .unwrap();
    let mut gpx = HashMap::new();
    gpx.insert("111".to_string(), SAMPLE_GPX.to_vec());
    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![a_tour("111", "Followed the plan", "hike")],
        gpx,
        ..Default::default()
    });

    let summary = sync_selected_tours(&db.pool, &store, client, &recorded_sel(&["111"]))
        .await
        .unwrap();

    assert!(summary.failed.is_none());
    assert_eq!(count_proposals(&db.pool).await.unwrap(), 1);
}

#[tokio::test]
async fn us103_a_tour_whose_check_fails_is_still_synced() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    // A trip whose stored track has no line cannot be judged.
    insert_trip(
        &db.pool,
        &NewTrip {
            name: "Lineless",
            activity_type: ActivityType::Hiking,
            tz_name: "UTC",
            stats: &gpx::compute_stats(&[]),
            geojson: "{}",
            gpx: b"x",
            trip_kind: TripKind::Planned,
        },
    )
    .await
    .unwrap();
    let mut gpx = HashMap::new();
    gpx.insert("111".to_string(), SAMPLE_GPX.to_vec());
    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![a_tour("111", "Walk", "hike")],
        gpx,
        ..Default::default()
    });

    let summary = sync_selected_tours(&db.pool, &store, client, &recorded_sel(&["111"]))
        .await
        .unwrap();

    assert_eq!(summary.imported.len(), 1);
}
