//! The archive as the QMapShack exporter reads it (US-36, US-51): every
//! trip with its tags, for `GET /api/export/trips` (ADR-0022's 2026-09-19
//! amendment). Trips and tags are read in one transaction, so under WAL the
//! list is one consistent state of the archive however the server commits
//! meanwhile — the exporter's change detection and removal pass both rely on
//! it. The geometry is not part of it: the exporter fetches that per trip,
//! only for the trips it writes. The guide-book collections come likewise,
//! for `GET /api/export/collections` (US-89, the 2026-10-11 amendment).

use std::collections::HashMap;

use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use crate::models::{ExportCollection, ExportCollectionTrack, ExportTrip, ExportWaypoint, Tag};

use super::collection::TRACK_ACTIVITY;

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

/// Every collection, in id order, each with its tracks and waypoints in
/// file order — read in one transaction, and unfiltered, as the trips are.
pub async fn list_export_collections(
    pool: &SqlitePool,
) -> Result<Vec<ExportCollection>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mut tracks: HashMap<i64, Vec<ExportCollectionTrack>> = HashMap::new();
    let rows = sqlx::query(&format!(
        "SELECT k.collection_id, k.id, k.name, {TRACK_ACTIVITY} AS activity_type, \
                k.distance_m, k.ascent_m, k.descent_m, k.min_lat, k.min_lon, k.max_lat, k.max_lon \
         FROM collection_track k JOIN collection c ON c.id = k.collection_id \
         ORDER BY k.position"
    ))
    .fetch_all(&mut *tx)
    .await?;
    for row in rows {
        tracks
            .entry(row.get("collection_id"))
            .or_default()
            .push(ExportCollectionTrack {
                id: row.get("id"),
                name: row.get("name"),
                activity_type: row.get("activity_type"),
                distance_m: row.get("distance_m"),
                ascent_m: row.get("ascent_m"),
                descent_m: row.get("descent_m"),
                min_lat: row.get("min_lat"),
                min_lon: row.get("min_lon"),
                max_lat: row.get("max_lat"),
                max_lon: row.get("max_lon"),
            });
    }
    let mut waypoints: HashMap<i64, Vec<ExportWaypoint>> = HashMap::new();
    let rows = sqlx::query(
        "SELECT collection_id, id, position, name, description, lat, lon, ele, symbol \
         FROM waypoint ORDER BY position",
    )
    .fetch_all(&mut *tx)
    .await?;
    for row in rows {
        waypoints
            .entry(row.get("collection_id"))
            .or_default()
            .push(ExportWaypoint {
                id: row.get("id"),
                position: row.get("position"),
                name: row.get("name"),
                description: row.get("description"),
                lat: row.get("lat"),
                lon: row.get("lon"),
                ele: row.get("ele"),
                symbol: row.get("symbol"),
            });
    }
    let collections =
        sqlx::query("SELECT id, name, description, created_at FROM collection ORDER BY id")
            .map(|row: SqliteRow| {
                let id = row.get("id");
                ExportCollection {
                    id,
                    name: row.get("name"),
                    description: row.get("description"),
                    created_at: row.get("created_at"),
                    tracks: tracks.remove(&id).unwrap_or_default(),
                    waypoints: waypoints.remove(&id).unwrap_or_default(),
                }
            })
            .fetch_all(&mut *tx)
            .await?;
    tx.commit().await?;
    Ok(collections)
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

    mod collections {
        use super::super::*;
        use crate::models::ActivityType;
        use crate::server::db::testing::TestDb;
        use crate::server::geojson::build_track_geojson;
        use crate::server::gpx::{compute_stats, TrackPoint};
        use crate::server::repo::{
            insert_collection, update_collection_track, CollectionTrackChange, NewCollection,
            NewCollectionFile, NewCollectionTrack, NewWaypoint,
        };

        fn points(lat: f64) -> [TrackPoint; 2] {
            [
                TrackPoint {
                    lat,
                    lon: 8.6,
                    ele: Some(1000.0),
                    time: None,
                },
                TrackPoint {
                    lat: lat + 0.01,
                    lon: 8.62,
                    ele: Some(1100.0),
                    time: None,
                },
            ]
        }

        fn waypoint<'a>(name: Option<&'a str>, symbol: Option<&'a str>) -> NewWaypoint<'a> {
            NewWaypoint {
                name,
                description: Some("DNT"),
                lat: 61.48,
                lon: 8.62,
                ele: None,
                symbol,
                track: None,
            }
        }

        async fn insert(
            pool: &SqlitePool,
            name: &str,
            activity: Option<ActivityType>,
            track_names: &[&str],
            waypoints: &[NewWaypoint<'_>],
        ) -> i64 {
            let pts = points(61.4);
            let stats = compute_stats(&pts);
            let geojson = build_track_geojson(&pts);
            let tracks: Vec<NewCollectionTrack> = track_names
                .iter()
                .map(|name| NewCollectionTrack {
                    name,
                    stats: &stats,
                    geojson: &geojson,
                    gpx: b"<gpx/>",
                })
                .collect();
            insert_collection(
                pool,
                &NewCollection {
                    name,
                    description: "50 Touren",
                    activity_type: activity,
                    files: &[NewCollectionFile {
                        file_name: "book.gpx",
                        gpx: b"<gpx/>",
                    }],
                    tracks: &tracks,
                    waypoints,
                },
            )
            .await
            .expect("insert collection")
        }

        #[tokio::test]
        async fn us89_lists_every_collection_with_its_tracks_and_waypoints_in_file_order() {
            let db = TestDb::new().await;
            let book = insert(
                &db.pool,
                "Jotunheimen",
                Some(ActivityType::Hiking),
                &["Besseggen", "Galdhøpiggen"],
                &[
                    waypoint(Some("Gjendebu"), Some("Lodge")),
                    waypoint(None, None),
                ],
            )
            .await;
            let empty = insert(&db.pool, "Empty", None, &[], &[]).await;

            let collections = list_export_collections(&db.pool).await.unwrap();

            assert_eq!(collections.len(), 2);
            let first = &collections[0];
            assert_eq!(first.id, book);
            assert_eq!(first.name, "Jotunheimen");
            assert_eq!(first.description, "50 Touren");
            assert!(first.created_at.ends_with('Z'), "{}", first.created_at);
            let names: Vec<&str> = first.tracks.iter().map(|t| t.name.as_str()).collect();
            assert_eq!(names, ["Besseggen", "Galdhøpiggen"]);
            let track = &first.tracks[0];
            assert_eq!(track.activity_type, ActivityType::Hiking);
            assert!(track.distance_m > 0.0);
            assert_eq!((track.min_lat, track.max_lat), (61.4, 61.41));
            assert_eq!((track.min_lon, track.max_lon), (8.6, 8.62));
            assert_eq!(first.waypoints.len(), 2);
            let gjendebu = &first.waypoints[0];
            assert_eq!(gjendebu.position, 1);
            assert_eq!(gjendebu.name.as_deref(), Some("Gjendebu"));
            assert_eq!(gjendebu.description.as_deref(), Some("DNT"));
            assert_eq!(gjendebu.symbol.as_deref(), Some("Lodge"));
            assert_eq!(
                (gjendebu.lat, gjendebu.lon, gjendebu.ele),
                (61.48, 8.62, None)
            );
            assert_eq!(first.waypoints[1].position, 2);
            assert_eq!(first.waypoints[1].name, None);
            assert_eq!(first.waypoints[1].symbol, None);

            assert_eq!(collections[1].id, empty);
            assert!(collections[1].tracks.is_empty());
            assert!(collections[1].waypoints.is_empty());
        }

        #[tokio::test]
        async fn us89_a_track_goes_by_its_own_activity_else_its_collections_else_unknown() {
            let db = TestDb::new().await;
            insert(&db.pool, "Book", None, &["Plain", "Own"], &[]).await;
            let collections = list_export_collections(&db.pool).await.unwrap();
            let own = collections[0].tracks[1].id;
            update_collection_track(
                &db.pool,
                own,
                &CollectionTrackChange {
                    name: None,
                    activity_type: Some(Some(ActivityType::Cycling)),
                },
            )
            .await
            .unwrap();

            let collections = list_export_collections(&db.pool).await.unwrap();
            let activities: Vec<ActivityType> = collections[0]
                .tracks
                .iter()
                .map(|t| t.activity_type)
                .collect();
            assert_eq!(activities, [ActivityType::Unknown, ActivityType::Cycling]);
        }
    }
}
