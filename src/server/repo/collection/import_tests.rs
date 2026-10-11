//! US-88 — the parked upload and the applied outcome, against a real SQLite
//! file (ADR-0012).

use super::*;
use crate::models::ActivityType;
use crate::server::db::testing::TestDb;
use crate::server::geojson::build_track_geojson;
use crate::server::gpx::{compute_stats, TrackPoint, TrackStats};
use crate::server::repo::{
    get_collection, insert_collection, list_collection_files, list_collection_waypoints,
    update_collection_track, CollectionTrackChange, NewCollection,
};
use time::macros::datetime;

const NOW: OffsetDateTime = datetime!(2026-10-11 12:00 UTC);

fn points(lat: f64) -> Vec<TrackPoint> {
    (0..5)
        .map(|i| TrackPoint {
            lat,
            lon: 9.2 + f64::from(i) * 0.001,
            ele: Some(1000.0 + f64::from(i)),
            time: None,
        })
        .collect()
}

fn derived(lat: f64) -> (TrackStats, String) {
    let points = points(lat);
    (compute_stats(&points), build_track_geojson(&points))
}

fn waypoint(name: &str) -> NewWaypoint<'_> {
    NewWaypoint {
        name: Some(name),
        description: None,
        lat: 46.5,
        lon: 9.2,
        ele: None,
        symbol: None,
        track: None,
    }
}

/// A collection "Lakes" with tracks "A" and "B" and waypoints "Hut" (on
/// "A") and "Pass", imported from one file.
async fn a_collection(pool: &SqlitePool) -> i64 {
    a_collection_named(pool, "Lakes").await
}

/// [`a_collection`] under another name — no two share one (US-86).
async fn a_collection_named(pool: &SqlitePool, name: &str) -> i64 {
    let (stats, geojson) = derived(46.5);
    let track = |name| NewCollectionTrack {
        name,
        stats: &stats,
        geojson: &geojson,
        gpx: b"<gpx/>",
    };
    let mut hut = waypoint("Hut");
    hut.track = Some(0);
    insert_collection(
        pool,
        &NewCollection {
            name,
            description: "",
            activity_type: None,
            files: &[NewCollectionFile {
                file_name: "part-1.gpx",
                gpx: b"<gpx>1</gpx>",
            }],
            tracks: &[track("A"), track("B")],
            waypoints: &[hut, waypoint("Pass")],
        },
    )
    .await
    .expect("insert")
}

async fn held(pool: &SqlitePool, id: i64) -> Held {
    load_held(&mut pool.acquire().await.unwrap(), id)
        .await
        .expect("held")
}

async fn apply(pool: &SqlitePool, id: i64, plan: &CollectionImportPlan<'_>) {
    let mut tx = pool.begin().await.unwrap();
    apply_collection_import(&mut tx, id, plan)
        .await
        .expect("apply");
    tx.commit().await.unwrap();
}

fn names(collection: &crate::models::CollectionDetail) -> Vec<&str> {
    collection.tracks.iter().map(|t| t.name.as_str()).collect()
}

// ── The parked upload ────────────────────────────────────────────────────────

