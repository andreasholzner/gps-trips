//! Timezone lookup for photo-timestamp interpolation (US-4, ADR-0019). Sole
//! owner of the `tzf-rs`/`time-tz` dependency — the rest of the codebase only
//! sees this module's narrow coordinate -> timezone -> UTC-offset surface.

use std::sync::OnceLock;

use time::{OffsetDateTime, PrimitiveDateTime};
use time_tz::PrimitiveDateTimeExt;

use crate::server::gpx::{TimedPoint, TrackPoint};

static FINDER: OnceLock<tzf_rs::DefaultFinder> = OnceLock::new();

fn finder() -> &'static tzf_rs::DefaultFinder {
    FINDER.get_or_init(tzf_rs::DefaultFinder::new)
}

/// Guess a trip's IANA timezone from its start coordinate (US-4). Always
/// returns a *recognized* name: `tzf-rs` (the geo lookup) and `time-tz` (the
/// tzdata `resolve_to_utc` resolves against) are independently-versioned
/// datasets, so if the geo lookup ever returns a name our own tzdata doesn't
/// recognize, this falls back to `"UTC"` rather than silently storing an
/// unresolvable name that would permanently break interpolation for the trip.
pub fn guess_timezone(lon: f64, lat: f64) -> String {
    validated_or_utc(finder().get_tz_name(lon, lat).to_string())
}

/// As `guess_timezone`, but takes the trip's start coordinate from the first
/// of a slice of `TrackPoint`s — `"UTC"` if the slice is empty (defensive
/// only; `gpx::parse_gpx` guarantees at least one point in practice).
pub fn guess_timezone_from_track(points: &[TrackPoint]) -> String {
    match points.first() {
        Some(p) => guess_timezone(p.lon, p.lat),
        None => "UTC".to_string(),
    }
}

/// As `guess_timezone_from_track`, but from a slice of `TimedPoint`s (used
/// where the caller already has the track's parsed GeoJSON rather than raw
/// GPX points — see `geojson::parse_timed_points`).
pub fn guess_timezone_from_timed_points(points: &[TimedPoint]) -> String {
    match points.first() {
        Some(p) => guess_timezone(p.lon, p.lat),
        None => "UTC".to_string(),
    }
}

fn validated_or_utc(name: String) -> String {
    if is_known_timezone(&name) {
        name
    } else {
        "UTC".to_string()
    }
}

/// `true` if `name` is a timezone `time-tz`'s bundled IANA database
/// recognizes — used to validate the owner's optional override at import time.
pub fn is_known_timezone(name: &str) -> bool {
    time_tz::timezones::get_by_name(name).is_some()
}

/// Resolve a wall-clock capture time to UTC using the named timezone,
/// DST-aware. `None` if `tz_name` is unrecognized, or the wall-clock time
/// falls in a DST "spring-forward" gap with no valid interpretation. An
/// ambiguous "fall-back" instant deterministically resolves to the first
/// (pre-transition) offset.
pub fn resolve_to_utc(tz_name: &str, naive: PrimitiveDateTime) -> Option<OffsetDateTime> {
    let tz = time_tz::timezones::get_by_name(tz_name)?;
    naive.assume_timezone(tz).take_first()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn guess_timezone_resolves_oslo() {
        assert_eq!(guess_timezone(10.7522, 59.9139), "Europe/Oslo");
    }

    #[test]
    fn guess_timezone_resolves_cape_town() {
        // South Africa has no DST; the IANA database groups it under Johannesburg.
        assert_eq!(guess_timezone(18.4241, -33.9249), "Africa/Johannesburg");
    }

    #[test]
    fn is_known_timezone_accepts_a_real_iana_name() {
        assert!(is_known_timezone("Europe/Oslo"));
    }

    #[test]
    fn is_known_timezone_rejects_an_unrecognized_name() {
        assert!(!is_known_timezone("Not/A_Zone"));
    }

    #[test]
    fn resolve_to_utc_is_dst_aware_for_the_same_zone() {
        let winter = datetime!(2024-01-15 12:00);
        let summer = datetime!(2024-07-15 12:00);

        let winter_utc = resolve_to_utc("Europe/Oslo", winter).unwrap();
        let summer_utc = resolve_to_utc("Europe/Oslo", summer).unwrap();

        assert_eq!(winter_utc.offset().whole_hours(), 1);
        assert_eq!(summer_utc.offset().whole_hours(), 2);
    }

    #[test]
    fn resolve_to_utc_returns_none_for_an_unrecognized_zone() {
        let at = datetime!(2024-01-15 12:00);
        assert!(resolve_to_utc("Not/A_Zone", at).is_none());
    }

    // ── Code review fix: a geo-lookup result our own tzdata doesn't
    // recognize falls back to "UTC" instead of being stored unchecked ────

    #[test]
    fn validated_or_utc_keeps_a_recognized_name() {
        assert_eq!(validated_or_utc("Europe/Oslo".to_string()), "Europe/Oslo");
    }

    #[test]
    fn validated_or_utc_falls_back_for_an_unrecognized_name() {
        assert_eq!(validated_or_utc("Not/A_Zone".to_string()), "UTC");
    }

    #[test]
    fn guess_timezone_from_track_uses_the_first_points_coordinates() {
        let points = [TrackPoint {
            lat: 59.9139,
            lon: 10.7522,
            ele: None,
            time: None,
        }];
        assert_eq!(guess_timezone_from_track(&points), "Europe/Oslo");
    }

    #[test]
    fn guess_timezone_from_track_falls_back_to_utc_for_an_empty_track() {
        assert_eq!(guess_timezone_from_track(&[]), "UTC");
    }

    #[test]
    fn guess_timezone_from_timed_points_uses_the_first_points_coordinates() {
        let points = [TimedPoint {
            time: datetime!(2024-06-01 08:00 UTC),
            lat: 59.9139,
            lon: 10.7522,
        }];
        assert_eq!(guess_timezone_from_timed_points(&points), "Europe/Oslo");
    }

    #[test]
    fn guess_timezone_from_timed_points_falls_back_to_utc_when_empty() {
        assert_eq!(guess_timezone_from_timed_points(&[]), "UTC");
    }
}
