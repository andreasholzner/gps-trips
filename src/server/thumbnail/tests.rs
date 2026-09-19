use super::*;
use crate::config::{photo::MAX_DIMENSION as BOUND, thumbnail::MAX_DIMENSION as THUMB};
use crate::server::location;
use fixtures::{valid_jpeg_bytes, valid_png_bytes};
use image::{GenericImageView, Pixel, Rgb, RgbImage};

// US-22: `guess_image_format` — a photo's real extension/content-type
// from its bytes, since Komoot's photo CDN response carries neither.

#[test]
fn guess_image_format_detects_jpeg() {
    assert_eq!(
        guess_image_format(&valid_jpeg_bytes(20, 10)),
        ("jpg", "image/jpeg")
    );
}

#[test]
fn guess_image_format_detects_png() {
    assert_eq!(
        guess_image_format(&valid_png_bytes(20, 10)),
        ("png", "image/png")
    );
}

#[test]
fn guess_image_format_falls_back_to_jpeg_for_undecodable_bytes() {
    assert_eq!(guess_image_format(b"not an image"), ("jpg", "image/jpeg"));
}

fn decode(bytes: &[u8]) -> DynamicImage {
    image::load_from_memory(bytes).expect("thumbnail must be a valid, decodable image")
}

/// A small, lossless (no JPEG round-trip) in-memory image with a
/// distinctive red pixel in the top-left corner and the rest blue —
/// enough to tell `apply_orientation`'s transforms apart exactly, by
/// checking where the red pixel ends up.
fn marked_image(width: u32, height: u32) -> DynamicImage {
    let mut img = RgbImage::from_pixel(width, height, Rgb([0, 0, 255]));
    img.put_pixel(0, 0, Rgb([255, 0, 0]));
    DynamicImage::ImageRgb8(img)
}

/// A thumbnail must exist, so a test can decode it.
fn thumbnail_of(bytes: &[u8], orientation: Option<u16>) -> DynamicImage {
    let thumb = process_photo(bytes, orientation)
        .thumbnail
        .expect("must decode a valid image");
    decode(&thumb)
}

#[test]
fn us5_generate_thumbnail_never_upscales_a_smaller_image() {
    // Regression guard: `.thumbnail()` scales to fit the box on either
    // side, which upscales a source already smaller than the box on both
    // axes — the opposite of "loads fast" (US-5).
    let (width, height) = (THUMB * 3 / 4, THUMB / 2);
    let decoded = thumbnail_of(&valid_jpeg_bytes(width, height), None);
    assert_eq!(decoded.width(), width);
    assert_eq!(decoded.height(), height);
}

#[test]
fn us5_generate_thumbnail_shrinks_a_larger_image_to_the_max_dimension() {
    let decoded = thumbnail_of(&valid_jpeg_bytes(800, 600), None);
    // Aspect ratio (4:3) preserved: long edge is width.
    assert_eq!(decoded.width(), THUMB);
    assert_eq!(decoded.height(), THUMB * 3 / 4);
}

#[test]
fn us5_generate_thumbnail_returns_none_for_undecodable_bytes() {
    assert!(process_photo(b"not an image at all", None)
        .thumbnail
        .is_none());
}

#[test]
fn us5_apply_orientation_1_or_absent_is_a_no_op() {
    let img = marked_image(10, 20);
    let out = apply_orientation(img.clone(), 1);
    assert_eq!(out.get_pixel(0, 0), img.get_pixel(0, 0));
    assert_eq!(out.width(), 10);
    assert_eq!(out.height(), 20);
}

#[test]
fn us5_apply_orientation_3_rotates_180_degrees() {
    let img = marked_image(10, 20);
    let out = apply_orientation(img, 3);
    // The top-left marker must now be at the bottom-right corner.
    assert_eq!(out.get_pixel(9, 19), Rgb([255, 0, 0]).to_rgba());
}

#[test]
fn us5_apply_orientation_6_rotates_90_degrees_clockwise() {
    let img = marked_image(10, 20);
    let out = apply_orientation(img, 6);
    // A 90 deg CW rotation swaps dimensions; the top-left marker moves
    // to the top-right corner.
    assert_eq!(out.width(), 20);
    assert_eq!(out.height(), 10);
    assert_eq!(out.get_pixel(19, 0), Rgb([255, 0, 0]).to_rgba());
}

#[test]
fn us5_apply_orientation_8_rotates_90_degrees_counterclockwise() {
    let img = marked_image(10, 20);
    let out = apply_orientation(img, 8);
    assert_eq!(out.width(), 20);
    assert_eq!(out.height(), 10);
    // A 90 deg CCW rotation moves the top-left marker to the bottom-left corner.
    assert_eq!(out.get_pixel(0, 9), Rgb([255, 0, 0]).to_rgba());
}

#[test]
fn us5_apply_orientation_2_flips_horizontally() {
    let img = marked_image(10, 20);
    let out = apply_orientation(img, 2);
    assert_eq!(out.get_pixel(9, 0), Rgb([255, 0, 0]).to_rgba());
}

#[test]
fn us5_apply_orientation_4_flips_vertically() {
    let img = marked_image(10, 20);
    let out = apply_orientation(img, 4);
    assert_eq!(out.get_pixel(0, 19), Rgb([255, 0, 0]).to_rgba());
}

