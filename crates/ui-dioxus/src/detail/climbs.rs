//! The trip's climbs under its elevation profile (US-81), in track order:
//! each one's length, the height it gains and how fast it was climbed.

use dioxus::prelude::*;
use trip_archive_types::Climb;

use crate::{format, rates};

/// The list; nothing at all for a trip without climbs. A climb without
/// times — a planned trip's — reads its rate as a dash.
#[component]
pub fn ClimbsList(climbs: Vec<Climb>) -> Element {
    if climbs.is_empty() {
        return rsx! {};
    }
    rsx! {
        section { id: "climbs",
            h2 { "Climbs" }
            div { class: "table-scroll",
                table {
                    thead {
                        tr {
                            th { scope: "col", "#" }
                            th { scope: "col", class: "num", "Distance" }
                            th { scope: "col", class: "num", "Height" }
                            th { scope: "col", class: "num", "Climbing rate" }
                        }
                    }
                    tbody {
                        for (index, climb) in climbs.iter().enumerate() {
                            tr { key: "{index}",
                                td { "{index + 1}" }
                                td { class: "num", {format::km(climb.end_m - climb.start_m)} }
                                td { class: "num", {format::metres(Some(climb.gain_m))} }
                                td { class: "num", {format::climbing_rate(rate(climb))} }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One climb's rate, when it was climbed with times.
fn rate(climb: &Climb) -> Option<f64> {
    climb
        .moving_secs
        .and_then(|secs| rates::climbing_rate(climb.gain_m, secs as f64))
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    fn a_climb(start_m: f64, end_m: f64, gain_m: f64, moving_secs: Option<i64>) -> Climb {
        Climb {
            start_m,
            end_m,
            gain_m,
            moving_secs,
        }
    }

    fn rows(html: &str) -> Vec<String> {
        html.split("<tr>")
            .skip(2)
            .map(|row| {
                row.split("</tr>")
                    .next()
                    .unwrap_or_default()
                    .replace("<td class=\"num\">", "|")
                    .replace("<td>", "")
                    .replace("</td>", "")
            })
            .collect()
    }

    #[test]
    fn us81_each_climb_shows_its_distance_height_and_rate_in_track_order() {
        let climbs = vec![
            a_climb(300.0, 1300.0, 100.4, Some(900)),
            a_climb(4000.0, 6500.0, 250.0, Some(1500)),
        ];

        let html = render(move || rsx! { ClimbsList { climbs: climbs.clone() } });

        assert!(html.contains(r#"id="climbs""#), "{html}");
        assert_eq!(
            rows(&html),
            ["1|1.00 km|100 m|402 m/h", "2|2.50 km|250 m|600 m/h"],
            "{html}"
        );
    }

    #[test]
    fn us81_a_climb_without_times_reads_its_rate_as_a_dash() {
        let climbs = vec![a_climb(300.0, 1300.0, 100.0, None)];

        let html = render(move || rsx! { ClimbsList { climbs: climbs.clone() } });

        assert_eq!(rows(&html), ["1|1.00 km|100 m|—"], "{html}");
    }

    #[test]
    fn us81_a_trip_without_climbs_lists_nothing() {
        let html = render(|| rsx! { ClimbsList { climbs: Vec::new() } });

        assert!(!html.contains("climbs"), "{html}");
    }
}
