//! Thumbnail generation (US-5, ADR-0020) and the size-bounded copy each
//! photo is stored as (US-54, ADR-0026). Sole owner of the `image` crate
//! dependency, mirroring how `location.rs`/`timezone.rs` isolate
//! `kamadak-exif`/`tzf-rs` behind a narrow module surface.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageReader};

use crate::config::{photo, thumbnail};

/// What one decode of an uploaded photo yields. Both halves are
/// best-effort: `None` never fails the import.
#[derive(Debug, Default)]
pub struct ProcessedPhoto {
    /// The JPEG to store instead of the upload, when the upload exceeds the
    /// size bound. `None` means store the upload itself: it is within the
    /// bound, or it could not be decoded (ADR-0026).
    pub stored: Option<Vec<u8>>,
    /// The thumbnail, already turned upright (US-5).
    pub thumbnail: Option<Vec<u8>>,
}

/// Decode a photo once and derive from it both the copy to store and the
/// thumbnail. `orientation` is the raw EXIF tag from
/// `location::PhotoMetadata`; it is applied to the thumbnail only, since the
/// stored copy keeps the original's EXIF, Orientation tag included.
pub fn process_photo(bytes: &[u8], orientation: Option<u16>) -> ProcessedPhoto {
    process_photo_within(bytes, orientation, photo::MAX_DECODE_BYTES)
}

/// [`process_photo`] with the decode allocation limit as a parameter, so a
/// test can exceed it without a gigantic fixture.
fn process_photo_within(bytes: &[u8], orientation: Option<u16>, max_decode: u64) -> ProcessedPhoto {
    let Some(decoded) = decode(bytes, max_decode) else {
        return ProcessedPhoto::default();
    };
    let Decoded {
        mut image,
        exif,
        icc,
    } = decoded;
    let mut stored = None;
    if exceeds(&image, photo::MAX_DIMENSION) {
        let bound = photo::MAX_DIMENSION;
        // `.thumbnail()` rather than a filtered `.resize()`: a filter's
        // vertical pass allocates an f32 RGBA buffer at the source's full
        // width — larger than the decoded photo itself — and the whole point
        // is that an import fits in the machine's memory (US-54). Its block
        // averaging aliases only when the size barely changes, and camera
        // photos are far past the bound. Reassigning frees the full-size
        // buffer before the thumbnail is made.
        image = image.thumbnail(bound, bound);
        stored = encode_jpeg(&image, photo::JPEG_QUALITY, exif, icc);
    }
    ProcessedPhoto {
        stored,
        thumbnail: make_thumbnail(image, orientation),
    }
}

/// A decoded photo and the metadata the stored copy carries over.
struct Decoded {
    image: DynamicImage,
    exif: Option<Vec<u8>>,
    icc: Option<Vec<u8>>,
}

/// `None` if the bytes are no image `image` understands, or if decoding
/// them would allocate more than `max_decode` bytes.
fn decode(bytes: &[u8], max_decode: u64) -> Option<Decoded> {
    let mut decoder = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    if decoder.total_bytes() > max_decode {
        tracing::warn!(
            bytes = decoder.total_bytes(),
            "photo too large to decode; storing it as uploaded"
        );
        return None;
    }
    let exif = decoder.exif_metadata().ok().flatten();
    let icc = decoder.icc_profile().ok().flatten();
    let image = DynamicImage::from_decoder(decoder).ok()?;
    Some(Decoded { image, exif, icc })
}

fn exceeds(image: &DynamicImage, bound: u32) -> bool {
    image.width() > bound || image.height() > bound
}

/// The most EXIF a JPEG can carry: one APP1 segment's 65,533 data bytes,
/// less its `Exif\0\0` header. The encoder does not check this — it wraps
/// the segment's 16-bit length and writes a corrupt file.
const MAX_JPEG_EXIF_LEN: usize = 65_533 - 6;

/// Encode as JPEG, carrying over the source's EXIF and colour profile when
/// it had them. `None` if the EXIF will not fit a JPEG: only a PNG or WebP
/// can hold that much, and the copy keeps its EXIF or is not made.
fn encode_jpeg(
    image: &DynamicImage,
    quality: u8,
    exif: Option<Vec<u8>>,
    icc: Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    if exif
        .as_ref()
        .is_some_and(|exif| exif.len() > MAX_JPEG_EXIF_LEN)
    {
        tracing::warn!("photo's EXIF is too large for a JPEG; storing it as uploaded");
        return None;
    }
    let mut out = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut out, quality);
    if let Some(exif) = exif {
        encoder.set_exif_metadata(exif).ok()?;
    }
    if let Some(icc) = icc {
        encoder.set_icc_profile(icc).ok()?;
    }
    encoder.encode_image(image).ok()?;
    Some(out)
}

/// The thumbnail of an already-decoded photo, turned upright. Always a JPEG
/// regardless of the source format (ADR-0020).
fn make_thumbnail(image: DynamicImage, orientation: Option<u16>) -> Option<Vec<u8>> {
    // `.thumbnail()` scales to fit the box on *either* side, upscaling a
    // smaller source — the opposite of "loads fast". Only shrink; a photo
    // already within bounds keeps its own (smaller) dimensions.
    let bound = thumbnail::MAX_DIMENSION;
    let resized = if exceeds(&image, bound) {
        image.thumbnail(bound, bound)
    } else {
        image
    };
    // Orient *after* resizing, not before: the target box is square, so a
    // rotate/flip commutes with fitting into it — resizing first means the
    // rotate/flip touches only a thumbnail-sized buffer.
    let thumb = apply_orientation(resized, orientation.unwrap_or(1));
    encode_jpeg(&thumb, thumbnail::JPEG_QUALITY, None, None)
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
/// `tests/it/us5_thumbnails.rs`, so there is one source of truth for "a real
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

    /// A solid-color JPEG carrying `tiff` (a raw TIFF/EXIF stream, e.g. from
    /// `location::fixtures`) as its EXIF APP1 segment — what a camera writes.
    pub fn jpeg_with_exif(width: u32, height: u32, tiff: &[u8]) -> Vec<u8> {
        let jpeg = valid_jpeg_bytes(width, height);
        // Marker, big-endian length (counting itself), "Exif\0\0", payload.
        let len = u16::try_from(2 + 6 + tiff.len()).expect("fixture EXIF fits a segment");
        let mut out = jpeg[0..2].to_vec(); // SOI
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(b"Exif\0\0");
        out.extend_from_slice(tiff);
        out.extend_from_slice(&jpeg[2..]);
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

    /// A solid-color PNG carrying `tiff` as its `eXIf` chunk — which, unlike
    /// a JPEG's APP1 segment, has no 64 KB limit.
    pub fn png_with_exif(width: u32, height: u32, tiff: &[u8]) -> Vec<u8> {
        let img = RgbImage::from_pixel(width, height, Rgb([10, 20, 30]));
        let mut out = Vec::new();
        let mut encoder = PngEncoder::new(&mut out);
        encoder.set_exif_metadata(tiff.to_vec()).unwrap();
        encoder
            .write_image(&img, width, height, image::ExtendedColorType::Rgb8)
            .unwrap();
        out
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
// Split into thumbnail/tests.rs to keep this file under the repo's 500-line cap.

#[cfg(test)]
mod tests;