#[tokio::test]
async fn us88_a_parked_upload_comes_back_in_order_and_is_spent_once_taken() {
    let db = TestDb::new().await;
    let files = [
        NewCollectionFile {
            file_name: "a.gpx",
            gpx: b"<a/>",
        },
        NewCollectionFile {
            file_name: "b.gpx",
            gpx: b"<b/>",
        },
    ];
    let id = insert_collection_import(&db.pool, None, &files, NOW)
        .await
        .unwrap()
        .expect("parked");

    let staged = get_collection_import(&db.pool, id).await.unwrap().unwrap();
    assert_eq!(staged.collection_id, None);
    assert_eq!(
        staged.files,
        [
            ("a.gpx".to_string(), b"<a/>".to_vec()),
            ("b.gpx".to_string(), b"<b/>".to_vec())
        ]
    );

    let mut tx = db.pool.begin().await.unwrap();
    assert!(take_collection_import_in_tx(&mut tx, id)
        .await
        .unwrap()
        .is_some());
    assert!(take_collection_import_in_tx(&mut tx, id)
        .await
        .unwrap()
        .is_none());
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn us88_an_upload_is_cancelled_or_swept_and_goes_with_its_collection() {
    let db = TestDb::new().await;
    let collection = a_collection(&db.pool).await;
    let old = insert_collection_import(&db.pool, None, &[], NOW - time::Duration::hours(48))
        .await
        .unwrap()
        .expect("parked");
    let current = insert_collection_import(&db.pool, None, &[], NOW)
        .await
        .unwrap()
        .expect("parked");
    let for_collection = insert_collection_import(&db.pool, Some(collection), &[], NOW)
        .await
        .unwrap()
        .expect("parked");

    assert_eq!(
        sweep_collection_imports(&db.pool, NOW - time::Duration::hours(24))
            .await
            .unwrap(),
        1
    );
    assert!(get_collection_import(&db.pool, old)
        .await
        .unwrap()
        .is_none());
    assert!(delete_collection_import(&db.pool, current).await.unwrap());
    assert!(!delete_collection_import(&db.pool, current).await.unwrap());
    crate::server::repo::delete_collection(&db.pool, collection)
        .await
        .unwrap();
    assert!(get_collection_import(&db.pool, for_collection)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn us88_an_upload_for_a_collection_deleted_meanwhile_is_not_parked() {
    let db = TestDb::new().await;
    let id = a_collection(&db.pool).await;
    crate::server::repo::delete_collection(&db.pool, id)
        .await
        .unwrap();

    let parked = insert_collection_import(&db.pool, Some(id), &[], NOW).await;

    assert_eq!(parked.unwrap(), None);
}

// ── What the collection holds ────────────────────────────────────────────────

#[tokio::test]
async fn us88_what_the_collection_holds_is_read_in_its_order() {
    let db = TestDb::new().await;
    let id = a_collection(&db.pool).await;

    let held = held(&db.pool, id).await;

    let names: Vec<&str> = held.tracks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["A", "B"]);
    assert_eq!(held.tracks[0].line.len(), 5);
    assert_eq!(held.tracks[0].line[1], (46.5, 9.201));
    assert_eq!(held.waypoints[1].name.as_deref(), Some("Pass"));
}

// ── Applying the outcome ─────────────────────────────────────────────────────

#[tokio::test]
async fn us88_a_replaced_track_keeps_its_id_place_and_own_activity() {
    let db = TestDb::new().await;
    let id = a_collection(&db.pool).await;
    let held = held(&db.pool, id).await;
    let (a, b) = (held.tracks[0].id, held.tracks[1].id);
    update_collection_track(
        &db.pool,
        a,
        &CollectionTrackChange {
            activity_type: Some(Some(ActivityType::Hiking)),
            ..CollectionTrackChange::default()
        },
    )
    .await
    .unwrap();
    let (stats, geojson) = derived(46.6);

    apply(
        &db.pool,
        id,
        &CollectionImportPlan {
            replace_all: false,
            remove_tracks: &[],
            remove_waypoints: &[],
            tracks: &[
                PlannedTrack::Update(
                    a,
                    NewCollectionTrack {
                        name: "A",
                        stats: &stats,
                        geojson: &geojson,
                        gpx: b"<new/>",
                    },
                ),
                PlannedTrack::Keep(b),
            ],
            waypoints: &[],
            files: &[],
        },
    )
    .await;

    let collection = get_collection(&db.pool, id).await.unwrap().unwrap();
    assert_eq!(names(&collection), ["A", "B"]);
    assert_eq!(collection.tracks[0].id, a);
    assert_eq!(collection.tracks[0].activity_type, ActivityType::Hiking);
    let after = load_held(&mut db.pool.acquire().await.unwrap(), id)
        .await
        .unwrap();
    assert_eq!(after.tracks[0].line[0], (46.6, 9.2));
}

#[tokio::test]
async fn us88_added_items_come_last_and_removed_ones_go() {
    let db = TestDb::new().await;
    let id = a_collection(&db.pool).await;
    let held = held(&db.pool, id).await;
    let (stats, geojson) = derived(46.7);
    let new = NewCollectionTrack {
        name: "C",
        stats: &stats,
        geojson: &geojson,
        gpx: b"<c/>",
    };

    apply(
        &db.pool,
        id,
        &CollectionImportPlan {
            replace_all: false,
            remove_tracks: &[held.tracks[1].id],
            remove_waypoints: &[held.waypoints[1].id],
            tracks: &[
                PlannedTrack::Keep(held.tracks[0].id),
                PlannedTrack::Insert(new),
            ],
            waypoints: &[PlannedWaypoint::Insert(
                waypoint("Lake"),
                TrackRef::Set(Some(1)),
            )],
            files: &[NewCollectionFile {
                file_name: "part-2.gpx",
                gpx: b"<gpx>2</gpx>",
            }],
        },
    )
    .await;

    let collection = get_collection(&db.pool, id).await.unwrap().unwrap();
    assert_eq!(names(&collection), ["A", "C"]);
    let waypoints = list_collection_waypoints(&db.pool, id).await.unwrap();
    let wp_names: Vec<_> = waypoints.iter().map(|w| w.name.as_deref()).collect();
    assert_eq!(wp_names, [Some("Hut"), Some("Lake")]);
    assert_eq!(
        waypoints[1].collection_track_id,
        Some(collection.tracks[1].id)
    );
    let files: Vec<String> = list_collection_files(&db.pool, id)
        .await
        .unwrap()
        .into_iter()
        .map(|f| f.file_name)
        .collect();
    assert_eq!(files, ["part-1.gpx", "part-2.gpx"]);
}

#[tokio::test]
async fn us88_a_replaced_waypoint_takes_the_files_details_and_keeps_or_changes_its_track() {
    let db = TestDb::new().await;
    let id = a_collection(&db.pool).await;
    let held = held(&db.pool, id).await;
    let mut hut = waypoint("Hut");
    hut.symbol = Some("Lodge");
    let mut pass = waypoint("Pass");
    pass.ele = Some(2065.0);

    apply(
        &db.pool,
        id,
        &CollectionImportPlan {
            replace_all: false,
            remove_tracks: &[],
            remove_waypoints: &[],
            tracks: &[
                PlannedTrack::Keep(held.tracks[0].id),
                PlannedTrack::Keep(held.tracks[1].id),
            ],
            waypoints: &[
                PlannedWaypoint::Update(held.waypoints[0].id, hut, TrackRef::Unchanged),
                PlannedWaypoint::Update(held.waypoints[1].id, pass, TrackRef::Set(Some(1))),
            ],
            files: &[],
        },
    )
    .await;

    let waypoints = list_collection_waypoints(&db.pool, id).await.unwrap();
    assert_eq!(waypoints[0].id, held.waypoints[0].id);
    assert_eq!(waypoints[0].symbol.as_deref(), Some("Lodge"));
    assert_eq!(waypoints[0].collection_track_id, Some(held.tracks[0].id));
    assert_eq!(waypoints[1].ele, Some(2065.0));
    assert_eq!(waypoints[1].collection_track_id, Some(held.tracks[1].id));
}

#[tokio::test]
async fn us88_replacing_the_whole_collection_keeps_only_its_details() {
    let db = TestDb::new().await;
    let id = a_collection(&db.pool).await;
    let (stats, geojson) = derived(47.0);
    let new = NewCollectionTrack {
        name: "Z",
        stats: &stats,
        geojson: &geojson,
        gpx: b"<z/>",
    };

    apply(
        &db.pool,
        id,
        &CollectionImportPlan {
            replace_all: true,
            remove_tracks: &[],
            remove_waypoints: &[],
            tracks: &[PlannedTrack::Insert(new)],
            waypoints: &[],
            files: &[NewCollectionFile {
                file_name: "new.gpx",
                gpx: b"<new/>",
            }],
        },
    )
    .await;

    let collection = get_collection(&db.pool, id).await.unwrap().unwrap();
    assert_eq!(collection.name, "Lakes");
    assert_eq!(names(&collection), ["Z"]);
    assert_eq!(collection.waypoint_count, 0);
    let files = list_collection_files(&db.pool, id).await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].file_name, "new.gpx");
}

