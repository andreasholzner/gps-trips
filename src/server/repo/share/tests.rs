//! US-53 — the share repository, against a real SQLite file (ADR-0012).

use super::*;
use crate::models::{ActivityType, LocationSource, TripKind};
use crate::server::db::testing::TestDb;
use crate::server::geojson::build_track_geojson;
use crate::server::gpx::{compute_stats, parse_gpx};
use crate::server::repo::{delete_trip, insert_photo, insert_trip, NewPhoto, NewTrip};

const SAMPLE_GPX: &[u8] = include_bytes!("../../../../tests/fixtures/sample.gpx");

async fn a_trip(pool: &SqlitePool, name: &str) -> i64 {
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
            trip_kind: TripKind::Recorded,
        },
    )
    .await
    .expect("insert_trip")
}

async fn a_photo(pool: &SqlitePool, trip_id: i64, key: &str, thumb: &str) {
    let mut tx = pool.begin().await.unwrap();
    insert_photo(
        &mut tx,
        trip_id,
        &NewPhoto {
            original_name: "a.jpg",
            content_type: Some("image/jpeg"),
            byte_len: 1,
            blob_key: key,
            thumbnail_key: Some(thumb),
            lat: None,
            lon: None,
            location_source: LocationSource::None,
            taken_at: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

fn noon() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap()
}

async fn a_share(
    pool: &SqlitePool,
    token: &str,
    trip_ids: &[i64],
    expires_at: Option<OffsetDateTime>,
) -> i64 {
    insert_share(
        pool,
        &NewShare {
            token,
            label: Some("For Kari"),
            trip_ids,
            created_at: noon(),
            expires_at,
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn us53_a_token_resolves_to_its_share() {
    let db = TestDb::new().await;
    let trip = a_trip(&db.pool, "Oslo").await;
    let id = a_share(&db.pool, "tok", &[trip], None).await;

    assert_eq!(
        resolve_share(&db.pool, "tok", noon()).await.unwrap(),
        Some(id)
    );
    assert_eq!(
        resolve_share(&db.pool, "other", noon()).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn us53_an_expired_share_does_not_resolve() {
    let db = TestDb::new().await;
    let trip = a_trip(&db.pool, "Oslo").await;
    let expires = noon() + time::Duration::seconds(1);
    let id = a_share(&db.pool, "tok", &[trip], Some(expires)).await;

    assert_eq!(
        resolve_share(&db.pool, "tok", noon()).await.unwrap(),
        Some(id)
    );
    assert_eq!(resolve_share(&db.pool, "tok", expires).await.unwrap(), None);
}

#[tokio::test]
async fn us53_deleting_a_trip_takes_it_out_of_the_share_and_an_emptied_share_ends() {
    let db = TestDb::new().await;
    let first = a_trip(&db.pool, "First").await;
    let second = a_trip(&db.pool, "Second").await;
    let id = a_share(&db.pool, "tok", &[first, second], None).await;

    delete_trip(&db.pool, first).await.unwrap();
    assert!(!share_covers_trip(&db.pool, id, first).await.unwrap());
    assert_eq!(
        resolve_share(&db.pool, "tok", noon()).await.unwrap(),
        Some(id)
    );

    delete_trip(&db.pool, second).await.unwrap();
    assert_eq!(resolve_share(&db.pool, "tok", noon()).await.unwrap(), None);
}

#[tokio::test]
async fn us53_a_deleted_share_row_ends_the_share() {
    // What stopping a share will do (US-69).
    let db = TestDb::new().await;
    let trip = a_trip(&db.pool, "Oslo").await;
    let id = a_share(&db.pool, "tok", &[trip], None).await;

    sqlx::query("DELETE FROM share WHERE id = ?")
        .bind(id)
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(resolve_share(&db.pool, "tok", noon()).await.unwrap(), None);
    assert!(!share_covers_trip(&db.pool, id, trip).await.unwrap());
}

#[tokio::test]
async fn us53_a_share_covers_only_the_trips_it_names() {
    let db = TestDb::new().await;
    let shared = a_trip(&db.pool, "Shared").await;
    let private = a_trip(&db.pool, "Private").await;
    let id = a_share(&db.pool, "tok", &[shared], None).await;

    assert!(share_covers_trip(&db.pool, id, shared).await.unwrap());
    assert!(!share_covers_trip(&db.pool, id, private).await.unwrap());
}

#[tokio::test]
async fn us53_a_share_covers_only_its_trips_photos() {
    let db = TestDb::new().await;
    let shared = a_trip(&db.pool, "Shared").await;
    let private = a_trip(&db.pool, "Private").await;
    a_photo(
        &db.pool,
        shared,
        "trips/1/1-a.jpg",
        "trips/1/thumbs/1-a.jpg",
    )
    .await;
    a_photo(
        &db.pool,
        private,
        "trips/2/1-b.jpg",
        "trips/2/thumbs/1-b.jpg",
    )
    .await;
    let id = a_share(&db.pool, "tok", &[shared], None).await;

    for key in ["trips/1/1-a.jpg", "trips/1/thumbs/1-a.jpg"] {
        assert!(share_covers_blob(&db.pool, id, key).await.unwrap(), "{key}");
    }
    for key in [
        "trips/2/1-b.jpg",
        "trips/2/thumbs/1-b.jpg",
        "trips/1/../2/1-b.jpg",
        "trips/1",
    ] {
        assert!(
            !share_covers_blob(&db.pool, id, key).await.unwrap(),
            "{key}"
        );
    }
}

#[tokio::test]
async fn us53_the_shared_trips_are_listed_with_the_label() {
    let db = TestDb::new().await;
    let first = a_trip(&db.pool, "First").await;
    let _other = a_trip(&db.pool, "Not shared").await;
    let second = a_trip(&db.pool, "Second").await;
    let id = a_share(&db.pool, "tok", &[second, first], None).await;

    assert_eq!(
        share_label(&db.pool, id).await.unwrap().as_deref(),
        Some("For Kari")
    );
    let names: Vec<String> = list_shared_trips(&db.pool, id)
        .await
        .unwrap()
        .into_iter()
        .map(|trip| trip.name)
        .collect();
    // Same start time (one fixture), so the id breaks the tie.
    assert_eq!(names, ["First", "Second"]);
}

#[tokio::test]
async fn us53_a_repeated_trip_is_stored_once() {
    let db = TestDb::new().await;
    let trip = a_trip(&db.pool, "Oslo").await;
    let id = a_share(&db.pool, "tok", &[trip, trip], None).await;
    assert_eq!(list_shared_trips(&db.pool, id).await.unwrap().len(), 1);
}
