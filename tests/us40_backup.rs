//! US-40: a consistent backup of the deployed archive, pulled onto the laptop
//! into a directory laid out like a data directory, for the borg jobs.
//!
//! The server half is `GET /api/backup/database`, a `VACUUM INTO` snapshot;
//! the client half is `server::backup::client`, driven here against a real
//! server on a loopback port, since what it does is HTTP.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::{
    body::to_bytes,
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::IntoResponse,
    Router,
};
use common::{delete, get, import_request_with_photos, send, test_app, SAMPLE_GPX, TEST_PASSWORD};
use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, Connection};
use trip_archive::config::storage::{BLOBS_SUBDIR, DB_FILENAME};
use trip_archive::server::archive_client::ClientError;
use trip_archive::server::backup::client::{self, BackupError, Options};

const PHOTO: &[u8] = include_bytes!("fixtures/geotagged.jpg");

/// Import one trip with one photo, which stores two blobs: the original and
/// its thumbnail.
async fn import_trip_with_photo(app: &Router) {
    let response = send(
        app,
        import_request_with_photos(SAMPLE_GPX, &[("photo.jpg", PHOTO)]),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}

/// Serve `app` on a free loopback port, for the client to reach over HTTP.
async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

/// The router, counting the photo blobs it serves.
fn counting_media(app: Router, count: Arc<AtomicUsize>) -> Router {
    app.layer(middleware::from_fn(move |request: Request, next: Next| {
        let count = count.clone();
        async move {
            if request.uri().path().starts_with("/media/") {
                count.fetch_add(1, Ordering::SeqCst);
            }
            next.run(request).await
        }
    }))
}

/// The router, failing every photo blob of trip `trip_id`.
fn failing_media_of_trip(app: Router, trip_id: i64) -> Router {
    let prefix = format!("/media/trips/{trip_id}/");
    app.layer(middleware::from_fn(move |request: Request, next: Next| {
        let fails = request.uri().path().starts_with(&prefix);
        async move {
            if fails {
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            } else {
                next.run(request).await
            }
        }
    }))
}

fn options(url: &str, target: &Path) -> Options {
    Options {
        url: url.to_owned(),
        password: TEST_PASSWORD.to_owned(),
        target: target.to_path_buf(),
    }
}

/// Every file under `dir`, relative to it, sorted.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.push(path.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    let mut out = Vec::new();
    if dir.exists() {
        walk(dir, dir, &mut out);
    }
    out.sort();
    out
}

/// How many trips the SQLite file at `path` holds, opened read-only.
async fn trips_in(path: &Path) -> i64 {
    let mut conn = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .connect()
        .await
        .unwrap();
    let count = sqlx::query_scalar("SELECT COUNT(*) FROM trip")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    conn.close().await.unwrap();
    count
}

// ── The snapshot endpoint ────────────────────────────────────────────────────

#[tokio::test]
async fn us40_the_snapshot_is_a_database_holding_the_archive() {
    let (app, _dir) = test_app().await;
    import_trip_with_photo(&app).await;

    let response = get(&app, "/api/backup/database").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "application/vnd.sqlite3"
    );
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let out = tempfile::tempdir().unwrap();
    let snapshot = out.path().join("snapshot.db");
    std::fs::write(&snapshot, &bytes).unwrap();
    assert_eq!(trips_in(&snapshot).await, 1);
}

