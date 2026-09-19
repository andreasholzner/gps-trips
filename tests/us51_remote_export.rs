//! US-51 acceptance tests, exporter half: `qmapshack::run_export` reaching
//! the archive over HTTP (ADR-0022's 2026-09-19 amendment), against a real
//! served router. US-36/US-37's own criteria — what lands in the target and
//! how a re-run reconciles it — are asserted, unchanged, over the same
//! transport in their own files.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::IntoResponse,
    Router,
};
use common::{import_sample, serve, test_app, TEST_PASSWORD};
use trip_archive::server::qmapshack::{self, config::ExportConfig};

fn config(target: &Path) -> ExportConfig {
    let toml = format!(
        "url = \"https://archive.test\"\n\
         target_db = {:?}\nfolder_template = \"Trips/{{activity_type}}\"\n\
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

fn is_geometry(path: &str) -> bool {
    path.starts_with("/api/trips/") && path.ends_with("/track.geojson")
}

/// The router, counting the track geometries it serves.
fn counting_geometry(app: Router, count: Arc<AtomicUsize>) -> Router {
    app.layer(middleware::from_fn(move |request: Request, next: Next| {
        let count = count.clone();
        async move {
            if is_geometry(request.uri().path()) {
                count.fetch_add(1, Ordering::SeqCst);
            }
            next.run(request).await
        }
    }))
}

/// The router, answering the export list with `status` and `body` instead.
fn replacing_list(app: Router, status: StatusCode, body: &'static str) -> Router {
    app.layer(middleware::from_fn(move |request: Request, next: Next| {
        let replaced = request.uri().path() == "/api/export/trips";
        async move {
            if replaced {
                (status, body).into_response()
            } else {
                next.run(request).await
            }
        }
    }))
}

/// The router, as if trip `trip_id` were deleted after the list was read:
/// listed, but its geometry answers 404.
fn geometry_gone_for(app: Router, trip_id: i64) -> Router {
    let gone = format!("/api/trips/{trip_id}/track.geojson");
    app.layer(middleware::from_fn(move |request: Request, next: Next| {
        let is_gone = request.uri().path() == gone;
        async move {
            if is_gone {
                StatusCode::NOT_FOUND.into_response()
            } else {
                next.run(request).await
            }
        }
    }))
}

/// Every file in `dir`, with its contents: the target and its backups.
fn snapshot_of(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    files.sort();
    files
}

#[tokio::test]
async fn us51_an_unchanged_trip_has_no_geometry_fetched() {
    let (app, _dir) = test_app().await;
    import_sample(&app).await;
    import_sample(&app).await;
    let count = Arc::new(AtomicUsize::new(0));
    let url = serve(counting_geometry(app, count.clone())).await;
    let out = tempfile::tempdir().unwrap();
    let cfg = config(&out.path().join("export.db"));

    let first = qmapshack::run_export(&url, TEST_PASSWORD, &cfg)
        .await
        .unwrap();
    assert_eq!(first.inserted, 2);
    assert_eq!(count.load(Ordering::SeqCst), 2, "one geometry per new trip");

    let second = qmapshack::run_export(&url, TEST_PASSWORD, &cfg)
        .await
        .unwrap();
    assert_eq!(second.skipped, 2);
    assert_eq!(count.load(Ordering::SeqCst), 2, "none for unchanged trips");
}

#[tokio::test]
async fn us51_a_run_without_the_complete_list_leaves_the_target_untouched() {
    let (app, _dir) = test_app().await;
    import_sample(&app).await;
    let out = tempfile::tempdir().unwrap();
    let cfg = config(&out.path().join("export.db"));
    let url = serve(app.clone()).await;
    qmapshack::run_export(&url, TEST_PASSWORD, &cfg)
        .await
        .unwrap();
    let before = snapshot_of(out.path());

    // Nothing listens here: the listener is dropped as soon as bound.
    let unreachable = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    };
    let failing = [
        ("unreachable", unreachable, TEST_PASSWORD),
        ("wrong password", url.clone(), "not the password"),
        (
            "error status",
            serve(replacing_list(
                app.clone(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "",
            ))
            .await,
            TEST_PASSWORD,
        ),
        (
            "truncated list",
            serve(replacing_list(
                app.clone(),
                StatusCode::OK,
                r#"[{"id":1,"name":"Tur""#,
            ))
            .await,
            TEST_PASSWORD,
        ),
        (
            "malformed list",
            serve(replacing_list(app.clone(), StatusCode::OK, "<html>")).await,
            TEST_PASSWORD,
        ),
    ];
    for (case, url, password) in failing {
        let result = qmapshack::run_export(&url, password, &cfg).await;
        assert!(result.is_err(), "{case}: {result:?}");
        assert!(
            snapshot_of(out.path()) == before,
            "{case}: the target was touched, or backed up"
        );
    }
}

#[tokio::test]
async fn us51_a_run_while_sign_ins_are_locked_out_leaves_the_target_untouched() {
    let (app, _dir) = test_app().await;
    import_sample(&app).await;
    let out = tempfile::tempdir().unwrap();
    let cfg = config(&out.path().join("export.db"));
    let url = serve(app).await;
    qmapshack::run_export(&url, TEST_PASSWORD, &cfg)
        .await
        .unwrap();
    let before = snapshot_of(out.path());

    for _ in 0..5 {
        let _ = qmapshack::run_export(&url, "not the password", &cfg).await;
    }
    let result = qmapshack::run_export(&url, TEST_PASSWORD, &cfg).await;

    let err = result.expect_err("sign-ins are locked out");
    assert!(
        format!("{err:#}").contains("too many failed attempts"),
        "{err:#}"
    );
    assert!(snapshot_of(out.path()) == before);
}

#[tokio::test]
async fn us51_a_trip_deleted_during_the_run_fails_alone_and_the_next_run_completes() {
    let (app, _dir) = test_app().await;
    let kept = import_sample(&app).await;
    let deleted = import_sample(&app).await;
    let out = tempfile::tempdir().unwrap();
    let cfg = config(&out.path().join("export.db"));

    let url = serve(geometry_gone_for(app.clone(), deleted)).await;
    let first = qmapshack::run_export(&url, TEST_PASSWORD, &cfg)
        .await
        .unwrap();
    assert_eq!((first.inserted, first.failed), (1, 1), "trip {kept} alone");

    let url = serve(app).await;
    let second = qmapshack::run_export(&url, TEST_PASSWORD, &cfg)
        .await
        .unwrap();
    assert_eq!((second.inserted, second.skipped, second.failed), (1, 1, 0));
}
