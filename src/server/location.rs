//! Per-photo map placement (US-3: EXIF GPS; US-4 will add timestamp-based
//! interpolation for photos without GPS). This module owns everything about
//! *where* a photo goes on the map; `photos.rs::ingest_photos` calls into it
//! and stores the result, never touching EXIF directly itself.
//!
//! GPS extraction is always best-effort (ADR-0017): a missing, unparseable,
//! or out-of-range position is `None`, never an error — an untrusted upload
//! with corrupt or absent EXIF must not block the import. Callers still get
//! a `tracing::debug!` breadcrumb on the `None` path (logged at the call
//! site in `photos.rs`, since that's where the photo's name is known).

use std::io::Cursor;

use exif::{In, Tag};

/// A decoded EXIF GPS position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpsPosition {
    pub lat: f64,
    pub lon: f64,
}

/// Read EXIF GPS coordinates out of raw image bytes, if present and valid.
/// `None` for: an unparseable container, no GPS IFD, an unrecognized N/S/E/W
/// reference byte, or a lat/lon outside the valid range. Never panics, never
/// returns an `Err` — the caller always gets a position or nothing.
pub fn extract_gps(bytes: &[u8]) -> Option<GpsPosition> {
    let exif = exif::Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()?;

    let lat = dms_field(&exif, Tag::GPSLatitude, Tag::GPSLatitudeRef)?;
    let lon = dms_field(&exif, Tag::GPSLongitude, Tag::GPSLongitudeRef)?;
    validate_range(lat, lon)
}

/// Read one DMS coordinate (three rationals) plus its hemisphere reference
/// tag, and convert to signed decimal degrees.
fn dms_field(exif: &exif::Exif, value_tag: Tag, ref_tag: Tag) -> Option<f64> {
    let value_field = exif.get_field(value_tag, In::PRIMARY)?;
    let ref_field = exif.get_field(ref_tag, In::PRIMARY)?;

    let (deg, min, sec) = as_dms_triple(&value_field.value)?;
    let hemisphere = hemisphere_of(&ref_field.value)?;
    Some(dms_to_decimal(deg, min, sec, hemisphere))
}

/// Pull the three (degrees, minutes, seconds) rationals out of an EXIF field
/// value, as `f64`. `None` if the value isn't exactly three rationals.
fn as_dms_triple(value: &exif::Value) -> Option<(f64, f64, f64)> {
    match value {
        exif::Value::Rational(rationals) if rationals.len() == 3 => Some((
            rationals[0].to_f64(),
            rationals[1].to_f64(),
            rationals[2].to_f64(),
        )),
        _ => None,
    }
}

/// The sign an EXIF hemisphere-reference field implies (e.g. `GPSLatitudeRef`'s
/// `"N\0"`). `'N'`/`'E'` are positive, `'S'`/`'W'` are negative; anything else
/// (including a field that isn't a single-character ASCII value) is not a
/// recognized reference and invalidates the whole position — it does not
/// silently default to positive.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Hemisphere {
    Positive,
    Negative,
}

fn hemisphere_of(value: &exif::Value) -> Option<Hemisphere> {
    let exif::Value::Ascii(strings) = value else {
        return None;
    };
    match strings.first()?.first()? {
        b'N' | b'E' => Some(Hemisphere::Positive),
        b'S' | b'W' => Some(Hemisphere::Negative),
        _ => None,
    }
}

/// Degrees/minutes/seconds (as EXIF's `GPSLatitude`/`GPSLongitude` encode
/// them — three rationals) plus a hemisphere, to signed decimal degrees.
/// Pure, total function: no validation here, only conversion (ADR-0012's
/// "DMS→decimal EXIF conversion" pure-logic unit).
fn dms_to_decimal(degrees: f64, minutes: f64, seconds: f64, hemisphere: Hemisphere) -> f64 {
    let magnitude = degrees + minutes / 60.0 + seconds / 3600.0;
    match hemisphere {
        Hemisphere::Positive => magnitude,
        Hemisphere::Negative => -magnitude,
    }
}

