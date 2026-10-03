//! US-77 acceptance tests — the server's half of the statistics screen.
//!
//! The screen's figures are added up in the SPA (`crates/ui-dioxus`,
//! `stats`). What stays here is what it adds them up from:
//! `GET /api/stats/trips` answers every dated recorded trip with its local
//! dates, distance, ascent and moving time, and how many recorded trips had
//! no dates to count. Moving time is stored with each trip, computed under its
//! activity's threshold (`config::moving_time`), kept up to date when that
//! activity changes, and backfilled for trips imported before it existed.
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

use axum::http::{Method, StatusCode};

use crate::common::{
    body_string, get, import, import_request_with_fields, json_request, send, send_unauthenticated,
    test_app, test_app_with_state, trip_id_from_redirect, LATE_EVENING_GPX, SAMPLE_GPX,
};

const UNTIMED_GPX: &[u8] = include_bytes!("../fixtures/untimed.gpx");

async fn stats(app: &axum::Router) -> serde_json::Value {
    let response = get(app, "/api/stats/trips").await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_str(&body_string(response).await).unwrap()
}

async fn import_with(app: &axum::Router, gpx: &[u8], fields: &[(&str, &str)]) -> i64 {
    let response = send(app, import_request_with_fields(gpx, fields, &[])).await;
    trip_id_from_redirect(&response)
}

/// The stats row of trip `id`.
fn row(stats: &serde_json::Value, id: i64) -> serde_json::Value {
    stats["trips"]
        .as_array()
        .unwrap()
        .iter()
        .find(|trip| trip["id"] == id)
        .unwrap_or_else(|| panic!("trip {id} missing from {stats}"))
        .clone()
}

/// A two-point track near Oslo, from `start` to `end` (RFC 3339).
fn track_between(start: &str, end: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="trip-archive-test" xmlns="http://www.topografix.com/GPX/1/1">
  <trk><name>Long Way</name><trkseg>
    <trkpt lat="59.9139" lon="10.7522"><ele>10.0</ele><time>{start}</time></trkpt>
    <trkpt lat="60.9139" lon="10.7522"><ele>90.0</ele><time>{end}</time></trkpt>
  </trkseg></trk>
</gpx>"#
    )
    .into_bytes()
}

#[tokio::test]
async fn us77_each_dated_recorded_trip_comes_with_what_the_figures_are_made_of() {
    let (app, _dir) = test_app().await;
    let id = import_with(
        &app,
        SAMPLE_GPX,
        &[("name", "Oslo Hills Walk"), ("activity_type", "hiking")],
    )
    .await;

    let trip = row(&stats(&app).await, id);

    assert_eq!(trip["name"], "Oslo Hills Walk");
    assert_eq!(trip["activity_type"], "hiking");
    assert_eq!(trip["start_date"], "2024-06-01");
    assert_eq!(trip["end_date"], "2024-06-01");
    let distance = trip["distance_m"].as_f64().unwrap();
    assert!(distance > 1_000.0 && distance < 2_500.0, "{distance}");
    assert!((trip["ascent_m"].as_f64().unwrap() - 40.0).abs() < 0.1);
    // Both half-hour legs are faster than hiking's 1 km/h.
    assert_eq!(trip["moving_secs"], 3600);
}

#[tokio::test]
async fn us77_dates_are_local_to_the_trip() {
    let (app, _dir) = test_app().await;
    // 22:30 UTC on 1 June is past midnight in Oslo.
    let evening = import_with(&app, LATE_EVENING_GPX, &[]).await;
    let long = import_with(
        &app,
        &track_between("2024-07-01T06:00:00Z", "2024-07-03T18:00:00Z"),
        &[],
    )
    .await;

    let stats = stats(&app).await;

    assert_eq!(row(&stats, evening)["start_date"], "2024-06-02");
    assert_eq!(row(&stats, evening)["end_date"], "2024-06-02");
    assert_eq!(row(&stats, long)["start_date"], "2024-07-01");
    assert_eq!(row(&stats, long)["end_date"], "2024-07-03");
}

#[tokio::test]
async fn us77_planned_trips_are_left_out_and_undated_ones_only_counted() {
    let (app, _dir) = test_app().await;
    let recorded = import_with(&app, SAMPLE_GPX, &[]).await;
    import_with(&app, SAMPLE_GPX, &[("kind", "planned")]).await;
    import(&app, UNTIMED_GPX).await;

    let stats = stats(&app).await;

    let ids: Vec<i64> = stats["trips"]
        .as_array()
        .unwrap()
        .iter()
        .map(|trip| trip["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [recorded]);
    assert_eq!(stats["undated"], 1);
}

#[tokio::test]
async fn us77_the_answer_carries_todays_date_to_compare_the_years_at() {
    let (app, _dir) = test_app().await;

    let today = stats(&app).await["today"].as_str().unwrap().to_string();

    let expected = time::OffsetDateTime::now_utc().date();
    assert_eq!(today, expected.to_string());
}

#[tokio::test]
async fn us77_moving_time_follows_the_activity() {
    let (app, _dir) = test_app().await;
    let id = import_with(&app, SAMPLE_GPX, &[("activity_type", "hiking")]).await;

    // One trip's edit: neither leg reaches cycling's 3 km/h.
    let response = send(
        &app,
        json_request(
            Method::PATCH,
            &format!("/api/trips/{id}"),
            r#"{"activity_type":"cycling"}"#,
        ),
    )
    .await;
    assert!(response.status().is_success(), "{}", response.status());
    assert_eq!(row(&stats(&app).await, id)["moving_secs"], 0);

    // Many trips' edit: only the ~1.6 km/h leg reaches kayaking's 1.5 km/h.
    let response = send(
        &app,
        json_request(
            Method::POST,
            "/api/trips/activity_type",
            &format!(r#"{{"trip_ids":[{id}],"activity_type":"kayaking"}}"#),
        ),
    )
    .await;
    assert!(response.status().is_success(), "{}", response.status());
    assert_eq!(row(&stats(&app).await, id)["moving_secs"], 1800);

    // A rename leaves it alone.
    send(
        &app,
        json_request(
            Method::PATCH,
            &format!("/api/trips/{id}"),
            r#"{"name":"Paddle"}"#,
        ),
    )
    .await;
    assert_eq!(row(&stats(&app).await, id)["moving_secs"], 1800);
}

#[tokio::test]
async fn us77_trips_imported_before_moving_time_existed_are_backfilled() {
    let (app, state, _dir) = test_app_with_state(None).await;
    let id = import_with(&app, SAMPLE_GPX, &[("activity_type", "hiking")]).await;
    let untimed = import(&app, UNTIMED_GPX).await;
    let untimed = trip_id_from_redirect(&untimed);
    sqlx::query("UPDATE trip SET moving_secs = NULL")
        .execute(&state.pool)
        .await
        .unwrap();

    let filled = trip_archive::server::repo::backfill_moving_secs(&state.pool)
        .await
        .unwrap();

    // The untimed trip has no moving time to fill in, and is not tried.
    assert_eq!(filled, 1);
    assert_eq!(row(&stats(&app).await, id)["moving_secs"], 3600);
    let left: Option<i64> = sqlx::query_scalar("SELECT moving_secs FROM trip WHERE id = ?")
        .bind(untimed)
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(left, None);
    // A second run finds nothing left to do.
    assert_eq!(
        trip_archive::server::repo::backfill_moving_secs(&state.pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn us77_statistics_are_the_owners_alone() {
    let (app, _dir) = test_app().await;

    let response = send_unauthenticated(
        &app,
        axum::http::Request::get("/api/stats/trips")
            .body(axum::body::Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
