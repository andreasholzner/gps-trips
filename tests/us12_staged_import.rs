//! US-12 — choosing a trip name with the suggested `YYYY-mm-dd` prefix
//! already in the field, which needs the GPX parsed *before* the name is
//! entered.
//!
//! Acceptance criteria: the import screen is two-phase. Phase one uploads the
//! GPX and gets back a suggested name carrying the track's start date; phase
//! two confirms the name, activity type (US-11), kind (US-31) and timezone,
//! and only then does a trip exist. Staging derives the track once and parks
//! it; confirming promotes that derivation into a trip rather than parsing
//! the file a second time.
//!
//! The properties that make the staged row safe are asserted here too, since
//! nothing else in the archive knows it exists: staging alone creates no
//! trip, confirming consumes the row, and a refused confirm leaves it intact
//! for a retry.

mod common;

use axum::http::{Method, StatusCode};
use common::{
    body_string, confirm_import_request, delete, error_message, get, json_request, send,
    stage_import, stage_import_request, test_app, LATE_EVENING_GPX, SAMPLE_GPX, UNNAMED_GPX,
};
use serde_json::Value;
use trip_archive::models::TripDetail;

/// The GPX both fixtures start on, and so the prefix every suggestion here
/// carries.
const TRACK_DATE: &str = "2024-06-01";

/// Confirm a staged import with a JSON body, returning the raw response.
async fn confirm(app: &axum::Router, staging_id: i64, body: &str) -> axum::response::Response {
    send(app, confirm_import_request(staging_id, body)).await
}

/// Confirm and return the new trip's id, asserting the 201.
async fn confirm_ok(app: &axum::Router, staging_id: i64, body: &str) -> i64 {
    let response = confirm(app, staging_id, body).await;
    let status = response.status();
    let body = body_string(response).await;
    assert_eq!(status, StatusCode::CREATED, "confirm failed: {body}");
    serde_json::from_str::<Value>(&body).expect("JSON")["id"]
        .as_i64()
        .expect("the confirm response names the new trip")
}

