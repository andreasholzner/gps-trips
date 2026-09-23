//! US-66 — find the trips still to tidy up: those without a proper name, and
//! those with photos left unplaced. A sibling of `filter.rs` rather than part
//! of it, to keep both under the repo's 500-line cap.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::models::LocationSource;
use crate::server::repo::{insert_photo, NewPhoto};

async fn trip_named(pool: &SqlitePool, name: &str) -> i64 {
    insert_trip(
        pool,
        &NewTrip {
            name,
            activity_type: ActivityType::Hiking,
            tz_name: "Europe/Oslo",
            stats: &stats_at(datetime!(2024-01-01 08:00 UTC)),
            geojson: "{}",
            gpx: b"x",
            trip_kind: TripKind::Recorded,
        },
    )
    .await
    .unwrap()
}

async fn add_photo(pool: &SqlitePool, trip_id: i64, location_source: LocationSource) {
    // `blob_key` is unique, and a trip may hold several photos of one kind.
    static NEXT_KEY: AtomicUsize = AtomicUsize::new(0);
    let key = NEXT_KEY.fetch_add(1, Ordering::Relaxed).to_string();
    let placed = location_source != LocationSource::None;
    let mut tx = pool.begin().await.unwrap();
    insert_photo(
        &mut tx,
        trip_id,
        &NewPhoto {
            original_name: "p.jpg",
            content_type: Some("image/jpeg"),
            byte_len: 1,
            blob_key: &key,
            thumbnail_key: None,
            lat: placed.then_some(59.9),
            lon: placed.then_some(10.7),
            location_source,
            taken_at: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

async fn names(pool: &SqlitePool, filter: &TripFilter) -> Vec<String> {
    let mut names: Vec<_> = list_trips(pool, filter)
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn us66_the_unnamed_filter_keeps_trips_whose_name_does_not_lead_with_a_dated_title() {
    let db = TestDb::new().await;
    for name in [
        "2024-06-01 Oslo Hills Walk",
        "2024-06-01Hike",
        "Evening ride",
        "2024-06-01",
        "2024-06-01   ",
        "2024-13-45 Impossible date",
        "24-06-01 Short year",
    ] {
        trip_named(&db.pool, name).await;
    }

    let filter = TripFilter {
        unnamed: true,
        ..Default::default()
    };
    assert_eq!(
        names(&db.pool, &filter).await,
        vec![
            "2024-06-01",
            "2024-06-01   ",
            "2024-13-45 Impossible date",
            "24-06-01 Short year",
            "Evening ride",
        ]
    );
}

#[tokio::test]
async fn us66_the_unplaced_filter_keeps_trips_with_at_least_one_unplaced_photo() {
    let db = TestDb::new().await;
    trip_named(&db.pool, "2024-01-01 No photos").await;
    let all_placed = trip_named(&db.pool, "2024-01-01 All placed").await;
    let one_unplaced = trip_named(&db.pool, "2024-01-01 One unplaced").await;
    for source in [
        LocationSource::Exif,
        LocationSource::Interpolated,
        LocationSource::Provided,
    ] {
        add_photo(&db.pool, all_placed, source).await;
    }
    add_photo(&db.pool, one_unplaced, LocationSource::Exif).await;
    add_photo(&db.pool, one_unplaced, LocationSource::None).await;
    add_photo(&db.pool, one_unplaced, LocationSource::None).await;

    let filter = TripFilter {
        unplaced_photos: true,
        ..Default::default()
    };
    // Listed once, however many of its photos are unplaced.
    assert_eq!(
        names(&db.pool, &filter).await,
        vec!["2024-01-01 One unplaced"]
    );
}

#[tokio::test]
async fn us66_both_filters_together_keep_only_trips_matching_both() {
    let db = TestDb::new().await;
    let named_unplaced = trip_named(&db.pool, "2024-01-01 Named").await;
    let unnamed_unplaced = trip_named(&db.pool, "Unnamed with photo").await;
    trip_named(&db.pool, "Unnamed without photo").await;
    add_photo(&db.pool, named_unplaced, LocationSource::None).await;
    add_photo(&db.pool, unnamed_unplaced, LocationSource::None).await;

    let filter = TripFilter {
        unnamed: true,
        unplaced_photos: true,
        ..Default::default()
    };
    assert_eq!(names(&db.pool, &filter).await, vec!["Unnamed with photo"]);
}
