//! US-14 — filter the trip list by a geographic region drawn on the map.
//! Bbox-overlap against the `trip` table's stored bounding-box columns, no
//! PostGIS (ADR-0011). A sibling of `filter.rs` (US-13's tests) so both stay
//! well under the repo's 500-line cap.

use super::*;
use crate::models::BoundingBox;

/// Insert a trip whose stored bounding box is exactly `bbox`, for the US-14
/// region tests — a fixed GPX fixture can only ever produce one box, and these
/// tests need trips placed at chosen positions relative to the region.
async fn insert_trip_at(pool: &SqlitePool, name: &str, bbox: BoundingBox) -> i64 {
    let mut stats = stats(1_000.0, datetime!(2024-01-01 08:00 UTC));
    stats.min_lat = bbox.min_lat;
    stats.min_lon = bbox.min_lon;
    stats.max_lat = bbox.max_lat;
    stats.max_lon = bbox.max_lon;

    insert_trip(
        pool,
        &NewTrip {
            name,
            activity_type: ActivityType::Hiking,
            tz_name: "Europe/Oslo",
            stats: &stats,
            geojson: "{}",
            gpx: b"x",
            trip_kind: TripKind::Recorded,
        },
    )
    .await
    .unwrap()
}

/// The region every US-14 test below selects: a 1°×1° box around (10..11 E,
/// 59..60 N).
fn region() -> BoundingBox {
    BoundingBox {
        min_lat: 59.0,
        min_lon: 10.0,
        max_lat: 60.0,
        max_lon: 11.0,
    }
}

fn bbox(min_lat: f64, min_lon: f64, max_lat: f64, max_lon: f64) -> BoundingBox {
    BoundingBox {
        min_lat,
        min_lon,
        max_lat,
        max_lon,
    }
}

async fn names_in_region(pool: &SqlitePool, region: BoundingBox) -> Vec<String> {
    let filter = TripFilter {
        region: Some(region),
        ..Default::default()
    };
    list_trips(pool, &filter)
        .await
        .unwrap()
        .iter()
        .map(|t| t.name.clone())
        .collect()
}

#[tokio::test]
async fn us14_a_trip_wholly_inside_the_region_matches() {
    let db = TestDb::new().await;
    insert_trip_at(&db.pool, "Inside", bbox(59.4, 10.4, 59.6, 10.6)).await;

    assert_eq!(names_in_region(&db.pool, region()).await, vec!["Inside"]);
}

#[tokio::test]
async fn us14_a_trip_overlapping_the_region_only_partly_matches() {
    let db = TestDb::new().await;
    // Crosses the region's western edge: overlaps on both axes, so it matches.
    insert_trip_at(&db.pool, "Straddling", bbox(59.4, 9.5, 59.6, 10.5)).await;

    assert_eq!(
        names_in_region(&db.pool, region()).await,
        vec!["Straddling"]
    );
}

#[tokio::test]
async fn us14_a_trip_enclosing_the_region_matches() {
    let db = TestDb::new().await;
    insert_trip_at(&db.pool, "Enclosing", bbox(50.0, 0.0, 70.0, 20.0)).await;

    assert_eq!(names_in_region(&db.pool, region()).await, vec!["Enclosing"]);
}

#[tokio::test]
async fn us14_a_trip_touching_the_region_edge_matches() {
    let db = TestDb::new().await;
    // Boxes that share only an edge count as overlapping — the same inclusive
    // boundary the distance and date filters use.
    insert_trip_at(&db.pool, "Touching", bbox(59.4, 9.0, 59.6, 10.0)).await;

    assert_eq!(names_in_region(&db.pool, region()).await, vec!["Touching"]);
}

#[tokio::test]
async fn us14_a_trip_outside_the_region_does_not_match() {
    let db = TestDb::new().await;
    insert_trip_at(&db.pool, "Inside", bbox(59.4, 10.4, 59.6, 10.6)).await;
    // Same latitudes, far to the east — a trip must overlap on *both* axes.
    insert_trip_at(&db.pool, "East", bbox(59.4, 20.4, 59.6, 20.6)).await;
    // Same longitudes, far to the south.
    insert_trip_at(&db.pool, "South", bbox(40.4, 10.4, 40.6, 10.6)).await;

    assert_eq!(names_in_region(&db.pool, region()).await, vec!["Inside"]);
}

#[tokio::test]
async fn us14_a_trip_without_a_bounding_box_never_matches() {
    let db = TestDb::new().await;
    let id = insert_trip_at(&db.pool, "Boxless", bbox(59.4, 10.4, 59.6, 10.6)).await;
    sqlx::query(
        "UPDATE trip SET min_lat = NULL, min_lon = NULL, max_lat = NULL, max_lon = NULL \
         WHERE id = ?",
    )
    .bind(id)
    .execute(&db.pool)
    .await
    .unwrap();

    assert!(names_in_region(&db.pool, region()).await.is_empty());
}

#[tokio::test]
async fn us14_a_zero_area_region_matches_the_trips_containing_that_point() {
    let db = TestDb::new().await;
    insert_trip_at(&db.pool, "Around", bbox(59.4, 10.4, 59.6, 10.6)).await;
    insert_trip_at(&db.pool, "Elsewhere", bbox(59.4, 20.4, 59.6, 20.6)).await;

    let point = bbox(59.5, 10.5, 59.5, 10.5);
    assert_eq!(names_in_region(&db.pool, point).await, vec!["Around"]);
}

#[tokio::test]
async fn us14_the_region_combines_with_other_filters_as_and() {
    let db = TestDb::new().await;
    let inside = bbox(59.4, 10.4, 59.6, 10.6);
    insert_trip_at(&db.pool, "Inside Hike", inside).await;
    insert_trip_at(&db.pool, "Outside Hike", bbox(59.4, 20.4, 59.6, 20.6)).await;

    let filter = TripFilter {
        region: Some(region()),
        name_query: Some("inside".to_string()),
        ..Default::default()
    };
    let trips = list_trips(&db.pool, &filter).await.unwrap();
    assert_eq!(trips.len(), 1);
    assert_eq!(trips[0].name, "Inside Hike");
}
