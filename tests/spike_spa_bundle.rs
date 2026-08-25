//! The Dioxus spike's built bundle, served by the app itself (docs/dioxus-spike.md).
//!
//! The decided target shape is a client-side-rendered SPA served as static
//! files by Axum, so the spike has to prove that shape works — not just
//! `dx serve`'s dev server. The bundle is mounted at `/app`, from an `app/`
//! folder inside the assets directory, so the deployable unit stays "binary +
//! adjacent `public/`" (ADR-0016) with nothing new to configure.
//!
//! Not a user story: this file goes away with the spike, whichever framework
//! wins.

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
async fn a_deep_link_into_the_spa_falls_back_to_its_index() {
    // The SPA routes `/app/trips/:id` client-side, so opening or reloading
    // that URL must still hand the browser the app shell rather than a 404.
    let _guard = ASSETS_ENV_LOCK.lock().await;
    let assets = assets_dir_with_bundle();

    std::env::set_var("TRIP_ARCHIVE_ASSETS_DIR", assets.path());
    let (app, _db_dir) = common::test_app().await;
    let response = common::get(&app, "/app/trips/42").await;
    std::env::remove_var("TRIP_ARCHIVE_ASSETS_DIR");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(common::body_bytes(response).await, INDEX_HTML);
}

#[tokio::test]
async fn the_server_rendered_pages_are_untouched_by_the_spike() {
    // The spike is additive: `/` and `/trips/:id` keep serving the existing
    // PoC UI while both live side by side.
    let (app, _db_dir) = common::test_app().await;
    let id = common::import_sample(&app).await;

    let list = common::get(&app, "/").await;
    let detail = common::get(&app, &format!("/trips/{id}")).await;

    assert_eq!(list.status(), StatusCode::OK);
    assert!(common::body_string(list).await.contains("<h1>Trips</h1>"));
    assert_eq!(detail.status(), StatusCode::OK);
    assert!(common::body_string(detail)
        .await
        .contains(&common::detail_name_fragment("Oslo Hills Walk")));
}
