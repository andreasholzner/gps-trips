//! Decide where a photo goes on the map (US-3: EXIF GPS; US-4: timestamp
//! interpolation): a pure decision, isolated from `photos.rs`'s I/O (blob
//! storage, DB writes) so it is testable directly, without a database or
//! temp files.

use time::OffsetDateTime;

use crate::models::LocationSource;
use crate::server::{
    gpx::{self, TimedPoint},
    location, timezone,
};

/// The per-trip context placement needs beyond one photo's own EXIF data
/// (US-4): the track's timed points, for interpolating photos without GPS,
/// and the trip's assumed timezone, for resolving EXIF capture time to UTC
/// (ADR-0009). Grouped into one struct rather than growing parameter lists,
/// matching `NewPhoto`'s existing precedent.
pub struct TripPhotoContext<'a> {
    pub timed_points: &'a [TimedPoint],
    pub tz_name: Option<&'a str>,
}

/// Decide a photo's map position from its EXIF metadata and the trip's
/// context. `known_location` (US-22: a location an external source, e.g.
/// Komoot, already supplied for this photo) wins over everything when
/// present; otherwise EXIF GPS wins (US-3); otherwise a capture time
/// resolved to a UTC instant within the track's range gives an interpolated
/// position (US-4); anything else is `location_source = none`. Pure — no I/O;
/// callers decide what (if anything) to log about a `none` outcome.
pub fn resolve_placement(
    metadata: location::PhotoMetadata,
    ctx: &TripPhotoContext<'_>,
    known_location: Option<(f64, f64)>,
) -> (Option<f64>, Option<f64>, LocationSource) {
    if let Some((lat, lon)) = known_location {
        return (Some(lat), Some(lon), LocationSource::Provided);
    }
    if let Some(pos) = metadata.gps {
        return (Some(pos.lat), Some(pos.lon), LocationSource::Exif);
    }
    match metadata
        .capture_time
        .and_then(|c| capture_time_to_utc(&c, ctx.tz_name))
        .and_then(|at| gpx::interpolate_position(ctx.timed_points, at))
    {
        Some((lat, lon)) => (Some(lat), Some(lon), LocationSource::Interpolated),
        None => (None, None, LocationSource::None),
    }
}

