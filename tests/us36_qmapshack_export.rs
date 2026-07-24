//! US-36 acceptance tests (ADR-0012): exporting the whole archive into a
//! QMapShack-compatible database via `qmapshack::run_export` — the composed
//! entry point the `qmapshack_export` binary is a thin shell around (the
//! binary itself is not unit-tested, per the komoot binaries' precedent).

use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use time::macros::datetime;
use time::OffsetDateTime;

use trip_archive::models::{ActivityType, TripKind};
use trip_archive::server::gpx::{compute_stats, TrackPoint};
use trip_archive::server::qmapshack::{self, config::ExportConfig, decode, target};
use trip_archive::server::repo::{insert_trip, NewTrip};
use trip_archive::server::{db, geojson};

/// A fresh archive DB (real temp file + migrations, ADR-0012) whose TempDir
/// also hosts the export target path and config.
async fn test_archive() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let pool = db::create_pool(&dir.path().join("archive.db"))
        .await
        .expect("create archive pool");
    (dir, pool)
}

/// Insert a trip built from synthetic points via the public import pipeline
/// (`compute_stats` + `build_track_geojson` + `insert_trip`), so the exported
/// geometry has known coordinates/elevations/times to assert against.
async fn insert_test_trip(
    pool: &SqlitePool,
    name: &str,
    activity_type: ActivityType,
    trip_kind: TripKind,
    points: &[TrackPoint],
) -> i64 {
    let stats = compute_stats(points);
    let geojson = geojson::build_track_geojson(points);
    insert_trip(
        pool,
        &NewTrip {
            name,
            activity_type,
            tz_name: "Europe/Oslo",
            stats: &stats,
            geojson: &geojson,
            gpx: b"<gpx/>",
            trip_kind,
        },
    )
    .await
    .expect("insert trip")
}

fn timed_point(lat: f64, lon: f64, ele: f64, time: OffsetDateTime) -> TrackPoint {
    TrackPoint {
        lat,
        lon,
        ele: Some(ele),
        time: Some(time),
    }
}

/// Three points in 2024, ~Oslo area.
fn points_2024() -> Vec<TrackPoint> {
    vec![
        timed_point(59.91, 10.75, 100.0, datetime!(2024-06-01 08:00:00 UTC)),
        timed_point(59.92, 10.76, 150.0, datetime!(2024-06-01 08:10:00 UTC)),
        timed_point(59.93, 10.77, 120.0, datetime!(2024-06-01 08:20:00 UTC)),
    ]
}

fn points_2023() -> Vec<TrackPoint> {
    vec![
        timed_point(61.10, 8.50, 900.0, datetime!(2023-02-11 09:00:00 UTC)),
        timed_point(61.11, 8.52, 950.0, datetime!(2023-02-11 09:30:00 UTC)),
    ]
}

/// No timestamps at all → trip has no start_time → `{year}` = undated bucket.
fn points_undated() -> Vec<TrackPoint> {
    vec![
        TrackPoint {
            lat: 58.0,
            lon: 7.0,
            ele: None,
            time: None,
        },
        TrackPoint {
            lat: 58.01,
            lon: 7.01,
            ele: None,
            time: None,
        },
    ]
}

fn config_for(dir: &tempfile::TempDir, target_name: &str) -> ExportConfig {
    let target = dir.path().join(target_name);
    // US-39: every ActivityType (incl. unknown) and TripKind must have an
    // explicit mapping, or ExportConfig::from_toml_str rejects the config.
    let toml = format!(
        "target_db = {:?}\nfolder_template = \"Trips/{{year}}/{{activity_type}}\"\n\
         [activity_type_names]\n\
         unknown = \"Unspecified\"\n\
         hiking = \"Hiking\"\n\
         mountaineering = \"Mountaineering\"\n\
         cycling = \"Cycling\"\n\
         bikepacking = \"Bikepacking\"\n\
         kayaking = \"Kayaking\"\n\
         ski_touring = \"Ski touring\"\n\
         cross_country_skiing = \"Cross-country skiing\"\n\
         snow_shoe = \"Snowshoeing\"\n\
         [trip_type_names]\n\
         recorded = \"Recorded\"\n\
         planned = \"Planned\"\n",
        target.to_str().expect("utf-8 temp path")
    );
    ExportConfig::from_toml_str(&toml).expect("valid config parses")
}

