//! US-66 — find the trips still to tidy up: those without a proper name, and
//! those with photos left unplaced; US-101 — or with photos Komoot parked at
//! the track's start or end; US-106 — and which of a trip's photos those
//! are, by the same rule. A sibling of `filter.rs` rather than part of it,
//! to keep both under the repo's 500-line cap.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::models::LocationSource;
use crate::server::repo::{insert_photo, photos_to_place, place_photo, NewPhoto};

async fn trip_named(pool: &SqlitePool, name: &str) -> i64 {
    trip_with_geojson(pool, name, "{}").await
}

/// A track from `START` over a midpoint to `END`, as `[lon, lat, ele]`.
const START: (f64, f64) = (59.90, 10.70);
const MIDDLE: (f64, f64) = (59.92, 10.75);
const END: (f64, f64) = (59.95, 10.80);
const ELSEWHERE: (f64, f64) = (61.00, 12.00);
const TRACK: &str = r#"{"type":"Feature","properties":{},"geometry":{"type":"LineString",
    "coordinates":[[10.70,59.90,100.0],[10.75,59.92,120.0],[10.80,59.95,110.0]]}}"#;

async fn trip_with_track(pool: &SqlitePool, name: &str) -> i64 {
    trip_with_geojson(pool, name, TRACK).await
}

async fn trip_with_geojson(pool: &SqlitePool, name: &str, geojson: &str) -> i64 {
    insert_trip(
        pool,
        &NewTrip {
            name,
            activity_type: ActivityType::Hiking,
            tz_name: "Europe/Oslo",
            stats: &stats_at(datetime!(2024-01-01 08:00 UTC)),
            geojson,
            gpx: b"x",
            trip_kind: TripKind::Recorded,
        },
    )
    .await
    .unwrap()
}

/// A placed photo goes far off `TRACK`'s ends, so whether it counts as
/// parked (US-101) never depends on the trip it is added to.
async fn add_photo(pool: &SqlitePool, trip_id: i64, location_source: LocationSource) -> i64 {
    let placed = location_source != LocationSource::None;
    add_photo_at(pool, trip_id, location_source, placed.then_some(ELSEWHERE)).await
}

