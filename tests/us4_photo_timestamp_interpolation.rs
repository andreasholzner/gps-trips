//! US-4 — photos without GPS are placed by matching their timestamp to the
//! track, so untagged shots still appear.
//!
//! Acceptance criteria: a non-geotagged photo whose time falls within the
//! track range gets an interpolated position (`location_source = interpolated`);
//! one outside the range is left unplaced (`location_source = none`).
//!
//! `tests/fixtures/sample.gpx` (Oslo, 2024-06-01T08:00:00Z–09:00:00Z) auto-guesses
//! the "Europe/Oslo" timezone (+02:00 in June) from its start coordinate.

mod common;

use axum::http::StatusCode;
use common::{add_photos_request, body_string, import_sample_with_photos, send, test_app};
use trip_archive::server::location::fixtures::{
    capture_time_bytes, geotagged_bytes_with_capture_time,
};

#[tokio::test]
async fn us4_non_geotagged_photo_with_a_timestamp_inside_the_track_range_is_interpolated() {
    let (app, _dir) = test_app().await;
    // 10:15 local ("Europe/Oslo", +02:00 in June) == 08:15 UTC, inside the
    // track's 08:00-09:00 UTC range.
    let bytes = capture_time_bytes("2024:06:01 10:15:00", None);
    let id = import_sample_with_photos(&app, &[("untagged.jpg", &bytes)]).await;

    let response = common::get(&app, &format!("/api/trips/{id}/photos")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    assert_eq!(json[0]["location_source"], "interpolated");
    let lat = json[0]["lat"].as_f64().expect("lat must be present");
    let lon = json[0]["lon"].as_f64().expect("lon must be present");
    // Within the track's bounding box (59.9139-59.9250 lat, 10.7522-10.7650 lon).
    assert!((59.9..=59.93).contains(&lat), "lat={lat}");
    assert!((10.74..=10.77).contains(&lon), "lon={lon}");
}

#[tokio::test]
async fn us4_non_geotagged_photo_with_a_timestamp_outside_the_track_range_is_unplaced() {
    let (app, _dir) = test_app().await;
    // 23:00 local ("Europe/Oslo", +02:00 in June) == 21:00 UTC, well outside
    // the track's 08:00-09:00 UTC range.
    let bytes = capture_time_bytes("2024:06:01 23:00:00", None);
    let id = import_sample_with_photos(&app, &[("untagged.jpg", &bytes)]).await;

    let response = common::get(&app, &format!("/api/trips/{id}/photos")).await;
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    assert_eq!(json[0]["location_source"], "none");
    assert!(json[0]["lat"].is_null());
    assert!(json[0]["lon"].is_null());
}

#[tokio::test]
async fn us4_exif_gps_still_wins_over_interpolation_even_with_an_in_range_timestamp() {
    let (app, _dir) = test_app().await;
    let bytes = geotagged_bytes_with_capture_time(45.5, 10.26, "2024:06:01 10:15:00");
    let id = import_sample_with_photos(&app, &[("both.jpg", &bytes)]).await;

    let response = common::get(&app, &format!("/api/trips/{id}/photos")).await;
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    assert_eq!(json[0]["location_source"], "exif");
    assert!((json[0]["lat"].as_f64().unwrap() - 45.5).abs() < 1e-3);
    assert!((json[0]["lon"].as_f64().unwrap() - 10.26).abs() < 1e-3);
}

#[tokio::test]
async fn us4_photos_added_later_via_add_photos_endpoint_are_also_interpolated() {
    let (app, _dir) = test_app().await;
    let id = import_sample_with_photos(&app, &[]).await;

    let bytes = capture_time_bytes("2024:06:01 10:15:00", None);
    let response = send(&app, add_photos_request(id, &[("later.jpg", &bytes)])).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let photos_response = common::get(&app, &format!("/api/trips/{id}/photos")).await;
    let json: serde_json::Value =
        serde_json::from_str(&body_string(photos_response).await).unwrap();

    assert_eq!(json[0]["location_source"], "interpolated");
    assert!(json[0]["lat"].as_f64().is_some());
    assert!(json[0]["lon"].as_f64().is_some());
}

#[tokio::test]
async fn us4_a_trip_missing_its_timezone_is_backfilled_the_first_time_photos_are_added() {
    let (app, dir) = test_app().await;
    let id = import_sample_with_photos(&app, &[]).await;

    // Simulate a trip imported before `tz_name` existed (the migration's
    // `NULL` default for pre-existing rows).
    let pool = trip_archive::server::db::create_pool(&dir.path().join("test.db"))
        .await
        .expect("reopen test db");
    sqlx::query("UPDATE trip SET tz_name = NULL WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await
        .expect("clear tz_name");

    let bytes = capture_time_bytes("2024:06:01 10:15:00", None);
    let response = send(&app, add_photos_request(id, &[("later.jpg", &bytes)])).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    // The photo still gets interpolated (the timezone was guessed on the fly)...
    let photos_response = common::get(&app, &format!("/api/trips/{id}/photos")).await;
    let json: serde_json::Value =
        serde_json::from_str(&body_string(photos_response).await).unwrap();
    assert_eq!(json[0]["location_source"], "interpolated");

    // ...and the guess was persisted back onto the trip row.
    let tz_name: Option<String> = sqlx::query_scalar("SELECT tz_name FROM trip WHERE id = ?")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tz_name.as_deref(), Some("Europe/Oslo"));
}
