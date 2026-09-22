//! US-62 acceptance tests — "the trip's page shows its numbers at a glance and
//! lets me look at a photo properly".
//!
//! What this file covers is the data the screen reads its times from: the
//! offsets served with the track, the local start date on the trip, and each
//! photo's capture time. How the screen renders them is `crates/ui-dioxus`'
//! business and the browser layer's (ADR-0012).
//!
//! Drives the real Axum router in-process against a real temp SQLite DB (ADR-0012).

use crate::common::{body_string, get, import_sample, import_sample_with_photos, test_app};
use trip_archive::server::location::fixtures::{
    capture_time_bytes, geotagged_bytes_with_capture_time,
};

// ── The readout: offsets along the served track ─────────────────────────────

#[tokio::test]
async fn us62_the_served_track_carries_the_offsets_its_points_were_in() {
    // SAMPLE_GPX is a June morning in Oslo: one zone, one offset, +02:00.
    let (app, _dir) = test_app().await;
    let id = import_sample(&app).await;

    let response = get(&app, &format!("/api/trips/{id}/track.geojson")).await;
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    assert_eq!(
        json["properties"]["utc_offsets"],
        serde_json::json!([[0, 7200]]),
        "{json}"
    );
    // The timestamps the readout reads are still there, one per point.
    assert_eq!(
        json["properties"]["timestamps"].as_array().unwrap().len(),
        3
    );
}

// ── The date beside the trip's name ─────────────────────────────────────────

#[tokio::test]
async fn us62_the_trip_carries_the_date_it_started_on_where_it_started() {
    // LATE_EVENING_GPX starts at 22:30 UTC on the 1st, half past midnight on
    // the 2nd in Oslo: the date beside the name is the owner's, the one US-12
    // put in front of the name, not the one UTC was on.
    let (app, _dir) = test_app().await;
    let response = crate::common::import(&app, crate::common::LATE_EVENING_GPX).await;
    let id = crate::common::trip_id_from_redirect(&response);

    let response = get(&app, &format!("/api/trips/{id}")).await;
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    assert_eq!(json["start_date"], "2024-06-02", "{json}");
}

// ── The captions: each photo's capture time, and the zone it was taken in ───

async fn photos_of(app: &axum::Router, id: i64) -> serde_json::Value {
    let response = get(app, &format!("/api/trips/{id}/photos")).await;
    serde_json::from_str(&body_string(response).await).unwrap()
}

#[tokio::test]
async fn us62_a_photo_carries_the_instant_it_was_taken_and_its_offset() {
    // 10:15 on the camera's clock, on a June morning in Oslo: 08:15 UTC,
    // stored as UTC (ADR-0009) and captioned at +02:00.
    let (app, _dir) = test_app().await;
    let bytes = capture_time_bytes("2024:06:01 10:15:00", None);
    let id = import_sample_with_photos(&app, &[("morning.jpg", &bytes)]).await;

    let json = photos_of(&app, id).await;

    assert_eq!(json[0]["taken_at"], "2024-06-01T08:15:00Z", "{json}");
    assert_eq!(json[0]["taken_offset_secs"], 7200, "{json}");
}

#[tokio::test]
async fn us62_a_photo_off_the_track_still_says_when_it_was_taken() {
    // Taken long after the track ended, so US-4 cannot place it — but its
    // time is known, in the trip's own zone, and the caption says so.
    let (app, _dir) = test_app().await;
    let bytes = capture_time_bytes("2024:06:01 23:00:00", None);
    let id = import_sample_with_photos(&app, &[("late.jpg", &bytes)]).await;

    let json = photos_of(&app, id).await;

    assert_eq!(json[0]["location_source"], "none", "{json}");
    assert_eq!(json[0]["taken_at"], "2024-06-01T21:00:00Z", "{json}");
    assert_eq!(json[0]["taken_offset_secs"], 7200, "{json}");
}

#[tokio::test]
async fn us62_a_photo_is_captioned_in_the_zone_it_was_taken_in() {
    // EXIF GPS in New York on a trip in Oslo: the offset is the one where the
    // photo was, not where the trip started.
    let (app, _dir) = test_app().await;
    let bytes = geotagged_bytes_with_capture_time(40.7128, -74.006, "2024:06:01 10:15:00");
    let id = import_sample_with_photos(&app, &[("elsewhere.jpg", &bytes)]).await;

    let json = photos_of(&app, id).await;

    assert_eq!(json[0]["taken_offset_secs"], -4 * 3600, "{json}");
}

#[tokio::test]
async fn us62_a_photo_with_no_capture_time_carries_none() {
    // No caption rather than a dash: the screen has nothing to say.
    let (app, _dir) = test_app().await;
    let id = import_sample_with_photos(&app, &[("plain.jpg", b"\xFF\xD8\xFF-no-exif")]).await;

    let json = photos_of(&app, id).await;

    assert!(json[0]["taken_at"].is_null(), "{json}");
    assert!(json[0]["taken_offset_secs"].is_null(), "{json}");
}