// ── The backup command ───────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn us40_a_first_backup_is_a_data_directory_a_restore_can_run_on() {
    let (app, server_dir) = test_app().await;
    import_trip_with_photo(&app).await;
    let url = serve(app).await;
    let target = tempfile::tempdir().unwrap();

    let report = client::run(&options(&url, target.path())).await.unwrap();

    assert_eq!(report.fetched, 2, "the photo and its thumbnail");
    assert_eq!(report.removed, 0);
    let server_blobs = server_dir.path().join(common::TEST_BLOBS_SUBDIR);
    let backup_blobs = target.path().join(BLOBS_SUBDIR);
    assert_eq!(files_under(&backup_blobs), files_under(&server_blobs));
    for file in files_under(&server_blobs) {
        assert_eq!(
            std::fs::read(backup_blobs.join(&file)).unwrap(),
            std::fs::read(server_blobs.join(&file)).unwrap(),
            "{file:?}"
        );
    }
    assert_eq!(
        files_under(target.path())
            .into_iter()
            .filter(|f| !f.starts_with(BLOBS_SUBDIR))
            .collect::<Vec<_>>(),
        vec![PathBuf::from(DB_FILENAME)],
        "nothing but the database beside the photos"
    );

    // The restore: an archive started on the backup serves the trip and its photo.
    let restored = common::test_app_on(target.path()).await;
    let trips = to_bytes(get(&restored, "/api/trips").await.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&trips).contains("Oslo Hills Walk"));
    let photo = files_under(&backup_blobs)
        .into_iter()
        .find(|f| !f.to_string_lossy().contains("thumbs"))
        .unwrap();
    let media = get(&restored, &format!("/media/{}", photo.display())).await;
    assert_eq!(media.status(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn us40_a_later_backup_fetches_only_the_new_photos() {
    let (app, _server_dir) = test_app().await;
    import_trip_with_photo(&app).await;
    let fetches = Arc::new(AtomicUsize::new(0));
    let url = serve(counting_media(app.clone(), fetches.clone())).await;
    let target = tempfile::tempdir().unwrap();
    client::run(&options(&url, target.path())).await.unwrap();

    import_trip_with_photo(&app).await;
    fetches.store(0, Ordering::SeqCst);
    let report = client::run(&options(&url, target.path())).await.unwrap();

    assert_eq!(report.fetched, 2);
    assert_eq!(
        fetches.load(Ordering::SeqCst),
        2,
        "the old photos stayed put"
    );
    assert_eq!(trips_in(&target.path().join(DB_FILENAME)).await, 2);
    assert_eq!(files_under(&target.path().join(BLOBS_SUBDIR)).len(), 4);
}

#[tokio::test(flavor = "multi_thread")]
async fn us40_a_stale_wal_beside_the_old_database_does_not_reach_the_new_one() {
    // A backup opened as a data directory in between — a restore rehearsal
    // that was not shut down cleanly — leaves a WAL beside the database.
    // SQLite would replay it into whatever file carries that name next.
    let (app, _server_dir) = test_app().await;
    let url = serve(app).await;
    let target = tempfile::tempdir().unwrap();
    client::run(&options(&url, target.path())).await.unwrap();
    let db = target.path().join(DB_FILENAME);
    let wal = target.path().join(format!("{DB_FILENAME}-wal"));
    let pool = trip_archive::server::db::create_pool(&db).await.unwrap();
    sqlx::query("PRAGMA wal_autocheckpoint = 0")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tag (name) VALUES ('stale')")
        .execute(&pool)
        .await
        .unwrap();
    let stale = std::fs::read(&wal).unwrap();
    pool.close().await;
    std::fs::write(&wal, stale).unwrap();

    client::run(&options(&url, target.path())).await.unwrap();

    assert!(!wal.exists(), "the stale WAL was removed");
    let mut conn = SqliteConnectOptions::new()
        .filename(&db)
        .connect()
        .await
        .unwrap();
    let tags: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tag")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert_eq!(tags, 0, "the new database is the server's, untouched");
}

#[tokio::test(flavor = "multi_thread")]
async fn us40_the_photos_of_a_deleted_trip_leave_the_backup() {
    let (app, _server_dir) = test_app().await;
    import_trip_with_photo(&app).await;
    let url = serve(app.clone()).await;
    let target = tempfile::tempdir().unwrap();
    client::run(&options(&url, target.path())).await.unwrap();

    assert_eq!(
        delete(&app, "/api/trips/1").await.status(),
        StatusCode::NO_CONTENT
    );
    let report = client::run(&options(&url, target.path())).await.unwrap();

    assert_eq!(report.removed, 2);
    assert_eq!(trips_in(&target.path().join(DB_FILENAME)).await, 0);
    assert_eq!(
        files_under(&target.path().join(BLOBS_SUBDIR)),
        Vec::<PathBuf>::new()
    );
    assert!(
        !target.path().join(BLOBS_SUBDIR).join("trips/1").exists(),
        "no empty directories left behind"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn us40_a_failed_run_leaves_the_previous_backup_as_it_was() {
    let (app, _server_dir) = test_app().await;
    import_trip_with_photo(&app).await;
    let url = serve(app.clone()).await;
    let target = tempfile::tempdir().unwrap();
    client::run(&options(&url, target.path())).await.unwrap();
    let before = files_under(target.path());

    import_trip_with_photo(&app).await;
    let failing = serve(failing_media_of_trip(app, 2)).await;
    let result = client::run(&options(&failing, target.path())).await;

    assert!(
        matches!(result, Err(BackupError::Client(ClientError::Status { .. }))),
        "{result:?}"
    );
    assert_eq!(trips_in(&target.path().join(DB_FILENAME)).await, 1);
    assert_eq!(
        files_under(target.path()),
        before,
        "nothing added, nothing lost"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn us40_a_photo_missing_on_the_server_is_reported_not_fatal() {
    // A row whose blob is gone is damage on the server; the rest of the
    // archive still needs its backup, and the owner needs to hear about it.
    let (app, server_dir) = test_app().await;
    import_trip_with_photo(&app).await;
    let blobs = server_dir.path().join(common::TEST_BLOBS_SUBDIR);
    let original = files_under(&blobs)
        .into_iter()
        .find(|path| !path.starts_with("trips/1/thumbs"))
        .expect("the original photo");
    std::fs::remove_file(blobs.join(&original)).unwrap();
    let url = serve(app).await;
    let target = tempfile::tempdir().unwrap();

    let report = client::run(&options(&url, target.path())).await.unwrap();

    assert_eq!(report.missing, vec![original.to_str().unwrap().to_owned()]);
    assert_eq!(report.fetched, 1, "the thumbnail still arrived");
    assert_eq!(trips_in(&target.path().join(DB_FILENAME)).await, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn us40_a_wrong_password_is_refused_before_anything_is_written() {
    let (app, _server_dir) = test_app().await;
    let url = serve(app).await;
    let target = tempfile::tempdir().unwrap();
    let mut wrong = options(&url, target.path());
    wrong.password = "not the password".to_owned();

    let result = client::run(&wrong).await;

    assert!(
        matches!(result, Err(BackupError::Client(ClientError::Refused))),
        "{result:?}"
    );
    assert_eq!(files_under(target.path()), Vec::<PathBuf>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn us40_a_missing_target_directory_is_an_error_not_created() {
    // The target is an external disk: if it is not mounted, the mount point
    // (or its parent) is missing, and writing a backup onto the laptop's own
    // disk in its place would be a backup nobody finds.
    let (app, _server_dir) = test_app().await;
    let url = serve(app).await;
    let parent = tempfile::tempdir().unwrap();
    let missing = parent.path().join("not-mounted");

    let result = client::run(&options(&url, &missing)).await;

    assert!(
        matches!(result, Err(BackupError::NoTarget(_))),
        "{result:?}"
    );
    assert!(!missing.exists());
}
