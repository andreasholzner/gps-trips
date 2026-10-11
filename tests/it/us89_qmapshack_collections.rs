//! US-89 acceptance tests, exporter half (ADR-0022's 2026-10-11 amendment):
//! the export writes every guide-book collection beside the trips — its
//! tracks as track items in their activity colors, its waypoints as
//! waypoint items carrying their symbols, one folder per collection under
//! the collections' own template — and re-runs reconcile them as US-37 does
//! trips, leaving alone every item it did not create. Driven against a
//! running test server, as the exporter reaches the archive (US-51).

use crate::common::{
    self, delete, import_collection, import_sample, json_request, send, GUIDE_BOOK_GPX,
    GUIDE_SINGLE_GPX, TEST_PASSWORD,
};

use axum::http::{Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::Router;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use sqlx::SqlitePool;
use trip_archive::server::db;
use trip_archive::server::qmapshack::collections::{track_keyqms, waypoint_keyqms};
use trip_archive::server::qmapshack::{self, config::ExportConfig, decode};

/// A fresh archive served over HTTP, its pool for what no route changes,
/// and a router to import and edit through.
struct Archive {
    dir: tempfile::TempDir,
    pool: SqlitePool,
    app: Router,
    url: String,
}

async fn archive() -> Archive {
    let dir = tempfile::tempdir().expect("create tempdir");
    let pool = db::create_pool(&dir.path().join("archive.db"))
        .await
        .expect("create archive pool");
    let app = common::router_over(pool.clone(), dir.path());
    let url = common::serve(app.clone()).await;
    Archive {
        dir,
        pool,
        app,
        url,
    }
}

impl Archive {
    fn config(&self) -> ExportConfig {
        let target = self.dir.path().join("export.db");
        let toml = format!(
            "url = \"https://archive.test\"\n\
             target_db = {:?}\nfolder_template = \"Trips/{{activity_type}}\"\n\
             collection_folder_template = \"Guidebooks/{{collection}}\"\n\
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
        ExportConfig::from_toml_str(&toml).expect("valid config")
    }

    async fn export(&self) -> qmapshack::ExportOutcome {
        qmapshack::run_export(&self.url, TEST_PASSWORD, &self.config())
            .await
            .expect("export run")
    }

    /// The exported target, opened as the exporter opens it.
    async fn target(&self) -> SqlitePool {
        SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(self.dir.path().join("export.db"))
                .journal_mode(SqliteJournalMode::Delete),
        )
        .await
        .expect("open exported target")
    }

    async fn track_ids(&self, collection: i64) -> Vec<i64> {
        sqlx::query_scalar(
            "SELECT id FROM collection_track WHERE collection_id = ? ORDER BY position",
        )
        .bind(collection)
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    async fn waypoint_ids(&self, collection: i64) -> Vec<i64> {
        sqlx::query_scalar("SELECT id FROM waypoint WHERE collection_id = ? ORDER BY position")
            .bind(collection)
            .fetch_all(&self.pool)
            .await
            .unwrap()
    }

    async fn patch(&self, uri: &str, body: &str) {
        let response = send(&self.app, json_request(Method::PATCH, uri, body)).await;
        assert!(
            response.status().is_success(),
            "{uri}: {}",
            response.status()
        );
    }
}

/// An item's type, name, data, and the folder path it is linked under
/// (`None` once trashed).
async fn item(target: &SqlitePool, keyqms: &str) -> (i64, String, Vec<u8>, Option<String>) {
    let (id, item_type, name, data): (i64, i64, String, Vec<u8>) =
        sqlx::query_as("SELECT id, type, name, data FROM items WHERE keyqms = ?")
            .bind(keyqms)
            .fetch_one(target)
            .await
            .unwrap_or_else(|e| panic!("no item {keyqms}: {e}"));
    let folder: Option<String> = sqlx::query_scalar(
        "SELECT parent.name || '/' || f.name FROM folder2item f2i \
         JOIN folders f ON f.id = f2i.parent \
         JOIN folder2folder ff ON ff.child = f.id JOIN folders parent ON parent.id = ff.parent \
         WHERE f2i.child = ?",
    )
    .bind(id)
    .fetch_optional(target)
    .await
    .unwrap();
    (item_type, name, data, folder)
}

async fn trashed(target: &SqlitePool, keyqms: &str) -> bool {
    let trash: Option<String> = sqlx::query_scalar("SELECT trash FROM items WHERE keyqms = ?")
        .bind(keyqms)
        .fetch_one(target)
        .await
        .unwrap();
    trash.is_some()
}

#[tokio::test]
async fn us89_collections_are_exported_beside_the_trips_in_folders_of_their_own() {
    let archive = archive().await;
    let trip = import_sample(&archive.app).await;
    let book = import_collection(
        &archive.app,
        GUIDE_BOOK_GPX,
        &[("activity_type", "cycling")],
    )
    .await;
    let single = import_collection(&archive.app, GUIDE_SINGLE_GPX, &[("name", "Nordkette")]).await;
    let tracks = archive.track_ids(book).await;
    archive
        .patch(
            &format!("/api/collection-tracks/{}", tracks[2]),
            r#"{"activity_type":"hiking"}"#,
        )
        .await;

    let outcome = archive.export().await;

    assert_eq!(
        outcome.inserted,
        1 + 3 + 2 + 1 + 2,
        "a trip, 4 tracks, 4 waypoints"
    );
    assert_eq!(outcome.failed, 0);
    let target = archive.target().await;

    let (_, _, _, folder) = item(&target, &qmapshack::keyqms(trip)).await;
    assert_eq!(
        folder.as_deref(),
        Some("Trips/Unspecified"),
        "trips keep their folders"
    );

    let (item_type, name, data, folder) = item(&target, &track_keyqms(tracks[0])).await;
    assert_eq!(item_type, 2, "a track item");
    assert_eq!(name, "Stage 1: Splügen");
    assert_eq!(folder.as_deref(), Some("Guidebooks/Lakes & Passes"));
    let decoded = decode::decode_track_item(&data).expect("track blob decodes");
    assert_eq!(
        decoded.color.as_deref(),
        Some("DarkBlue"),
        "cycling's color"
    );
    assert_eq!(decoded.trk_type.as_deref(), Some("cycling"));
    assert!(!decoded.points.is_empty());
    assert!(
        decoded.points.iter().all(|p| p.time.is_none()),
        "a guide book's times are not when anyone went"
    );
    let (_, _, data, _) = item(&target, &track_keyqms(tracks[2])).await;
    let decoded = decode::decode_track_item(&data).unwrap();
    assert_eq!(
        decoded.color.as_deref(),
        Some("DarkRed"),
        "its own activity's color"
    );

    let waypoints = archive.waypoint_ids(book).await;
    let (item_type, name, data, folder) = item(&target, &waypoint_keyqms(waypoints[0])).await;
    assert_eq!(item_type, 1, "a waypoint item");
    assert_eq!(name, "San Bernardino Pass");
    assert_eq!(folder.as_deref(), Some("Guidebooks/Lakes & Passes"));
    let decoded = decode::decode_waypoint_item(&data).expect("waypoint blob decodes");
    assert_eq!(decoded.sym.as_deref(), Some("Triangle, Blue"));
    assert_eq!(decoded.desc.as_deref(), Some("Top of the first climb"));
    assert_eq!((decoded.lat, decoded.lon), (46.4958, 9.1705));
    assert_eq!(decoded.ele, Some(2065));
    assert_eq!(decoded.hash, decoded.chunk_md5);
    let (_, _, data, _) = item(&target, &waypoint_keyqms(waypoints[1])).await;
    let decoded = decode::decode_waypoint_item(&data).unwrap();
    assert_eq!(decoded.sym, None, "no symbol in the file, none written");

    let unnamed = archive.waypoint_ids(single).await[1];
    let (_, name, _, folder) = item(&target, &waypoint_keyqms(unnamed)).await;
    assert_eq!(
        name, "Waypoint 2",
        "numbered by its place in the collection"
    );
    assert_eq!(folder.as_deref(), Some("Guidebooks/Nordkette"));
}

#[tokio::test]
async fn us89_an_unchanged_collection_is_skipped_on_the_next_run() {
    let archive = archive().await;
    import_collection(&archive.app, GUIDE_BOOK_GPX, &[]).await;
    archive.export().await;

    let second = archive.export().await;

    assert_eq!(second.inserted, 0);
    assert_eq!(second.updated, 0);
    assert_eq!(second.skipped, 3 + 2);
}

#[tokio::test]
async fn us89_a_collection_added_since_the_last_run_is_inserted_on_the_next() {
    let archive = archive().await;
    import_collection(&archive.app, GUIDE_BOOK_GPX, &[]).await;
    archive.export().await;
    let added = import_collection(&archive.app, GUIDE_SINGLE_GPX, &[]).await;

    let second = archive.export().await;

    assert_eq!(second.inserted, 1 + 2, "its track and its waypoints");
    assert_eq!(
        second.skipped,
        3 + 2,
        "the first collection's are unchanged"
    );
    let target = archive.target().await;
    let track = archive.track_ids(added).await[0];
    let (_, name, _, folder) = item(&target, &track_keyqms(track)).await;
    assert_eq!(name, "Nordkette");
    assert!(
        folder.is_some_and(|f| f.starts_with("Guidebooks/")),
        "in a folder of its own under the collections' template"
    );
}

#[tokio::test]
async fn us89_changed_collections_tracks_and_waypoints_are_updated() {
    let archive = archive().await;
    let book = import_collection(&archive.app, GUIDE_BOOK_GPX, &[]).await;
    let tracks = archive.track_ids(book).await;
    let waypoints = archive.waypoint_ids(book).await;
    archive.export().await;

    archive
        .patch(
            &format!("/api/collection-tracks/{}", tracks[0]),
            r#"{"name":"Etappe 1"}"#,
        )
        .await;
    // A geometry replaced in place, as a US-88 re-import does it: moved
    // north, bounds and all.
    let geojson: String = sqlx::query_scalar("SELECT geojson FROM collection_track WHERE id = ?")
        .bind(tracks[1])
        .fetch_one(&archive.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE collection_track SET geojson = ?, min_lat = min_lat + 0.01, \
         max_lat = max_lat + 0.01 WHERE id = ?",
    )
    .bind(geojson.replace("46.43", "46.44"))
    .bind(tracks[1])
    .execute(&archive.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE waypoint SET symbol = 'Summit' WHERE id = ?")
        .bind(waypoints[1])
        .execute(&archive.pool)
        .await
        .unwrap();

    let second = archive.export().await;
    assert_eq!(
        second.updated, 3,
        "a renamed track, a replaced one, a waypoint"
    );
    assert_eq!(second.skipped, 1 + 1);
    assert_eq!(second.failed, 0);
    let target = archive.target().await;
    let (_, name, ..) = item(&target, &track_keyqms(tracks[0])).await;
    assert_eq!(name, "Etappe 1");
    let (_, _, data, _) = item(&target, &track_keyqms(tracks[1])).await;
    let points = decode::decode_track_item(&data).unwrap().points;
    assert!(
        points.iter().any(|p| p.lat == 46.44),
        "the new geometry is written"
    );
    let (_, _, data, _) = item(&target, &waypoint_keyqms(waypoints[1])).await;
    assert_eq!(
        decode::decode_waypoint_item(&data).unwrap().sym.as_deref(),
        Some("Summit")
    );

    // A renamed collection is a folder of its new name, every item moved.
    archive
        .patch(
            &format!("/api/collections/{book}"),
            r#"{"name":"Seen und Pässe"}"#,
        )
        .await;
    let third = archive.export().await;
    assert_eq!(third.updated, 3 + 2);
    for keyqms in tracks
        .iter()
        .map(|&id| track_keyqms(id))
        .chain(waypoints.iter().map(|&id| waypoint_keyqms(id)))
    {
        let (_, _, _, folder) = item(&target, &keyqms).await;
        assert_eq!(
            folder.as_deref(),
            Some("Guidebooks/Seen und Pässe"),
            "{keyqms}"
        );
    }
}

#[tokio::test]
async fn us89_deleted_collections_tracks_and_waypoints_go_to_the_trash() {
    let archive = archive().await;
    let book = import_collection(&archive.app, GUIDE_BOOK_GPX, &[]).await;
    let single = import_collection(&archive.app, GUIDE_SINGLE_GPX, &[]).await;
    let tracks = archive.track_ids(book).await;
    let waypoints = archive.waypoint_ids(book).await;
    let single_track = archive.track_ids(single).await[0];
    let single_waypoints = archive.waypoint_ids(single).await;
    archive.export().await;

    let response = delete(
        &archive.app,
        &format!("/api/collection-tracks/{}", tracks[1]),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    sqlx::query("DELETE FROM waypoint WHERE id = ?")
        .bind(waypoints[0])
        .execute(&archive.pool)
        .await
        .unwrap();
    let response = delete(&archive.app, &format!("/api/collections/{single}")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let second = archive.export().await;

    assert_eq!(second.removed, 1 + 1 + 1 + 2);
    assert_eq!(second.failed, 0);
    let target = archive.target().await;
    for gone in [
        track_keyqms(tracks[1]),
        waypoint_keyqms(waypoints[0]),
        track_keyqms(single_track),
        waypoint_keyqms(single_waypoints[0]),
        waypoint_keyqms(single_waypoints[1]),
    ] {
        assert!(trashed(&target, &gone).await, "{gone} is in the trash");
    }
    assert!(!trashed(&target, &track_keyqms(tracks[0])).await);
    assert!(!trashed(&target, &waypoint_keyqms(waypoints[1])).await);
}

#[tokio::test]
async fn us89_items_the_export_did_not_create_are_untouched() {
    let archive = archive().await;
    import_collection(&archive.app, GUIDE_BOOK_GPX, &[]).await;
    archive.export().await;
    let target = archive.target().await;
    // The owner's own waypoint, filed in the collection's folder.
    let folder: i64 = sqlx::query_scalar("SELECT id FROM folders WHERE name = 'Lakes & Passes'")
        .fetch_one(&target)
        .await
        .unwrap();
    let own = sqlx::query(
        "INSERT INTO items (type, keyqms, icon, name, data, hash) \
         VALUES (1, 'd41d8cd98f00b204e9800998ecf8427e', x'00', 'My hut', x'00', 'x')",
    )
    .execute(&target)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("INSERT INTO folder2item (parent, child) VALUES (?, ?)")
        .bind(folder)
        .bind(own)
        .execute(&target)
        .await
        .unwrap();

    let second = archive.export().await;

    assert_eq!(second.removed, 0);
    assert!(!trashed(&target, "d41d8cd98f00b204e9800998ecf8427e").await);
}

#[tokio::test]
async fn us89_a_run_without_the_complete_collection_list_writes_nothing() {
    let archive = archive().await;
    import_collection(&archive.app, GUIDE_BOOK_GPX, &[]).await;
    let failing = archive.app.clone().layer(middleware::from_fn(
        |request: Request<axum::body::Body>, next: Next| async move {
            if request.uri().path() == "/api/export/collections" {
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            } else {
                next.run(request).await
            }
        },
    ));
    let url = common::serve(failing).await;

    let result = qmapshack::run_export(&url, TEST_PASSWORD, &archive.config()).await;

    assert!(result.is_err(), "the run fails");
    assert!(
        !archive.dir.path().join("export.db").exists(),
        "the target is not even created"
    );
}

#[test]
fn us89_a_run_nobody_watches_stops_when_collections_may_share_the_trips_folders() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("export.db");
    let config = dir.path().join("qmapshack_export.toml");
    // Both top folders are "Trips"; stdin is no terminal, as under cron.
    let toml = format!(
        "url = \"https://archive.test\"\n\
         target_db = {:?}\nfolder_template = \"Trips/{{year}}\"\n\
         collection_folder_template = \"Trips/{{collection}}\"\n\
         [activity_type_names]\n\
         unknown = \"U\"\nhiking = \"H\"\nmountaineering = \"M\"\ncycling = \"C\"\n\
         bikepacking = \"B\"\nkayaking = \"K\"\nski_touring = \"S\"\n\
         cross_country_skiing = \"X\"\nsnow_shoe = \"N\"\n\
         [trip_type_names]\nrecorded = \"R\"\nplanned = \"P\"\n",
        target.to_str().unwrap()
    );
    std::fs::write(&config, toml).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_qmapshack_export"))
        .arg("--config")
        .arg(&config)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run the exporter");

    assert!(
        !output.status.success(),
        "a non-zero exit for cron to alert on"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("WARNING"), "{stderr}");
    assert!(stderr.contains("Nothing exported"), "{stderr}");
    assert!(!target.exists(), "nothing written");
}