/// Resolve an EXIF capture time to UTC (ADR-0009): an embedded
/// `OffsetTimeOriginal` always wins; otherwise fall back to the trip's
/// assumed timezone (ADR-0019). `None` if there's no trip timezone to fall
/// back on, or the DST-aware resolution itself fails (e.g. a wall-clock time
/// that falls in a "spring-forward" gap).
///
/// An embedded offset that's syntactically valid but out of range (e.g. a
/// corrupt `OffsetTimeOriginal` like `"+99:99"`, which `kamadak-exif` parses
/// into a nonsensical minute count with no range check of its own) is treated
/// the same as no embedded offset at all, falling back to the trip timezone
/// instead of failing outright — consistent with every other malformed-EXIF
/// case in this pipeline being best-effort, not fatal.
fn capture_time_to_utc(
    capture: &location::CaptureTime,
    tz_name: Option<&str>,
) -> Option<OffsetDateTime> {
    if let Some(offset_minutes) = capture.embedded_offset_minutes {
        if let Ok(offset) = time::UtcOffset::from_whole_seconds(offset_minutes * 60) {
            return Some(capture.naive.assume_offset(offset));
        }
    }
    timezone::resolve_to_utc(tz_name?, capture.naive)
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::location::{CaptureTime, GpsPosition, PhotoMetadata};
    use time::macros::datetime;

    fn no_track_ctx() -> TripPhotoContext<'static> {
        TripPhotoContext {
            timed_points: &[],
            tz_name: None,
        }
    }

    fn track_ctx(timed_points: &[TimedPoint]) -> TripPhotoContext<'_> {
        TripPhotoContext {
            timed_points,
            tz_name: Some("Europe/Oslo"),
        }
    }

    fn sample_track() -> [TimedPoint; 2] {
        [
            TimedPoint {
                time: datetime!(2024-06-01 08:00 UTC),
                lat: 0.0,
                lon: 0.0,
            },
            TimedPoint {
                time: datetime!(2024-06-01 10:00 UTC),
                lat: 10.0,
                lon: 20.0,
            },
        ]
    }

    // ── US-3: EXIF GPS ────────────────────────────────────────────────────

    #[test]
    fn resolve_placement_uses_exif_gps_when_present() {
        let metadata = PhotoMetadata {
            gps: Some(GpsPosition {
                lat: 45.5,
                lon: 10.26,
            }),
            capture_time: None,
            orientation: None,
        };
        let (lat, lon, source) = resolve_placement(metadata, &no_track_ctx(), None);
        assert_eq!(source, LocationSource::Exif);
        assert_eq!(lat, Some(45.5));
        assert_eq!(lon, Some(10.26));
    }

    #[test]
    fn resolve_placement_is_none_with_no_gps_and_no_capture_time() {
        let (lat, lon, source) = resolve_placement(PhotoMetadata::default(), &no_track_ctx(), None);
        assert_eq!(source, LocationSource::None);
        assert_eq!(lat, None);
        assert_eq!(lon, None);
    }

    // ── US-22: a known location supplied by an external source (Komoot) ───

    #[test]
    fn resolve_placement_uses_known_location_when_present() {
        let metadata = PhotoMetadata {
            gps: Some(GpsPosition {
                lat: 45.5,
                lon: 10.26,
            }),
            capture_time: None,
            orientation: None,
        };
        let (lat, lon, source) = resolve_placement(metadata, &no_track_ctx(), Some((69.7, 18.9)));
        assert_eq!(source, LocationSource::Provided);
        assert_eq!(lat, Some(69.7));
        assert_eq!(lon, Some(18.9));
    }

    #[test]
    fn resolve_placement_falls_back_to_exif_when_known_location_is_none() {
        let metadata = PhotoMetadata {
            gps: Some(GpsPosition {
                lat: 45.5,
                lon: 10.26,
            }),
            capture_time: None,
            orientation: None,
        };
        let (lat, lon, source) = resolve_placement(metadata, &no_track_ctx(), None);
        assert_eq!(source, LocationSource::Exif);
        assert_eq!(lat, Some(45.5));
        assert_eq!(lon, Some(10.26));
    }

    // ── US-4: timestamp interpolation ─────────────────────────────────────

    #[test]
    fn resolve_placement_interpolates_a_capture_time_within_the_track_range() {
        let timed = sample_track();
        // 11:00 local ("Europe/Oslo", +02:00 in June) == 09:00 UTC, the
        // midpoint of the track's 08:00-10:00 UTC range.
        let metadata = PhotoMetadata {
            gps: None,
            capture_time: Some(CaptureTime {
                naive: datetime!(2024-06-01 11:00:00),
                embedded_offset_minutes: None,
            }),
            orientation: None,
        };
        let (lat, lon, source) = resolve_placement(metadata, &track_ctx(&timed), None);
        assert_eq!(source, LocationSource::Interpolated);
        assert!((lat.unwrap() - 5.0).abs() < 1e-9);
        assert!((lon.unwrap() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn resolve_placement_is_none_for_a_capture_time_outside_the_track_range() {
        let timed = sample_track();
        // 23:00 local == 21:00 UTC, well outside the 08:00-10:00 range.
        let metadata = PhotoMetadata {
            gps: None,
            capture_time: Some(CaptureTime {
                naive: datetime!(2024-06-01 23:00:00),
                embedded_offset_minutes: None,
            }),
            orientation: None,
        };
        let (lat, lon, source) = resolve_placement(metadata, &track_ctx(&timed), None);
        assert_eq!(source, LocationSource::None);
        assert_eq!(lat, None);
        assert_eq!(lon, None);
    }

    #[test]
    fn resolve_placement_is_none_when_capture_time_present_but_no_track_points() {
        let metadata = PhotoMetadata {
            gps: None,
            capture_time: Some(CaptureTime {
                naive: datetime!(2024-06-01 09:00:00),
                embedded_offset_minutes: None,
            }),
            orientation: None,
        };
        let (_, _, source) = resolve_placement(metadata, &no_track_ctx(), None);
        assert_eq!(source, LocationSource::None);
    }

    #[test]
    fn resolve_placement_prefers_exif_gps_over_interpolation_even_within_range() {
        let timed = sample_track();
        let metadata = PhotoMetadata {
            gps: Some(GpsPosition {
                lat: 45.5,
                lon: 10.26,
            }),
            capture_time: Some(CaptureTime {
                naive: datetime!(2024-06-01 11:00:00), // in-range too
                embedded_offset_minutes: None,
            }),
            orientation: None,
        };
        let (lat, lon, source) = resolve_placement(metadata, &track_ctx(&timed), None);
        assert_eq!(source, LocationSource::Exif);
        assert_eq!(lat, Some(45.5));
        assert_eq!(lon, Some(10.26));
    }

    // ── Code review fix: an out-of-range embedded offset falls back to the
    // trip timezone instead of failing outright ──────────────────────────

    #[test]
    fn capture_time_to_utc_falls_back_to_trip_timezone_when_embedded_offset_is_out_of_range() {
        // "+99:99" parses successfully (kamadak-exif applies no range check)
        // into an offset of 99*60+99 = 6039 minutes, which is out of range
        // once converted to seconds.
        let capture = CaptureTime {
            naive: datetime!(2024-06-01 09:00:00),
            embedded_offset_minutes: Some(99 * 60 + 99),
        };
        let utc = capture_time_to_utc(&capture, Some("Europe/Oslo"))
            .expect("must fall back to the trip timezone, not fail outright");
        // Europe/Oslo is +02:00 in June, so 09:00 local -> 07:00 UTC.
        assert_eq!(utc, datetime!(2024-06-01 07:00:00 UTC));
    }

    #[test]
    fn capture_time_to_utc_returns_none_for_an_out_of_range_offset_with_no_trip_timezone() {
        let capture = CaptureTime {
            naive: datetime!(2024-06-01 09:00:00),
            embedded_offset_minutes: Some(99 * 60 + 99),
        };
        assert!(capture_time_to_utc(&capture, None).is_none());
    }

    #[test]
    fn resolve_placement_interpolates_when_embedded_offset_is_out_of_range() {
        let timed = sample_track();
        // Garbage OffsetTimeOriginal -> falls back to the trip's
        // "Europe/Oslo" timezone (+02:00 in June): 11:00 local -> 09:00 UTC,
        // the midpoint.
        let metadata = PhotoMetadata {
            gps: None,
            capture_time: Some(CaptureTime {
                naive: datetime!(2024-06-01 11:00:00),
                embedded_offset_minutes: Some(99 * 60 + 99),
            }),
            orientation: None,
        };
        let (lat, lon, source) = resolve_placement(metadata, &track_ctx(&timed), None);
        assert_eq!(source, LocationSource::Interpolated);
        assert!((lat.unwrap() - 5.0).abs() < 1e-9);
        assert!((lon.unwrap() - 10.0).abs() < 1e-9);
    }
}