/// `None` if `lat`/`lon` fall outside their valid ranges — defends against
/// corrupt or hostile EXIF in an untrusted upload.
fn validate_range(lat: f64, lon: f64) -> Option<GpsPosition> {
    if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) {
        Some(GpsPosition { lat, lon })
    } else {
        None
    }
}

/// Hand-built minimal TIFF/EXIF byte fixtures, real enough for `kamadak-exif`
/// to parse for real (ADR-0012, ADR-0017) — no image tooling
/// (PIL/piexif/exiftool) is available in this environment, and none is
/// needed. `kamadak-exif`'s container sniffing recognizes a bare TIFF byte
/// stream directly (a TIFF file *is* the EXIF IFD structure — no JPEG
/// SOI/APP1 wrapping needed), so these are plain, minimal TIFF files: an
/// 8-byte header, an IFD0 with a single GPSInfo-IFD-pointer entry, and a GPS
/// IFD with the four tags this module reads.
///
/// `pub`, gated by `test-support` as well as `test`, so both this module's
/// own unit tests *and* `tests/us3_photo_map_placement.rs`'s integration
/// tests call the same byte-builder — see the `[dev-dependencies]` entry in
/// `Cargo.toml` that enables `test-support` for `tests/`. A single source of
/// truth for the TIFF layout, instead of two hand-maintained copies.
#[cfg(any(test, feature = "test-support"))]
pub mod fixtures {
    const TIFF_HEADER_LEN: u32 = 8;

    fn u16_le(n: u16) -> [u8; 2] {
        n.to_le_bytes()
    }
    fn u32_le(n: u32) -> [u8; 4] {
        n.to_le_bytes()
    }

    /// One EXIF IFD entry: tag, type, count, and a 4-byte value/offset field.
    fn ifd_entry(tag: u16, kind: u16, count: u32, value_or_offset: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::with_capacity(12);
        out.extend_from_slice(&u16_le(tag));
        out.extend_from_slice(&u16_le(kind));
        out.extend_from_slice(&u32_le(count));
        out.extend_from_slice(&value_or_offset);
        out
    }

    /// An ASCII EXIF value up to 4 bytes (incl. the trailing NUL), inline in
    /// the entry's value field — exactly how `GPSLatitudeRef`/`GPSLongitudeRef`
    /// ("N\0", "S\0", ...) are stored.
    fn inline_ascii(byte: u8) -> [u8; 4] {
        [byte, 0, 0, 0]
    }

    /// Encode `decimal` degrees as an EXIF (degrees, minutes, seconds)
    /// rational triple: 8 bytes each (numerator/denominator, both u32 LE).
    /// Seconds keep 4 decimal digits of precision (denominator 10000).
    fn dms_rationals(decimal: f64) -> Vec<u8> {
        let abs = decimal.abs();
        let deg = abs.floor();
        let min_full = (abs - deg) * 60.0;
        let min = min_full.floor();
        let sec = (min_full - min) * 60.0;

        let mut out = Vec::with_capacity(24);
        out.extend_from_slice(&u32_le(deg as u32));
        out.extend_from_slice(&u32_le(1));
        out.extend_from_slice(&u32_le(min as u32));
        out.extend_from_slice(&u32_le(1));
        out.extend_from_slice(&u32_le((sec * 10_000.0).round() as u32));
        out.extend_from_slice(&u32_le(10_000));
        out
    }

    const TYPE_ASCII: u16 = 2;
    const TYPE_LONG: u16 = 4;
    const TYPE_RATIONAL: u16 = 5;
    const TAG_GPS_INFO_IFD_POINTER: u16 = 0x8825;
    const TAG_GPS_LATITUDE_REF: u16 = 1;
    const TAG_GPS_LATITUDE: u16 = 2;
    const TAG_GPS_LONGITUDE_REF: u16 = 3;
    const TAG_GPS_LONGITUDE: u16 = 4;

