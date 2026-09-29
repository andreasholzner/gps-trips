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
    shades(activity)[0]
}

/// The activity's color and three shades of it, for telling a share's trips
/// of one activity apart (US-72). Each next shade is the one farthest (CIE
/// ΔE) from those before it, and every shade is closer to its own activity's
/// color than to any other activity's, so a shade never passes for another
/// activity.
pub fn shades(activity: ActivityType) -> [&'static str; 4] {
    match activity {
        ActivityType::Unknown => ["#6b6b6b", "#3a3a3a", "#8c8c8c", "#5b6470"],
        ActivityType::Hiking => ["#b2182b", "#9b2543", "#ee1950", "#e36257"],
        ActivityType::Mountaineering => ["#f03b20", "#ca5013", "#fd6751", "#fc671e"],
        ActivityType::Cycling => ["#1f4e9c", "#1c4a76", "#3c60d1", "#1d3293"],
        ActivityType::Bikepacking => ["#4292e0", "#5cb3e4", "#2176a5", "#5c89ef"],
        ActivityType::Kayaking => ["#0e8a8a", "#07a586", "#04574c", "#27c3c3"],
        ActivityType::SkiTouring => ["#6a1b9a", "#5b2275", "#8747df", "#a015bf"],
        ActivityType::CrossCountrySkiing => ["#b0329e", "#90296d", "#d946d7", "#8c3d91"],
        ActivityType::SnowShoe => ["#e377d0", "#dc93ca", "#e86feb", "#ca5da1"],
    }
}

/// Each trip's color, given the trips' activities in list order (US-72): its
/// activity's color for the first trip of that activity, the next shade for
/// the next, and the color again once every shade is taken. A share's map
/// and the trip list's lines (US-73) both take their colors from here.
pub fn in_list_order(activities: impl IntoIterator<Item = ActivityType>) -> Vec<&'static str> {
    let mut taken: Vec<(ActivityType, usize)> = Vec::new();
    activities
        .into_iter()
        .map(|activity| {
            let before = match taken.iter_mut().find(|(seen, _)| *seen == activity) {
                Some((_, count)) => {
                    *count += 1;
                    *count - 1
                }
                None => {
                    taken.push((activity, 1));
                    0
                }
            };
            let shades = shades(activity);
            shades[before % shades.len()]
        })
        .collect()
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
                    Swatch { color }
                    "{label}"
                }
            }
        }
    }
}

/// A short stroke in `color`, like the line on the map it stands for.
#[component]
pub fn Swatch(color: &'static str) -> Element {
    rsx! {
        // An SVG attribute rather than a `style`: the page's CSP refuses
        // inline styles.
        svg {
            class: "swatch",
            view_box: "0 0 24 4",
            "aria-hidden": "true",
            rect { width: "24", height: "4", rx: "2", fill: "{color}" }
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
    fn an_activitys_first_shade_is_its_color() {
        // US-72: a share's first trip of an activity looks as US-75 drew it.
        for activity in EVERY {
            assert_eq!(shades(activity)[0], color(activity), "{activity}");
        }
    }

    #[test]
    fn no_shade_is_used_twice_across_all_activities() {
        let all: Vec<_> = EVERY.iter().flat_map(|a| shades(*a)).collect();
        for (i, shade) in all.iter().enumerate() {
            assert!(!all[i + 1..].contains(shade), "{shade} is used twice");
        }
    }

    #[test]
    fn the_shades_come_most_distinct_first() {
        // The palette page's shades, each next one the farthest from those
        // already taken, worked out once from the palette.
        assert_eq!(shades(Hiking), ["#b2182b", "#9b2543", "#ee1950", "#e36257"]);
        assert_eq!(
            shades(Unknown),
            ["#6b6b6b", "#3a3a3a", "#8c8c8c", "#5b6470"]
        );
    }

    #[test]
    fn trips_of_one_activity_get_shades_of_its_color_in_list_order() {
        // US-72.
        assert_eq!(
            in_list_order([Hiking, Cycling, Hiking]),
            ["#b2182b", "#1f4e9c", "#9b2543"]
        );
    }

    #[test]
    fn a_fifth_trip_of_one_activity_starts_over_at_its_color() {
        assert_eq!(
            in_list_order([Hiking; 5]),
            ["#b2182b", "#9b2543", "#ee1950", "#e36257", "#b2182b"]
        );
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
