//! US-81 acceptance tests — the server's half of the climbing rate.
//!
//! Finding the climbs is unit-tested in `src/server/climbs/tests.rs`; the
//! rates are worked out in the SPA (`crates/ui-dioxus`). What stays here:
//!
//! * *a trip's climbs' height and moving time are computed at import, again
//!   when its activity changes, and backfilled* —
//!   `us81_a_trips_climbs_are_found_and_its_figures_stored`,
//!   `us81_the_climbs_follow_the_activity`,
//!   `us81_trips_stored_before_the_climbs_existed_are_backfilled`.
//! * *a trip without timestamps has no climbing rate, though its climbs are
//!   listed* — `us81_a_trip_without_times_lists_its_climbs_without_moving_time`.
//! * *the trip page lists the climbs, for the owner and a share's recipient*
//!   — `us81_a_shared_trips_climbs_are_the_shares_alone`,
//!   `us81_the_climbs_are_the_owners_or_a_shares`.
//! * *the Statistics and Summary screens, and their shared counterparts,
//!   read the figures* — `us81_every_screen_that_shows_a_rate_is_served_the_figures`.
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use trip_archive::models::Climb;

use crate::common::{
    body_string, get, import_request_with_fields, json_request, send, send_unauthenticated,
    test_app, test_app_with_state, trip_id_from_redirect,
};