#[tokio::test]
async fn us88_another_collections_items_are_never_touched() {
    let db = TestDb::new().await;
    let one = a_collection(&db.pool).await;
    let other = a_collection_named(&db.pool, "Other lakes").await;
    let others = held(&db.pool, other).await;

    apply(
        &db.pool,
        one,
        &CollectionImportPlan {
            replace_all: false,
            remove_tracks: &[others.tracks[0].id],
            remove_waypoints: &[others.waypoints[0].id],
            tracks: &[],
            waypoints: &[],
            files: &[],
        },
    )
    .await;

    let after = held(&db.pool, other).await;
    assert_eq!(after.tracks.len(), 2);
    assert_eq!(after.waypoints.len(), 2);
}

// ── The kept files ───────────────────────────────────────────────────────────

#[tokio::test]
async fn us88_a_kept_file_downloads_byte_for_byte() {
    let db = TestDb::new().await;
    let id = a_collection(&db.pool).await;
    let files = list_collection_files(&db.pool, id).await.unwrap();

    let file = crate::server::repo::get_collection_file(&db.pool, files[0].id)
        .await
        .unwrap();

    assert_eq!(
        file,
        Some(("part-1.gpx".to_string(), b"<gpx>1</gpx>".to_vec()))
    );
}

#[tokio::test]
async fn us88_a_collection_imported_before_keeps_its_file_under_its_name() {
    let dir = tempfile::TempDir::new().unwrap();
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(dir.path().join("old.db"))
            .create_if_missing(true),
    )
    .await
    .unwrap();
    let mut before = sqlx::migrate!("./migrations");
    before.migrations = before
        .migrations
        .iter()
        .filter(|m| m.version < 26)
        .cloned()
        .collect::<Vec<_>>()
        .into();
    before.run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO collection (name, description, created_at, gpx) \
         VALUES ('Lakes', '', '2026-10-01T00:00:00Z', x'3c6770782f3e')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO collection_track (collection_id, position, name, distance_m, ascent_m, \
             descent_m, min_lat, min_lon, max_lat, max_lon, geojson, gpx) \
         SELECT id, 1, 'A', 0, 0, 0, 0, 0, 0, 0, '{}', x'00' FROM collection",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    // Its tracks are still there: nothing cascaded.
    let tracks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM collection_track")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tracks, 1);

    let id: i64 = sqlx::query_scalar("SELECT id FROM collection")
        .fetch_one(&pool)
        .await
        .unwrap();
    let files = list_collection_files(&pool, id).await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].file_name, "Lakes.gpx");
    let file = crate::server::repo::get_collection_file(&pool, files[0].id)
        .await
        .unwrap();
    assert_eq!(file.unwrap().1, b"<gpx/>");
}
