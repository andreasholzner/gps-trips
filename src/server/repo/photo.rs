//! Photo association rows (US-2, US-3). Trip/track CRUD lives in the sibling
//! `trip` module.

use sqlx::{sqlite::SqliteRow, Row, Sqlite, SqlitePool, Transaction};
use time::{OffsetDateTime, UtcOffset};

use crate::models::{LocationSource, Photo};

use super::to_rfc3339;

/// Fields for a new `photo` row. The `created_at` timestamp is set on insert;
/// the image bytes themselves are written to the `BlobStore` under `blob_key`.
/// `lat`/`lon`/`location_source` place the photo on the map (US-3/US-4).
/// `thumbnail_key` (US-5) is `None` when thumbnail generation failed for this
/// photo — never blocks the insert.
pub struct NewPhoto<'a> {
    pub original_name: &'a str,
    pub content_type: Option<&'a str>,
    pub byte_len: i64,
    pub blob_key: &'a str,
    pub thumbnail_key: Option<&'a str>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub location_source: LocationSource,
    /// When the photo was taken (US-62), stored as UTC whatever offset it
    /// arrives in (ADR-0009).
    pub taken_at: Option<OffsetDateTime>,
}

/// Insert one photo row associating it with `trip_id` (US-2). Runs on the
/// caller's transaction so it commits atomically with the rest of an import.
/// Returns the new photo id.
pub async fn insert_photo(
    tx: &mut Transaction<'_, Sqlite>,
    trip_id: i64,
    photo: &NewPhoto<'_>,
) -> Result<i64, sqlx::Error> {
    let created_at = to_rfc3339(OffsetDateTime::now_utc());
    let id = sqlx::query(
        r#"INSERT INTO photo
               (trip_id, original_name, content_type, byte_len, blob_key, thumbnail_key,
                created_at, lat, lon, location_source, taken_at)
           VALUES (?,?,?,?,?,?,?,?,?,?,?)"#,
    )
    .bind(trip_id)
    .bind(photo.original_name)
    .bind(photo.content_type)
    .bind(photo.byte_len)
    .bind(photo.blob_key)
    .bind(photo.thumbnail_key)
    .bind(&created_at)
    .bind(photo.lat)
    .bind(photo.lon)
    .bind(photo.location_source)
    .bind(
        photo
            .taken_at
            .map(|t| to_rfc3339(t.to_offset(UtcOffset::UTC))),
    )
    .execute(&mut **tx)
    .await?
    .last_insert_rowid();
    Ok(id)
}

/// How many photos a trip already has — used to assign stable, non-colliding
/// blob keys when photos are appended to a trip later (US-2).
pub async fn count_photos(
    tx: &mut Transaction<'_, Sqlite>,
    trip_id: i64,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM photo WHERE trip_id = ?")
        .bind(trip_id)
        .fetch_one(&mut **tx)
        .await
}

/// Every column [`Photo`] carries, for the queries below to share.
const PHOTO_COLUMNS: &str = "id, trip_id, original_name, content_type, byte_len, blob_key, \
     thumbnail_key, created_at, lat, lon, location_source, taken_at";

/// List a trip's photos, oldest first (US-2). Reads only the `photo` table.
pub async fn list_photos(pool: &SqlitePool, trip_id: i64) -> Result<Vec<Photo>, sqlx::Error> {
    sqlx::query(&format!(
        "SELECT {PHOTO_COLUMNS} FROM photo WHERE trip_id = ? ORDER BY id"
    ))
    .bind(trip_id)
    .map(row_to_photo)
    .fetch_all(pool)
    .await
}

/// Every photo that has no capture time yet, grouped by trip — what the
/// US-62 backfill reads back out of the stored copies.
pub async fn list_photos_without_taken_at(pool: &SqlitePool) -> Result<Vec<Photo>, sqlx::Error> {
    sqlx::query(&format!(
        "SELECT {PHOTO_COLUMNS} FROM photo WHERE taken_at IS NULL ORDER BY trip_id, id"
    ))
    .map(row_to_photo)
    .fetch_all(pool)
    .await
}

