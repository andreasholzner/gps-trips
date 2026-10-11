//! US-89 acceptance tests, server half: `GET /api/export/collections`, the
//! collections the QMapShack exporter reconciles against (ADR-0022's
//! 2026-10-11 amendment). Its removal passes treat a track or waypoint
//! missing from this list as deleted, so the list holds every collection —
//! never paged, never filtered. That it needs a session is part of US-19's
//! table of routes.

use crate::common::{body_string, delete, get, import_collection, test_app, GUIDE_BOOK_GPX};
use axum::http::StatusCode;
use trip_archive::models::{ActivityType, ExportCollection};

async fn export_list(app: &axum::Router) -> Vec<ExportCollection> {
    let response = get(app, "/api/export/collections").await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_str(&body_string(response).await).expect("a JSON list of collections")
}

#[tokio::test]
async fn us89_the_export_list_holds_every_collection_with_its_tracks_and_waypoints() {
    let (app, _dir) = test_app().await;
    let book = import_collection(&app, GUIDE_BOOK_GPX, &[("activity_type", "cycling")]).await;
    let other = import_collection(&app, GUIDE_BOOK_GPX, &[("name", "Other")]).await;

    let collections = export_list(&app).await;

    assert_eq!(
        collections.iter().map(|c| c.id).collect::<Vec<_>>(),
        [book, other]
    );
    let first = &collections[0];
    assert_eq!(first.name, "Lakes & Passes");
    assert!(!first.tracks.is_empty());
    assert!(first
        .tracks
        .iter()
        .all(|t| t.activity_type == ActivityType::Cycling));
    let pass = &first.waypoints[0];
    assert_eq!(pass.name.as_deref(), Some("San Bernardino Pass"));
    assert_eq!(pass.symbol.as_deref(), Some("Triangle, Blue"));
    assert_eq!(pass.ele, Some(2065.0));
    assert_eq!(first.waypoints[1].symbol, None);
    assert_eq!(collections[1].name, "Other");
}

#[tokio::test]
async fn us89_a_deleted_collection_leaves_the_export_list() {
    let (app, _dir) = test_app().await;
    let book = import_collection(&app, GUIDE_BOOK_GPX, &[]).await;
    assert_eq!(
        delete(&app, &format!("/api/collections/{book}"))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );

    assert!(export_list(&app).await.is_empty());
}
