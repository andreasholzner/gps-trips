use sqlx::{sqlite::SqliteRow, Row, Sqlite, SqlitePool, Transaction};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::models::{Photo, TripDetail, TripSummary};
use crate::server::gpx::TrackStats;

/// Insert a new trip together with its derived geometry and the original GPX
/// file, in a single transaction (ADR-0003). Returns the new trip id.
pub async fn insert_trip(
    pool: &SqlitePool,
    name: &str,
    activity_type: &str,
    stats: &TrackStats,
    geojson: &str,
    gpx: &[u8],
) -> Result<i64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let trip_id = insert_trip_in_tx(&mut tx, name, activity_type, stats, geojson, gpx).await?;
    tx.commit().await?;
    Ok(trip_id)
}

/// Insert the trip + track rows on an existing transaction, without committing.
/// Import drives this directly so the trip, its track and its photos all land in
/// one transaction — a failed import leaves no partial trip (ADR-0004).
pub async fn insert_trip_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    name: &str,
    activity_type: &str,
    stats: &TrackStats,
    geojson: &str,
    gpx: &[u8],
) -> Result<i64, sqlx::Error> {
    let created_at = to_rfc3339(OffsetDateTime::now_utc());
    let start_time = stats.start_time.map(to_rfc3339);
    let end_time = stats.end_time.map(to_rfc3339);

    let trip_id = sqlx::query(
        r#"INSERT INTO trip
               (name, activity_type, start_time, end_time, duration_secs,
                distance_m, ascent_m, descent_m,
                min_lat, min_lon, max_lat, max_lon,
                created_at)
           VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)"#,
    )
    .bind(name)
    .bind(activity_type)
    .bind(&start_time)
    .bind(&end_time)
    .bind(stats.duration_secs)
    .bind(stats.distance_m)
    .bind(stats.ascent_m)
    .bind(stats.descent_m)
    .bind(stats.min_lat)
    .bind(stats.min_lon)
    .bind(stats.max_lat)
    .bind(stats.max_lon)
    .bind(&created_at)
    .execute(&mut **tx)
    .await?
    .last_insert_rowid();

    sqlx::query("INSERT INTO track (trip_id, geojson, gpx) VALUES (?,?,?)")
        .bind(trip_id)
        .bind(geojson)
        .bind(gpx)
        .execute(&mut **tx)
        .await?;

    Ok(trip_id)
}

/// Fields for a new `photo` row. The `created_at` timestamp is set on insert;
/// the image bytes themselves are written to the `BlobStore` under `blob_key`.
pub struct NewPhoto<'a> {
    pub original_name: &'a str,
    pub content_type: Option<&'a str>,
    pub byte_len: i64,
    pub blob_key: &'a str,
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
        r#"INSERT INTO photo (trip_id, original_name, content_type, byte_len, blob_key, created_at)
           VALUES (?,?,?,?,?,?)"#,
    )
    .bind(trip_id)
    .bind(photo.original_name)
    .bind(photo.content_type)
    .bind(photo.byte_len)
    .bind(photo.blob_key)
    .bind(&created_at)
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

/// List a trip's photos, oldest first (US-2). Reads only the `photo` table.
pub async fn list_photos(pool: &SqlitePool, trip_id: i64) -> Result<Vec<Photo>, sqlx::Error> {
    sqlx::query(
        r#"SELECT id, trip_id, original_name, content_type, byte_len, blob_key, created_at
           FROM photo WHERE trip_id = ? ORDER BY id"#,
    )
    .bind(trip_id)
    .map(|row: SqliteRow| Photo {
        id: row.get("id"),
        trip_id: row.get("trip_id"),
        original_name: row.get("original_name"),
        content_type: row.get("content_type"),
        byte_len: row.get("byte_len"),
        blob_key: row.get("blob_key"),
        created_at: row.get("created_at"),
    })
    .fetch_all(pool)
    .await
}

