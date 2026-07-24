//! Transaction-scoped archive reads for the QMapShack exporter (US-36,
//! ADR-0022): the whole export run reads through one open transaction, so
//! under WAL it sees a single consistent snapshot of the archive no matter
//! what the server commits concurrently — the exporter-side replacement for
//! the in-process US-26 lock a separate CLI process can't take. Follows the
//! `insert_trip_in_tx` precedent for tx-scoped variants.

use sqlx::{sqlite::SqliteRow, Row, Sqlite, Transaction};

use crate::models::{ActivityType, Tag, TripKind};

/// Everything the exporter needs per trip besides geometry and tags.
#[derive(Debug)]
pub struct ExportTrip {
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    pub trip_kind: TripKind,
    /// RFC-3339 UTC (ADR-0009), `None` for trips whose GPX had no times.
    pub start_time: Option<String>,
    pub tz_name: Option<String>,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    pub duration_secs: Option<i64>,
}

/// Every trip in the archive, in id order — the exporter is full-library by
/// design (ADR-0022), so there is deliberately no filter parameter.
pub async fn list_trips_for_export(
    tx: &mut Transaction<'_, Sqlite>,
) -> Result<Vec<ExportTrip>, sqlx::Error> {
    sqlx::query(
        r#"SELECT id, name, activity_type, trip_kind, start_time, tz_name,
                  distance_m, ascent_m, descent_m, duration_secs
           FROM trip ORDER BY id"#,
    )
    .map(|row: SqliteRow| ExportTrip {
        id: row.get("id"),
        name: row.get("name"),
        activity_type: row.get("activity_type"),
        trip_kind: row.get("trip_kind"),
        start_time: row.get("start_time"),
        tz_name: row.get("tz_name"),
        distance_m: row.get("distance_m"),
        ascent_m: row.get("ascent_m"),
        descent_m: row.get("descent_m"),
        duration_secs: row.get("duration_secs"),
    })
    .fetch_all(&mut **tx)
    .await
}

/// Tx-scoped twin of `repo::get_track_geojson`.
pub async fn get_track_geojson_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    trip_id: i64,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT geojson FROM track WHERE trip_id = ?")
        .bind(trip_id)
        .fetch_optional(&mut **tx)
        .await
}

/// Tx-scoped twin of `repo::list_trip_tags` (same alphabetical order).
pub async fn list_trip_tags_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    trip_id: i64,
) -> Result<Vec<Tag>, sqlx::Error> {
    sqlx::query(
        r#"SELECT tag.id, tag.name FROM tag
           JOIN trip_tag ON trip_tag.tag_id = tag.id
           WHERE trip_tag.trip_id = ?
           ORDER BY tag.name"#,
    )
    .bind(trip_id)
    .map(|row: SqliteRow| Tag {
        id: row.get("id"),
        name: row.get("name"),
    })
    .fetch_all(&mut **tx)
    .await
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::db::testing::TestDb;
    use crate::server::geojson::build_track_geojson;
    use crate::server::gpx::{compute_stats, TrackPoint, TrackStats};
    use crate::server::repo::{self, NewTrip};
    use time::macros::datetime;

    async fn insert_trip_with(
        pool: &sqlx::SqlitePool,
        name: &str,
        kind: TripKind,
        start: Option<time::OffsetDateTime>,
    ) -> i64 {
        let points = [
            TrackPoint {
                lat: 59.91,
                lon: 10.75,
                ele: Some(100.0),
                time: start,
            },
            TrackPoint {
                lat: 59.92,
                lon: 10.76,
                ele: Some(120.0),
                time: start.map(|t| t + time::Duration::minutes(10)),
            },
        ];
        let stats = TrackStats {
            start_time: start,
            end_time: start.map(|t| t + time::Duration::minutes(10)),
            ..compute_stats(&points)
        };
        repo::insert_trip(
            pool,
            &NewTrip {
                name,
                activity_type: ActivityType::Hiking,
                tz_name: "Europe/Oslo",
                stats: &stats,
                geojson: &build_track_geojson(&points),
                gpx: b"<gpx/>",
                trip_kind: kind,
            },
        )
        .await
        .expect("insert trip")
    }

    #[tokio::test]
    async fn lists_every_trip_of_every_kind_with_its_fields() {
        let db = TestDb::new().await;
        let start = datetime!(2024-06-01 08:00:00 UTC);
        let recorded =
            insert_trip_with(&db.pool, "Recorded", TripKind::Recorded, Some(start)).await;
        let planned = insert_trip_with(&db.pool, "Planned", TripKind::Planned, None).await;

        let mut tx = db.pool.begin().await.unwrap();
        let trips = list_trips_for_export(&mut tx).await.unwrap();
        assert_eq!(trips.len(), 2, "planned trips are exported too");

        let first = trips.iter().find(|t| t.id == recorded).unwrap();
        assert_eq!(first.name, "Recorded");
        assert_eq!(first.trip_kind, TripKind::Recorded);
        assert_eq!(first.activity_type, ActivityType::Hiking);
        assert_eq!(first.start_time.as_deref(), Some("2024-06-01T08:00:00Z"));
        assert_eq!(first.tz_name.as_deref(), Some("Europe/Oslo"));
        assert!(first.distance_m > 0.0);
        assert_eq!(first.duration_secs, Some(600));

        let second = trips.iter().find(|t| t.id == planned).unwrap();
        assert_eq!(second.trip_kind, TripKind::Planned);
        assert_eq!(second.start_time, None);
    }

    #[tokio::test]
    async fn tx_scoped_reads_match_their_pool_based_counterparts() {
        let db = TestDb::new().await;
        let id = insert_trip_with(
            &db.pool,
            "Tagged",
            TripKind::Recorded,
            Some(datetime!(2024-06-01 08:00:00 UTC)),
        )
        .await;
        let tag_id = repo::get_or_create_tag(&db.pool, "fjell").await.unwrap();
        repo::add_trip_tag(&db.pool, id, tag_id).await.unwrap();

        let mut tx = db.pool.begin().await.unwrap();
        let geojson = get_track_geojson_in_tx(&mut tx, id).await.unwrap();
        let tags = list_trip_tags_in_tx(&mut tx, id).await.unwrap();
        drop(tx);

        assert_eq!(
            geojson,
            repo::get_track_geojson(&db.pool, id).await.unwrap(),
            "same geometry through the transaction"
        );
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "fjell");

        let mut tx = db.pool.begin().await.unwrap();
        assert_eq!(
            get_track_geojson_in_tx(&mut tx, 9999).await.unwrap(),
            None,
            "missing trip yields None, not an error"
        );
    }
}
