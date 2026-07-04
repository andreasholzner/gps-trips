//! Per-photo map placement (US-3: EXIF GPS; US-4: timestamp-based
//! interpolation for photos without GPS). This module owns everything about
//! reading EXIF metadata relevant to placement; `photos.rs::ingest_photos`
//! calls into it and stores the result, never touching EXIF directly itself.
//!
//! Extraction is always best-effort (ADR-0017): a missing, unparseable, or
//! out-of-range value is `None`, never an error — an untrusted upload with
//! corrupt or absent EXIF must not block the import. Callers still get a
//! `tracing::debug!` breadcrumb on the `None` path (logged at the call site
//! in `photos.rs`, since that's where the photo's name is known).

use std::io::Cursor;

use exif::{In, Tag};

/// A decoded EXIF GPS position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpsPosition {
    pub lat: f64,
    pub lon: f64,
}

/// A photo's EXIF capture time: wall-clock (no zone) plus an optional
/// EXIF-embedded UTC offset (`OffsetTimeOriginal`), which always takes
/// priority over a trip's timezone assumption when present (ADR-0009).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureTime {
    pub naive: time::PrimitiveDateTime,
    pub embedded_offset_minutes: Option<i32>,
}

/// The EXIF metadata `ingest_photos` needs from one photo, read in a single
/// container-parse pass (US-3's GPS, US-4's capture time, and US-5's
/// orientation are all ordinary EXIF fields, so there is no reason to open
/// the container more than once).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PhotoMetadata {
    pub gps: Option<GpsPosition>,
    pub capture_time: Option<CaptureTime>,
    /// The raw EXIF `Orientation` tag value (1-8), if present and numeric.
    /// Interpreting what each value means is `thumbnail.rs`'s job (US-5,
    /// ADR-0020) — this module only reads the tag.
    pub orientation: Option<u16>,
}

/// Read a photo's EXIF GPS position and capture time, if present and valid.
/// Each is independently best-effort: an unparseable container yields both
/// `None`; otherwise `gps`/`capture_time` are each `None` on their own if
/// that specific data is absent or invalid. Never panics, never returns an
/// `Err` — the caller always gets a (possibly empty) `PhotoMetadata`.
pub fn extract_photo_metadata(bytes: &[u8]) -> PhotoMetadata {
    let Ok(exif) = exif::Reader::new().read_from_container(&mut Cursor::new(bytes)) else {
        return PhotoMetadata::default();
    };
    PhotoMetadata {
        gps: read_gps(&exif),
        capture_time: read_capture_time(&exif),
        orientation: read_orientation(&exif),
    }
}

/// `None` for: no GPS IFD, an unrecognized N/S/E/W reference byte, or a
/// lat/lon outside the valid range.
fn read_gps(exif: &exif::Exif) -> Option<GpsPosition> {
    let lat = dms_field(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef)?;
    let lon = dms_field(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef)?;
    validate_range(lat, lon)
}

/// `None` for: no `DateTimeOriginal` field, or a malformed one. A present but
/// malformed `OffsetTimeOriginal` is ignored (falls back to no embedded
/// offset) rather than invalidating the whole capture time.
fn read_capture_time(exif: &exif::Exif) -> Option<CaptureTime> {
    let raw = ascii_field(exif, Tag::DateTimeOriginal)?;
    let mut dt = exif::DateTime::from_ascii(raw).ok()?;

    // A present-but-malformed offset is ignored (falls back to no embedded
    // offset) rather than invalidating the whole capture time.
    if let Some(offset_raw) = ascii_field(exif, Tag::OffsetTimeOriginal) {
        let _ = dt.parse_offset(offset_raw);
    }

    let month = time::Month::try_from(dt.month).ok()?;
    let date = time::Date::from_calendar_date(dt.year as i32, month, dt.day).ok()?;
    let time_of_day = time::Time::from_hms(dt.hour, dt.minute, dt.second).ok()?;
    Some(CaptureTime {
        naive: time::PrimitiveDateTime::new(date, time_of_day),
        embedded_offset_minutes: dt.offset.map(|m| m as i32),
    })
}

/// `None` if `Orientation` is absent or isn't a numeric value (e.g. a
/// malformed tag). No range validation beyond that — an out-of-range numeric
/// value (outside 1-8) is passed through and `thumbnail.rs` treats it as a
/// no-op, the same "don't invalidate the rest" stance `read_capture_time`
/// takes for a malformed `OffsetTimeOriginal`.
fn read_orientation(exif: &exif::Exif) -> Option<u16> {
    let field = exif.get_field(Tag::Orientation, In::PRIMARY)?;
    let value = field.value.get_uint(0)?;
    u16::try_from(value).ok()
}