async fn trip(app: &axum::Router, id: i64) -> TripDetail {
    let response = get(app, &format!("/api/trips/{id}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_str(&body_string(response).await).expect("trip JSON")
}

// ── Phase one: the suggestion ────────────────────────────────────────────────

#[tokio::test]
async fn us12_a_named_track_suggests_the_date_prefix_followed_by_its_name() {
    let (app, _dir) = test_app().await;

    let staged = stage_import(&app, SAMPLE_GPX).await;

    assert_eq!(
        staged.suggested_name,
        format!("{TRACK_DATE} Oslo Hills Walk")
    );
    assert_eq!(staged.start_date.as_deref(), Some(TRACK_DATE));
    assert_eq!(staged.gpx_name.as_deref(), Some("Oslo Hills Walk"));
    assert!(staged.staging_id > 0);
}

#[tokio::test]
async fn us12_a_track_without_a_name_suggests_the_bare_date_prefix() {
    // The point of the story: the owner sees the date already in the field
    // and types the rest of the name after it.
    let (app, _dir) = test_app().await;

    let staged = stage_import(&app, UNNAMED_GPX).await;

    assert_eq!(staged.suggested_name, format!("{TRACK_DATE} "));
    assert_eq!(staged.start_date.as_deref(), Some(TRACK_DATE));
    assert_eq!(staged.gpx_name, None);
}

#[tokio::test]
async fn us12_the_suggested_date_is_the_day_the_track_was_ridden() {
    // The track starts 22:30 UTC, which is 00:30 the next day in Europe/Oslo
    // where it was recorded. The owner names the trip after the day they were
    // out on it, so the prefix has to be read in the track's own timezone —
    // otherwise the one field this story exists to prefill arrives wrong.
    let (app, _dir) = test_app().await;

    let staged = stage_import(&app, LATE_EVENING_GPX).await;

    assert_eq!(staged.suggested_name, "2024-06-02 Midnight Ride");
    assert_eq!(staged.start_date.as_deref(), Some("2024-06-02"));
    assert_eq!(staged.timezone, "Europe/Oslo");
}

#[tokio::test]
async fn us12_the_suggestion_carries_the_guessed_timezone_and_the_tracks_stats() {
    // The confirm form prefills its timezone override with the guess
    // (US-4, ADR-0019), and shows what was parsed so the owner can tell they
    // picked the right file.
    let (app, _dir) = test_app().await;

    let staged = stage_import(&app, SAMPLE_GPX).await;

    assert_eq!(staged.timezone, "Europe/Oslo");
    assert!(staged.distance_m > 0.0, "{staged:?}");
    assert!(staged.ascent_m > 0.0, "{staged:?}");
    assert_eq!(staged.duration_secs, Some(3600));
}

#[tokio::test]
async fn us12_staging_a_gpx_creates_no_trip() {
    // The reason a staged row is not a trip row: an import abandoned at the
    // naming step must leave the archive exactly as it was.
    let (app, _dir) = test_app().await;

    stage_import(&app, SAMPLE_GPX).await;

    let trips = body_string(get(&app, "/api/trips").await).await;
    assert_eq!(trips, "[]", "staging must not create a trip; got: {trips}");
}

#[tokio::test]
async fn us12_a_gpx_the_archive_cannot_use_is_refused_at_the_staging_step() {
    // US-1's rejection, moved to where the owner now meets it — before any
    // naming, rather than after filling in a form.
    let (app, _dir) = test_app().await;

    let response = send(&app, stage_import_request(b"not xml at all")).await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(!error_message(response).await.is_empty(), "it says why");
}

#[tokio::test]
async fn us12_staging_without_a_gpx_field_is_rejected_with_400() {
    let (app, _dir) = test_app().await;

    let response = send(&app, json_request(Method::POST, "/api/import/staged", "{}")).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ── Phase two: the confirmation ──────────────────────────────────────────────

#[tokio::test]
async fn us12_confirming_creates_the_trip_the_owner_described() {
    // The whole story in one test: the suggestion is a suggestion, and what
    // the owner actually typed is what is stored — with US-11's activity type
    // and US-31's kind alongside it.
    let (app, _dir) = test_app().await;
    let staged = stage_import(&app, SAMPLE_GPX).await;

    let id = confirm_ok(
        &app,
        staged.staging_id,
        r#"{"name":"2024-06-01 Nordmarka","activity_type":"hiking","kind":"planned","timezone":"Europe/Berlin"}"#,
    )
    .await;

    let trip = trip(&app, id).await;
    assert_eq!(trip.name, "2024-06-01 Nordmarka");
    assert_eq!(trip.activity_type.as_str(), "hiking");
    assert_eq!(trip.tz_name.as_deref(), Some("Europe/Berlin"));
    assert!(
        trip.distance_m > 0.0,
        "the stats came from the staged parse"
    );

    let planned = body_string(get(&app, "/api/trips?kind=planned").await).await;
    assert!(planned.contains(&format!("\"id\":{id}")), "got: {planned}");
}

#[tokio::test]
async fn us12_confirming_with_nothing_filled_in_falls_back_as_the_import_always_did() {
    // The single-step form's name precedence is deliberately unchanged: an
    // empty name still means the GPX track's own name, and the guessed
    // timezone still stands in for an unset override.
    let (app, _dir) = test_app().await;
    let staged = stage_import(&app, SAMPLE_GPX).await;

    let id = confirm_ok(&app, staged.staging_id, "{}").await;

    let trip = trip(&app, id).await;
    assert_eq!(trip.name, "Oslo Hills Walk");
    assert_eq!(trip.activity_type.as_str(), "unknown");
    assert_eq!(trip.tz_name.as_deref(), Some("Europe/Oslo"));
}

#[tokio::test]
async fn us12_the_confirmed_trip_keeps_the_track_and_the_original_gpx() {
    // Promotion stores what staging derived, so the track (ADR-0003) and the
    // byte-for-byte original (US-21) survive the two-phase detour.
    let (app, _dir) = test_app().await;
    let staged = stage_import(&app, SAMPLE_GPX).await;

    let id = confirm_ok(&app, staged.staging_id, "{}").await;

    let geojson = body_string(get(&app, &format!("/api/trips/{id}/track.geojson")).await).await;
    assert!(geojson.contains("LineString"), "got: {geojson}");

    let original = common::body_bytes(get(&app, &format!("/api/trips/{id}/gpx")).await).await;
    assert_eq!(original, SAMPLE_GPX, "the stored GPX is the uploaded one");
}

#[tokio::test]
async fn us12_confirming_consumes_the_staged_import() {
    // A double submit, or the Back button onto a confirmed form, must not
    // import the same file twice.
    let (app, _dir) = test_app().await;
    let staged = stage_import(&app, SAMPLE_GPX).await;
    confirm_ok(&app, staged.staging_id, "{}").await;

    let again = confirm(&app, staged.staging_id, "{}").await;

    assert_eq!(again.status(), StatusCode::NOT_FOUND);
    let trips: Value = serde_json::from_str(&body_string(get(&app, "/api/trips").await).await)
        .expect("trip list JSON");
    assert_eq!(trips.as_array().expect("a list").len(), 1);
}

#[tokio::test]
async fn us12_confirming_a_staged_import_that_is_gone_says_so() {
    let (app, _dir) = test_app().await;

    let response = confirm(&app, 9_999, "{}").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn us12_a_refused_confirmation_leaves_the_staged_import_for_another_try() {
    // US-11/US-31's 400 on an unrecognized value, in a flow where losing the
    // staged parse would mean re-picking the file.
    let (app, _dir) = test_app().await;
    let staged = stage_import(&app, SAMPLE_GPX).await;

    let refused = confirm(
        &app,
        staged.staging_id,
        r#"{"activity_type":"teleportation"}"#,
    )
    .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);

    let id = confirm_ok(&app, staged.staging_id, r#"{"activity_type":"cycling"}"#).await;
    assert_eq!(trip(&app, id).await.activity_type.as_str(), "cycling");
}

#[tokio::test]
async fn us12_an_unrecognized_kind_is_refused_at_confirmation() {
    let (app, _dir) = test_app().await;
    let staged = stage_import(&app, SAMPLE_GPX).await;

    let response = confirm(&app, staged.staging_id, r#"{"kind":"scheduled"}"#).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(error_message(response).await.contains("kind"));
}

// ── Abandoning ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn us12_an_abandoned_staged_import_can_be_taken_back() {
    // The screen cancels when the owner leaves or picks a different file, so
    // an abandoned parse does not wait for the sweeper.
    let (app, _dir) = test_app().await;
    let staged = stage_import(&app, SAMPLE_GPX).await;

    let cancelled = delete(&app, &format!("/api/import/staged/{}", staged.staging_id)).await;
    assert_eq!(cancelled.status(), StatusCode::NO_CONTENT);

    let response = confirm(&app, staged.staging_id, "{}").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body_string(get(&app, "/api/trips").await).await, "[]");
}

#[tokio::test]
async fn us12_cancelling_a_staged_import_that_is_gone_says_so() {
    let (app, _dir) = test_app().await;

    let response = delete(&app, "/api/import/staged/9999").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
