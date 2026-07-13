//! Thumbnail generation (US-5, ADR-0020). Sole owner of the `image` crate
//! dependency, mirroring how `location.rs`/`timezone.rs` isolate
//! `kamadak-exif`/`tzf-rs` behind a narrow module surface.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageReader};

use crate::config::thumbnail::{JPEG_QUALITY, MAX_DIMENSION};

/// Generate a thumbnail from a photo's original bytes, honoring EXIF
/// orientation (`orientation`, the raw tag value from
/// `location::PhotoMetadata`). Always re-encoded as JPEG regardless of the
/// source format (ADR-0020). `None` if the bytes can't be decoded as an
/// image `image` understands — never fatal to the import, the same
/// best-effort stance `location.rs` takes for EXIF extraction.
pub fn generate_thumbnail(bytes: &[u8], orientation: Option<u16>) -> Option<Vec<u8>> {
    let img = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    // `.thumbnail()` scales to fit the box on *either* side, upscaling a
    // smaller source — the opposite of "loads fast". Only shrink; a photo
    // already within bounds keeps its own (smaller) dimensions.
    let resized = if img.width() > MAX_DIMENSION || img.height() > MAX_DIMENSION {
        img.thumbnail(MAX_DIMENSION, MAX_DIMENSION)
    } else {
        img
    };
    // Orient *after* resizing, not before: the target box is square, so a
    // rotate/flip commutes with fitting into it — resizing first means the
    // rotate/flip touches at most a 400x400 buffer instead of the original
    // full-resolution photo (often 10x+ larger on each axis).
    let thumb = apply_orientation(resized, orientation.unwrap_or(1));

    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(&thumb)
        .ok()?;
    Some(out)
}

/// Guess an image's real format from its magic bytes, returning
/// `(extension, content_type)`. Used wherever a photo's bytes arrive without
/// a trustworthy filename/`Content-Type` of their own (US-22: Komoot's photo
/// CDN response carries neither) — storing the blob under the wrong
/// extension makes `http.rs`'s `content_type_from_path` serve it with the
/// wrong `Content-Type` later (see `thumbnail_key`'s doc comment for the
/// same trap). Falls back to `("jpg", "image/jpeg")` when the format can't
/// be determined, matching this module's best-effort stance elsewhere.
pub fn guess_image_format(bytes: &[u8]) -> (&'static str, &'static str) {
    match image::guess_format(bytes) {
        Ok(image::ImageFormat::Png) => ("png", "image/png"),
        Ok(image::ImageFormat::Gif) => ("gif", "image/gif"),
        Ok(image::ImageFormat::WebP) => ("webp", "image/webp"),
        _ => ("jpg", "image/jpeg"),
    }
}

/// Standard EXIF orientation values 1-8 -> the rotate/flip that makes the
/// pixel buffer display right-side-up. Absent or unrecognized (including 1)
/// is a no-op.
fn apply_orientation(img: DynamicImage, orientation: u16) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

/// A small, real, decodable JPEG test fixture — shared by this module's own
/// unit tests, sibling modules' tests (`photos.rs`, `delete.rs`), and
/// `tests/us5_thumbnails.rs`, so there is one source of truth for "a real
/// image `image` can decode" instead of hand-maintained per-file copies.
/// Gated the same way `location::fixtures` is (ADR-0012's precedent).
#[cfg(any(test, feature = "test-support"))]
pub mod fixtures {
    use image::{codecs::jpeg::JpegEncoder, codecs::png::PngEncoder, ImageEncoder, Rgb, RgbImage};

    /// A solid-color JPEG at the given dimensions. Big enough (e.g.
    /// 800x600) that `.thumbnail()` always shrinks it (never upscales), if
    /// the caller needs deterministic resize behavior.
    pub fn valid_jpeg_bytes(width: u32, height: u32) -> Vec<u8> {
        let img = RgbImage::from_pixel(width, height, Rgb([10, 20, 30]));
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, 90)
            .encode_image(&img)
            .unwrap();
        out
    }

    /// A solid-color PNG at the given dimensions — a non-JPEG fixture for
    /// exercising format detection (`guess_image_format`).
    pub fn valid_png_bytes(width: u32, height: u32) -> Vec<u8> {
        let img = RgbImage::from_pixel(width, height, Rgb([10, 20, 30]));
        let mut out = Vec::new();
        PngEncoder::new(&mut out)
            .write_image(&img, width, height, image::ExtendedColorType::Rgb8)
            .unwrap();
        out
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn us5_generate_thumbnail_never_upscales_a_smaller_image() {
        // Regression guard: `.thumbnail()` scales to fit the box on either
        // side, which upscales a source already smaller than 400px on both
        // axes — the opposite of "loads fast" (US-5).
        let bytes = valid_jpeg_bytes(300, 200);
        let thumb = generate_thumbnail(&bytes, None).expect("must decode a valid JPEG");
        let decoded = decode(&thumb);
        assert_eq!(decoded.width(), 300);
        assert_eq!(decoded.height(), 200);
    }

    #[test]
    fn us5_generate_thumbnail_shrinks_a_larger_image_to_the_max_dimension() {
        let bytes = valid_jpeg_bytes(800, 600);
        let thumb = generate_thumbnail(&bytes, None).expect("must decode a valid JPEG");
        let decoded = decode(&thumb);
        assert!(decoded.width() <= 400 && decoded.height() <= 400);
        // Aspect ratio (4:3) preserved: long edge is width.
        assert_eq!(decoded.width(), 400);
        assert_eq!(decoded.height(), 300);
    }

    #[test]
    fn us5_generate_thumbnail_returns_none_for_undecodable_bytes() {
        assert!(generate_thumbnail(b"not an image at all", None).is_none());
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
        // source swaps the aspect ratio before resizing.
        let bytes = valid_jpeg_bytes(800, 600);
        let thumb = generate_thumbnail(&bytes, Some(6)).expect("must decode a valid JPEG");
        let decoded = decode(&thumb);
        // Post-rotation source is 600x800 (portrait); long edge (height) is 400.
        assert_eq!(decoded.width(), 300);
        assert_eq!(decoded.height(), 400);
    }
}
