//! The Dioxus SPA's built bundle, served by the app itself (US-41, ADR-0024):
//! a client-side-rendered SPA served as static files by Axum, mounted at
//! `/app` from an `app/` folder inside the assets directory, so the
//! deployable unit stays "binary + adjacent `public/`" (ADR-0016) with
//! nothing new to configure. Since US-44 there are no server-rendered pages
//! left beside it: every path one of them answered is a redirect into the
//! SPA, so a bookmark made before it went still reaches the screen that
//! replaced it.

use crate::common;

use axum::http::{header, StatusCode};

const INDEX_HTML: &[u8] = b"<!DOCTYPE html><title>Trip Archive</title><div id=\"main\"></div>";

/// A content-hashed file, named the way `dx build` names them.
const HASHED_ASSET: &str = "/app/assets/ui-dioxus-dxh0123456789abcdef.js";

/// An assets dir holding a stand-in SPA bundle, mirroring what `dx build`
/// emits: `app/index.html`, a file copied verbatim from the crate's
/// `public/`, and a content-hashed asset under `app/assets/`.
fn assets_dir_with_bundle() -> tempfile::TempDir {
    let assets = tempfile::tempdir().expect("assets dir");
    std::fs::create_dir_all(assets.path().join("app/assets")).unwrap();
    std::fs::write(assets.path().join("app/index.html"), INDEX_HTML).unwrap();
    std::fs::write(assets.path().join("app/manifest.webmanifest"), b"{}").unwrap();
    let hashed = HASHED_ASSET.strip_prefix("/app/").unwrap();
    std::fs::write(assets.path().join("app").join(hashed), b"// glue").unwrap();
    assets
}

/// GET each of `uris` from an app serving [`assets_dir_with_bundle`].
async fn get_from_bundle(uris: &[&str]) -> Vec<axum::response::Response> {
    let _guard = common::ASSETS_ENV_LOCK.lock().await;
    let assets = assets_dir_with_bundle();

    std::env::set_var("TRIP_ARCHIVE_ASSETS_DIR", assets.path());
    let (app, _db_dir) = common::test_app().await;
    let mut responses = Vec::new();
    for uri in uris {
        responses.push(common::get(&app, uri).await);
    }
    std::env::remove_var("TRIP_ARCHIVE_ASSETS_DIR");
    responses
}

#[tokio::test]
async fn the_spa_bundle_is_served_at_app() {
    let _guard = common::ASSETS_ENV_LOCK.lock().await;
    let assets = assets_dir_with_bundle();

    std::env::set_var("TRIP_ARCHIVE_ASSETS_DIR", assets.path());
    let (app, _db_dir) = common::test_app().await;
    let index = common::get(&app, "/app/").await;
    let glue = common::get(&app, HASHED_ASSET).await;
    std::env::remove_var("TRIP_ARCHIVE_ASSETS_DIR");

    assert_eq!(index.status(), StatusCode::OK);
    assert_eq!(common::body_bytes(index).await, INDEX_HTML);
    assert_eq!(glue.status(), StatusCode::OK);
}

#[tokio::test]
async fn the_archives_home_sends_the_owner_to_the_spa() {
    // US-52 retired the server-rendered trip list, which was `/`. The SPA is
    // the archive's home now, so an old bookmark lands on it rather than on
    // a 404 — and since US-44 took the last of the other pages, it is the
    // only UI there is.
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
async fn an_old_bookmark_to_a_trip_leads_to_its_screen_in_the_spa() {
    // US-42 deleted the server-rendered detail page. `/` got a redirect when
    // its page went (US-52) so old bookmarks kept working; a bookmarked or
    // shared trip is the same kind of URL and gets the same treatment.
    let (app, _db_dir) = common::test_app().await;

    let response = common::get(&app, "/trips/42").await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], "/app/trips/42");
}

#[tokio::test]
async fn an_old_bookmark_to_the_komoot_review_leads_to_its_screen_in_the_spa() {
    // US-44 deleted the last server-rendered page. Same treatment as the
    // three before it, and the path is unchanged under `/app`, so a
    // bookmark is one prefix away from where it always went.
    let (app, _db_dir) = common::test_app().await;

    let response = common::get(&app, "/komoot/sync").await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], "/app/komoot/sync");
}

#[tokio::test]
async fn a_deep_link_into_the_spa_falls_back_to_its_index() {
    // The SPA routes paths like `/app/trips/:id` client-side, so opening or
    // reloading such a URL must still hand the browser the app shell rather
    // than a 404.
    let _guard = common::ASSETS_ENV_LOCK.lock().await;
    let assets = assets_dir_with_bundle();

    std::env::set_var("TRIP_ARCHIVE_ASSETS_DIR", assets.path());
    let (app, _db_dir) = common::test_app().await;
    let response = common::get(&app, "/app/trips/42").await;
    std::env::remove_var("TRIP_ARCHIVE_ASSETS_DIR");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(common::body_bytes(response).await, INDEX_HTML);
}

#[tokio::test]
async fn a_deploy_reaches_the_browser_on_its_next_load() {
    // The app shell, and every other file whose name stays the same across
    // deploys, is checked with the server each time it is used. Without a
    // `Cache-Control` a browser may guess at a lifetime for it, and keep
    // running — or fail to start — the previous deploy's app for hours
    // after a new one (US-67: the installed app is as current as its last
    // deploy).
    let responses = get_from_bundle(&["/app/", "/app/trips/42", "/app/manifest.webmanifest"]).await;

    for response in responses {
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
    }
}

#[tokio::test]
async fn a_content_hashed_file_is_kept_by_the_browser() {
    // Its name changes whenever its content does, so a cached copy is never
    // out of date.
    let response = get_from_bundle(&[HASHED_ASSET]).await.remove(0);

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
}

#[tokio::test]
async fn a_hashed_file_from_an_earlier_deploy_is_not_found() {
    // An app shell cached before a deploy asks for files that deploy
    // replaced. They must not come back as the app shell — HTML where a
    // script was expected — let alone as something to keep for a year.
    let response = get_from_bundle(&["/app/assets/ui-dioxus-dxhfeedfeedfeedfeed.js"])
        .await
        .remove(0);

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(response.headers().get(header::CACHE_CONTROL).is_none());
}