/// Open the exported QMapShack DB for assertions.
async fn open_target(dir: &tempfile::TempDir, target_name: &str) -> SqlitePool {
    SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(dir.path().join(target_name))
            .read_only(true),
    )
    .await
    .expect("open exported target DB")
}

/// Resolve a folder path from the root (type 2) down, returning the folder id
/// at each named step; panics with context if a segment is missing.
async fn folder_id_at(target: &SqlitePool, path: &[&str]) -> i64 {
    let root: i64 = sqlx::query_scalar("SELECT id FROM folders WHERE type = 2 ORDER BY id LIMIT 1")
        .fetch_one(target)
        .await
        .expect("root folder exists");
    let mut parent = root;
    for segment in path {
        parent = sqlx::query_scalar(
            "SELECT f.id FROM folders f
             JOIN folder2folder ff ON ff.child = f.id
             WHERE ff.parent = ? AND f.name = ?",
        )
        .bind(parent)
        .bind(segment)
        .fetch_optional(target)
        .await
        .expect("folder lookup")
        .unwrap_or_else(|| panic!("folder segment {segment:?} missing under id {parent}"));
    }
    parent
}

#[tokio::test]
async fn us36_export_writes_every_trip_as_a_track_item_in_configured_folders() {
    let (dir, archive) = test_archive().await;

    let hike_2024 = insert_test_trip(
        &archive,
        "Nordmarka runde",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;
    let ski_2023 = insert_test_trip(
        &archive,
        "Skitur Filefjell",
        ActivityType::SkiTouring,
        TripKind::Recorded,
        &points_2023(),
    )
    .await;
    let undated = insert_test_trip(
        &archive,
        "Gammel tur uten tid",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_undated(),
    )
    .await;

    let cfg = config_for(&dir, "export.db");
    let outcome = qmapshack::run_export(&archive, &cfg)
        .await
        .expect("export succeeds");
    assert_eq!(outcome.inserted, 3);
    assert_eq!(outcome.skipped, 0);
    assert_eq!(outcome.failed, 0);

    let target = open_target(&dir, "export.db").await;

    // Every trip is an items row: type 2 (track), namespaced keyqms, a PNG
    // icon, and hash consistent with the blob's embedded hash.
    let items = sqlx::query("SELECT keyqms, name, type, icon, data, hash FROM items")
        .fetch_all(&target)
        .await
        .expect("read items");
    assert_eq!(items.len(), 3);
    for row in &items {
        assert_eq!(row.get::<i64, _>("type"), 2);
        let icon: Vec<u8> = row.get("icon");
        assert_eq!(&icon[..4], b"\x89PNG", "items.icon must be plain PNG bytes");
        let decoded = decode::decode_track_item(&row.get::<Vec<u8>, _>("data"))
            .expect("exported blob decodes cleanly");
        assert_eq!(decoded.hash, row.get::<String, _>("hash"));
        assert_eq!(decoded.keyqms, row.get::<String, _>("keyqms"));
    }

    // Folder placement per template "Trips/{year}/{activity_type}": groups
    // (type 3) for intermediate segments, project (type 4) for the leaf.
    for (trip_id, path) in [
        (hike_2024, vec!["Trips", "2024", "Hiking"]),
        (ski_2023, vec!["Trips", "2023", "Ski touring"]),
        (undated, vec!["Trips", "undated", "Hiking"]),
    ] {
        let leaf = folder_id_at(&target, &path).await;
        let leaf_type: i64 = sqlx::query_scalar("SELECT type FROM folders WHERE id = ?")
            .bind(leaf)
            .fetch_one(&target)
            .await
            .expect("leaf folder type");
        assert_eq!(leaf_type, 4, "leaf folder is a project");
        let linked: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM folder2item f2i
             JOIN items i ON i.id = f2i.child
             WHERE f2i.parent = ? AND i.keyqms = ?",
        )
        .bind(leaf)
        .bind(qmapshack::keyqms(trip_id))
        .fetch_one(&target)
        .await
        .expect("folder2item lookup");
        assert_eq!(linked, 1, "trip {trip_id} linked under {path:?}");
    }
    let trips_type: i64 = sqlx::query_scalar("SELECT type FROM folders WHERE name = 'Trips'")
        .fetch_one(&target)
        .await
        .expect("Trips folder");
    assert_eq!(trips_type, 3, "intermediate folder is a group");

    // Geometry round-trips: the 2024 hike's points come back with the same
    // coordinates, elevations and times.
    let blob: Vec<u8> = sqlx::query_scalar("SELECT data FROM items WHERE keyqms = ?")
        .bind(qmapshack::keyqms(hike_2024))
        .fetch_one(&target)
        .await
        .expect("hike blob");
    let track = decode::decode_track_item(&blob).expect("hike blob decodes");
    assert_eq!(track.name.as_deref(), Some("Nordmarka runde"));
    assert_eq!(track.points.len(), 3);
    let first = &track.points[0];
    let last = &track.points[2];
    assert!((first.lat - 59.91).abs() < 1e-9);
    assert!((first.lon - 10.75).abs() < 1e-9);
    assert_eq!(first.ele, Some(100));
    assert_eq!(first.time, Some(datetime!(2024-06-01 08:00:00 UTC)));
    assert!((last.lat - 59.93).abs() < 1e-9);
    assert_eq!(last.ele, Some(120));
    assert_eq!(last.time, Some(datetime!(2024-06-01 08:20:00 UTC)));
}

