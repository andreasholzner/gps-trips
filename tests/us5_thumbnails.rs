//! US-5 — thumbnails are generated automatically on import so galleries and
//! maps load fast.
//!
//! Acceptance criteria: each photo has a generated thumbnail; originals are
//! kept untouched; EXIF orientation is honored.

mod common;

use axum::http::StatusCode;
use common::{
    add_photos_request, body_bytes, body_string, import_sample_with_photos, send, test_app,
};
use trip_archive::server::location;
use trip_archive::server::thumbnail::fixtures::valid_jpeg_bytes;

/// A real, decodable JPEG (built via `image`, like `valid_jpeg_bytes`) that
/// also carries a real EXIF APP1 segment with an `Orientation` tag (the TIFF
/// payload reused from `location::fixtures`'s byte-level builder). Combines
/// both existing fixture-building blocks so the full ingest pipeline can be
/// exercised with an actual oriented photo, not just each half in isolation
/// (location.rs's extraction and thumbnail.rs's application are each already
/// unit-tested separately, but nothing else proves they're wired together).
///
/// `width`x`height`, left half red / right half blue, high JPEG quality so
/// the halves survive compression distinctly enough to assert on.
fn oriented_jpeg_bytes(width: u32, height: u32, orientation: u16) -> Vec<u8> {
    let img = image::RgbImage::from_fn(width, height, |x, _y| {
        if x < width / 2 {
            image::Rgb([255, 0, 0])
        } else {
            image::Rgb([0, 0, 255])
        }
    });
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95)
        .encode_image(&img)
        .unwrap();

    // A JPEG APP1 "Exif" segment: marker (0xFFE1) + big-endian length
    // (including the length field itself) + "Exif\0\0" + a raw TIFF stream.
    // `orientation_bytes` already builds exactly that TIFF stream.
    let tiff = location::fixtures::orientation_bytes(orientation);
    let mut app1 = vec![0xFF, 0xE1];
    let len = (2 + 6 + tiff.len()) as u16;
    app1.extend_from_slice(&len.to_be_bytes());
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(&tiff);

    let mut out = Vec::new();
    out.extend_from_slice(&jpeg[0..2]); // SOI
    out.extend_from_slice(&app1);
    out.extend_from_slice(&jpeg[2..]); // the real encoder's own segments + scan data
    out
}

#[tokio::test]
async fn us5_a_valid_photo_gets_a_thumbnail_url_distinct_from_the_original() {
    let (app, _dir) = test_app().await;
    let id = import_sample_with_photos(&app, &[("a.jpg", &valid_jpeg_bytes(800, 600))]).await;

    let response = common::get(&app, &format!("/api/trips/{id}/photos")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    let url = json[0]["url"].as_str().unwrap();
    let thumbnail_url = json[0]["thumbnail_url"].as_str().unwrap();
    assert_ne!(
        thumbnail_url, url,
        "thumbnail_url must differ from the original"
    );

    // The original is untouched: fetching `url` returns the exact bytes uploaded.
    let original_response = common::get(&app, url).await;
    assert_eq!(
        body_bytes(original_response).await,
        valid_jpeg_bytes(800, 600)
    );

    // The thumbnail is a smaller, distinct, decodable image.
    let thumb_response = common::get(&app, thumbnail_url).await;
    assert_eq!(thumb_response.status(), StatusCode::OK);
    let thumb_bytes = body_bytes(thumb_response).await;
    let thumb = image::load_from_memory(&thumb_bytes).expect("thumbnail must be decodable");
    assert!(thumb.width() <= 400 && thumb.height() <= 400);
}

#[tokio::test]
async fn us5_a_photo_whose_thumbnail_generation_failed_falls_back_to_the_original_url() {
    let (app, _dir) = test_app().await;
    // The existing US-2/US-3 regression fixture: valid multipart bytes, not a
    // decodable image.
    let id = import_sample_with_photos(&app, &[("plain.jpg", b"\xFF\xD8\xFF-fake-jpeg")]).await;

    let response = common::get(&app, &format!("/api/trips/{id}/photos")).await;
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();

    assert_eq!(json[0]["thumbnail_url"], json[0]["url"]);
}

#[tokio::test]
async fn us5_photos_added_later_via_add_photos_endpoint_also_get_a_thumbnail() {
    let (app, _dir) = test_app().await;
    let id = import_sample_with_photos(&app, &[]).await;

    let response = send(
        &app,
        add_photos_request(id, &[("later.jpg", &valid_jpeg_bytes(200, 100))]),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);

    let photos_response = common::get(&app, &format!("/api/trips/{id}/photos")).await;
    let json: serde_json::Value =
        serde_json::from_str(&body_string(photos_response).await).unwrap();

    let url = json[0]["url"].as_str().unwrap();
    let thumbnail_url = json[0]["thumbnail_url"].as_str().unwrap();
    assert_ne!(thumbnail_url, url);
}

#[tokio::test]
async fn us5_thumbnail_honors_exif_orientation_through_the_full_ingest_pipeline() {
    use image::GenericImageView;

    let (app, _dir) = test_app().await;
    // 80x40 landscape: left half red, right half blue. EXIF orientation 6
    // (rotate 90 clockwise) must turn this into a 40x80 portrait thumbnail
    // with red on top and blue on the bottom.
    let oriented = oriented_jpeg_bytes(80, 40, 6);
    let id = import_sample_with_photos(&app, &[("oriented.jpg", &oriented)]).await;

    let response = common::get(&app, &format!("/api/trips/{id}/photos")).await;
    let json: serde_json::Value = serde_json::from_str(&body_string(response).await).unwrap();
    let thumbnail_url = json[0]["thumbnail_url"].as_str().unwrap();

    let thumb_response = common::get(&app, thumbnail_url).await;
    let thumb_bytes = body_bytes(thumb_response).await;
    let thumb = image::load_from_memory(&thumb_bytes).expect("thumbnail must be decodable");

    assert_eq!(thumb.width(), 40);
    assert_eq!(thumb.height(), 80);
    let top = thumb.get_pixel(20, 10);
    let bottom = thumb.get_pixel(20, 70);
    assert!(
        top[0] > 180 && top[2] < 80,
        "expected red near the top after a 90deg CW correction, got {top:?}"
    );
    assert!(
        bottom[2] > 180 && bottom[0] < 80,
        "expected blue near the bottom after a 90deg CW correction, got {bottom:?}"
    );
}
