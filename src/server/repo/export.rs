//! The archive as the QMapShack exporter reads it (US-36, US-51): every
//! trip with its tags, for `GET /api/export/trips` (ADR-0022's 2026-09-19
//! amendment). Trips and tags are read in one transaction, so under WAL the
//! list is one consistent state of the archive however the server commits
//! meanwhile — the exporter's change detection and removal pass both rely on
//! it. The geometry is not part of it: the exporter fetches that per trip,
//! only for the trips it writes.

use std::collections::HashMap;

use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use crate::models::{ExportTrip, Tag};

/// Every trip in the archive, in id order, each with its tags in name order.
/// Full-library by design (ADR-0022): there is deliberately no filter.
pub async fn list_export_trips(pool: &SqlitePool) -> Result<Vec<ExportTrip>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mut tags: HashMap<i64, Vec<Tag>> = HashMap::new();
    let rows = sqlx::query(
        r#"SELECT trip_tag.trip_id, tag.id, tag.name FROM tag
           JOIN trip_tag ON trip_tag.tag_id = tag.id
           ORDER BY tag.name"#,
    )
    .fetch_all(&mut *tx)
    .await?;
    for row in rows {
        tags.entry(row.get("trip_id")).or_default().push(Tag {
            id: row.get("id"),
            name: row.get("name"),
        });
    }
    let trips = sqlx::query(
        r#"SELECT id, name, activity_type, trip_kind, start_time, tz_name,
                  distance_m, ascent_m, descent_m, duration_secs
           FROM trip ORDER BY id"#,
    )
    .map(|row: SqliteRow| {
        let id = row.get("id");
        ExportTrip {
            id,
            name: row.get("name"),
            activity_type: row.get("activity_type"),
            trip_kind: row.get("trip_kind"),
            start_time: row.get("start_time"),
            tz_name: row.get("tz_name"),
            distance_m: row.get("distance_m"),
            ascent_m: row.get("ascent_m"),
            descent_m: row.get("descent_m"),
            duration_secs: row.get("duration_secs"),
            tags: tags.remove(&id).unwrap_or_default(),
        }
    })
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(trips)
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ActivityType, TripKind};
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

        let trips = list_export_trips(&db.pool).await.unwrap();
        assert_eq!(trips.len(), 2, "planned trips are exported too");

        let first = trips.iter().find(|t| t.id == recorded).unwrap();
        assert_eq!(first.name, "Recorded");
        assert_eq!(first.trip_kind, TripKind::Recorded);
        assert_eq!(first.activity_type, ActivityType::Hiking);
        assert_eq!(first.start_time.as_deref(), Some("2024-06-01T08:00:00Z"));
        assert_eq!(first.tz_name.as_deref(), Some("Europe/Oslo"));
        assert!(first.distance_m > 0.0);
        assert_eq!(first.duration_secs, Some(600));
        assert!(first.tags.is_empty());

        let second = trips.iter().find(|t| t.id == planned).unwrap();
        assert_eq!(second.trip_kind, TripKind::Planned);
        assert_eq!(second.start_time, None);
    }

    #[tokio::test]
    async fn each_trip_carries_its_own_tags_in_name_order() {
        let db = TestDb::new().await;
        let start = Some(datetime!(2024-06-01 08:00:00 UTC));
        let first = insert_trip_with(&db.pool, "First", TripKind::Recorded, start).await;
        let second = insert_trip_with(&db.pool, "Second", TripKind::Recorded, start).await;
        for (trip, name) in [(first, "telt"), (first, "fjell"), (second, "bre")] {
            let tag_id = repo::get_or_create_tag(&db.pool, name).await.unwrap();
            repo::add_trip_tag(&db.pool, trip, tag_id).await.unwrap();
        }

        let trips = list_export_trips(&db.pool).await.unwrap();

        let names =
            |i: usize| -> Vec<&str> { trips[i].tags.iter().map(|t| t.name.as_str()).collect() };
        assert_eq!(names(0), ["fjell", "telt"]);
        assert_eq!(names(1), ["bre"]);
    }
}