/// The raw bytes of an EXIF ASCII field's first (and, for the fields this
/// module reads, only) string value. `None` if the tag is absent or isn't an
/// ASCII value — shared by `DateTimeOriginal` and `OffsetTimeOriginal`, both
/// read the same way.
fn ascii_field(exif: &exif::Exif, tag: Tag) -> Option<&Vec<u8>> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    match &field.value {
        exif::Value::Ascii(strings) => strings.first(),
        _ => None,
    }
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

#[cfg(any(test, feature = "test-support"))]
pub mod fixtures;

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::fixtures::{
        build_tiff, capture_time_bytes, geotagged_bytes_with_capture_time, gps_ifd,
        gps_ifd_with_malformed_ref, malformed_orientation_bytes, orientation_bytes,
    };
    use super::*;
    use time::macros::datetime;

    fn extract_gps(bytes: &[u8]) -> Option<GpsPosition> {
        extract_photo_metadata(bytes).gps
    }

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

    // ── US-4: capture-time extraction ────────────────────────────────────

    #[test]
    fn us4_extract_photo_metadata_reads_capture_time_without_an_embedded_offset() {
        let bytes = capture_time_bytes("2024:06:01 08:30:00", None);
        let capture = extract_photo_metadata(&bytes)
            .capture_time
            .expect("valid DateTimeOriginal must yield a capture time");
        assert_eq!(capture.naive, datetime!(2024-06-01 08:30:00));
        assert_eq!(capture.embedded_offset_minutes, None);
    }

    #[test]
    fn us4_extract_photo_metadata_reads_an_embedded_utc_offset() {
        let bytes = capture_time_bytes("2024:06:01 08:30:00", Some("+02:00"));
        let capture = extract_photo_metadata(&bytes).capture_time.unwrap();
        assert_eq!(capture.embedded_offset_minutes, Some(120));
    }

    #[test]
    fn us4_extract_photo_metadata_reads_a_negative_embedded_utc_offset() {
        let bytes = capture_time_bytes("2024:06:01 08:30:00", Some("-05:00"));
        let capture = extract_photo_metadata(&bytes).capture_time.unwrap();
        assert_eq!(capture.embedded_offset_minutes, Some(-300));
    }

    #[test]
    fn us4_extract_photo_metadata_capture_time_is_none_without_exif() {
        assert!(extract_photo_metadata(b"\xFF\xD8\xFF-fake-jpeg")
            .capture_time
            .is_none());
    }

    #[test]
    fn us4_extract_photo_metadata_capture_time_is_none_for_malformed_date_time_original() {
        let bytes = capture_time_bytes("not-a-date", None);
        assert!(extract_photo_metadata(&bytes).capture_time.is_none());
    }

    #[test]
    fn us4_extract_photo_metadata_ignores_a_malformed_embedded_offset() {
        // A garbage OffsetTimeOriginal must not invalidate the capture time
        // itself -- it just falls back to no embedded offset.
        let bytes = capture_time_bytes("2024:06:01 08:30:00", Some("garbage"));
        let capture = extract_photo_metadata(&bytes).capture_time.unwrap();
        assert_eq!(capture.embedded_offset_minutes, None);
    }

    #[test]
    fn us4_extract_photo_metadata_reads_both_gps_and_capture_time() {
        let bytes = geotagged_bytes_with_capture_time(45.5, 10.26, "2024:06:01 08:30:00");
        let metadata = extract_photo_metadata(&bytes);

        let gps = metadata.gps.expect("GPS must still be read");
        assert!((gps.lat - 45.5).abs() < 1e-3);
        assert!((gps.lon - 10.26).abs() < 1e-3);

        let capture = metadata
            .capture_time
            .expect("capture time must still be read");
        assert_eq!(capture.naive, datetime!(2024-06-01 08:30:00));
    }

    // ── US-5: orientation extraction ─────────────────────────────────────

    #[test]
    fn us5_extract_photo_metadata_reads_a_present_orientation() {
        let bytes = orientation_bytes(6);
        assert_eq!(extract_photo_metadata(&bytes).orientation, Some(6));
    }

    #[test]
    fn us5_extract_photo_metadata_orientation_is_none_when_absent() {
        let bytes = build_tiff(None);
        assert_eq!(extract_photo_metadata(&bytes).orientation, None);
    }

    #[test]
    fn us5_extract_photo_metadata_orientation_is_none_for_a_non_numeric_tag() {
        let bytes = malformed_orientation_bytes();
        assert_eq!(extract_photo_metadata(&bytes).orientation, None);
    }
}
