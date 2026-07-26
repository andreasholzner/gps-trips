//! US-37 acceptance tests (ADR-0012): re-running the export reconciles the
//! QMapShack database to the archive's current state — insert new trips,
//! update/re-link edited ones, trash deleted ones — while never touching
//! items or folders the exporter didn't create (ADR-0022 as amended:
//! change detection compares `items.name`, the `items.comment` summary, and
//! the config-resolved folder placement; exporter-owned items are fully
//! authoritative, so owner-side moves/trashing inside QMapShack are undone).

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use sqlx::SqlitePool;
use time::macros::datetime;
use time::OffsetDateTime;

use trip_archive::models::{ActivityType, TripKind};
use trip_archive::server::gpx::{compute_stats, TrackPoint};
use trip_archive::server::qmapshack::{self, config::ExportConfig, decode};
use trip_archive::server::repo::{
    add_trip_tag, delete_trip, get_or_create_tag, insert_trip, update_trip, NewTrip, TripEdit,
};
use trip_archive::server::{db, geojson};

/// A fresh archive DB (real temp file + migrations, ADR-0012) whose TempDir
/// also hosts the export target path.
async fn test_archive() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let pool = db::create_pool(&dir.path().join("archive.db"))
        .await
        .expect("create archive pool");
    (dir, pool)
}

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

fn points_2024() -> Vec<TrackPoint> {
    vec![
        timed_point(59.91, 10.75, 100.0, datetime!(2024-06-01 08:00:00 UTC)),
        timed_point(59.92, 10.76, 150.0, datetime!(2024-06-01 08:10:00 UTC)),
    ]
}

fn points_2023() -> Vec<TrackPoint> {
    vec![
        timed_point(61.10, 8.50, 900.0, datetime!(2023-02-11 09:00:00 UTC)),
        timed_point(61.11, 8.52, 950.0, datetime!(2023-02-11 09:30:00 UTC)),
    ]
}

fn config_for(dir: &tempfile::TempDir, target_name: &str) -> ExportConfig {
    let target = dir.path().join(target_name);
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

/// Open the exported target DB read-write, for both assertions and for
/// simulating the owner's own edits inside QMapShack. Pins the rollback
/// journal like the exporter does — sqlx's WAL default would convert the
/// QMapShack file, and mode ping-pong across the exporter's own opens
/// risks SQLITE_BUSY flakiness.
async fn open_target(dir: &tempfile::TempDir, target_name: &str) -> SqlitePool {
    SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(dir.path().join(target_name))
            .journal_mode(SqliteJournalMode::Delete),
    )
    .await
    .expect("open exported target DB")
}

/// The single items row for a trip: (item id, name, comment, data, trash).
async fn item_row(
    target: &SqlitePool,
    trip_id: i64,
) -> (i64, String, String, Vec<u8>, Option<String>) {
    sqlx::query_as("SELECT id, name, comment, data, trash FROM items WHERE keyqms = ?")
        .bind(qmapshack::keyqms(trip_id))
        .fetch_one(target)
        .await
        .expect("item row for trip")
}

/// Names of the folders a trip's item is linked under (empty = trashed).
async fn linked_folder_names(target: &SqlitePool, trip_id: i64) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT f.name FROM folders f
         JOIN folder2item f2i ON f2i.parent = f.id
         JOIN items i ON i.id = f2i.child
         WHERE i.keyqms = ? ORDER BY f.name",
    )
    .bind(qmapshack::keyqms(trip_id))
    .fetch_all(target)
    .await
    .expect("folder links for trip")
}

