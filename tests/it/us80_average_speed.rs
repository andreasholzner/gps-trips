//! US-80 acceptance tests — the server's half of the average speed.
//!
//! The speeds themselves are worked out in the SPA (`crates/ui-dioxus`):
//! over one trip on its page, and over many on the Statistics and Summary
//! screens, always as a moving distance over a moving time. What stays here
//! is the moving distance they are worked out from:
//!
//! * *counted over the stretches the moving time counts, so a break lowers
//!   neither* — `us80_the_moving_distance_is_counted_where_the_moving_time_is`
//!   (and `server::moving_time`'s unit tests).
//! * *computed at import, again when the activity changes, and backfilled* —
//!   `us80_the_moving_distance_follows_the_activity`,
//!   `us80_trips_stored_before_the_moving_distance_existed_are_backfilled`.
//! * *a trip without timestamps has no average speed* —
//!   `us80_a_trip_without_times_has_no_moving_distance`.
//! * *the trip page, the Statistics and Summary screens, and their shared
//!   counterparts read it* — `us80_every_screen_that_shows_a_speed_is_served_the_figures`.
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};

use crate::common::{
    body_string, get, import_request_with_fields, json_request, send, send_unauthenticated,
    test_app, test_app_with_state, trip_id_from_redirect, SAMPLE_GPX,
};

const UNTIMED_GPX: &[u8] = include_bytes!("../fixtures/untimed.gpx");

async fn import_with(app: &axum::Router, gpx: &[u8], fields: &[(&str, &str)]) -> i64 {
    let response = send(app, import_request_with_fields(gpx, fields, &[])).await;
    trip_id_from_redirect(&response)
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::OK, "got {body}");
    serde_json::from_str(&body).unwrap()
}

async fn detail(app: &axum::Router, id: i64) -> serde_json::Value {
    json(get(app, &format!("/api/trips/{id}")).await).await
}

async fn change_activity(app: &axum::Router, id: i64, activity: &str) {
    let response = send(
        app,
        json_request(
            Method::PATCH,
            &format!("/api/trips/{id}"),
            &format!(r#"{{"activity_type":"{activity}"}}"#),
        ),
    )
    .await;
    assert!(response.status().is_success(), "{}", response.status());
}

fn metres(trip: &serde_json::Value) -> f64 {
    trip["moving_distance_m"]
        .as_f64()
        .unwrap_or_else(|| panic!("no moving distance in {trip}"))
}

#[tokio::test]
async fn us80_the_moving_distance_is_counted_where_the_moving_time_is() {
    let (app, _dir) = test_app().await;
    let id = import_with(&app, SAMPLE_GPX, &[("activity_type", "hiking")]).await;

    let trip = detail(&app, id).await;

    // Both legs reach hiking's 1 km/h: the whole walk moved.
    assert_eq!(trip["moving_secs"], 3600);
    let whole = trip["distance_m"].as_f64().unwrap();
    assert!((metres(&trip) - whole).abs() < 0.01, "{trip}");
}

#[tokio::test]
async fn us80_the_moving_distance_follows_the_activity() {
    let (app, _dir) = test_app().await;
    let id = import_with(&app, SAMPLE_GPX, &[("activity_type", "hiking")]).await;
    let whole = metres(&detail(&app, id).await);

    // Only the ~1.6 km/h leg reaches kayaking's 1.5 km/h.
    change_activity(&app, id, "kayaking").await;
    let trip = detail(&app, id).await;
    assert_eq!(trip["moving_secs"], 1800);
    assert!(metres(&trip) > 0.0 && metres(&trip) < whole, "{trip}");

    // Neither reaches cycling's 3 km/h.
    change_activity(&app, id, "cycling").await;
    assert_eq!(metres(&detail(&app, id).await), 0.0);
}

#[tokio::test]
async fn us80_a_trip_without_times_has_no_moving_distance() {
    let (app, _dir) = test_app().await;
    let id = import_with(&app, UNTIMED_GPX, &[]).await;

    let trip = detail(&app, id).await;

    assert_eq!(trip["moving_distance_m"], serde_json::Value::Null);
    assert_eq!(trip["moving_secs"], serde_json::Value::Null);
}

#[tokio::test]
async fn us80_trips_stored_before_the_moving_distance_existed_are_backfilled() {
    let (app, state, _dir) = test_app_with_state(None).await;
    let id = import_with(&app, SAMPLE_GPX, &[("activity_type", "hiking")]).await;
    let expected = metres(&detail(&app, id).await);
    // As the migration leaves a trip stored before it: moving time, no distance.
    sqlx::query("UPDATE trip SET moving_distance_m = NULL")
        .execute(&state.pool)
        .await
        .unwrap();

    let filled = trip_archive::server::repo::backfill_moving_secs(&state.pool)
        .await
        .unwrap();

    assert_eq!(filled, 1);
    assert_eq!(metres(&detail(&app, id).await), expected);
}

#[tokio::test]
async fn us80_every_screen_that_shows_a_speed_is_served_the_figures() {
    let (app, _dir) = test_app().await;
    let id = import_with(&app, SAMPLE_GPX, &[("activity_type", "hiking")]).await;
    let expected = metres(&detail(&app, id).await);
    let tagged = send(
        &app,
        json_request(
            Method::POST,
            &format!("/api/trips/{id}/tags"),
            r#"{"name":"alps"}"#,
        ),
    )
    .await;
    assert_eq!(tagged.status(), StatusCode::CREATED);

    let stats = json(get(&app, "/api/stats/trips").await).await;
    assert_eq!(metres(&stats["trips"][0]), expected);
    let summary = json(get(&app, "/api/stats/tags?tags=alps").await).await;
    assert_eq!(metres(&summary["trips"][0]), expected);

    // The recipient's trip page and summary.
    let share =
        |body: serde_json::Value| json_request(Method::POST, "/api/shares", &body.to_string());
    let visit = |uri: String| Request::builder().uri(uri).body(Body::empty()).unwrap();
    for (body, summarised) in [
        (serde_json::json!({ "trip_ids": [id] }), false),
        (serde_json::json!({ "tags": ["alps"] }), true),
    ] {
        let created = json_created(send(&app, share(body)).await).await;
        let token = created["token"].as_str().unwrap();
        let overview =
            json(send_unauthenticated(&app, visit(format!("/s/{token}/api/share"))).await).await;
        if summarised {
            assert_eq!(
                metres(&overview["summary"]["trips"][0]),
                expected,
                "{overview}"
            );
        }
        let trip =
            json(send_unauthenticated(&app, visit(format!("/s/{token}/api/trips/{id}"))).await)
                .await;
        assert_eq!(trip["moving_secs"], 3600, "{trip}");
        assert_eq!(metres(&trip), expected);
    }
}

async fn json_created(response: axum::response::Response) -> serde_json::Value {
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::CREATED, "got {body}");
    serde_json::from_str(&body).unwrap()
}
