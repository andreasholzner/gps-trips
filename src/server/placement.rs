//! Decide where a photo goes on the map (US-3: EXIF GPS; US-4: timestamp
//! interpolation, read against the offsets the track itself was in — US-64):
//! a pure decision, isolated from `photos.rs`'s I/O (blob storage, DB writes)
//! so it is testable directly, without a database or temp files.

use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset};

use crate::models::LocationSource;
use crate::server::{
    gpx::{self, TimedPoint},
    location, timezone,
};

/// The per-trip context placement needs beyond one photo's own EXIF data
/// (US-4): the track's timed points, for interpolating photos without GPS,
/// the offsets in force along it, for reading a photo's wall clock (US-64),
/// and the trip's assumed timezone, still the last fallback (ADR-0009).
/// Grouped into one struct rather than growing parameter lists, matching
/// `NewPhoto`'s existing precedent.
pub struct TripPhotoContext<'a> {
    pub timed_points: &'a [TimedPoint],
    pub tz_name: Option<&'a str>,
    /// Built by [`TripPhotoContext::new`] rather than supplied, so the
    /// per-point lookups behind it happen once per trip and not once per
    /// photo — and so no caller can forget them.
    offsets: Vec<(usize, UtcOffset)>,
}

impl<'a> TripPhotoContext<'a> {
    pub fn new(timed_points: &'a [TimedPoint], tz_name: Option<&'a str>) -> Self {
        Self {
            timed_points,
            tz_name,
            offsets: timezone::offset_transitions(timed_points),
        }
    }

    /// The instant at which this track's *local* clock read `naive` (US-64).
    ///
    /// Each stretch of the track over which the offset is constant implies one
    /// candidate instant, and a candidate counts only if it falls inside the
    /// stretch that implied it — which makes the answer the point where the
    /// clock the camera was set by read what it wrote, rather than what one
    /// zone says about the whole trip. The stretches are contiguous, so a
    /// track that crosses nothing is a single stretch spanning all of it and
    /// every photo on it resolves exactly as it does today.
    ///
    /// `None` when no stretch matches: going east skips an hour of local time,
    /// and a photo written inside it was never read by this track's clock.
    /// Where two stretches match — going west repeats an hour — the first in
    /// track order wins, which is also the earlier instant, since a local hour
    /// only repeats where the offset *decreases*. That is the tie-break
    /// [`timezone::resolve_to_utc`] already makes for an ambiguous DST hour.
    fn clock_read(&self, naive: PrimitiveDateTime) -> Option<OffsetDateTime> {
        let track_end = self.timed_points.last()?.time;
        for (i, &(start, offset)) in self.offsets.iter().enumerate() {
            let stretch_start = self.timed_points[start].time;
            // Up to, but not including, the next stretch's first point — so
            // the stretches cover the track with no instant falling between
            // two of them. The last one runs to the end of the track.
            let candidate = naive.assume_offset(offset);
            let within = match self.offsets.get(i + 1) {
                Some(&(next, _)) => candidate < self.timed_points[next].time,
                None => candidate <= track_end,
            };
            if within && candidate >= stretch_start {
                return Some(candidate);
            }
        }
        None
    }
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
        .and_then(|c| capture_time_to_utc(&c, ctx))
        .and_then(|at| gpx::interpolate_position(ctx.timed_points, at))
    {
        Some((lat, lon)) => (Some(lat), Some(lon), LocationSource::Interpolated),
        None => (None, None, LocationSource::None),
    }
}

