//! US-76 — an activity type that fits the track, suggested at import and
//! on the edit form.
//!
//! Acceptance criteria: the staged parse returns the suggestion, which the
//! activity selector starts on; the edit form is offered the same for the
//! trip as stored. A track the rules cannot place — or no ground data —
//! gets no suggestion, and the confirm still stores what the owner picked.
//!
//! Tested against synthetic tracks of each activity — along real roads and
//! paths taken from OpenStreetMap, or straight lines across country and
//! water, none of them the owner's — over a ground fixture cut from the
//! real database around them (`places_build cut-ground`, © OpenStreetMap
//! contributors, ODbL).

use axum::http::StatusCode;
use trip_archive::models::{ActivityType, TripSuggestion};

use crate::common::{
    body_string, confirm_import_request, get, send, stage_import, test_app, test_app_with_ground,
    REGION_ALPS_GPX,
};

const CYCLING: &[u8] = include_bytes!("../fixtures/activities/cycling.gpx");
const BIKEPACKING: &[u8] = include_bytes!("../fixtures/activities/bikepacking.gpx");
const HIKING: &[u8] = include_bytes!("../fixtures/activities/hiking.gpx");
const MOUNTAINEERING: &[u8] = include_bytes!("../fixtures/activities/mountaineering.gpx");
const KAYAKING: &[u8] = include_bytes!("../fixtures/activities/kayaking.gpx");
const CROSS_COUNTRY_SKIING: &[u8] =
    include_bytes!("../fixtures/activities/cross_country_skiing.gpx");
const SKI_TOURING: &[u8] = include_bytes!("../fixtures/activities/ski_touring.gpx");

/// Stage and confirm `gpx` with `activity_type`; the new trip's id.
async fn import_as(app: &axum::Router, gpx: &[u8], activity_type: &str) -> i64 {
    let staged = stage_import(app, gpx).await;
    let body = serde_json::json!({ "activity_type": activity_type }).to_string();
    let response = send(app, confirm_import_request(staged.staging_id, &body)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_str(&body_string(response).await).expect("JSON");
    body["id"].as_i64().expect("an id")
}

async fn offered(app: &axum::Router, id: i64) -> TripSuggestion {
    let response = get(app, &format!("/api/trips/{id}/suggestion")).await;
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_str(&body_string(response).await).expect("suggestion JSON")
}

#[tokio::test]
async fn us76_the_staged_parse_suggests_the_activity_the_track_looks_like() {
    let (app, _dir) = test_app_with_ground().await;

    for (gpx, expected) in [
        (CYCLING, ActivityType::Cycling),
        (BIKEPACKING, ActivityType::Bikepacking),
        (HIKING, ActivityType::Hiking),
        (MOUNTAINEERING, ActivityType::Mountaineering),
        (KAYAKING, ActivityType::Kayaking),
        (CROSS_COUNTRY_SKIING, ActivityType::CrossCountrySkiing),
        (SKI_TOURING, ActivityType::SkiTouring),
    ] {
        let staged = stage_import(&app, gpx).await;
        assert_eq!(staged.suggested_activity, Some(expected));
    }
}

#[tokio::test]
async fn us76_a_track_the_ground_data_does_not_reach_gets_no_suggestion() {
    // The Alps: outside what the fixture covers.
    let (app, _dir) = test_app_with_ground().await;

    assert_eq!(
        stage_import(&app, REGION_ALPS_GPX).await.suggested_activity,
        None
    );
}

#[tokio::test]
async fn us76_without_the_ground_database_there_is_no_suggestion() {
    let (app, _dir) = test_app().await;

    assert_eq!(stage_import(&app, CYCLING).await.suggested_activity, None);
}

#[tokio::test]
async fn us76_the_owner_s_pick_is_stored_whatever_was_suggested() {
    let (app, _dir) = test_app_with_ground().await;

    let id = import_as(&app, CYCLING, "snow_shoe").await;

    let response = get(&app, &format!("/api/trips/{id}")).await;
    let trip: serde_json::Value = serde_json::from_str(&body_string(response).await).expect("JSON");
    assert_eq!(trip["activity_type"], "snow_shoe");
}

#[tokio::test]
async fn us76_the_edit_form_is_offered_the_activity_for_the_trip_as_stored() {
    let (app, _dir) = test_app_with_ground().await;
    let id = import_as(&app, KAYAKING, "").await;

    assert_eq!(
        offered(&app, id).await.activity_type,
        Some(ActivityType::Kayaking)
    );
    // Only offered.
    let response = get(&app, &format!("/api/trips/{id}")).await;
    let trip: serde_json::Value = serde_json::from_str(&body_string(response).await).expect("JSON");
    assert_eq!(trip["activity_type"], "unknown");
}