/// The original GPX file plus the trip name, for serving a download (US-21).
pub struct GpxDownload {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Fetch the original GPX of a trip together with its name (for the download
/// filename), or `None` if no such trip exists. One query via the 1:1 join.
pub async fn get_original_gpx(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<GpxDownload>, sqlx::Error> {
    sqlx::query(
        r#"SELECT t.name AS name, k.gpx AS gpx
           FROM trip t JOIN track k ON k.trip_id = t.id
           WHERE t.id = ?"#,
    )
    .bind(id)
    .map(|row: SqliteRow| GpxDownload {
        name: row.get("name"),
        bytes: row.get("gpx"),
    })
    .fetch_optional(pool)
    .await
}

/// Format a timestamp as RFC-3339 for storage. Formatting a valid `OffsetDateTime`
/// with the well-known RFC-3339 description cannot fail, so a failure is a bug.
fn to_rfc3339(t: OffsetDateTime) -> String {
    t.format(&Rfc3339)
        .expect("RFC-3339 formatting of a valid OffsetDateTime never fails")
}

/// List all trips as lightweight summaries, most recent first (US-6).
/// Reads only the `trip` table — never the track geometry — so it stays cheap.
/// `start_time` may be NULL (GPX without times); SQLite sorts NULLs last under DESC.
pub async fn list_trips(pool: &SqlitePool) -> Result<Vec<TripSummary>, sqlx::Error> {
    sqlx::query(
        r#"SELECT id, name, start_time, distance_m, ascent_m, duration_secs
           FROM trip
           ORDER BY start_time DESC, id DESC"#,
    )
    .map(|row: SqliteRow| TripSummary {
        id: row.get("id"),
        name: row.get("name"),
        start_time: row.get("start_time"),
        distance_m: row.get("distance_m"),
        ascent_m: row.get("ascent_m"),
        duration_secs: row.get("duration_secs"),
    })
    .fetch_all(pool)
    .await
}

/// Fetch a trip's track geometry as the stored GeoJSON string (US-7), or `None`
/// if the trip has no track. Reads only the `track` table; the blob is returned
/// verbatim so the map and elevation chart share a single fetch (ADR-0005/0006).
pub async fn get_track_geojson(pool: &SqlitePool, id: i64) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT geojson FROM track WHERE trip_id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Fetch full trip detail by id, or `None` if no such trip exists.
pub async fn get_trip(pool: &SqlitePool, id: i64) -> Result<Option<TripDetail>, sqlx::Error> {
    sqlx::query(
        r#"SELECT id, name, activity_type, start_time, end_time,
                  distance_m, ascent_m, descent_m, duration_secs,
                  min_lat, min_lon, max_lat, max_lon
           FROM trip WHERE id = ?"#,
    )
    .bind(id)
    .map(row_to_detail)
    .fetch_optional(pool)
    .await
}

fn row_to_detail(row: SqliteRow) -> TripDetail {
    TripDetail {
        id: row.get("id"),
        name: row.get("name"),
        activity_type: row.get("activity_type"),
        start_time: row.get("start_time"),
        end_time: row.get("end_time"),
        distance_m: row.get("distance_m"),
        ascent_m: row.get("ascent_m"),
        descent_m: row.get("descent_m"),
        duration_secs: row.get("duration_secs"),
        min_lat: row.get("min_lat"),
        min_lon: row.get("min_lon"),
        max_lat: row.get("max_lat"),
        max_lon: row.get("max_lon"),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::db::testing::TestDb;
    use crate::server::geojson::build_track_geojson;
    use crate::server::gpx::{compute_stats, parse_gpx};
    use time::macros::datetime;

    const SAMPLE_GPX: &[u8] = include_bytes!("../../tests/fixtures/sample.gpx");

    async fn insert_sample_trip(pool: &SqlitePool) -> i64 {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let stats = compute_stats(&track.points);
        let geojson = build_track_geojson(&track.points);
        insert_trip(
            pool,
            "Oslo Hills Walk",
            "hiking",
            &stats,
            &geojson,
            SAMPLE_GPX,
        )
        .await
        .expect("insert_trip")
    }

    /// Minimal stats with a chosen start time, for ordering tests.
    fn stats_at(start: OffsetDateTime) -> TrackStats {
        TrackStats {
            distance_m: 1_000.0,
            ascent_m: 10.0,
            descent_m: 5.0,
            duration_secs: Some(600),
            start_time: Some(start),
            end_time: Some(start),
            min_lat: 0.0,
            min_lon: 0.0,
            max_lat: 0.0,
            max_lon: 0.0,
        }
    }

    #[tokio::test]
    async fn us1_insert_trip_returns_positive_id() {
        let db = TestDb::new().await;
        let id = insert_sample_trip(&db.pool).await;
        assert!(id > 0);
    }

    #[tokio::test]
    async fn us1_inserted_trip_can_be_retrieved_by_id() {
        let db = TestDb::new().await;
        let id = insert_sample_trip(&db.pool).await;
        let detail = get_trip(&db.pool, id).await.unwrap().expect("trip exists");
        assert_eq!(detail.id, id);
        assert_eq!(detail.name, "Oslo Hills Walk");
        assert_eq!(detail.activity_type, "hiking");
    }

    #[tokio::test]
    async fn us1_inserted_trip_stores_derived_stats() {
        let db = TestDb::new().await;
        let id = insert_sample_trip(&db.pool).await;
        let detail = get_trip(&db.pool, id).await.unwrap().unwrap();
        assert!(
            detail.distance_m > 1_000.0 && detail.distance_m < 2_500.0,
            "distance {:.1} m out of expected range",
            detail.distance_m
        );
    }

    #[tokio::test]
    async fn us1_inserted_trip_stores_track_geojson_blob() {
        let db = TestDb::new().await;
        let id = insert_sample_trip(&db.pool).await;
        let geojson: String = sqlx::query_scalar("SELECT geojson FROM track WHERE trip_id = ?")
            .bind(id)
            .fetch_one(&db.pool)
            .await
            .expect("track row exists");
        let parsed: serde_json::Value = serde_json::from_str(&geojson).unwrap();
        assert_eq!(parsed["geometry"]["type"], "LineString");
    }

    #[tokio::test]
    async fn us21_stores_and_returns_original_gpx_verbatim() {
        let db = TestDb::new().await;
        let id = insert_sample_trip(&db.pool).await;
        let download = get_original_gpx(&db.pool, id)
            .await
            .unwrap()
            .expect("original GPX exists");
        assert_eq!(download.name, "Oslo Hills Walk");
        assert_eq!(
            download.bytes, SAMPLE_GPX,
            "stored GPX must match the uploaded bytes exactly"
        );
    }

    #[tokio::test]
    async fn us21_original_gpx_is_none_for_unknown_trip() {
        let db = TestDb::new().await;
        assert!(get_original_gpx(&db.pool, 999).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn us1_deleting_trip_cascades_to_track() {
        let db = TestDb::new().await;
        let id = insert_sample_trip(&db.pool).await;
        sqlx::query("DELETE FROM trip WHERE id = ?")
            .bind(id)
            .execute(&db.pool)
            .await
            .unwrap();
        let remaining: Option<String> =
            sqlx::query_scalar("SELECT geojson FROM track WHERE trip_id = ?")
                .bind(id)
                .fetch_optional(&db.pool)
                .await
                .unwrap();
        assert!(
            remaining.is_none(),
            "cascade delete should remove the track row"
        );
    }

    // ── US-7: relive a trip (track geometry for the map + elevation chart) ────

    #[tokio::test]
    async fn us7_get_track_geojson_returns_stored_blob() {
        let db = TestDb::new().await;
        let id = insert_sample_trip(&db.pool).await;
        let geojson = get_track_geojson(&db.pool, id)
            .await
            .unwrap()
            .expect("track geometry exists");
        let parsed: serde_json::Value = serde_json::from_str(&geojson).unwrap();
        assert_eq!(parsed["geometry"]["type"], "LineString");
    }

    #[tokio::test]
    async fn us7_get_track_geojson_is_none_for_unknown_trip() {
        let db = TestDb::new().await;
        assert!(get_track_geojson(&db.pool, 999).await.unwrap().is_none());
    }

    // ── US-6: browse the trip list ───────────────────────────────────────────

    #[tokio::test]
    async fn us6_list_trips_is_empty_for_a_new_db() {
        let db = TestDb::new().await;
        assert!(list_trips(&db.pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn us6_list_trips_returns_summary_fields() {
        let db = TestDb::new().await;
        insert_sample_trip(&db.pool).await;
        let trips = list_trips(&db.pool).await.unwrap();
        assert_eq!(trips.len(), 1);
        let t = &trips[0];
        assert_eq!(t.name, "Oslo Hills Walk");
        assert!(t.distance_m > 1_000.0);
        assert_eq!(t.ascent_m, Some(40.0));
        assert_eq!(t.duration_secs, Some(3600));
        assert!(t.start_time.as_deref().unwrap().starts_with("2024-06-01"));
    }

    #[tokio::test]
    async fn us6_list_trips_orders_most_recent_first() {
        let db = TestDb::new().await;
        insert_trip(
            &db.pool,
            "Older",
            "hiking",
            &stats_at(datetime!(2024-01-01 08:00 UTC)),
            "{}",
            b"x",
        )
        .await
        .unwrap();
        insert_trip(
            &db.pool,
            "Newer",
            "hiking",
            &stats_at(datetime!(2024-06-01 08:00 UTC)),
            "{}",
            b"x",
        )
        .await
        .unwrap();

        let trips = list_trips(&db.pool).await.unwrap();
        assert_eq!(trips[0].name, "Newer");
        assert_eq!(trips[1].name, "Older");
    }

    // ── US-2: attach photos to a trip ────────────────────────────────────────

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

    #[tokio::test]
    async fn us6_list_trips_does_not_require_track_geometry() {
        let db = TestDb::new().await;
        let id = insert_sample_trip(&db.pool).await;
        // Remove the geometry; the list must still work (it reads only `trip`).
        sqlx::query("DELETE FROM track WHERE trip_id = ?")
            .bind(id)
            .execute(&db.pool)
            .await
            .unwrap();
        let trips = list_trips(&db.pool).await.unwrap();
        assert_eq!(trips.len(), 1, "list must not depend on the track row");
    }
}
