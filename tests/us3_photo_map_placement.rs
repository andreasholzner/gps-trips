//! US-3 — photos with EXIF GPS appear on the map where they were taken.
//!
//! Acceptance criteria: a geotagged photo shows a marker at its EXIF
//! coordinates; `location_source = exif`.

mod common;

use axum::http::StatusCode;
use common::{add_photos_request, body_string, import_sample_with_photos, send, test_app};
use trip_archive::server::location::fixtures::geotagged_bytes;

#[tokio::test]
async fn us3_geotagged_photo_appears_in_photos_json_with_exif_location_source_and_coordinates() {
    let (app, _dir) = test_app().await;
    let geotagged = geotagged_bytes(45.5, 10.26);
    let id = import_sample_with_photos(&app, &[("geo.jpg", &geotagged)]).await;

    let response = common::get(&app, &format!("/api/trips/{id}/photos")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    assert_eq!(json[0]["location_source"], "exif");
    assert!((json[0]["lat"].as_f64().unwrap() - 45.5).abs() < 1e-3);
    assert!((json[0]["lon"].as_f64().unwrap() - 10.26).abs() < 1e-3);
}

#[tokio::test]
async fn us3_non_geotagged_photo_appears_in_photos_json_with_none_location_source_and_null_coordinates(
) {
    let (app, _dir) = test_app().await;
    let id = import_sample_with_photos(&app, &[("plain.jpg", b"\xFF\xD8\xFF-fake-jpeg")]).await;

    let response = common::get(&app, &format!("/api/trips/{id}/photos")).await;
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    assert_eq!(json[0]["location_source"], "none");
    assert!(json[0]["lat"].is_null());
    assert!(json[0]["lon"].is_null());
}

#[tokio::test]
async fn us3_import_with_mixed_geotagged_and_non_geotagged_photos_reports_each_correctly() {
    let (app, _dir) = test_app().await;
    let geotagged = geotagged_bytes(-33.9, 18.4);
    let id = import_sample_with_photos(
        &app,
        &[
            ("geo.jpg", geotagged.as_slice()),
            ("plain.jpg", b"\xFF\xD8\xFF-fake-jpeg".as_slice()),
        ],
    )
    .await;

    let response = common::get(&app, &format!("/api/trips/{id}/photos")).await;
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    let photos = json.as_array().unwrap();
    assert_eq!(photos.len(), 2);

    let geo = photos
        .iter()
        .find(|p| p["original_name"] == "geo.jpg")
        .expect("geotagged photo present");
    assert_eq!(geo["location_source"], "exif");
    assert!((geo["lat"].as_f64().unwrap() - -33.9).abs() < 1e-3);
    assert!((geo["lon"].as_f64().unwrap() - 18.4).abs() < 1e-3);

    let plain = photos
        .iter()
        .find(|p| p["original_name"] == "plain.jpg")
        .expect("non-geotagged photo present");
    assert_eq!(plain["location_source"], "none");
    assert!(plain["lat"].is_null());
    assert!(plain["lon"].is_null());
}

#[tokio::test]
async fn us3_photos_added_later_via_add_photos_endpoint_also_get_location_extracted() {
    let (app, _dir) = test_app().await;
    let id = import_sample_with_photos(&app, &[]).await;

    let geotagged = geotagged_bytes(51.5, -0.12);
    let response = send(&app, add_photos_request(id, &[("later.jpg", &geotagged)])).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let photos_response = common::get(&app, &format!("/api/trips/{id}/photos")).await;
    let json: serde_json::Value =
        serde_json::from_str(&body_string(photos_response).await).unwrap();

    assert_eq!(json[0]["location_source"], "exif");
    assert!((json[0]["lat"].as_f64().unwrap() - 51.5).abs() < 1e-3);
    assert!((json[0]["lon"].as_f64().unwrap() - -0.12).abs() < 1e-3);
}
