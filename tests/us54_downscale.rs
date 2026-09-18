//! US-54 — photos are downscaled when they are imported, so the volume holds
//! years of trips and an import fits in the machine's memory (ADR-0026).
//!
//! Acceptance criteria covered here, through both upload routes: a photo
//! larger than the bound is served as a JPEG scaled down to it, carrying the
//! original's EXIF; its placement is read from the upload. A photo within the
//! bound is stored byte-for-byte (US-5's `…_distinct_from_the_original`), an
//! undecodable one as uploaded (`photos.rs`/`thumbnail.rs` unit tests), and a
//! Komoot-synced one goes through the same path (`komoot_sync` tests).

mod common;

use axum::http::{header, StatusCode};
use common::{
    add_photos_request, body_bytes, body_string, import_sample_with_photos, send, test_app,
};
use trip_archive::config::photo::MAX_DIMENSION;
use trip_archive::server::location;
use trip_archive::server::thumbnail::fixtures::jpeg_with_exif;

/// A camera-like photo just past the bound, geotagged in its EXIF.
fn oversized_geotagged() -> Vec<u8> {
    jpeg_with_exif(
        MAX_DIMENSION + 100,
        (MAX_DIMENSION + 100) / 2,
        &location::fixtures::geotagged_bytes(47.26, 11.39),
    )
}

/// The trip's only photo, as the API lists it, and the bytes it serves.
async fn the_photo(app: &axum::Router, id: i64) -> (serde_json::Value, String, Vec<u8>) {
    let response = common::get(app, &format!("/api/trips/{id}/photos")).await;
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    let photo = json[0].clone();
    let served = common::get(app, photo["url"].as_str().unwrap()).await;
    assert_eq!(served.status(), StatusCode::OK);
    let content_type = served.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .to_string();
    (photo, content_type, body_bytes(served).await)
}

fn assert_downscaled_with_exif(content_type: &str, served: &[u8], original: &[u8]) {
    assert_eq!(content_type, "image/jpeg");
    let image = image::load_from_memory(served).expect("served photo decodes");
    assert_eq!(image.width(), MAX_DIMENSION);
    assert_eq!(image.height(), MAX_DIMENSION / 2);
    assert_eq!(
        location::extract_photo_metadata(served),
        location::extract_photo_metadata(original),
        "the stored copy keeps the original's EXIF"
    );
}

#[tokio::test]
async fn us54_a_photo_imported_with_the_trip_is_stored_downscaled() {
    let (app, _dir) = test_app().await;
    let original = oversized_geotagged();
    let id = import_sample_with_photos(&app, &[("IMG_0001.JPG", &original)]).await;

    let (photo, content_type, served) = the_photo(&app, id).await;

    assert_downscaled_with_exif(&content_type, &served, &original);
    assert_eq!(photo["original_name"], "IMG_0001.JPG");
    assert_eq!(photo["byte_len"], served.len());
    assert_eq!(photo["lat"], 47.26);
    assert_eq!(photo["lon"], 11.39);
}

#[tokio::test]
async fn us54_a_photo_added_to_an_existing_trip_is_stored_downscaled() {
    let (app, _dir) = test_app().await;
    let id = import_sample_with_photos(&app, &[]).await;
    let original = oversized_geotagged();

    let response = send(&app, add_photos_request(id, &[("later.jpg", &original)])).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let (photo, content_type, served) = the_photo(&app, id).await;
    assert_downscaled_with_exif(&content_type, &served, &original);
    assert_eq!(photo["lat"], 47.26);
}
