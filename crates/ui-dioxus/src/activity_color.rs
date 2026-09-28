//! Each activity's color on the maps (US-75), and the legend that names the
//! colors a map shows. The families follow the QMapShack export's (US-36) —
//! reds for walking, blues for cycling, cyan for kayaking — except that
//! snowshoeing joins the ski magentas here, while the export keeps it green.
//!
//! Decided here, where `cargo test` reaches it; the map scripts only draw
//! the color they are handed (ADR-0025).

use dioxus::prelude::*;
use trip_archive_types::ActivityType;

/// The color a trip of this activity is drawn in, on every map.
pub fn color(activity: ActivityType) -> &'static str {
    match activity {
        ActivityType::Unknown => "#6b6b6b",
        ActivityType::Hiking => "#b2182b",
        ActivityType::Mountaineering => "#f03b20",
        ActivityType::Cycling => "#1f4e9c",
        ActivityType::Bikepacking => "#4292e0",
        ActivityType::Kayaking => "#0e8a8a",
        ActivityType::SkiTouring => "#6a1b9a",
        ActivityType::CrossCountrySkiing => "#b0329e",
        ActivityType::SnowShoe => "#e377d0",
    }
}

/// What a map's legend lists for the activities it shows: each activity
/// once, as a label and its color, in the order the activity picker lists
/// them, an unspecified activity last. Empty when the map shows a single
/// activity — or none — since then there is nothing to tell apart.
pub fn legend(shown: impl IntoIterator<Item = ActivityType>) -> Vec<(&'static str, &'static str)> {
    let shown: Vec<ActivityType> = shown.into_iter().collect();
    let entries: Vec<_> = ActivityType::SELECTABLE
        .into_iter()
        .chain([ActivityType::Unknown])
        .filter(|activity| shown.contains(activity))
        .map(|activity| (label(activity), color(activity)))
        .collect();
    if entries.len() < 2 {
        return Vec::new();
    }
    entries
}

/// The picker's label, except for an unspecified activity, whose picker
/// label (`— unspecified —`) is written for a `<select>`.
fn label(activity: ActivityType) -> &'static str {
    match activity {
        ActivityType::Unknown => "Unspecified",
        other => other.label(),
    }
}

/// The legend under a map that shows several activities: a short stroke in
/// each color, like the line it stands for, and the activity's name.
#[component]
pub fn ActivityLegend(shown: Vec<ActivityType>) -> Element {
    let entries = legend(shown);
    if entries.is_empty() {
        return rsx! {};
    }
    rsx! {
        ul { class: "map-legend",
            for (label, color) in entries {
                li { key: "{label}",
                    // An SVG attribute rather than a `style`: the page's
                    // CSP refuses inline styles.
                    svg {
                        class: "map-legend-swatch",
                        view_box: "0 0 24 4",
                        "aria-hidden": "true",
                        rect { width: "24", height: "4", rx: "2", fill: "{color}" }
                    }
                    "{label}"
                }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;
    use ActivityType::*;

    const EVERY: [ActivityType; 9] = [
        Unknown,
        Hiking,
        Mountaineering,
        Cycling,
        Bikepacking,
        Kayaking,
        SkiTouring,
        CrossCountrySkiing,
        SnowShoe,
    ];

    #[test]
    fn every_activity_is_a_hex_color() {
        for activity in EVERY {
            let hex = color(activity);
            assert!(
                hex.len() == 7
                    && hex.starts_with('#')
                    && hex[1..].chars().all(|c| c.is_ascii_hexdigit()),
                "{activity}: {hex}"
            );
        }
    }

    #[test]
    fn no_two_activities_share_a_color() {
        for (i, a) in EVERY.iter().enumerate() {
            for b in &EVERY[i + 1..] {
                assert_ne!(color(*a), color(*b), "{a} and {b}");
            }
        }
    }

    #[test]
    fn the_palette_is_the_agreed_one() {
        assert_eq!(color(Hiking), "#b2182b");
        assert_eq!(color(Mountaineering), "#f03b20");
        assert_eq!(color(Cycling), "#1f4e9c");
        assert_eq!(color(Bikepacking), "#4292e0");
        assert_eq!(color(Kayaking), "#0e8a8a");
        assert_eq!(color(SkiTouring), "#6a1b9a");
        assert_eq!(color(CrossCountrySkiing), "#b0329e");
        // Snowshoeing is a magenta on the maps, unlike in the export.
        assert_eq!(color(SnowShoe), "#e377d0");
        assert_eq!(color(Unknown), "#6b6b6b");
    }

    #[test]
    fn a_single_activity_needs_no_legend() {
        assert!(legend([Hiking, Hiking, Hiking]).is_empty());
        assert!(legend([]).is_empty());
    }

    #[test]
    fn the_legend_names_each_shown_activity_once_in_picker_order() {
        assert_eq!(
            legend([Kayaking, Unknown, Hiking, Kayaking]),
            vec![
                ("Hiking", "#b2182b"),
                ("Kayaking", "#0e8a8a"),
                ("Unspecified", "#6b6b6b"),
            ]
        );
    }

    #[test]
    fn the_legend_lists_nothing_that_is_not_shown() {
        let entries = legend([Cycling, SnowShoe]);

        assert_eq!(
            entries,
            vec![("Cycling", "#1f4e9c"), ("Snowshoeing", "#e377d0")]
        );
    }

    #[test]
    fn the_legend_renders_a_swatch_and_label_per_entry() {
        let html = render(|| rsx! { ActivityLegend { shown: vec![Hiking, Cycling] } });

        assert!(html.contains("map-legend"), "{html}");
        assert!(
            html.contains("Hiking") && html.contains("Cycling"),
            "{html}"
        );
        assert!(
            html.contains("#b2182b") && html.contains("#1f4e9c"),
            "{html}"
        );
    }

    #[test]
    fn a_single_activity_renders_no_legend() {
        let html = render(|| rsx! { ActivityLegend { shown: vec![Hiking] } });

        assert!(!html.contains("map-legend"), "{html}");
    }
}
