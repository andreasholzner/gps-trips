//! US-68 — as the owner, I can see which version of the archive I'm
//! running, and whether my open page is older than the server.
//!
//! The server's half: it reports the version it was built with through the
//! JSON API (ADR-0008), behind the same login as every other route — the
//! latter asserted in `us19_auth`'s route table. The comparison, and the
//! colour it picks, are the SPA's and tested in its crate.

use crate::common::{body_string, get, test_app};
use axum::http::StatusCode;
use trip_archive::models::{AppVersion, VERSION};

#[tokio::test]
async fn us68_the_server_reports_the_version_it_was_built_with() {
    let (app, _dir) = test_app().await;

    let response = get(&app, "/api/version").await;

    assert_eq!(response.status(), StatusCode::OK);
    let reported: AppVersion = serde_json::from_str(&body_string(response).await).unwrap();
    assert_eq!(reported.version, VERSION);
}