/// Resolve an EXIF capture time to UTC (ADR-0009): an embedded
/// `OffsetTimeOriginal` always wins, because a photo that carries its own
/// offset already names an instant and has nothing to guess about; otherwise
/// the track is searched for the point at which its own local clock read this
/// wall clock (US-64); otherwise — an hour the track skipped, or no track at
/// all — the trip's assumed timezone (ADR-0019), which is what the archive
/// resolved every photo by before. `None` if none of those yields an instant:
/// no trip timezone to fall back on, or a wall-clock time that falls in a
/// "spring-forward" gap of the zone itself.
///
/// An embedded offset that's syntactically valid but out of range (e.g. a
/// corrupt `OffsetTimeOriginal` like `"+99:99"`, which `kamadak-exif` parses
/// into a nonsensical minute count with no range check of its own) is treated
/// the same as no embedded offset at all, falling back to the trip timezone
/// instead of failing outright — consistent with every other malformed-EXIF
/// case in this pipeline being best-effort, not fatal.
fn capture_time_to_utc(
    capture: &location::CaptureTime,
    ctx: &TripPhotoContext<'_>,
) -> Option<OffsetDateTime> {
    if let Some(offset_minutes) = capture.embedded_offset_minutes {
        if let Ok(offset) = UtcOffset::from_whole_seconds(offset_minutes * 60) {
            return Some(capture.naive.assume_offset(offset));
        }
    }
    ctx.clock_read(capture.naive)
        .or_else(|| timezone::resolve_to_utc(ctx.tz_name?, capture.naive))
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::location::{CaptureTime, GpsPosition, PhotoMetadata};
    use time::macros::datetime;

    /// Two points in Finnmark, Norway — Europe/Oslo, and the trip that
    /// crosses nothing: US-64 must place its photos exactly where the archive
    /// places them today. Real coordinates, because placement now reads the
    /// offsets from the track itself (ADR-0012: `tzf-rs` carries its boundary
    /// data in the binary, so this needs no fixture and no network).
    const NORWAY_START: (f64, f64) = (25.0, 69.0);
    const NORWAY_END: (f64, f64) = (25.4, 69.2);
    /// Across the border in Finnish Lapland — Europe/Helsinki, an hour ahead.
    const FINLAND_START: (f64, f64) = (27.0, 68.9);
    const FINLAND_MID: (f64, f64) = (27.4, 68.8);
    const FINLAND_END: (f64, f64) = (27.8, 68.7);

    fn at(lon_lat: (f64, f64), time: OffsetDateTime) -> TimedPoint {
        TimedPoint {
            time,
            lat: lon_lat.1,
            lon: lon_lat.0,
        }
    }

    fn no_track_ctx() -> TripPhotoContext<'static> {
        TripPhotoContext::new(&[], None)
    }

    fn track_ctx(timed_points: &[TimedPoint]) -> TripPhotoContext<'_> {
        TripPhotoContext::new(timed_points, Some("Europe/Oslo"))
    }

    fn sample_track() -> [TimedPoint; 2] {
        [
            at(NORWAY_START, datetime!(2024-06-01 08:00 UTC)),
            at(NORWAY_END, datetime!(2024-06-01 10:00 UTC)),
        ]
    }

    /// The midpoint of `sample_track` — where a photo whose wall clock reads
    /// 11:00 in Europe/Oslo (+02:00 in June, so 09:00 UTC) belongs.
    const SAMPLE_MIDPOINT: (f64, f64) = (25.2, 69.1);

    fn assert_at(lat: Option<f64>, lon: Option<f64>, expected: (f64, f64)) {
        assert!(
            (lat.unwrap() - expected.1).abs() < 1e-9,
            "lat {lat:?} is not {}",
            expected.1
        );
        assert!(
            (lon.unwrap() - expected.0).abs() < 1e-9,
            "lon {lon:?} is not {}",
            expected.0
        );
    }

    fn photo_taken_at(naive: time::PrimitiveDateTime) -> PhotoMetadata {
        PhotoMetadata {
            gps: None,
            capture_time: Some(CaptureTime {
                naive,
                embedded_offset_minutes: None,
            }),
            orientation: None,
        }
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
        assert_at(lat, lon, SAMPLE_MIDPOINT);
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

    // ── US-64: the offsets the track itself was in ────────────────────────

    /// Eastward: two points in Norway (+02:00), then three in Finland
    /// (+03:00). Local noon happens twice on this track in the sense that
    /// matters — 12:00 to 13:00 local is skipped at the crossing.
    fn crossing_into_finland() -> [TimedPoint; 5] {
        [
            at(NORWAY_START, datetime!(2024-06-01 08:00 UTC)),
            at(NORWAY_END, datetime!(2024-06-01 09:00 UTC)),
            at(FINLAND_START, datetime!(2024-06-01 10:00 UTC)),
            at(FINLAND_MID, datetime!(2024-06-01 11:00 UTC)),
            at(FINLAND_END, datetime!(2024-06-01 12:00 UTC)),
        ]
    }

    #[test]
    fn a_photo_taken_before_the_crossing_is_placed_on_the_norwegian_side() {
        let timed = crossing_into_finland();
        // 10:30 by the camera's clock, which was on Norwegian time: halfway
        // between the two Norwegian points (08:00-09:00 UTC).
        let (lat, lon, source) = resolve_placement(
            photo_taken_at(datetime!(2024-06-01 10:30:00)),
            &track_ctx(&timed),
            None,
        );
        assert_eq!(source, LocationSource::Interpolated);
        assert_at(lat, lon, (25.2, 69.1));
    }

    #[test]
    fn a_photo_taken_after_the_crossing_is_placed_where_the_clock_read_it() {
        let timed = crossing_into_finland();
        // 13:30 local in Finland is 10:30 UTC — halfway between the first two
        // Finnish points. The trip's own zone (Europe/Oslo) would make it
        // 11:30 UTC and put the photo an hour further along the track.
        let (lat, lon, source) = resolve_placement(
            photo_taken_at(datetime!(2024-06-01 13:30:00)),
            &track_ctx(&timed),
            None,
        );
        assert_eq!(source, LocationSource::Interpolated);
        assert_at(lat, lon, (27.2, 68.85));
    }

    #[test]
    fn a_photo_written_in_the_hour_the_crossing_skipped_falls_back_to_the_trips_zone() {
        let timed = crossing_into_finland();
        // Going east skips 12:00-13:00 local: no stretch of this track ever
        // read 12:30, so the archive places the photo the way it does today,
        // by the trip's own zone (+02:00 -> 10:30 UTC).
        let (lat, lon, source) = resolve_placement(
            photo_taken_at(datetime!(2024-06-01 12:30:00)),
            &track_ctx(&timed),
            None,
        );
        assert_eq!(source, LocationSource::Interpolated);
        assert_at(lat, lon, (27.2, 68.85));
    }

    #[test]
    fn a_photo_in_an_hour_the_crossing_repeated_takes_the_earlier_one() {
        // Westward: Finland (+03:00) into Norway (+02:00), which makes the
        // local hour 12:00-13:00 happen twice.
        let timed = [
            at(FINLAND_START, datetime!(2024-06-01 08:00 UTC)),
            at(FINLAND_MID, datetime!(2024-06-01 09:00 UTC)),
            at(NORWAY_END, datetime!(2024-06-01 10:00 UTC)),
            at(NORWAY_START, datetime!(2024-06-01 11:00 UTC)),
        ];
        // 12:30 is both 09:30 UTC (+03:00, still in Finland) and 10:30 UTC
        // (+02:00, already in Norway); the earlier wins, the same tie-break
        // `timezone::resolve_to_utc` makes for an ambiguous DST hour.
        let (lat, lon, source) = resolve_placement(
            photo_taken_at(datetime!(2024-06-01 12:30:00)),
            &track_ctx(&timed),
            None,
        );
        assert_eq!(source, LocationSource::Interpolated);
        assert_at(lat, lon, (26.4, 69.0));
    }

    #[test]
    fn a_photo_in_the_hour_a_dst_change_repeated_takes_the_earlier_reading() {
        // Europe/Oslo falls back at 03:00 local on 2024-10-27 (01:00 UTC), so
        // a trip sitting out that night is two stretches: +02:00, then +01:00.
        let timed = [
            at(NORWAY_START, datetime!(2024-10-27 00:00 UTC)),
            at(NORWAY_END, datetime!(2024-10-27 02:00 UTC)),
        ];
        // 02:30 by the camera's clock happened twice. The earlier reading
        // wins — 00:30 UTC, a quarter of the way along — which is what the
        // trip's own zone resolves it to as well (`resolve_to_utc` takes the
        // first solution), so this trip is placed exactly as it is today.
        let (lat, lon, source) = resolve_placement(
            photo_taken_at(datetime!(2024-10-27 02:30:00)),
            &track_ctx(&timed),
            None,
        );
        assert_eq!(source, LocationSource::Interpolated);
        assert_at(lat, lon, (25.1, 69.05));
    }

    #[test]
    fn a_photo_with_its_own_offset_is_not_searched_for() {
        let timed = crossing_into_finland();
        // An embedded OffsetTimeOriginal names an instant outright: 13:30
        // at +03:00 is 10:30 UTC, whatever the track was doing.
        let metadata = PhotoMetadata {
            gps: None,
            capture_time: Some(CaptureTime {
                naive: datetime!(2024-06-01 13:30:00),
                embedded_offset_minutes: Some(180),
            }),
            orientation: None,
        };
        let (lat, lon, source) = resolve_placement(metadata, &track_ctx(&timed), None);
        assert_eq!(source, LocationSource::Interpolated);
        assert_at(lat, lon, (27.2, 68.85));
    }

    #[test]
    fn a_photo_on_a_trip_with_no_timezone_of_its_own_is_still_placed_by_the_track() {
        let timed = crossing_into_finland();
        let ctx = TripPhotoContext::new(&timed, None);
        let (lat, lon, source) =
            resolve_placement(photo_taken_at(datetime!(2024-06-01 13:30:00)), &ctx, None);
        assert_eq!(source, LocationSource::Interpolated);
        assert_at(lat, lon, (27.2, 68.85));
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
        let ctx = TripPhotoContext::new(&[], Some("Europe/Oslo"));
        let utc = capture_time_to_utc(&capture, &ctx)
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
        assert!(capture_time_to_utc(&capture, &no_track_ctx()).is_none());
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
        assert_at(lat, lon, SAMPLE_MIDPOINT);
    }
}
