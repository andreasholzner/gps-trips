//! US-32 acceptance tests — "when I list all trips, planned and recorded
//! trips are listed separately".
//!
//! Acceptance criteria (docs/requirements.md, clarified during planning):
//! the trip list page (`GET /`) shows a Recorded/Planned tab, defaulting to
//! Recorded; switching tabs preserves any active filters (US-13); the JSON
//! list (`GET /api/trips`) is unaffected unless `?kind=` is explicitly given.
//!
//! No import path can create a planned trip yet (that's US-31) — tests seed
//! one directly via SQL, the same technique the repo-level tests use.

mod common;

use axum::http::StatusCode;
use common::{body_string, get, import_sample, test_app_with_state};

async fn seed_planned_trip(pool: &sqlx::SqlitePool, name: &str) {
    sqlx::query(
        r#"INSERT INTO trip (name, activity_type, tz_name, distance_m, created_at, trip_kind)
           VALUES (?, 'hiking', 'Europe/Oslo', 1000.0, '2024-01-01T00:00:00Z', 'planned')"#,
    )
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn us32_json_api_returns_both_kinds_when_kind_is_not_given() {
    let (app, state, _dir) = test_app_with_state(None).await;
    import_sample(&app).await;
    seed_planned_trip(&state.pool, "Dream Trip").await;

    let response = get(&app, "/api/trips").await;
    let body = body_string(response).await;
    let trips: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap();
    assert_eq!(trips.len(), 2, "got: {body}");
}

#[tokio::test]
async fn us32_json_api_filters_by_kind_when_given() {
    let (app, state, _dir) = test_app_with_state(None).await;
    import_sample(&app).await;
    seed_planned_trip(&state.pool, "Dream Trip").await;

    let body = body_string(get(&app, "/api/trips?kind=planned").await).await;
    let trips: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap();
    assert_eq!(trips.len(), 1);
    assert_eq!(trips[0]["name"], "Dream Trip");
    assert_eq!(trips[0]["trip_kind"], "planned");
}

#[tokio::test]
async fn us32_an_unrecognized_kind_filter_is_rejected_with_400() {
    let (app, _state, _dir) = test_app_with_state(None).await;

    let response = get(&app, "/api/trips?kind=hypothetical").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