async fn add_photo_at(
    pool: &SqlitePool,
    trip_id: i64,
    location_source: LocationSource,
    position: Option<(f64, f64)>,
) -> i64 {
    // `blob_key` is unique, and a trip may hold several photos of one kind.
    static NEXT_KEY: AtomicUsize = AtomicUsize::new(0);
    let key = NEXT_KEY.fetch_add(1, Ordering::Relaxed).to_string();
    let mut tx = pool.begin().await.unwrap();
    let id = insert_photo(
        &mut tx,
        trip_id,
        &NewPhoto {
            original_name: "p.jpg",
            content_type: Some("image/jpeg"),
            byte_len: 1,
            blob_key: &key,
            thumbnail_key: None,
            lat: position.map(|(lat, _)| lat),
            lon: position.map(|(_, lon)| lon),
            location_source,
            taken_at: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

fn unplaced_filter() -> TripFilter {
    TripFilter {
        unplaced_photos: true,
        ..Default::default()
    }
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

/// `meters` north of `position` — a degree of latitude is about 111.2 km.
fn north_of((lat, lon): (f64, f64), meters: f64) -> (f64, f64) {
    (lat + meters / 111_195.0, lon)
}

#[tokio::test]
async fn us101_a_photo_komoot_provided_at_the_start_or_end_counts_as_unplaced() {
    let db = TestDb::new().await;
    let at_start = trip_with_track(&db.pool, "2024-01-01 At start").await;
    let at_end = trip_with_track(&db.pool, "2024-01-01 At end").await;
    let in_middle = trip_with_track(&db.pool, "2024-01-01 In middle").await;
    add_photo_at(&db.pool, at_start, LocationSource::Provided, Some(START)).await;
    add_photo_at(&db.pool, at_end, LocationSource::Provided, Some(END)).await;
    add_photo_at(&db.pool, in_middle, LocationSource::Provided, Some(MIDDLE)).await;

    assert_eq!(
        names(&db.pool, &unplaced_filter()).await,
        vec!["2024-01-01 At end", "2024-01-01 At start"]
    );
}

#[tokio::test]
async fn us101_at_allows_the_configured_distance_and_no_more() {
    let reach = crate::config::unplaced_photos::PARKED_REACH_M;
    let db = TestDb::new().await;
    let near = trip_with_track(&db.pool, "2024-01-01 Near").await;
    let far = trip_with_track(&db.pool, "2024-01-01 Far").await;
    let near_position = north_of(START, reach - 10.0);
    let far_position = north_of(END, reach + 10.0);
    add_photo_at(
        &db.pool,
        near,
        LocationSource::Provided,
        Some(near_position),
    )
    .await;
    add_photo_at(&db.pool, far, LocationSource::Provided, Some(far_position)).await;

    assert_eq!(
        names(&db.pool, &unplaced_filter()).await,
        vec!["2024-01-01 Near"]
    );
}

#[tokio::test]
async fn us101_positions_placed_otherwise_never_count_even_at_the_start_or_end() {
    let db = TestDb::new().await;
    let trip = trip_with_track(&db.pool, "2024-01-01 Meant").await;
    for source in [
        LocationSource::Exif,
        LocationSource::Interpolated,
        LocationSource::Manual,
    ] {
        add_photo_at(&db.pool, trip, source, Some(START)).await;
        add_photo_at(&db.pool, trip, source, Some(END)).await;
    }

    assert!(names(&db.pool, &unplaced_filter()).await.is_empty());
}

#[tokio::test]
async fn us101_a_parked_photo_placed_by_hand_no_longer_counts() {
    let db = TestDb::new().await;
    let trip = trip_with_track(&db.pool, "2024-01-01 Placed").await;
    let photo = add_photo_at(&db.pool, trip, LocationSource::Provided, Some(START)).await;
    // Placed by hand right where it was: the owner meant it there.
    place_photo(&db.pool, trip, photo, START.0, START.1)
        .await
        .unwrap()
        .unwrap();

    assert!(names(&db.pool, &unplaced_filter()).await.is_empty());
}

#[tokio::test]
async fn us101_a_trip_without_a_track_is_matched_only_on_unplaced_photos() {
    let db = TestDb::new().await;
    let provided = trip_with_track(&db.pool, "2024-01-01 Provided").await;
    let unplaced = trip_with_track(&db.pool, "2024-01-01 Unplaced").await;
    sqlx::query("DELETE FROM track")
        .execute(&db.pool)
        .await
        .unwrap();
    add_photo_at(&db.pool, provided, LocationSource::Provided, Some(START)).await;
    add_photo(&db.pool, unplaced, LocationSource::None).await;

    assert_eq!(
        names(&db.pool, &unplaced_filter()).await,
        vec!["2024-01-01 Unplaced"]
    );
}

#[tokio::test]
async fn us101_a_trip_with_parked_and_unplaced_photos_is_listed_once() {
    let db = TestDb::new().await;
    let trip = trip_with_track(&db.pool, "2024-01-01 Both").await;
    add_photo_at(&db.pool, trip, LocationSource::Provided, Some(START)).await;
    add_photo_at(&db.pool, trip, LocationSource::Provided, Some(END)).await;
    add_photo(&db.pool, trip, LocationSource::None).await;

    assert_eq!(
        names(&db.pool, &unplaced_filter()).await,
        vec!["2024-01-01 Both"]
    );
}

#[tokio::test]
async fn us101_parked_photos_still_combine_with_the_other_filters() {
    let db = TestDb::new().await;
    let named = trip_with_track(&db.pool, "2024-01-01 Named").await;
    let unnamed = trip_with_track(&db.pool, "Unnamed").await;
    add_photo_at(&db.pool, named, LocationSource::Provided, Some(START)).await;
    add_photo_at(&db.pool, unnamed, LocationSource::Provided, Some(END)).await;

    let filter = TripFilter {
        unnamed: true,
        ..unplaced_filter()
    };
    assert_eq!(names(&db.pool, &filter).await, vec!["Unnamed"]);
}

#[tokio::test]
async fn us101_a_malformed_track_is_skipped_rather_than_failing_the_list() {
    let db = TestDb::new().await;
    // A Point's coordinates: `[0]` is a number, not a position.
    let point = r#"{"type":"Feature","properties":{},
        "geometry":{"type":"Point","coordinates":[10.70,59.90]}}"#;
    let malformed = trip_with_geojson(&db.pool, "2024-01-01 Malformed", point).await;
    let unplaced = trip_with_track(&db.pool, "2024-01-01 Unplaced").await;
    add_photo_at(&db.pool, malformed, LocationSource::Provided, Some(START)).await;
    add_photo(&db.pool, unplaced, LocationSource::None).await;

    assert_eq!(
        names(&db.pool, &unplaced_filter()).await,
        vec!["2024-01-01 Unplaced"]
    );
}

#[tokio::test]
async fn us101_only_the_trips_asked_about_are_looked_at() {
    let db = TestDb::new().await;
    let parked = trip_with_track(&db.pool, "2024-01-01 Parked").await;
    let unplaced = trip_with_track(&db.pool, "2024-01-01 Unplaced").await;
    let left_out = trip_with_track(&db.pool, "2024-01-01 Left out").await;
    add_photo_at(&db.pool, parked, LocationSource::Provided, Some(END)).await;
    add_photo(&db.pool, unplaced, LocationSource::None).await;
    add_photo_at(&db.pool, left_out, LocationSource::Provided, Some(START)).await;

    let mut conn = db.pool.acquire().await.unwrap();
    let to_place = trips_with_photos_to_place(&mut conn, &[parked, unplaced])
        .await
        .unwrap();
    assert_eq!(to_place, HashSet::from([parked, unplaced]));
}

// ── US-106: which of a trip's photos are still to place ─────────────────

#[tokio::test]
async fn us106_the_photos_to_place_are_the_unplaced_ones_and_those_parked_at_either_end() {
    let db = TestDb::new().await;
    let trip = trip_with_track(&db.pool, "2024-01-01 Mixed").await;
    let unplaced = add_photo(&db.pool, trip, LocationSource::None).await;
    let at_start = add_photo_at(&db.pool, trip, LocationSource::Provided, Some(START)).await;
    let at_end = add_photo_at(&db.pool, trip, LocationSource::Provided, Some(END)).await;
    add_photo_at(&db.pool, trip, LocationSource::Provided, Some(MIDDLE)).await;
    // Near the start, but meant there.
    for source in [
        LocationSource::Exif,
        LocationSource::Interpolated,
        LocationSource::Manual,
    ] {
        add_photo_at(&db.pool, trip, source, Some(START)).await;
    }

    assert_eq!(
        photos_to_place(&db.pool, trip).await.unwrap(),
        HashSet::from([unplaced, at_start, at_end])
    );
}

#[tokio::test]
async fn us106_a_parked_photo_placed_by_hand_is_no_longer_to_place() {
    let db = TestDb::new().await;
    let trip = trip_with_track(&db.pool, "2024-01-01 Placed").await;
    let photo = add_photo_at(&db.pool, trip, LocationSource::Provided, Some(END)).await;
    place_photo(&db.pool, trip, photo, END.0, END.1)
        .await
        .unwrap()
        .unwrap();

    assert!(photos_to_place(&db.pool, trip).await.unwrap().is_empty());
}

#[tokio::test]
async fn us106_on_a_trip_without_a_track_only_unplaced_photos_are_to_place() {
    let db = TestDb::new().await;
    let trip = trip_with_track(&db.pool, "2024-01-01 Trackless").await;
    sqlx::query("DELETE FROM track")
        .execute(&db.pool)
        .await
        .unwrap();
    add_photo_at(&db.pool, trip, LocationSource::Provided, Some(START)).await;
    let unplaced = add_photo(&db.pool, trip, LocationSource::None).await;

    assert_eq!(
        photos_to_place(&db.pool, trip).await.unwrap(),
        HashSet::from([unplaced])
    );
}

#[tokio::test]
async fn us106_only_the_trips_own_photos_are_to_place() {
    let db = TestDb::new().await;
    let trip = trip_with_track(&db.pool, "2024-01-01 Own").await;
    let other = trip_with_track(&db.pool, "2024-01-01 Other").await;
    let own = add_photo(&db.pool, trip, LocationSource::None).await;
    add_photo(&db.pool, other, LocationSource::None).await;
    add_photo_at(&db.pool, other, LocationSource::Provided, Some(START)).await;

    assert_eq!(
        photos_to_place(&db.pool, trip).await.unwrap(),
        HashSet::from([own])
    );
}

#[tokio::test]
async fn us106_the_filter_lists_exactly_the_trips_with_a_photo_to_place() {
    // The badge and the filter must never disagree: the trips the filter
    // keeps are those with at least one photo the gallery marks.
    let db = TestDb::new().await;
    let mut trips = Vec::new();
    for (name, source, position) in [
        ("2024-01-01 Unplaced", LocationSource::None, None),
        ("2024-01-01 At start", LocationSource::Provided, Some(START)),
        ("2024-01-01 At end", LocationSource::Provided, Some(END)),
        (
            "2024-01-01 In middle",
            LocationSource::Provided,
            Some(MIDDLE),
        ),
        ("2024-01-01 From EXIF", LocationSource::Exif, Some(START)),
        ("2024-01-01 By hand", LocationSource::Manual, Some(END)),
    ] {
        let trip = trip_with_track(&db.pool, name).await;
        add_photo_at(&db.pool, trip, source, position).await;
        trips.push((name.to_string(), trip));
    }

    let mut marked = Vec::new();
    for (name, trip) in trips {
        if !photos_to_place(&db.pool, trip).await.unwrap().is_empty() {
            marked.push(name);
        }
    }
    marked.sort();
    assert_eq!(names(&db.pool, &unplaced_filter()).await, marked);
}