/// A walk near Oslo: 300 m flat, 1 km up 100 m, 300 m flat, 10 m and — when
/// `timed` — 10 s a step.
fn hill_gpx(timed: bool) -> Vec<u8> {
    const DEGREES_PER_10_M: f64 = 10.0 / 111_195.0;
    let mut points = String::new();
    let mut elevation = 100.0;
    for step in 0..=160 {
        if (31..=130).contains(&step) {
            elevation += 1.0;
        }
        let lat = 59.9 + step as f64 * DEGREES_PER_10_M;
        let time = if timed {
            let at =
                time::macros::datetime!(2024-06-01 08:00 UTC) + time::Duration::seconds(step * 10);
            let at = at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap();
            format!("<time>{at}</time>")
        } else {
            String::new()
        };
        points.push_str(&format!(
            r#"<trkpt lat="{lat}" lon="10.75"><ele>{elevation}</ele>{time}</trkpt>"#
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="trip-archive-test" xmlns="http://www.topografix.com/GPX/1/1">
  <trk><name>Hill</name><trkseg>{points}</trkseg></trk>
</gpx>"#
    )
    .into_bytes()
}

async fn import_hill(app: &axum::Router, fields: &[(&str, &str)], timed: bool) -> i64 {
    let response = send(
        app,
        import_request_with_fields(&hill_gpx(timed), fields, &[]),
    )
    .await;
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

async fn climbs(app: &axum::Router, id: i64) -> Vec<Climb> {
    serde_json::from_value(json(get(app, &format!("/api/trips/{id}/climbs")).await).await).unwrap()
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

fn gain(trip: &serde_json::Value) -> f64 {
    trip["climb_gain_m"]
        .as_f64()
        .unwrap_or_else(|| panic!("no climb gain in {trip}"))
}

#[tokio::test]
async fn us81_a_trips_climbs_are_found_and_its_figures_stored() {
    let (app, _dir) = test_app().await;
    let id = import_hill(&app, &[("activity_type", "hiking")], true).await;

    let found = climbs(&app, id).await;
    let trip = detail(&app, id).await;

    assert_eq!(found.len(), 1, "{found:?}");
    assert!((found[0].gain_m - 100.0).abs() < 1.0, "{found:?}");
    assert!(found[0].start_m < found[0].end_m);
    let secs = found[0].moving_secs.expect("a timed climb");
    assert!((secs - 1000).abs() <= 50, "{secs}");
    assert!((gain(&trip) - found[0].gain_m).abs() < 1e-9, "{trip}");
    assert_eq!(trip["climb_secs"], secs);
}

#[tokio::test]
async fn us81_the_climbs_follow_the_activity() {
    let (app, _dir) = test_app().await;
    let id = import_hill(&app, &[("activity_type", "hiking")], true).await;

    // Kayaking has no climbs.
    change_activity(&app, id, "kayaking").await;
    assert!(climbs(&app, id).await.is_empty());
    let trip = detail(&app, id).await;
    assert_eq!(gain(&trip), 0.0);
    assert_eq!(trip["climb_secs"], 0);

    // A 10 % ride of 100 m is a climb again.
    change_activity(&app, id, "cycling").await;
    assert_eq!(climbs(&app, id).await.len(), 1);
    assert!(gain(&detail(&app, id).await) > 99.0);
}

#[tokio::test]
async fn us81_a_trip_without_times_lists_its_climbs_without_moving_time() {
    let (app, _dir) = test_app().await;
    let id = import_hill(&app, &[("activity_type", "hiking")], false).await;

    let found = climbs(&app, id).await;
    let trip = detail(&app, id).await;

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].moving_secs, None);
    assert_eq!(trip["climb_gain_m"], serde_json::Value::Null);
    assert_eq!(trip["climb_secs"], serde_json::Value::Null);
}

#[tokio::test]
async fn us81_trips_stored_before_the_climbs_existed_are_backfilled() {
    let (app, state, _dir) = test_app_with_state(None).await;
    let id = import_hill(&app, &[("activity_type", "hiking")], true).await;
    let expected = detail(&app, id).await;
    sqlx::query("UPDATE trip SET climb_gain_m = NULL, climb_secs = NULL")
        .execute(&state.pool)
        .await
        .unwrap();

    let filled = trip_archive::server::repo::backfill_moving_secs(&state.pool)
        .await
        .unwrap();

    assert_eq!(filled, 1);
    let trip = detail(&app, id).await;
    assert_eq!(trip["climb_gain_m"], expected["climb_gain_m"]);
    assert_eq!(trip["climb_secs"], expected["climb_secs"]);
}

#[tokio::test]
async fn us81_the_climbs_are_the_owners_or_a_shares() {
    let (app, _dir) = test_app().await;
    let id = import_hill(&app, &[("activity_type", "hiking")], true).await;

    let response = send_unauthenticated(
        &app,
        Request::builder()
            .uri(format!("/api/trips/{id}/climbs"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        get(&app, "/api/trips/999/climbs").await.status(),
        StatusCode::NOT_FOUND
    );
}

async fn share(app: &axum::Router, body: serde_json::Value) -> String {
    let response = send(
        app,
        json_request(Method::POST, "/api/shares", &body.to_string()),
    )
    .await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::CREATED, "got {body}");
    serde_json::from_str::<serde_json::Value>(&body).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string()
}

fn visit(uri: String) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn us81_a_shared_trips_climbs_are_the_shares_alone() {
    let (app, _dir) = test_app().await;
    let shared = import_hill(&app, &[("activity_type", "hiking")], true).await;
    let other = import_hill(&app, &[("activity_type", "hiking")], true).await;
    let token = share(&app, serde_json::json!({ "trip_ids": [shared] })).await;

    let response =
        send_unauthenticated(&app, visit(format!("/s/{token}/api/trips/{shared}/climbs"))).await;
    let found: Vec<Climb> = serde_json::from_value(json(response).await).unwrap();
    let outside =
        send_unauthenticated(&app, visit(format!("/s/{token}/api/trips/{other}/climbs"))).await;

    assert_eq!(found, climbs(&app, shared).await);
    assert_eq!(outside.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us81_every_screen_that_shows_a_rate_is_served_the_figures() {
    let (app, _dir) = test_app().await;
    let id = import_hill(&app, &[("activity_type", "hiking")], true).await;
    let expected = detail(&app, id).await;
    let tagged = send(
        &app,
        json_request(
            Method::POST,
            &format!("/api/trips/{id}/tags"),
            r#"{"name":"hills"}"#,
        ),
    )
    .await;
    assert_eq!(tagged.status(), StatusCode::CREATED);
    let same = |trip: &serde_json::Value| {
        assert_eq!(trip["climb_gain_m"], expected["climb_gain_m"], "{trip}");
        assert_eq!(trip["climb_secs"], expected["climb_secs"], "{trip}");
    };

    same(&json(get(&app, "/api/stats/trips").await).await["trips"][0]);
    same(&json(get(&app, "/api/stats/tags?tags=hills").await).await["trips"][0]);
    let token = share(&app, serde_json::json!({ "tags": ["hills"] })).await;
    let overview =
        json(send_unauthenticated(&app, visit(format!("/s/{token}/api/share"))).await).await;
    same(&overview["summary"]["trips"][0]);
    same(
        &json(send_unauthenticated(&app, visit(format!("/s/{token}/api/trips/{id}"))).await).await,
    );
}
