//! The Dioxus SPA's built bundle, served by the app itself (US-41, ADR-0024):
//! a client-side-rendered SPA served as static files by Axum, mounted at
//! `/app` from an `app/` folder inside the assets directory, so the
//! deployable unit stays "binary + adjacent `public/`" (ADR-0016) with
//! nothing new to configure. The server-rendered pages at `/` are untouched
//! and keep their own tests until US-52 retires the list page (ADR-0012's
//! migration rule).

mod common;

use axum::http::StatusCode;

/// Env vars are process-global; serialize the tests that set one.
static ASSETS_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const INDEX_HTML: &[u8] = b"<!DOCTYPE html><title>Trip Archive</title><div id=\"main\"></div>";

/// An assets dir holding a stand-in SPA bundle: `app/index.html` plus one
/// hashed asset, mirroring what `dx build` emits.
fn assets_dir_with_bundle() -> tempfile::TempDir {
    let assets = tempfile::tempdir().expect("assets dir");
    std::fs::create_dir_all(assets.path().join("app/wasm")).unwrap();
    std::fs::write(assets.path().join("app/index.html"), INDEX_HTML).unwrap();
    std::fs::write(assets.path().join("app/wasm/ui_dioxus.js"), b"// glue").unwrap();
    assets
}

#[tokio::test]
async fn the_spa_bundle_is_served_at_app() {
    let _guard = ASSETS_ENV_LOCK.lock().await;
    let assets = assets_dir_with_bundle();

    std::env::set_var("TRIP_ARCHIVE_ASSETS_DIR", assets.path());
    let (app, _db_dir) = common::test_app().await;
    let index = common::get(&app, "/app/").await;
    let glue = common::get(&app, "/app/wasm/ui_dioxus.js").await;
    std::env::remove_var("TRIP_ARCHIVE_ASSETS_DIR");

    assert_eq!(index.status(), StatusCode::OK);
    assert_eq!(common::body_bytes(index).await, INDEX_HTML);
    assert_eq!(glue.status(), StatusCode::OK);
}

#[tokio::test]
async fn the_archives_home_sends_the_owner_to_the_spa() {
    // US-52 retired the server-rendered trip list, which was `/`. The SPA is
    // the archive's home now, so an old bookmark lands on it rather than on
    // a 404. The remaining proof-of-concept pages — the trip detail, the
    // import form, the Komoot review — are untouched and stay reachable
    // until US-42/43/44 replace them.
    let (app, _db_dir) = common::test_app().await;

    let response = common::get(&app, "/").await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers()["location"],
        "/app/",
        "the home page must lead to the SPA"
    );
}

#[tokio::test]
async fn a_deep_link_into_the_spa_falls_back_to_its_index() {
    // The SPA routes paths like `/app/trips/:id` client-side, so opening or
    // reloading such a URL must still hand the browser the app shell rather
    // than a 404.
    let _guard = ASSETS_ENV_LOCK.lock().await;
    let assets = assets_dir_with_bundle();

    std::env::set_var("TRIP_ARCHIVE_ASSETS_DIR", assets.path());
    let (app, _db_dir) = common::test_app().await;
    let response = common::get(&app, "/app/trips/42").await;
    std::env::remove_var("TRIP_ARCHIVE_ASSETS_DIR");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(common::body_bytes(response).await, INDEX_HTML);
}
