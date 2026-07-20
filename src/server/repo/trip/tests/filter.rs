//! US-13 — filter the trip list by activity type, date interval, distance,
//! and free search of the name. Split out of the parent `tests.rs` (US-1/
//! US-6/US-7/US-9/US-15/US-21's tests) purely to keep that file under the
//! repo's 500-line cap.

use super::*;

async fn insert_trip_with(
    pool: &SqlitePool,
    name: &str,
    activity_type: ActivityType,
    distance_m: f64,
    start: OffsetDateTime,
) -> i64 {
    insert_trip(
        pool,
        name,
        activity_type,
        "Europe/Oslo",
        &stats(distance_m, start),
        "{}",
        b"x",
        TripKind::Recorded,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn us13_min_dist_is_inclusive_of_the_boundary() {
    let db = TestDb::new().await;
    let t = datetime!(2024-01-01 08:00 UTC);
    insert_trip_with(&db.pool, "Short", ActivityType::Hiking, 4_999.0, t).await;
    insert_trip_with(&db.pool, "AtBoundary", ActivityType::Hiking, 5_000.0, t).await;
    insert_trip_with(&db.pool, "Longer", ActivityType::Hiking, 5_001.0, t).await;

    let filter = TripFilter {
        min_dist_m: Some(5_000.0),
        ..Default::default()
    };
    let names: Vec<_> = list_trips(&db.pool, &filter)
        .await
        .unwrap()
        .iter()
        .map(|t| t.name.clone())
        .collect();
    assert_eq!(names, vec!["Longer", "AtBoundary"]);
}

#[tokio::test]
async fn us13_max_dist_is_inclusive_of_the_boundary() {
    let db = TestDb::new().await;
    let t = datetime!(2024-01-01 08:00 UTC);
    insert_trip_with(&db.pool, "Shorter", ActivityType::Hiking, 4_999.0, t).await;
    insert_trip_with(&db.pool, "AtBoundary", ActivityType::Hiking, 5_000.0, t).await;
    insert_trip_with(&db.pool, "Longer", ActivityType::Hiking, 5_001.0, t).await;

    let filter = TripFilter {
        max_dist_m: Some(5_000.0),
        ..Default::default()
    };
    let names: Vec<_> = list_trips(&db.pool, &filter)
        .await
        .unwrap()
        .iter()
        .map(|t| t.name.clone())
        .collect();
    assert_eq!(names, vec!["AtBoundary", "Shorter"]);
}

#[tokio::test]
async fn us13_date_range_is_inclusive_of_both_boundaries() {
    let db = TestDb::new().await;
    insert_trip_with(
        &db.pool,
        "Before",
        ActivityType::Hiking,
        1_000.0,
        datetime!(2024-05-31 23:59 UTC),
    )
    .await;
    insert_trip_with(
        &db.pool,
        "From",
        ActivityType::Hiking,
        1_000.0,
        datetime!(2024-06-01 00:00 UTC),
    )
    .await;
    insert_trip_with(
        &db.pool,
        "To",
        ActivityType::Hiking,
        1_000.0,
        datetime!(2024-06-05 23:59 UTC),
    )
    .await;
    insert_trip_with(
        &db.pool,
        "After",
        ActivityType::Hiking,
        1_000.0,
        datetime!(2024-06-06 00:00 UTC),
    )
    .await;

    let filter = TripFilter {
        from: Some("2024-06-01".to_string()),
        to: Some("2024-06-05".to_string()),
        ..Default::default()
    };
    let names: Vec<_> = list_trips(&db.pool, &filter)
        .await
        .unwrap()
        .iter()
        .map(|t| t.name.clone())
        .collect();
    assert_eq!(names, vec!["To", "From"]);
}

#[tokio::test]
async fn us13_activity_type_filter_matches_only_that_activity() {
    let db = TestDb::new().await;
    let t = datetime!(2024-01-01 08:00 UTC);
    insert_trip_with(&db.pool, "Bike", ActivityType::Cycling, 1_000.0, t).await;
    insert_trip_with(&db.pool, "Hike", ActivityType::Hiking, 1_000.0, t).await;

    let filter = TripFilter {
        activity_type: Some(ActivityType::Cycling),
        ..Default::default()
    };
    let trips = list_trips(&db.pool, &filter).await.unwrap();
    assert_eq!(trips.len(), 1);
    assert_eq!(trips[0].name, "Bike");
}

#[tokio::test]
async fn us13_default_filter_returns_every_activity_type() {
    let db = TestDb::new().await;
    let t = datetime!(2024-01-01 08:00 UTC);
    insert_trip_with(&db.pool, "Bike", ActivityType::Cycling, 1_000.0, t).await;
    insert_trip_with(&db.pool, "Hike", ActivityType::Hiking, 1_000.0, t).await;

    let trips = list_trips(&db.pool, &TripFilter::default()).await.unwrap();
    assert_eq!(trips.len(), 2);
}

#[tokio::test]
async fn us13_name_search_is_case_insensitive_substring() {
    let db = TestDb::new().await;
    let t = datetime!(2024-01-01 08:00 UTC);
    insert_trip_with(
        &db.pool,
        "Oslo Hills Walk",
        ActivityType::Hiking,
        1_000.0,
        t,
    )
    .await;
    insert_trip_with(
        &db.pool,
        "Bergen Fjord Ride",
        ActivityType::Cycling,
        1_000.0,
        t,
    )
    .await;

    let filter = TripFilter {
        name_query: Some("oslo".to_string()),
        ..Default::default()
    };
    let trips = list_trips(&db.pool, &filter).await.unwrap();
    assert_eq!(trips.len(), 1);
    assert_eq!(trips[0].name, "Oslo Hills Walk");
}

#[tokio::test]
async fn us13_name_search_treats_percent_and_underscore_literally() {
    let db = TestDb::new().await;
    insert_trip_with(
        &db.pool,
        "50% Effort",
        ActivityType::Hiking,
        1_000.0,
        datetime!(2024-01-01 08:00 UTC),
    )
    .await;
    insert_trip_with(
        &db.pool,
        "50X Effort",
        ActivityType::Hiking,
        1_000.0,
        datetime!(2024-01-02 08:00 UTC),
    )
    .await;

    let filter = TripFilter {
        name_query: Some("50%".to_string()),
        ..Default::default()
    };
    let trips = list_trips(&db.pool, &filter).await.unwrap();
    assert_eq!(
        trips.len(),
        1,
        "'%' in the query must match literally, not as a wildcard"
    );
    assert_eq!(trips[0].name, "50% Effort");
}

#[tokio::test]
async fn us13_combining_filters_is_and_not_or() {
    let db = TestDb::new().await;
    let t = datetime!(2024-01-01 08:00 UTC);
    insert_trip_with(&db.pool, "Short Hike", ActivityType::Hiking, 1_000.0, t).await;
    insert_trip_with(&db.pool, "Long Hike", ActivityType::Hiking, 20_000.0, t).await;
    insert_trip_with(&db.pool, "Long Ride", ActivityType::Cycling, 20_000.0, t).await;

    let filter = TripFilter {
        activity_type: Some(ActivityType::Hiking),
        min_dist_m: Some(10_000.0),
        ..Default::default()
    };
    let trips = list_trips(&db.pool, &filter).await.unwrap();
    assert_eq!(trips.len(), 1);
    assert_eq!(trips[0].name, "Long Hike");
}