#[test]
fn us5_generate_thumbnail_honors_orientation_end_to_end() {
    // A large enough source that `.thumbnail()` always shrinks (never
    // upscales) after the orientation swap, so the resulting dimensions
    // are deterministic: a 6 (90 deg CW) reorientation of an 800x600
    // source swaps the aspect ratio.
    let decoded = thumbnail_of(&valid_jpeg_bytes(800, 600), Some(6));
    // Post-rotation source is 600x800 (portrait); the long edge is height.
    assert_eq!(decoded.width(), THUMB * 3 / 4);
    assert_eq!(decoded.height(), THUMB);
}

// ── US-54: the stored copy is bounded in size (ADR-0026) ─────────────────

/// Just past the stored bound on the long edge, so the copy is always
/// shrunk, without paying for a camera-sized image in every test run.
fn oversized() -> (u32, u32) {
    (BOUND + 100, (BOUND + 100) / 2)
}

fn stored_copy(bytes: &[u8]) -> Vec<u8> {
    process_photo(bytes, None)
        .stored
        .expect("an oversized photo must get a stored copy")
}

#[test]
fn us54_a_photo_larger_than_the_bound_is_stored_scaled_down_to_it() {
    let (width, height) = oversized();
    let stored = decode(&stored_copy(&valid_jpeg_bytes(width, height)));
    assert_eq!(stored.width(), BOUND);
    assert_eq!(stored.height(), BOUND / 2);
}

#[test]
fn us54_a_photo_within_the_bound_is_stored_as_uploaded() {
    // `None` tells the caller to keep the upload's own bytes: nothing is
    // re-compressed without being made smaller.
    let processed = process_photo(&valid_jpeg_bytes(BOUND, BOUND / 2), None);
    assert!(processed.stored.is_none());
    assert!(processed.thumbnail.is_some(), "it still gets a thumbnail");
}

#[test]
fn us54_an_oversized_non_jpeg_photo_is_stored_as_a_jpeg() {
    let (width, height) = oversized();
    let stored = stored_copy(&valid_png_bytes(width, height));
    assert_eq!(guess_image_format(&stored), ("jpg", "image/jpeg"));
}

#[test]
fn us54_the_stored_copy_keeps_the_originals_exif_and_its_pixels_unrotated() {
    let (width, height) = oversized();
    let original = fixtures::jpeg_with_exif(
        width,
        height,
        &location::fixtures::geotagged_bytes(59.91, 10.75),
    );
    let stored = stored_copy(&original);

    let from_original = location::extract_photo_metadata(&original);
    let from_stored = location::extract_photo_metadata(&stored);
    assert!(from_original.gps.is_some(), "the fixture must be geotagged");
    assert_eq!(from_stored, from_original);
}

#[test]
fn us54_the_stored_copy_is_not_reoriented_so_its_orientation_tag_stays_true() {
    let (width, height) = oversized();
    let original =
        fixtures::jpeg_with_exif(width, height, &location::fixtures::orientation_bytes(6));
    let processed = process_photo(&original, Some(6));

    let stored = processed.stored.expect("oversized");
    assert_eq!(
        location::extract_photo_metadata(&stored).orientation,
        Some(6)
    );
    let pixels = decode(&stored);
    assert!(pixels.width() > pixels.height(), "still landscape as shot");
    // The thumbnail, which carries no EXIF, is the one turned upright.
    let thumb = decode(&processed.thumbnail.expect("thumbnail"));
    assert!(thumb.height() > thumb.width());
}

#[test]
fn us54_an_undecodable_photo_is_stored_as_uploaded() {
    let processed = process_photo(b"not an image at all", None);
    assert!(processed.stored.is_none());
}

#[test]
fn us54_a_photo_too_large_to_decode_within_the_limit_is_stored_as_uploaded() {
    let (width, height) = oversized();
    let bytes = valid_jpeg_bytes(width, height);
    // One byte short of the decoded RGB buffer.
    let limit = u64::from(width) * u64::from(height) * 3 - 1;

    let processed = process_photo_within(&bytes, None, limit);

    assert!(processed.stored.is_none());
    assert!(processed.thumbnail.is_none());
}

#[test]
fn us54_an_oversized_png_keeps_its_exif_in_the_stored_copy() {
    let (width, height) = oversized();
    let tiff = location::fixtures::geotagged_bytes(59.91, 10.75);
    let original = fixtures::png_with_exif(width, height, &tiff);

    let stored = stored_copy(&original);

    let gps = location::extract_photo_metadata(&stored).gps;
    assert!(gps.is_some(), "the EXIF came along");
}

#[test]
fn us54_exif_too_large_for_a_jpeg_keeps_the_photo_as_uploaded() {
    // A PNG's or WebP's EXIF can exceed the 64 KB a JPEG's APP1 segment
    // holds. Writing it anyway would store a corrupt JPEG in place of the
    // original; dropping it would break the copy's promise to keep it.
    let (width, height) = oversized();
    let mut tiff = location::fixtures::geotagged_bytes(59.91, 10.75);
    tiff.resize(70 * 1024, 0);
    let original = fixtures::png_with_exif(width, height, &tiff);

    let processed = process_photo(&original, None);

    assert!(processed.stored.is_none(), "stored as uploaded");
    assert!(processed.thumbnail.is_some(), "it still gets a thumbnail");
}

#[test]
fn us54_exif_that_just_fits_a_jpeg_is_kept_and_the_copy_decodes() {
    let (width, height) = oversized();
    let mut tiff = location::fixtures::geotagged_bytes(59.91, 10.75);
    tiff.resize(MAX_JPEG_EXIF_LEN, 0);
    let original = fixtures::png_with_exif(width, height, &tiff);

    let stored = stored_copy(&original);

    assert_eq!(decode(&stored).width(), BOUND);
    assert!(location::extract_photo_metadata(&stored).gps.is_some());
}