/// Record when a photo was taken (US-62), as UTC (ADR-0009).
pub async fn set_photo_taken_at(
    pool: &SqlitePool,
    id: i64,
    taken_at: OffsetDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE photo SET taken_at = ? WHERE id = ?")
        .bind(to_rfc3339(taken_at.to_offset(UtcOffset::UTC)))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

fn row_to_photo(row: SqliteRow) -> Photo {
    Photo {
        id: row.get("id"),
        trip_id: row.get("trip_id"),
        original_name: row.get("original_name"),
        content_type: row.get("content_type"),
        byte_len: row.get("byte_len"),
        blob_key: row.get("blob_key"),
        thumbnail_key: row.get("thumbnail_key"),
        created_at: row.get("created_at"),
        lat: row.get("lat"),
        lon: row.get("lon"),
        location_source: row.get("location_source"),
        taken_at: row.get("taken_at"),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ActivityType, TripKind};
    use crate::server::db::testing::TestDb;
    use crate::server::geojson::build_track_geojson;
    use crate::server::gpx::{compute_stats, parse_gpx};
    use crate::server::repo::{insert_trip, NewTrip};

    const SAMPLE_GPX: &[u8] = include_bytes!("../../../tests/fixtures/sample.gpx");

    async fn insert_sample_trip(pool: &SqlitePool) -> i64 {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let stats = compute_stats(&track.points);
        let geojson = build_track_geojson(&track.points);
        insert_trip(
            pool,
            &NewTrip {
                name: "Oslo Hills Walk",
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

    async fn add_photo(pool: &SqlitePool, trip_id: i64, name: &str, key: &str, len: i64) -> i64 {
        let mut tx = pool.begin().await.unwrap();
        let id = insert_photo(
            &mut tx,
            trip_id,
            &NewPhoto {
                original_name: name,
                content_type: Some("image/jpeg"),
                byte_len: len,
                blob_key: key,
                thumbnail_key: None,
                lat: None,
                lon: None,
                location_source: LocationSource::None,
                taken_at: None,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        id
    }

    async fn add_geotagged_photo(
        pool: &SqlitePool,
        trip_id: i64,
        name: &str,
        key: &str,
        len: i64,
        lat: f64,
        lon: f64,
    ) -> i64 {
        let mut tx = pool.begin().await.unwrap();
        let id = insert_photo(
            &mut tx,
            trip_id,
            &NewPhoto {
                original_name: name,
                content_type: Some("image/jpeg"),
                byte_len: len,
                blob_key: key,
                thumbnail_key: None,
                lat: Some(lat),
                lon: Some(lon),
                location_source: LocationSource::Exif,
                taken_at: None,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        id
    }

    #[tokio::test]
    async fn us2_inserted_photo_is_listed_with_its_metadata() {
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;
        add_photo(
            &db.pool,
            trip_id,
            "beach.jpg",
            "trips/1/0000-beach.jpg",
            1234,
        )
        .await;

        let photos = list_photos(&db.pool, trip_id).await.unwrap();
        assert_eq!(photos.len(), 1);
        let p = &photos[0];
        assert_eq!(p.trip_id, trip_id);
        assert_eq!(p.original_name, "beach.jpg");
        assert_eq!(p.content_type.as_deref(), Some("image/jpeg"));
        assert_eq!(p.byte_len, 1234);
        assert_eq!(p.blob_key, "trips/1/0000-beach.jpg");
    }

    #[tokio::test]
    async fn us2_list_photos_is_empty_for_a_trip_without_photos() {
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;
        assert!(list_photos(&db.pool, trip_id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn us2_list_photos_returns_only_the_given_trips_photos() {
        let db = TestDb::new().await;
        let a = insert_sample_trip(&db.pool).await;
        let b = insert_sample_trip(&db.pool).await;
        add_photo(&db.pool, a, "a.jpg", "trips/a/0000-a.jpg", 10).await;
        add_photo(&db.pool, b, "b.jpg", "trips/b/0000-b.jpg", 20).await;

        let photos_a = list_photos(&db.pool, a).await.unwrap();
        assert_eq!(photos_a.len(), 1);
        assert_eq!(photos_a[0].original_name, "a.jpg");
    }

    #[tokio::test]
    async fn us2_count_photos_reflects_inserts() {
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;
        add_photo(&db.pool, trip_id, "a.jpg", "trips/1/0000-a.jpg", 10).await;
        add_photo(&db.pool, trip_id, "b.jpg", "trips/1/0001-b.jpg", 10).await;

        let mut tx = db.pool.begin().await.unwrap();
        assert_eq!(count_photos(&mut tx, trip_id).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn us2_deleting_a_trip_cascades_to_its_photos() {
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;
        add_photo(&db.pool, trip_id, "a.jpg", "trips/1/0000-a.jpg", 10).await;

        sqlx::query("DELETE FROM trip WHERE id = ?")
            .bind(trip_id)
            .execute(&db.pool)
            .await
            .unwrap();

        assert!(list_photos(&db.pool, trip_id).await.unwrap().is_empty());
    }

    // ── US-3: photos with EXIF GPS appear on the map ─────────────────────

    #[tokio::test]
    async fn us3_insert_and_list_photo_round_trips_coordinates_and_location_source() {
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;
        add_geotagged_photo(
            &db.pool,
            trip_id,
            "beach.jpg",
            "trips/1/0000-beach.jpg",
            1234,
            45.5,
            10.26,
        )
        .await;

        let photos = list_photos(&db.pool, trip_id).await.unwrap();
        assert_eq!(photos.len(), 1);
        let p = &photos[0];
        assert_eq!(p.lat, Some(45.5));
        assert_eq!(p.lon, Some(10.26));
        assert_eq!(p.location_source, LocationSource::Exif);
    }

    #[tokio::test]
    async fn us3_list_photos_returns_none_lat_lon_for_non_geotagged_photo() {
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;
        add_photo(&db.pool, trip_id, "a.jpg", "trips/1/0000-a.jpg", 10).await;

        let photos = list_photos(&db.pool, trip_id).await.unwrap();
        assert_eq!(photos.len(), 1);
        let p = &photos[0];
        assert_eq!(p.lat, None);
        assert_eq!(p.lon, None);
        assert_eq!(p.location_source, LocationSource::None);
    }

    // ── US-5: generated thumbnails ────────────────────────────────────────

    #[tokio::test]
    async fn us62_a_photos_capture_time_is_stored_as_utc() {
        // Placement hands over an instant in whatever offset it read the clock
        // at; what is stored is UTC (ADR-0009).
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;

        let mut tx = db.pool.begin().await.unwrap();
        insert_photo(
            &mut tx,
            trip_id,
            &NewPhoto {
                original_name: "a.jpg",
                content_type: Some("image/jpeg"),
                byte_len: 10,
                blob_key: "trips/1/0000-a.jpg",
                thumbnail_key: None,
                lat: None,
                lon: None,
                location_source: LocationSource::None,
                taken_at: Some(time::macros::datetime!(2024-06-01 10:15 +02:00)),
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let photos = list_photos(&db.pool, trip_id).await.unwrap();
        assert_eq!(photos[0].taken_at.as_deref(), Some("2024-06-01T08:15:00Z"));
    }

    #[tokio::test]
    async fn us5_insert_and_list_photo_round_trips_the_thumbnail_key() {
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;

        let mut tx = db.pool.begin().await.unwrap();
        insert_photo(
            &mut tx,
            trip_id,
            &NewPhoto {
                original_name: "a.jpg",
                content_type: Some("image/jpeg"),
                byte_len: 10,
                blob_key: "trips/1/0000-a.jpg",
                thumbnail_key: Some("trips/1/thumbs/0000-a.jpg"),
                lat: None,
                lon: None,
                location_source: LocationSource::None,
                taken_at: None,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let photos = list_photos(&db.pool, trip_id).await.unwrap();
        assert_eq!(
            photos[0].thumbnail_key.as_deref(),
            Some("trips/1/thumbs/0000-a.jpg")
        );
    }

    #[tokio::test]
    async fn us5_list_photos_returns_none_thumbnail_key_when_generation_failed() {
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;
        add_photo(&db.pool, trip_id, "a.jpg", "trips/1/0000-a.jpg", 10).await;

        let photos = list_photos(&db.pool, trip_id).await.unwrap();
        assert_eq!(photos[0].thumbnail_key, None);
    }
}
