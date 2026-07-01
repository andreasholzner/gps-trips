//! US-10 — self-host the whole thing on my own machine.
//!
//! Acceptance criteria: single deployable binary + static assets; all data under a
//! configurable data directory; no external services required.
//!
//! The DB/blob-store side of "configurable data directory" is already exercised by every
//! other test via `tests/common::test_app` (a fresh `tempdir` per test, ADR-0012). This file
//! covers the piece specific to US-10: static assets must be resolvable independent of the
//! process's current working directory (ADR-0016), not hardcoded to a `public/` folder in
//! the CWD. `src/server/paths.rs` unit-tests the pure resolution logic; this proves
//! the real router serves from wherever `TRIP_ARCHIVE_ASSETS_DIR` points.

mod common;

use axum::http::StatusCode;

/// Env vars are process-global; serialize tests that touch `TRIP_ARCHIVE_ASSETS_DIR`.
/// Tokio's mutex (not `std::sync::Mutex`) because the guard is held across `.await`.
static ASSETS_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn us10_serves_static_assets_from_a_configured_dir_independent_of_cwd() {
    let _guard = ASSETS_ENV_LOCK.lock().await;

    // A stand-in assets dir that is emphatically not the crate's `public/`.
    let assets = tempfile::tempdir().expect("assets dir");
    std::fs::create_dir_all(assets.path().join("vendor")).unwrap();
    std::fs::write(
        assets.path().join("vendor/leaflet.css"),
        b"/* fixture css */",
    )
    .unwrap();

    std::env::set_var("TRIP_ARCHIVE_ASSETS_DIR", assets.path());
    let (app, _db_dir) = common::test_app().await;
    let response = common::get(&app, "/static/vendor/leaflet.css").await;
    std::env::remove_var("TRIP_ARCHIVE_ASSETS_DIR");

    assert_eq!(response.status(), StatusCode::OK);
    let body = common::body_string(response).await;
    assert_eq!(body, "/* fixture css */");
}

#[tokio::test]
async fn us10_missing_assets_dir_yields_404_not_a_panic() {
    let _guard = ASSETS_ENV_LOCK.lock().await;

    // A path that is never created on disk, e.g. a typo'd env var in a real deployment.
    let parent = tempfile::tempdir().expect("temp dir");
    let nonexistent = parent.path().join("does-not-exist");
    std::env::set_var("TRIP_ARCHIVE_ASSETS_DIR", &nonexistent);
    let (app, _db_dir) = common::test_app().await;
    let response = common::get(&app, "/static/vendor/leaflet.css").await;
    std::env::remove_var("TRIP_ARCHIVE_ASSETS_DIR");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