    /// Build a minimal little-endian TIFF/EXIF byte stream. `gps_ifd` is the
    /// already-assembled GPS IFD (or `None` to omit it entirely, producing a
    /// TIFF with no GPS data at all).
    pub fn build_tiff(gps_ifd: Option<Vec<u8>>) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"II"); // little-endian byte order
        out.extend_from_slice(&u16_le(42)); // TIFF magic number
        out.extend_from_slice(&u32_le(TIFF_HEADER_LEN)); // IFD0 starts right after the header

        match gps_ifd {
            None => {
                // IFD0 with zero entries, no GPS IFD pointer at all.
                out.extend_from_slice(&u16_le(0));
                out.extend_from_slice(&u32_le(0)); // no next IFD
            }
            Some(gps_ifd) => {
                let gps_ifd_offset = TIFF_HEADER_LEN + 2 + 12 + 4; // header + IFD0(count+1 entry+next)
                out.extend_from_slice(&u16_le(1));
                out.extend_from_slice(&ifd_entry(
                    TAG_GPS_INFO_IFD_POINTER,
                    TYPE_LONG,
                    1,
                    u32_le(gps_ifd_offset),
                ));
                out.extend_from_slice(&u32_le(0)); // no next IFD after IFD0
                out.extend_from_slice(&gps_ifd);
            }
        }
        out
    }

    /// A GPS IFD with all four tags this module reads, rationals stored
    /// after the IFD itself (they don't fit inline in a 4-byte value field).
    pub fn gps_ifd(lat: f64, lat_ref: u8, lon: f64, lon_ref: u8) -> Vec<u8> {
        // 8 (header) + 2+12+4 (IFD0) = offset where this GPS IFD begins.
        let gps_ifd_start = TIFF_HEADER_LEN + 2 + 12 + 4;
        let ifd_body_len = 2 + 4 * 12 + 4; // count + 4 entries + next-IFD offset
        let lat_rationals_offset = gps_ifd_start + ifd_body_len;
        let lon_rationals_offset = lat_rationals_offset + 24;

        let mut ifd = Vec::new();
        ifd.extend_from_slice(&u16_le(4)); // 4 entries
        ifd.extend_from_slice(&ifd_entry(
            TAG_GPS_LATITUDE_REF,
            TYPE_ASCII,
            2,
            inline_ascii(lat_ref),
        ));
        ifd.extend_from_slice(&ifd_entry(
            TAG_GPS_LATITUDE,
            TYPE_RATIONAL,
            3,
            u32_le(lat_rationals_offset),
        ));
        ifd.extend_from_slice(&ifd_entry(
            TAG_GPS_LONGITUDE_REF,
            TYPE_ASCII,
            2,
            inline_ascii(lon_ref),
        ));
        ifd.extend_from_slice(&ifd_entry(
            TAG_GPS_LONGITUDE,
            TYPE_RATIONAL,
            3,
            u32_le(lon_rationals_offset),
        ));
        ifd.extend_from_slice(&u32_le(0)); // no next IFD after the GPS IFD

        ifd.extend_from_slice(&dms_rationals(lat));
        ifd.extend_from_slice(&dms_rationals(lon));
        ifd
    }

    /// A GPS IFD whose latitude reference byte is neither `N` nor `S`.
    pub fn gps_ifd_with_malformed_ref(lat: f64, lon: f64, lon_ref: u8) -> Vec<u8> {
        gps_ifd(lat, b'X', lon, lon_ref)
    }

    /// A minimal geotagged TIFF/EXIF byte stream for `lat`/`lon` (signed
    /// decimal degrees).
    pub fn geotagged_bytes(lat: f64, lon: f64) -> Vec<u8> {
        let lat_ref = if lat < 0.0 { b'S' } else { b'N' };
        let lon_ref = if lon < 0.0 { b'W' } else { b'E' };
        build_tiff(Some(gps_ifd(lat, lat_ref, lon, lon_ref)))
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::fixtures::{build_tiff, gps_ifd, gps_ifd_with_malformed_ref};
    use super::*;

    #[test]
    fn us3_dms_to_decimal_converts_north_positive() {
        let decimal = dms_to_decimal(45.0, 30.0, 0.0, Hemisphere::Positive);
        assert!((decimal - 45.5).abs() < 1e-9);
    }

    #[test]
    fn us3_dms_to_decimal_converts_south_negative() {
        let decimal = dms_to_decimal(45.0, 30.0, 0.0, Hemisphere::Negative);
        assert!((decimal - -45.5).abs() < 1e-9);
    }

    #[test]
    fn us3_dms_to_decimal_converts_east_positive() {
        let decimal = dms_to_decimal(10.0, 15.0, 36.0, Hemisphere::Positive);
        assert!((decimal - 10.26).abs() < 1e-9);
    }

    #[test]
    fn us3_dms_to_decimal_converts_west_negative() {
        let decimal = dms_to_decimal(10.0, 15.0, 36.0, Hemisphere::Negative);
        assert!((decimal - -10.26).abs() < 1e-9);
    }

    #[test]
    fn us3_dms_to_decimal_handles_zero_minutes_and_seconds() {
        assert_eq!(dms_to_decimal(51.0, 0.0, 0.0, Hemisphere::Positive), 51.0);
    }

    #[test]
    fn us3_hemisphere_of_rejects_unrecognized_byte() {
        assert_eq!(
            hemisphere_of(&exif::Value::Ascii(vec![b"N".to_vec()])),
            Some(Hemisphere::Positive)
        );
        assert_eq!(
            hemisphere_of(&exif::Value::Ascii(vec![b"S".to_vec()])),
            Some(Hemisphere::Negative)
        );
        assert_eq!(
            hemisphere_of(&exif::Value::Ascii(vec![b"X".to_vec()])),
            None
        );
    }

    #[test]
    fn us3_extract_gps_reads_coordinates_from_valid_geotagged_jpeg() {
        let bytes = build_tiff(Some(gps_ifd(45.5, b'N', 10.26, b'E')));
        let pos = extract_gps(&bytes).expect("valid geotagged fixture must yield a position");
        assert!((pos.lat - 45.5).abs() < 1e-3);
        assert!((pos.lon - 10.26).abs() < 1e-3);
    }

    #[test]
    fn us3_extract_gps_handles_southern_and_western_hemispheres() {
        let bytes = build_tiff(Some(gps_ifd(33.9, b'S', 18.4, b'W')));
        let pos = extract_gps(&bytes).expect("valid geotagged fixture must yield a position");
        assert!((pos.lat - -33.9).abs() < 1e-3);
        assert!((pos.lon - -18.4).abs() < 1e-3);
    }

    #[test]
    fn us3_extract_gps_returns_none_for_jpeg_without_exif() {
        assert!(extract_gps(b"\xFF\xD8\xFF-fake-jpeg").is_none());
    }

    #[test]
    fn us3_extract_gps_returns_none_for_exif_without_gps_ifd() {
        let bytes = build_tiff(None);
        assert!(extract_gps(&bytes).is_none());
    }

    #[test]
    fn us3_extract_gps_returns_none_for_malformed_gps_ref_byte() {
        let bytes = build_tiff(Some(gps_ifd_with_malformed_ref(45.5, 10.26, b'E')));
        assert!(extract_gps(&bytes).is_none());
    }

    #[test]
    fn us3_extract_gps_returns_none_for_out_of_range_latitude() {
        let bytes = build_tiff(Some(gps_ifd(95.0, b'N', 10.26, b'E')));
        assert!(extract_gps(&bytes).is_none());
    }

    #[test]
    fn us3_extract_gps_returns_none_for_out_of_range_longitude() {
        let bytes = build_tiff(Some(gps_ifd(45.5, b'N', 185.0, b'E')));
        assert!(extract_gps(&bytes).is_none());
    }

    #[test]
    fn us3_extract_gps_returns_none_for_non_image_bytes() {
        assert!(extract_gps(b"not an image at all").is_none());
    }

    #[test]
    fn us3_extract_gps_returns_none_for_truncated_or_corrupt_container() {
        let mut bytes = build_tiff(Some(gps_ifd(45.5, b'N', 10.26, b'E')));
        bytes.truncate(bytes.len() / 2);
        assert!(extract_gps(&bytes).is_none());
    }
}
