use sqlx::{sqlite::SqliteRow, Row, SqlitePool};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::models::TripDetail;
use crate::server::gpx::TrackStats;

/// Insert a new trip and its track blob in a single transaction (ADR-0003).
/// Returns the new trip id.
pub async fn insert_trip(
    pool: &SqlitePool,
    name: &str,
    activity_type: &str,
    stats: &TrackStats,
    geojson: &str,
) -> Result<i64, sqlx::Error> {
    let created_at = to_rfc3339(OffsetDateTime::now_utc());
    let start_time = stats.start_time.map(to_rfc3339);
    let end_time = stats.end_time.map(to_rfc3339);

    let mut tx = pool.begin().await?;

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
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();

    sqlx::query("INSERT INTO track (trip_id, geojson) VALUES (?,?)")
        .bind(trip_id)
        .bind(geojson)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(trip_id)
}

/// Format a timestamp as RFC-3339 for storage. Formatting a valid `OffsetDateTime`
/// with the well-known RFC-3339 description cannot fail, so a failure is a bug.
fn to_rfc3339(t: OffsetDateTime) -> String {
    t.format(&Rfc3339)
        .expect("RFC-3339 formatting of a valid OffsetDateTime never fails")
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

    const SAMPLE_GPX: &[u8] = include_bytes!("../../tests/fixtures/sample.gpx");

    async fn insert_sample_trip(pool: &SqlitePool) -> i64 {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let stats = compute_stats(&track.points);
        let geojson = build_track_geojson(&track.points);
        insert_trip(pool, "Oslo Hills Walk", "hiking", &stats, &geojson)
            .await
            .expect("insert_trip")
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
}
