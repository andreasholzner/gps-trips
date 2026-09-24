//! Installing the archive on an Android home screen (US-67, ADR-0023): the
//! SPA carries a web app manifest, which is what makes Chrome offer to
//! install it, and what the installed app takes its name, icon, start screen
//! and full-screen display from. The files checked here are the real ones
//! from the SPA crate's `public/`, which `dx build` copies into the bundle
//! verbatim, so they keep the URLs the manifest and `index.html` give them.

use std::path::{Path, PathBuf};

use crate::common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};

/// The SPA crate's verbatim-copied files.
fn spa_public() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/ui-dioxus/public")
}

fn manifest() -> serde_json::Value {
    let text = std::fs::read_to_string(spa_public().join("manifest.webmanifest"))
        .expect("the SPA ships a manifest");
    serde_json::from_str(&text).expect("the manifest is JSON")
}

/// A PNG's width and height, from its `IHDR` chunk — which the format puts
/// first, straight after the 8-byte signature.
fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    assert_eq!(&bytes[12..16], b"IHDR");
    let be = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    (be(16), be(20))
}

#[test]
fn the_installed_app_has_its_own_name_and_opens_full_screen_on_the_trip_list() {
    let manifest = manifest();

    assert_eq!(manifest["name"], "Trip Archive");
    // What the home screen shows under the icon.
    assert_eq!(manifest["short_name"], "Trip Archive");
    // The trip list, which is also where the login screen appears once the
    // session has ended — the SPA decides that, whatever the route.
    assert_eq!(manifest["start_url"], "/app/");
    // Navigation outside the SPA would open the browser's controls again.
    assert_eq!(manifest["scope"], "/app/");
    assert_eq!(manifest["display"], "standalone");
    assert!(manifest["theme_color"].is_string());
    assert!(manifest["background_color"].is_string());
}

#[test]
fn the_installed_app_has_its_own_icon_in_every_size_chrome_asks_for() {
    let manifest = manifest();
    let icons = manifest["icons"].as_array().expect("icons");

    let mut declared = Vec::new();
    for icon in icons {
        let src = icon["src"].as_str().expect("src");
        let file = src
            .strip_prefix("/app/")
            .unwrap_or_else(|| panic!("{src} is not in the bundle"));
        let bytes = std::fs::read(spa_public().join(file))
            .unwrap_or_else(|err| panic!("{src} is declared but missing: {err}"));
        let (width, height) = png_size(&bytes);
        assert_eq!(icon["type"], "image/png", "{src}");
        assert_eq!(
            icon["sizes"],
            format!("{width}x{height}"),
            "{src} is declared at a size it is not"
        );
        let purpose = icon["purpose"].as_str().unwrap_or("any");
        declared.push((width, purpose.to_string()));
    }

    // Chrome's installability minimum, plus the one Android's launcher can
    // crop into its own shape instead of framing it on a white disc.
    for wanted in [(192, "any"), (512, "any"), (512, "maskable")] {
        assert!(
            declared.contains(&(wanted.0, wanted.1.to_string())),
            "no {wanted:?} icon among {declared:?}"
        );
    }
}

#[test]
fn the_app_shell_points_the_browser_at_the_manifest() {
    let index = std::fs::read_to_string(spa_public().join("../index.html")).unwrap();

    assert!(
        index.contains(r#"<link rel="manifest" href="/app/manifest.webmanifest""#),
        "{index}"
    );
    assert!(index.contains(r#"<meta name="theme-color""#), "{index}");
}

#[tokio::test]
async fn the_manifest_and_its_icons_are_served_before_signing_in() {
    // Chrome fetches them without the session cookie, and the app has to be
    // installable from its login screen as much as from anywhere else.
    let _guard = common::ASSETS_ENV_LOCK.lock().await;
    let assets = tempfile::tempdir().expect("assets dir");
    let app_dir = assets.path().join("app");
    std::fs::create_dir_all(&app_dir).unwrap();
    for file in ["manifest.webmanifest", "icon-192.png"] {
        std::fs::copy(spa_public().join(file), app_dir.join(file)).unwrap();
    }

    std::env::set_var("TRIP_ARCHIVE_ASSETS_DIR", assets.path());
    let (app, _db_dir) = common::test_app().await;
    let get = |uri: &str| Request::builder().uri(uri).body(Body::empty()).unwrap();
    let manifest = common::send_unauthenticated(&app, get("/app/manifest.webmanifest")).await;
    let icon = common::send_unauthenticated(&app, get("/app/icon-192.png")).await;
    std::env::remove_var("TRIP_ARCHIVE_ASSETS_DIR");

    assert_eq!(manifest.status(), StatusCode::OK);
    assert_eq!(
        manifest.headers()[header::CONTENT_TYPE],
        "application/manifest+json"
    );
    assert_eq!(icon.status(), StatusCode::OK);
    assert_eq!(icon.headers()[header::CONTENT_TYPE], "image/png");
}
