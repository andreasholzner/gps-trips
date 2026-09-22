//! Timezone lookup for photo-timestamp interpolation (US-4, ADR-0019). Sole
//! owner of the `tzf-rs`/`time-tz` dependency — the rest of the codebase only
//! sees this module's narrow coordinate -> timezone -> UTC-offset surface.

use std::sync::OnceLock;

use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset};
use time_tz::{OffsetDateTimeExt, PrimitiveDateTimeExt};

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

/// The calendar date an instant falls on in `tz_name` — the date the owner
/// would call that day, rather than the one UTC happens to be on.
///
/// Timestamps are stored and computed in UTC (ADR-0009), which is right for
/// ordering and arithmetic and wrong for naming: a ride that starts at half
/// past midnight in Oslo began the previous day in UTC, and an evening ride
/// west of Greenwich began the next one. `None` for a timezone this build's
/// tzdata does not recognize, which `guess_timezone` already prevents by
/// falling back to `"UTC"`.
pub fn local_date(tz_name: &str, instant: OffsetDateTime) -> Option<time::Date> {
    let tz = time_tz::timezones::get_by_name(tz_name)?;
    Some(instant.to_timezone(tz).date())
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

/// The UTC offsets in force along a track: the first point's offset and every
/// index at which it changes (US-64). A track that stays in one zone and
/// crosses no DST change is a single entry — which is every trip but the ones
/// this exists for.
///
/// Each point's zone comes from its own coordinate (`tzf-rs`) and its offset
/// from that zone at that instant (`time-tz`, so a DST change mid-track shows
/// up as much as a border crossing does). A point whose zone this build's
/// tzdata does not recognize keeps the offset already in force rather than
/// inventing a transition; when it is the track's first point there is nothing
/// in force yet, so the entry is `UTC` — the same fallback `guess_timezone`
/// makes. Empty for an empty track.
pub fn offset_transitions(points: &[TimedPoint]) -> Vec<(usize, UtcOffset)> {
    in_force(offset_changes(points.iter().copied().enumerate()))
}

/// Where the UTC offset changes along a track, in the order and under the
/// indices the points are given in (US-62): the served track numbers every
/// point, timed or not, and the readout looks a hovered index up in this.
///
/// Unlike [`offset_transitions`] an unresolvable zone is not papered over: it
/// is `None`, starting where it starts, so the readout can say its time is
/// UTC rather than label it with an offset it was never in.
pub fn offset_changes(
    points: impl IntoIterator<Item = (usize, TimedPoint)>,
) -> Vec<(usize, Option<UtcOffset>)> {
    changes_by(points, offset_at)
}

/// [`offset_changes`] with the lookup supplied, so the unresolvable case can
/// be tested — every real coordinate resolves.
fn changes_by(
    points: impl IntoIterator<Item = (usize, TimedPoint)>,
    lookup: impl Fn(f64, f64, OffsetDateTime) -> Option<UtcOffset>,
) -> Vec<(usize, Option<UtcOffset>)> {
    let mut changes: Vec<(usize, Option<UtcOffset>)> = Vec::new();
    for (index, point) in points {
        let offset = lookup(point.lon, point.lat, point.time);
        if changes.last().map(|&(_, current)| current) != Some(offset) {
            changes.push((index, offset));
        }
    }
    changes
}

/// The changes reduced to the offsets placement reads a wall clock against:
/// an unresolvable stretch keeps the offset already in force, and one at the
/// very start is `UTC`, the fallback `guess_timezone` makes.
fn in_force(changes: Vec<(usize, Option<UtcOffset>)>) -> Vec<(usize, UtcOffset)> {
    let mut transitions: Vec<(usize, UtcOffset)> = Vec::new();
    for (index, offset) in changes {
        let offset = match (offset, transitions.last()) {
            (Some(offset), _) => offset,
            (None, None) => UtcOffset::UTC,
            (None, Some(_)) => continue,
        };
        if transitions.last().map(|&(_, current)| current) != Some(offset) {
            transitions.push((index, offset));
        }
    }
    transitions
}

/// The UTC offset a photo taken at `at` is captioned in (US-62): the zone of
/// its own position where it has one, the trip's zone where it does not — the
/// same fallback placement makes for such a photo (US-4). `None` if the one
/// that applies does not resolve, and the caption then says UTC.
pub fn photo_offset(
    position: Option<(f64, f64)>,
    tz_name: Option<&str>,
    at: OffsetDateTime,
) -> Option<UtcOffset> {
    match position {
        Some((lat, lon)) => offset_at(lon, lat, at),
        None => {
            let tz = time_tz::timezones::get_by_name(tz_name?)?;
            Some(at.to_timezone(tz).offset())
        }
    }
}

/// The UTC offset in force at a coordinate at an instant — the geography
/// lookup and the DST lookup, composed. `None` if this build's tzdata does not
/// recognize the name the geo lookup returned.
fn offset_at(lon: f64, lat: f64, at: OffsetDateTime) -> Option<UtcOffset> {
    let tz = time_tz::timezones::get_by_name(finder().get_tz_name(lon, lat))?;
    Some(at.to_timezone(tz).offset())
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    /// Karasjok, Norway — Europe/Oslo, and about 50 km from the Finnish
    /// border the ski tour US-64 exists for crosses.
    const KARASJOK: (f64, f64) = (25.5140, 69.4720);
    /// Inari, Finland — Europe/Helsinki, an hour ahead of Karasjok.
    const INARI: (f64, f64) = (27.0290, 68.9060);

    fn at(lon_lat: (f64, f64), time: OffsetDateTime) -> TimedPoint {
        TimedPoint {
            time,
            lat: lon_lat.1,
            lon: lon_lat.0,
        }
    }

    #[test]
    fn local_date_is_the_date_where_the_track_is_not_where_utc_is() {
        // US-12: the suggested name leads with this date, so getting it from
        // UTC would offer the owner the wrong day for anything near midnight.
        assert_eq!(
            local_date("Europe/Oslo", datetime!(2024-06-01 22:30 UTC)),
            Some(time::macros::date!(2024 - 06 - 02)),
            "half past midnight in Oslo is still the 1st in UTC"
        );
        assert_eq!(
            local_date("America/New_York", datetime!(2024-06-02 01:30 UTC)),
            Some(time::macros::date!(2024 - 06 - 01)),
            "an evening ride in New York is already tomorrow in UTC"
        );
        // A same-day instant is unaffected, which is why the fixtures never
        // caught this.
        assert_eq!(
            local_date("Europe/Oslo", datetime!(2024-06-01 08:00 UTC)),
            Some(time::macros::date!(2024 - 06 - 01))
        );
    }

    #[test]
    fn local_date_declines_a_timezone_this_build_does_not_know() {
        assert_eq!(
            local_date("Not/A_Zone", datetime!(2024-06-01 08:00 UTC)),
            None
        );
    }

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

    // ── US-64: the offsets in force along a track ─────────────────────────

    #[test]
    fn offset_transitions_is_a_single_entry_for_a_track_inside_one_zone() {
        // The trip this changes nothing for: one offset over the whole track.
        let points = [
            at(KARASJOK, datetime!(2024-06-01 08:00 UTC)),
            at(KARASJOK, datetime!(2024-06-01 09:00 UTC)),
            at(KARASJOK, datetime!(2024-06-01 10:00 UTC)),
        ];
        assert_eq!(
            offset_transitions(&points),
            vec![(0, UtcOffset::from_hms(2, 0, 0).unwrap())],
            "Europe/Oslo is +02:00 in June, and nothing about the track changes it"
        );
    }

    #[test]
    fn offset_transitions_marks_the_index_where_the_track_crosses_a_border() {
        // The ski tour from Finnmark into Finnish Lapland: +02:00 becomes
        // +03:00 at the point the track is first on the Finnish side.
        let points = [
            at(KARASJOK, datetime!(2024-06-01 08:00 UTC)),
            at(KARASJOK, datetime!(2024-06-01 09:00 UTC)),
            at(INARI, datetime!(2024-06-01 10:00 UTC)),
            at(INARI, datetime!(2024-06-01 11:00 UTC)),
        ];
        assert_eq!(
            offset_transitions(&points),
            vec![
                (0, UtcOffset::from_hms(2, 0, 0).unwrap()),
                (2, UtcOffset::from_hms(3, 0, 0).unwrap()),
            ]
        );
    }

    #[test]
    fn offset_transitions_marks_a_dst_change_within_one_zone() {
        // Europe/Oslo falls back at 03:00 local on 2024-10-27, i.e. 01:00 UTC:
        // a multi-day trip sitting still across that night changes offset too.
        let points = [
            at(KARASJOK, datetime!(2024-10-27 00:30 UTC)),
            at(KARASJOK, datetime!(2024-10-27 01:30 UTC)),
        ];
        assert_eq!(
            offset_transitions(&points),
            vec![
                (0, UtcOffset::from_hms(2, 0, 0).unwrap()),
                (1, UtcOffset::from_hms(1, 0, 0).unwrap()),
            ]
        );
    }

    #[test]
    fn offset_transitions_is_empty_for_a_track_with_no_timed_points() {
        assert_eq!(offset_transitions(&[]), vec![]);
    }

    // ── US-62: the offsets in force along a track, as it is served ─────────

    #[test]
    fn a_photo_is_captioned_in_the_zone_of_its_own_position() {
        // Taken in Inari on a trip whose zone is Oslo's: Helsinki's +03:00.
        let at = datetime!(2024-06-01 10:00 UTC);
        assert_eq!(
            photo_offset(Some((INARI.1, INARI.0)), Some("Europe/Oslo"), at),
            Some(UtcOffset::from_hms(3, 0, 0).unwrap())
        );
    }

    #[test]
    fn a_photo_without_a_position_is_captioned_in_the_trips_zone() {
        let at = datetime!(2024-01-15 10:00 UTC);
        assert_eq!(
            photo_offset(None, Some("Europe/Oslo"), at),
            Some(UtcOffset::from_hms(1, 0, 0).unwrap()),
            "Oslo in January, DST-aware"
        );
    }

    #[test]
    fn a_photo_with_nothing_to_resolve_its_zone_by_has_no_offset() {
        let at = datetime!(2024-06-01 10:00 UTC);
        assert_eq!(photo_offset(None, None, at), None);
        assert_eq!(photo_offset(None, Some("Not/A_Zone"), at), None);
    }

    fn hours(h: i8) -> UtcOffset {
        UtcOffset::from_hms(h, 0, 0).unwrap()
    }

    #[test]
    fn offset_changes_keep_the_index_each_point_was_given() {
        // The served track numbers every point, timed or not — the chart's
        // own index — so a change is reported at the index it came in with,
        // not at its place among the timed points.
        let points = [
            (0, at(KARASJOK, datetime!(2024-06-01 08:00 UTC))),
            (3, at(INARI, datetime!(2024-06-01 10:00 UTC))),
        ];
        assert_eq!(
            offset_changes(points),
            vec![(0, Some(hours(2))), (3, Some(hours(3)))]
        );
    }

    #[test]
    fn offset_changes_report_a_zone_this_build_cannot_resolve_as_none() {
        // US-62: an unresolvable zone is said to be one, so the readout can
        // label its time UTC rather than borrow the offset before it.
        let lookup = |lon: f64, _: f64, _: OffsetDateTime| (lon < 26.0).then(|| hours(2));
        let points = [
            (0, at(KARASJOK, datetime!(2024-06-01 08:00 UTC))),
            (1, at(INARI, datetime!(2024-06-01 09:00 UTC))),
            (2, at(KARASJOK, datetime!(2024-06-01 10:00 UTC))),
        ];
        assert_eq!(
            changes_by(points, lookup),
            vec![(0, Some(hours(2))), (1, None), (2, Some(hours(2)))]
        );
    }

    #[test]
    fn in_force_keeps_the_offset_across_an_unresolvable_stretch() {
        // US-64's placement reads a wall clock against these, and an unknown
        // zone mid-track is no reason to invent a transition.
        assert_eq!(
            in_force(vec![(0, Some(hours(2))), (1, None), (2, Some(hours(2)))]),
            vec![(0, hours(2))]
        );
    }

    #[test]
    fn in_force_starts_at_utc_when_the_first_zone_is_unresolvable() {
        assert_eq!(
            in_force(vec![(0, None), (4, Some(hours(3)))]),
            vec![(0, UtcOffset::UTC), (4, hours(3))]
        );
    }
}