fn backup_files(dir: &tempfile::TempDir) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir.path())
        .expect("list tempdir")
        .map(|e| {
            e.expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| n.contains(".backup-"))
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn us36_version_gate_fails_clearly_without_writing_anything() {
    let (dir, archive) = test_archive().await;
    insert_test_trip(
        &archive,
        "En tur",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;

    // A target created by a "newer" QMapShack than this exporter targets.
    let target_path = dir.path().join("export.db");
    let mut conn = target::create_and_bootstrap(&target_path, "export")
        .await
        .expect("bootstrap target");
    sqlx::query("UPDATE versioninfo SET version = '7'")
        .execute(&mut conn)
        .await
        .expect("bump version");
    drop(conn);
    let bytes_before = std::fs::read(&target_path).expect("target bytes");

    let err = qmapshack::run_export(&archive, &config_for(&dir, "export.db"))
        .await
        .expect_err("mismatched version must refuse the export");

    let message = format!("{err:#}");
    assert!(message.contains('7'), "names the found version: {message}");
    assert!(
        message.contains('6'),
        "names the expected version: {message}"
    );
    assert_eq!(
        std::fs::read(&target_path).expect("target bytes"),
        bytes_before,
        "the target file is byte-identical"
    );
    assert_eq!(backup_files(&dir), Vec::<String>::new(), "no backup either");
}

#[tokio::test]
async fn us36_bootstraps_a_missing_target_with_full_schema() {
    let (dir, archive) = test_archive().await;
    insert_test_trip(
        &archive,
        "En tur",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;

    let outcome = qmapshack::run_export(&archive, &config_for(&dir, "fresh.db"))
        .await
        .expect("export bootstraps a missing target");
    assert_eq!(outcome.inserted, 1);

    let target = open_target(&dir, "fresh.db").await;
    let (version, db_type): (String, String) =
        sqlx::query_as("SELECT version, type FROM versioninfo")
            .fetch_one(&target)
            .await
            .expect("versioninfo");
    assert_eq!((version.as_str(), db_type.as_str()), ("6", "QMapShack"));

    let names: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type IN ('table', 'trigger')")
            .fetch_all(&target)
            .await
            .expect("schema objects");
    for expected in [
        "folders",
        "items",
        "folder2folder",
        "folder2item",
        "searchindex",
        "items_update_last_change",
        "folder2item_insert",
        "folder2item_delete",
        "searchindex_update",
        "searchindex_insert",
    ] {
        assert!(names.iter().any(|n| n == expected), "missing {expected}");
    }

    let root_name: String = sqlx::query_scalar("SELECT name FROM folders WHERE type = 2")
        .fetch_one(&target)
        .await
        .expect("root folder");
    assert_eq!(root_name, "fresh", "root folder is named after the file");

    assert_eq!(
        backup_files(&dir),
        Vec::<String>::new(),
        "nothing to back up"
    );
    assert!(
        !dir.path().join("fresh.db-journal").exists(),
        "rollback journal cleaned up after close"
    );
}

#[tokio::test]
async fn us36_rerun_skips_already_exported_trips() {
    let (dir, archive) = test_archive().await;
    insert_test_trip(
        &archive,
        "Tur A",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;
    insert_test_trip(
        &archive,
        "Tur B",
        ActivityType::SkiTouring,
        TripKind::Recorded,
        &points_2023(),
    )
    .await;

    let cfg = config_for(&dir, "export.db");
    let first = qmapshack::run_export(&archive, &cfg)
        .await
        .expect("first run");
    assert_eq!(first.inserted, 2);

    let second = qmapshack::run_export(&archive, &cfg)
        .await
        .expect("second run");
    assert_eq!(second.inserted, 0, "nothing new to insert");
    assert_eq!(
        second.skipped, 2,
        "existing items are skipped, not rewritten"
    );
    assert_eq!(second.failed, 0);

    let target = open_target(&dir, "export.db").await;
    let items: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items")
        .fetch_one(&target)
        .await
        .expect("count items");
    assert_eq!(items, 2, "no duplicates");
}

#[tokio::test]
async fn us36_creates_rolling_backup_and_prunes_by_retention() {
    let (dir, archive) = test_archive().await;
    insert_test_trip(
        &archive,
        "En tur",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;

    // First run bootstraps the target (no backup of a file that didn't exist).
    let cfg = config_for(&dir, "export.db");
    qmapshack::run_export(&archive, &cfg)
        .await
        .expect("first run");
    assert_eq!(backup_files(&dir), Vec::<String>::new());

    // Seed two recent and three ancient backups around the existing target.
    let format = time::macros::format_description!("[year][month][day]T[hour][minute][second]");
    let now = OffsetDateTime::now_utc();
    let mut seeded_recent = Vec::new();
    for (name, ts) in [
        ("recent1", now - time::Duration::days(1)),
        ("recent2", now - time::Duration::days(20)),
    ] {
        let stamp = ts.format(&format).expect("format timestamp");
        let file = format!("export.backup-{stamp}.db");
        std::fs::write(dir.path().join(&file), name).expect("seed backup");
        seeded_recent.push(file);
    }
    for stamp in ["20220101T000000", "20230101T000000", "20240101T000000"] {
        std::fs::write(
            dir.path().join(format!("export.backup-{stamp}.db")),
            "ancient",
        )
        .expect("seed backup");
    }

    // Second run backs up the existing target, then prunes: the union of
    // {recent} and {3 newest} is {new, recent1, recent2} — ancients go.
    qmapshack::run_export(&archive, &cfg)
        .await
        .expect("second run");

    let survivors = backup_files(&dir);
    assert_eq!(survivors.len(), 3, "new + two recent: {survivors:?}");
    for file in &seeded_recent {
        assert!(survivors.contains(file), "{file} kept: {survivors:?}");
    }
    assert!(
        !survivors
            .iter()
            .any(|n| n.contains("2022") || n.contains("2023")),
        "ancient backups pruned: {survivors:?}"
    );
}

#[tokio::test]
async fn us36_per_item_failure_continues_and_is_reported() {
    let (dir, archive) = test_archive().await;
    let good = insert_test_trip(
        &archive,
        "God tur",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;
    let broken = insert_test_trip(
        &archive,
        "Ødelagt tur",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2023(),
    )
    .await;
    sqlx::query("UPDATE track SET geojson = 'garbage' WHERE trip_id = ?")
        .bind(broken)
        .execute(&archive)
        .await
        .expect("corrupt one trip's geometry");

    let outcome = qmapshack::run_export(&archive, &config_for(&dir, "export.db"))
        .await
        .expect("the run itself completes");
    assert_eq!(outcome.inserted, 1);
    assert_eq!(outcome.failed, 1, "the corrupted trip is reported");

    let target = open_target(&dir, "export.db").await;
    let keys: Vec<String> = sqlx::query_scalar("SELECT keyqms FROM items")
        .fetch_all(&target)
        .await
        .expect("read items");
    assert_eq!(keys, [qmapshack::keyqms(good)], "the good trip made it");
}
