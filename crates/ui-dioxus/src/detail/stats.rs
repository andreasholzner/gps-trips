//! The trip's own numbers at the top of the detail screen (US-7/US-8).

use dioxus::prelude::*;
// The screen is `TripDetail`; so is the shape it shows.
use trip_archive_types::TripDetail as Trip;

use crate::format;

/// The trip's name and its stats — every one of them computed at import and
/// never entered by hand (US-8); this screen only reports them.
///
/// The day the trip started follows the name, muted (US-62) — unless the name
/// already leads with it, which nearly every one does. Only the date: which
/// day it was is what that line is for, and times are read where they
/// explain something, under the chart and the photos.
///
/// Each pair sits in a `div` of its own, which a `dl` allows, so the list can
/// lay out as a grid of label-over-value pairs (`app.css`).
#[component]
pub fn TripStats(trip: Trip) -> Element {
    let date = trip
        .start_date
        .as_deref()
        .filter(|_| !leads_with_a_date(&trip.name))
        .map(format::day);

    rsx! {
        hgroup {
            h1 { id: "trip-name", "{trip.name}" }
            if let Some(date) = date {
                p { id: "trip-date", "{date}" }
            }
        }
        dl { class: "stats",
            div {
                dt { "Activity" }
                dd { id: "trip-activity", "{trip.activity_type.label()}" }
            }
            div {
                dt { "Distance" }
                dd { {format::km(trip.distance_m)} }
            }
            div {
                dt { "Ascent" }
                dd { {format::metres(trip.ascent_m)} }
            }
            div {
                dt { "Descent" }
                dd { {format::metres(trip.descent_m)} }
            }
            div {
                dt { "Duration" }
                dd { {format::duration(trip.duration_secs)} }
            }
        }
    }
}

/// Whether a name starts with a `YYYY-MM-DD` date — the prefix US-12
/// suggests, and the shape nearly every name in the archive has.
fn leads_with_a_date(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 10
        && bytes[..10].iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        })
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{a_trip, render};
    use trip_archive_types::ActivityType;

    // US-7: the trip's own numbers, around the map and the gallery that
    // follow in later phases. US-8 computed them at import; the screen
    // reports them.
    #[test]
    fn the_screen_shows_the_trips_name_and_computed_stats() {
        let trip = a_trip("Oslo Hills Walk");

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert!(html.contains("Oslo Hills Walk"), "{html}");
        assert!(html.contains(ActivityType::Hiking.label()), "{html}");
        assert!(html.contains("12.35 km"), "{html}");
        assert!(html.contains("410 m"), "{html}");
        assert!(html.contains("395 m"), "{html}");
        assert!(html.contains("03:45:00"), "{html}");
    }

    #[test]
    fn a_trip_missing_optional_stats_shows_dashes_not_blanks() {
        let trip = Trip {
            start_time: None,
            start_date: None,
            ascent_m: None,
            descent_m: None,
            duration_secs: None,
            tz_name: None,
            ..a_trip("Bare Trip")
        };

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert!(html.contains("Bare Trip"), "{html}");
        assert!(html.contains("—"), "{html}");
    }

    // ── US-62: the numbers at a glance ───────────────────────────────────

    /// The labels of the stats list, in order.
    fn labels(html: &str) -> Vec<String> {
        html.split("<dt>")
            .skip(1)
            .map(|rest| rest.split("</dt>").next().unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn the_stats_are_the_five_measurements_and_nothing_else() {
        // The activity moves into the list; the start and the timezone
        // leave it — the zone for good, its job taken by the captions.
        let trip = a_trip("Oslo Hills Walk");

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert_eq!(
            labels(&html),
            ["Activity", "Distance", "Ascent", "Descent", "Duration"],
            "{html}"
        );
        assert!(html.contains(r#"id="trip-activity""#), "{html}");
        assert!(!html.contains("Europe/Oslo"), "{html}");
        assert!(!html.contains("09:30"), "no clock time at the top: {html}");
    }

    #[test]
    fn a_name_without_a_date_is_followed_by_the_day_the_trip_started() {
        let trip = a_trip("Oslo Hills Walk");

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert!(html.contains(r#"id="trip-date""#), "{html}");
        assert!(html.contains("11 Jul 2026"), "{html}");
    }

    #[test]
    fn a_name_that_leads_with_its_date_is_not_given_it_twice() {
        // The prefix US-12 suggests: the name already says which day it was.
        let trip = a_trip("2026-07-11 Oslo Hills Walk");

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert!(!html.contains("trip-date"), "{html}");
    }

    #[test]
    fn only_a_whole_leading_date_counts_as_one() {
        assert!(!leads_with_a_date("2026-7-11 Walk"));
        assert!(!leads_with_a_date("Walk 2026-07-11"));
        assert!(!leads_with_a_date("2026"));
        assert!(leads_with_a_date("2026-07-11"));
        assert!(leads_with_a_date("2026-07-11 Walk"));
    }

    #[test]
    fn a_trip_with_no_start_has_no_date_beside_its_name() {
        let trip = Trip {
            start_date: None,
            ..a_trip("Bare Trip")
        };

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert!(!html.contains("trip-date"), "{html}");
    }
}
