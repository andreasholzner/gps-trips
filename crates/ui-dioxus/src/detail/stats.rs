//! The trip's own numbers at the top of the detail screen (US-7/US-8).

use dioxus::prelude::*;
// The screen is `TripDetail`; so is the shape it shows.
use trip_archive_types::{ActivityType, TripDetail as Trip, TripKind};

use crate::activity_color;
use crate::activity_icon::ActivityIcon;
use crate::kinds::Kind;
use crate::list_table::KindSwatch;
use crate::{format, rates};

/// The trip's name and its stats — every one of them computed at import and
/// never entered by hand (US-8); this screen only reports them.
///
/// The day the trip started follows the name, muted (US-62) — unless the name
/// already leads with it, which nearly every one does. Only the date: which
/// day it was is what that line is for, and times are read where they
/// explain something, under the chart and the photos.
///
/// The heading names the trip's kind (US-108): its line style before the
/// name, as the list shows it (US-90), and spelled out beneath it.
///
/// Each pair sits in a `div` of its own, which a `dl` allows, so the list can
/// lay out as a grid of label-over-value pairs (`app.css`).
#[component]
pub fn TripStats(trip: Trip) -> Element {
    let kind_name = kind_name(trip.trip_kind);
    let date = trip
        .start_date
        .as_deref()
        .filter(|_| !leads_with_a_date(&trip.name))
        .map(format::day);
    // In motion only (US-80); a trip without times has none, and the entry
    // is left out rather than shown as a dash.
    let average = trip
        .moving_distance_m
        .zip(trip.moving_secs)
        .and_then(|(metres, secs)| rates::average_kmh(metres, secs as f64));
    // On the climbs alone (US-81); a trip with no climb, or no times, has
    // none, and the entry is left out as the average speed's is.
    let climbing = trip
        .climb_gain_m
        .zip(trip.climb_secs)
        .and_then(|(gain_m, secs)| rates::climbing_rate(gain_m, secs as f64));

    rsx! {
        hgroup {
            h1 { id: "trip-name",
                // An unspecified activity has nothing to show in front.
                if trip.activity_type != ActivityType::Unknown {
                    ActivityIcon { activity: trip.activity_type }
                }
                KindSwatch { kind: Kind::of_trip(trip.trip_kind), color: activity_color::color(trip.activity_type), name: kind_name }
                "{trip.name}"
            }
            // The kind and the date share one line, saving a line's height.
            p { id: "trip-kind-line",
                span { id: "trip-kind", "{kind_name}" }
                if let Some(date) = date {
                    " · "
                    span { id: "trip-date", "{date}" }
                }
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
            if let Some(kmh) = average {
                div {
                    dt { "Average speed" }
                    dd { {format::speed(Some(kmh))} }
                }
            }
            div {
                dt { "Ascent" }
                dd { {format::metres(trip.ascent_m)} }
            }
            if let Some(rate) = climbing {
                div {
                    dt { "Climbing rate" }
                    dd { {format::climbing_rate(Some(rate))} }
                }
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

/// What the heading calls a trip of `kind` (US-108) — both kinds named, so
/// the absence of a note never has to be read.
fn kind_name(kind: TripKind) -> &'static str {
    match kind {
        TripKind::Recorded => "Recorded trip",
        TripKind::Planned => "Planned trip",
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
    fn the_trips_name_is_led_by_its_activitys_icon_and_the_field_keeps_the_name() {
        let trip = a_trip("Oslo Hills Walk");

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        let heading = &html[html.find(r#"id="trip-name""#).unwrap()..];
        let heading = &heading[..heading.find("</h1>").unwrap()];
        assert!(heading.contains(r#"title="Hiking""#), "{heading}");
        assert!(heading.ends_with("Oslo Hills Walk"), "{heading}");
        assert!(html.contains(r#"id="trip-activity">Hiking<"#), "{html}");
    }

    #[test]
    fn an_unspecified_activity_puts_no_icon_before_the_name() {
        let trip = Trip {
            activity_type: ActivityType::Unknown,
            ..a_trip("Oslo Hills Walk")
        };

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert!(!html.contains("activity-icon"), "{html}");
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
    fn the_stats_are_the_seven_measurements_and_nothing_else() {
        // The activity moves into the list; the start and the timezone
        // leave it — the zone for good, its job taken by the captions.
        let trip = a_trip("Oslo Hills Walk");

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert_eq!(
            labels(&html),
            [
                "Activity",
                "Distance",
                "Average speed",
                "Ascent",
                "Climbing rate",
                "Descent",
                "Duration"
            ],
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
        assert!(html.contains("11. Jul. 2026"), "{html}");
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

    // ── US-108: recorded or planned ──────────────────────────────────────

    #[test]
    fn us108_the_heading_names_the_trips_kind_between_its_activity_and_its_name() {
        for (kind, dash, name) in [
            (
                TripKind::Recorded,
                r#"stroke-dasharray="none""#,
                "Recorded trip",
            ),
            (
                TripKind::Planned,
                r#"stroke-dasharray="8 6""#,
                "Planned trip",
            ),
        ] {
            let trip = Trip {
                trip_kind: kind,
                ..a_trip("Oslo Hills Walk")
            };

            let html = render(move || rsx! { TripStats { trip: trip.clone() } });

            let heading = &html[html.find(r#"id="trip-name""#).unwrap()..];
            let heading = &heading[..heading.find("</h1>").unwrap()];
            let activity = heading.find(r#"title="Hiking""#).expect("activity icon");
            let swatch = heading.find("kind-swatch").expect("kind icon");
            assert!(activity < swatch, "{heading}");
            assert!(
                heading[swatch..].contains(&format!(r#"title="{name}""#)),
                "{heading}"
            );
            assert!(
                heading[swatch..].contains(&format!(r#"aria-label="{name}""#)),
                "{heading}"
            );
            assert!(heading[swatch..].contains(dash), "{heading}");
            assert!(heading.ends_with("Oslo Hills Walk"), "{heading}");
            assert!(
                html.contains(&format!(r#"id="trip-kind">{name}<"#)),
                "{html}"
            );
        }
    }

    #[test]
    fn us108_the_kind_line_follows_the_name_and_the_date_follows_it() {
        let trip = a_trip("Oslo Hills Walk");

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        let name = html.find("</h1>").unwrap();
        let kind = html.find(r#"id="trip-kind""#).unwrap();
        let date = html.find(r#"id="trip-date""#).unwrap();
        assert!(name < kind && kind < date, "{html}");
    }

    #[test]
    fn us108_the_date_shares_the_kind_line_to_save_space() {
        let trip = a_trip("Oslo Hills Walk");

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        let line = html
            .split(r#"<p id="trip-kind-line">"#)
            .nth(1)
            .and_then(|rest| rest.split("</p>").next())
            .expect("one line beneath the name");
        assert!(line.contains(r#"id="trip-kind""#), "{line}");
        assert!(line.contains(r#"id="trip-date""#), "{line}");
        assert!(line.contains(" · "), "{line}");
    }

    #[test]
    fn us108_without_a_date_the_kind_line_holds_the_kind_alone() {
        let trip = Trip {
            start_date: None,
            ..a_trip("Oslo Hills Walk")
        };

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert!(html.contains(r#"id="trip-kind""#), "{html}");
        assert!(!html.contains(" · "), "{html}");
    }

    #[test]
    fn us108_an_unspecified_activity_leaves_the_kind_icon_alone_before_the_name() {
        let trip = Trip {
            activity_type: ActivityType::Unknown,
            trip_kind: TripKind::Planned,
            ..a_trip("Oslo Hills Walk")
        };

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert!(!html.contains("activity-icon"), "{html}");
        assert!(html.contains("kind-swatch"), "{html}");
        // In the unspecified activity's own grey, as in the list (US-90).
        assert!(html.contains(r##"stroke="#6b6b6b""##), "{html}");
    }

    // ── US-80: the average speed in motion ──────────────────────────────

    #[test]
    fn us80_the_average_speed_is_the_moving_distance_over_the_moving_time() {
        // 12 km in 3 h 20 min moving, though the trip took 3 h 45 min.
        let trip = a_trip("Oslo Hills Walk");

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert!(html.contains("<dd>3.6 km/h</dd>"), "{html}");
    }

    #[test]
    fn us80_a_trip_without_times_leaves_the_average_speed_out() {
        let untimed = Trip {
            moving_secs: None,
            moving_distance_m: None,
            ..a_trip("Planned Route")
        };
        let still = Trip {
            moving_secs: Some(0),
            moving_distance_m: Some(0.0),
            ..a_trip("Standing Still")
        };

        for trip in [untimed, still] {
            let html = render(move || rsx! { TripStats { trip: trip.clone() } });

            assert!(
                !labels(&html).contains(&"Average speed".to_string()),
                "{html}"
            );
        }
    }

    // ── US-81: the climbing rate ─────────────────────────────────────────

    #[test]
    fn us81_the_climbing_rate_is_the_climbs_height_over_their_moving_time() {
        // 300 m of climbs in 30 min moving on them.
        let trip = a_trip("Oslo Hills Walk");

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert!(html.contains("<dd>600 m/h</dd>"), "{html}");
    }

    #[test]
    fn us81_a_trip_without_climbs_or_times_leaves_the_climbing_rate_out() {
        let untimed = Trip {
            climb_gain_m: None,
            climb_secs: None,
            ..a_trip("Planned Route")
        };
        let flat = Trip {
            climb_gain_m: Some(0.0),
            climb_secs: Some(0),
            ..a_trip("Flat Ride")
        };

        for trip in [untimed, flat] {
            let html = render(move || rsx! { TripStats { trip: trip.clone() } });

            assert!(
                !labels(&html).contains(&"Climbing rate".to_string()),
                "{html}"
            );
        }
    }
}