#[tokio::test]
async fn us37_trip_added_since_last_run_is_inserted_on_the_next_run() {
    let (dir, archive) = test_archive().await;
    insert_test_trip(
        &archive,
        "Tur A",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;
    let cfg = config_for(&dir, "export.db");
    qmapshack::run_export(&archive, &cfg)
        .await
        .expect("first run");

    let new_trip = insert_test_trip(
        &archive,
        "Tur B",
        ActivityType::SkiTouring,
        TripKind::Recorded,
        &points_2023(),
    )
    .await;

    let second = qmapshack::run_export(&archive, &cfg)
        .await
        .expect("second run");
    assert_eq!(second.inserted, 1, "only the new trip is inserted");
    assert_eq!(second.updated, 0);
    assert_eq!(second.removed, 0);
    assert_eq!(second.skipped, 1);
    assert_eq!(second.failed, 0);

    let target = open_target(&dir, "export.db").await;
    let (_, name, ..) = item_row(&target, new_trip).await;
    assert_eq!(name, "Tur B");
    assert_eq!(
        linked_folder_names(&target, new_trip).await,
        ["Ski touring"],
        "placed per the folder template"
    );
}

#[tokio::test]
async fn us37_renamed_trip_is_updated_in_place() {
    let (dir, archive) = test_archive().await;
    let trip = insert_test_trip(
        &archive,
        "Gammelt navn",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;
    let cfg = config_for(&dir, "export.db");
    qmapshack::run_export(&archive, &cfg)
        .await
        .expect("first run");

    update_trip(&archive, trip, &TripEdit::named("Nytt navn"))
        .await
        .expect("rename trip");

    let second = qmapshack::run_export(&archive, &cfg)
        .await
        .expect("second run");
    assert_eq!(second.inserted, 0);
    assert_eq!(second.updated, 1, "the renamed trip is updated");
    assert_eq!(second.removed, 0);
    assert_eq!(second.failed, 0);

    let target = open_target(&dir, "export.db").await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items")
        .fetch_one(&target)
        .await
        .unwrap();
    assert_eq!(count, 1, "updated in place, no duplicate");

    let (_, name, comment, data, trash) = item_row(&target, trip).await;
    assert_eq!(name, "Nytt navn");
    assert!(trash.is_none());
    let decoded = decode::decode_track_item(&data).expect("updated blob decodes");
    assert_eq!(decoded.name.as_deref(), Some("Nytt navn"));
    assert_eq!(decoded.hash, decoded.chunk_md5);

    // The FTS index followed the comment (searchindex_update trigger).
    let indexed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM searchindex WHERE comment = ?")
        .bind(&comment)
        .fetch_one(&target)
        .await
        .unwrap();
    assert_eq!(indexed, 1, "search index carries the fresh comment");

    assert_eq!(
        linked_folder_names(&target, trip).await,
        ["Hiking"],
        "placement is unchanged by a pure rename"
    );
}

#[tokio::test]
async fn us37_activity_change_moves_the_item_and_refreshes_its_content() {
    let (dir, archive) = test_archive().await;
    let trip = insert_test_trip(
        &archive,
        "Fjelltur",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;
    let cfg = config_for(&dir, "export.db");
    qmapshack::run_export(&archive, &cfg)
        .await
        .expect("first run");
    let target = open_target(&dir, "export.db").await;
    let icon_before: Vec<u8> = sqlx::query_scalar("SELECT icon FROM items WHERE keyqms = ?")
        .bind(qmapshack::keyqms(trip))
        .fetch_one(&target)
        .await
        .unwrap();

    update_trip(
        &archive,
        trip,
        &TripEdit {
            activity_type: Some(ActivityType::SkiTouring),
            ..TripEdit::default()
        },
    )
    .await
    .expect("change activity type");

    let second = qmapshack::run_export(&archive, &cfg)
        .await
        .expect("second run");
    assert_eq!(second.updated, 1);
    assert_eq!(second.failed, 0);

    assert_eq!(
        linked_folder_names(&target, trip).await,
        ["Ski touring"],
        "re-linked under the new activity folder only"
    );
    // The old folder was left in place, never auto-deleted (ADR-0022 scoping).
    let old_folder: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM folders WHERE name = 'Hiking'")
        .fetch_one(&target)
        .await
        .unwrap();
    assert_eq!(old_folder, 1, "emptied folder survives");

    let (_, _, comment, data, _) = item_row(&target, trip).await;
    assert!(
        comment.contains("Ski touring"),
        "comment label follows: {comment}"
    );
    let decoded = decode::decode_track_item(&data).expect("blob decodes");
    assert_eq!(
        decoded.trk_type.as_deref(),
        Some(ActivityType::SkiTouring.as_str())
    );
    let icon_after: Vec<u8> = sqlx::query_scalar("SELECT icon FROM items WHERE keyqms = ?")
        .bind(qmapshack::keyqms(trip))
        .fetch_one(&target)
        .await
        .unwrap();
    assert_ne!(icon_before, icon_after, "icon follows the activity type");
}

#[tokio::test]
async fn us37_tag_change_updates_keywords_and_comment() {
    let (dir, archive) = test_archive().await;
    let trip = insert_test_trip(
        &archive,
        "Med tagger",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;
    let cfg = config_for(&dir, "export.db");
    qmapshack::run_export(&archive, &cfg)
        .await
        .expect("first run");

    let tag = get_or_create_tag(&archive, "fjell").await.expect("tag");
    add_trip_tag(&archive, trip, tag).await.expect("tag trip");

    let second = qmapshack::run_export(&archive, &cfg)
        .await
        .expect("second run");
    assert_eq!(second.updated, 1, "a tag edit is a detected change");
    assert_eq!(second.failed, 0);

    let target = open_target(&dir, "export.db").await;
    let (_, _, comment, data, _) = item_row(&target, trip).await;
    assert!(
        comment.contains("Tags: fjell"),
        "comment lists the tag: {comment}"
    );
    let decoded = decode::decode_track_item(&data).expect("blob decodes");
    assert_eq!(decoded.keywords, ["fjell"]);
}

#[tokio::test]
async fn us37_deleted_trip_is_moved_to_qmapshack_trash_once() {
    let (dir, archive) = test_archive().await;
    let keep = insert_test_trip(
        &archive,
        "Beholdes",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;
    let doomed = insert_test_trip(
        &archive,
        "Slettes",
        ActivityType::SkiTouring,
        TripKind::Recorded,
        &points_2023(),
    )
    .await;
    let cfg = config_for(&dir, "export.db");
    qmapshack::run_export(&archive, &cfg)
        .await
        .expect("first run");

    assert!(delete_trip(&archive, doomed).await.expect("delete trip"));

    let second = qmapshack::run_export(&archive, &cfg)
        .await
        .expect("second run");
    assert_eq!(second.removed, 1, "the deleted trip's item is removed");
    assert_eq!(second.skipped, 1);
    assert_eq!(second.failed, 0);

    let target = open_target(&dir, "export.db").await;
    // Trash, not a hard delete: the items row survives with trash set and
    // no folder links (QMapShack's own folder2item_delete trigger).
    let (_, _, _, _, trash) = item_row(&target, doomed).await;
    assert!(trash.is_some(), "trashed, not deleted");
    assert_eq!(
        linked_folder_names(&target, doomed).await,
        Vec::<String>::new()
    );
    let (_, _, _, _, keep_trash) = item_row(&target, keep).await;
    assert!(keep_trash.is_none(), "the surviving trip is untouched");

    // An already-trashed item is not counted again on the next run.
    let third = qmapshack::run_export(&archive, &cfg)
        .await
        .expect("third run");
    assert_eq!(third.removed, 0);
    assert_eq!(third.skipped, 1);
}

#[tokio::test]
async fn us37_unchanged_rerun_is_a_pure_skip_with_identical_bytes() {
    let (dir, archive) = test_archive().await;
    let trip = insert_test_trip(
        &archive,
        "Stabil tur",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;
    let cfg = config_for(&dir, "export.db");
    qmapshack::run_export(&archive, &cfg)
        .await
        .expect("first run");
    let target = open_target(&dir, "export.db").await;
    let (_, _, _, data_before, _) = item_row(&target, trip).await;
    let last_change_before: String =
        sqlx::query_scalar("SELECT last_change FROM items WHERE keyqms = ?")
            .bind(qmapshack::keyqms(trip))
            .fetch_one(&target)
            .await
            .unwrap();

    let second = qmapshack::run_export(&archive, &cfg)
        .await
        .expect("second run");
    assert_eq!(second.inserted, 0);
    assert_eq!(second.updated, 0);
    assert_eq!(second.removed, 0);
    assert_eq!(second.skipped, 1);

    let (_, _, _, data_after, _) = item_row(&target, trip).await;
    assert_eq!(data_before, data_after, "no blob churn on a no-op run");
    let last_change_after: String =
        sqlx::query_scalar("SELECT last_change FROM items WHERE keyqms = ?")
            .bind(qmapshack::keyqms(trip))
            .fetch_one(&target)
            .await
            .unwrap();
    assert_eq!(
        last_change_before, last_change_after,
        "no items write at all for an unchanged trip"
    );
}

#[tokio::test]
async fn us37_owner_created_items_and_folders_are_left_untouched() {
    let (dir, archive) = test_archive().await;
    let trip = insert_test_trip(
        &archive,
        "Egen tur",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;
    let cfg = config_for(&dir, "export.db");
    qmapshack::run_export(&archive, &cfg)
        .await
        .expect("first run");

    // The owner adds their own folder and waypoint item directly in
    // QMapShack (foreign, MD5-style keyqms — not our namespace).
    let target = open_target(&dir, "export.db").await;
    let owner_folder: i64 = sqlx::query("INSERT INTO folders (type, name) VALUES (4, 'Mine egne')")
        .execute(&target)
        .await
        .unwrap()
        .last_insert_rowid();
    let root: i64 = sqlx::query_scalar("SELECT id FROM folders WHERE type = 2")
        .fetch_one(&target)
        .await
        .unwrap();
    sqlx::query("INSERT INTO folder2folder (parent, child) VALUES (?, ?)")
        .bind(root)
        .bind(owner_folder)
        .execute(&target)
        .await
        .unwrap();
    let owner_item: i64 = sqlx::query(
        "INSERT INTO items (type, keyqms, icon, name, comment, data, hash) \
         VALUES (1, 'a3f5c8d92b1e4f6789abcdef01234567', X'89504E47', 'Eget punkt', 'mitt', X'00', 'h')",
    )
    .execute(&target)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("INSERT INTO folder2item (parent, child) VALUES (?, ?)")
        .bind(owner_folder)
        .bind(owner_item)
        .execute(&target)
        .await
        .unwrap();

    // Force real reconciliation work: an edit and a deletion in the archive.
    update_trip(&archive, trip, &TripEdit::named("Egen tur II"))
        .await
        .expect("rename");
    let doomed = insert_test_trip(
        &archive,
        "Kortvarig",
        ActivityType::Cycling,
        TripKind::Recorded,
        &points_2023(),
    )
    .await;
    qmapshack::run_export(&archive, &cfg)
        .await
        .expect("second run");
    assert!(delete_trip(&archive, doomed).await.expect("delete"));
    let third = qmapshack::run_export(&archive, &cfg)
        .await
        .expect("third run");
    assert_eq!(third.removed, 1);
    assert_eq!(third.failed, 0);

    let (name, comment, trash): (String, String, Option<String>) =
        sqlx::query_as("SELECT name, comment, trash FROM items WHERE id = ?")
            .bind(owner_item)
            .fetch_one(&target)
            .await
            .unwrap();
    assert_eq!(name, "Eget punkt");
    assert_eq!(comment, "mitt");
    assert!(trash.is_none(), "owner item never trashed");
    let owner_links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM folder2item WHERE child = ?")
        .bind(owner_item)
        .fetch_one(&target)
        .await
        .unwrap();
    assert_eq!(owner_links, 1, "owner item stays in the owner's folder");
    let folder_name: String = sqlx::query_scalar("SELECT name FROM folders WHERE id = ?")
        .bind(owner_folder)
        .fetch_one(&target)
        .await
        .unwrap();
    assert_eq!(folder_name, "Mine egne", "owner folder untouched");
}

#[tokio::test]
async fn us37_owner_trashed_or_refiled_exported_items_are_restored() {
    let (dir, archive) = test_archive().await;
    let trashed = insert_test_trip(
        &archive,
        "Kastet i QMS",
        ActivityType::Hiking,
        TripKind::Recorded,
        &points_2024(),
    )
    .await;
    let moved = insert_test_trip(
        &archive,
        "Flyttet i QMS",
        ActivityType::SkiTouring,
        TripKind::Recorded,
        &points_2023(),
    )
    .await;
    let cfg = config_for(&dir, "export.db");
    qmapshack::run_export(&archive, &cfg)
        .await
        .expect("first run");

    // Owner-side edits inside QMapShack: one item deleted (all links gone →
    // trigger trashes it), one moved into the owner's own folder.
    let target = open_target(&dir, "export.db").await;
    let (trashed_id, ..) = item_row(&target, trashed).await;
    sqlx::query("DELETE FROM folder2item WHERE child = ?")
        .bind(trashed_id)
        .execute(&target)
        .await
        .unwrap();
    let (moved_id, ..) = item_row(&target, moved).await;
    let owner_folder: i64 = sqlx::query("INSERT INTO folders (type, name) VALUES (4, 'Min mappe')")
        .execute(&target)
        .await
        .unwrap()
        .last_insert_rowid();
    sqlx::query("UPDATE folder2item SET parent = ? WHERE child = ?")
        .bind(owner_folder)
        .bind(moved_id)
        .execute(&target)
        .await
        .unwrap();

    let second = qmapshack::run_export(&archive, &cfg)
        .await
        .expect("second run");
    assert_eq!(second.updated, 2, "both owner edits are reconciled away");
    assert_eq!(second.failed, 0);

    let (_, _, _, _, trash) = item_row(&target, trashed).await;
    assert!(trash.is_none(), "restore clears the trash flag");
    assert_eq!(linked_folder_names(&target, trashed).await, ["Hiking"]);
    assert_eq!(
        linked_folder_names(&target, moved).await,
        ["Ski touring"],
        "moved back to the config-resolved folder"
    );
    // The owner's own folder is not deleted, just no longer holds our item.
    let owner_folder_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM folders WHERE name = 'Min mappe'")
            .fetch_one(&target)
            .await
            .unwrap();
    assert_eq!(owner_folder_count, 1);
}
