//! The tag summary's table (US-78): a column per chosen tag, the figures as
//! rows, grouped by activity with the activities together last. It only
//! lays out what `figures` added up.

use dioxus::prelude::*;
use trip_archive_types::ActivityType;

use super::figures::{self, Sums, TagFigures, TagSummary};
use crate::activity_color::{self, Swatch};
use crate::format;
use crate::stats::figures::DayRecord;
use crate::stats::Measure;
use crate::Route;

/// What a cell with nothing in it shows — the dash the rest of the app uses.
const NOTHING: &str = "—";

/// A figure's row: its label, and how it reads from a group's [`Sums`].
type FigureRow = (&'static str, fn(&Sums) -> String);

/// The figures of `tags` — those with trips — side by side. `colors` are
/// the tags' map colors, given with several tags only.
#[component]
pub fn FiguresTable(tags: Vec<(TagSummary, TagFigures)>, colors: Vec<&'static str>) -> Element {
    let summaries: Vec<TagSummary> = tags.iter().map(|(tag, _)| tag.clone()).collect();
    let activities = figures::activities(&summaries);
    let together = activities.len() > 1;
    let all: Vec<TagFigures> = tags.iter().map(|(_, figures)| figures.clone()).collect();
    rsx! {
        div { class: "table-scroll",
            table { id: "summary-figures", class: "stats-table summary-table",
                thead {
                    tr {
                        td {}
                        for (index, (tag, _)) in tags.iter().enumerate() {
                            th { key: "{tag.name}", scope: "col",
                                if let Some(color) = colors.get(index) {
                                    Swatch { color: *color }
                                    " "
                                }
                                "{tag.name}"
                            }
                        }
                    }
                }
                tbody {
                    tr {
                        th { scope: "row", "Dates" }
                        for figures in all.iter() {
                            td { {span(&figures.first, &figures.last)} }
                        }
                    }
                    tr {
                        th { scope: "row", "Longest day" }
                        for figures in all.iter() {
                            td { LongestDay { day: figures.longest_day.clone() } }
                        }
                    }
                }
                for activity in activities {
                    SumsGroup {
                        key: "{activity}",
                        activity: Some(activity),
                        sums: all.iter().map(|figures| figures.of(activity).cloned()).collect(),
                    }
                }
                if together {
                    SumsGroup {
                        activity: None,
                        sums: all.iter().map(|figures| Some(figures.together.clone())).collect(),
                    }
                }
            }
        }
    }
}

/// The dates a tag spans, as the owner reads dates (US-62).
fn span(first: &str, last: &str) -> String {
    if first == last {
        format::day(first)
    } else {
        format!("{} – {}", format::day(first), format::day(last))
    }
}

/// A tag's longest day: its distance, its date and the trips started on it.
#[component]
fn LongestDay(day: Option<DayRecord>) -> Element {
    let Some(day) = day else {
        return rsx! { "{NOTHING}" };
    };
    let count = day.trips.len();
    rsx! {
        span { class: "record-value", "{Measure::Distance.format(day.km)}" }
        " on {format::day(&day.date)}: "
        for (index, (id, name)) in day.trips.into_iter().enumerate() {
            Link { key: "{id}", to: Route::TripDetail { id }, "{name}" }
            if index + 1 < count {
                ", "
            }
        }
    }
}

/// One activity's figures, or the activities together's, under a heading
/// row: a cell per tag, a dash where a tag holds none of it.
#[component]
fn SumsGroup(activity: Option<ActivityType>, sums: Vec<Option<Sums>>) -> Element {
    let columns = sums.len() + 1;
    let rows: [FigureRow; 6] = [
        ("Trips", |s| s.trips.to_string()),
        ("Days out", |s| s.days_out.to_string()),
        ("Distance", |s| Measure::Distance.format(s.km)),
        ("Ascent", |s| Measure::Ascent.format(s.ascent_m)),
        ("Descent", |s| Measure::Ascent.format(s.descent_m)),
        ("Moving time", |s| {
            Measure::MovingTime.format(s.moving_hours)
        }),
    ];
    rsx! {
        tbody { class: "summary-group",
            tr {
                th { class: "summary-activity", colspan: "{columns}", scope: "colgroup",
                    match activity {
                        Some(activity) => rsx! {
                            Swatch { color: activity_color::color(activity) }
                            " {activity.label()}"
                        },
                        None => rsx! { "All activities" },
                    }
                }
            }
            for (label, value) in rows {
                tr { key: "{label}",
                    th { scope: "row", "{label}" }
                    for sums in sums.iter() {
                        td { class: "num",
                            {sums.as_ref().map_or_else(|| NOTHING.to_string(), value)}
                        }
                    }
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

    fn sums(trips: u32, km: f64) -> Sums {
        Sums {
            trips,
            days_out: trips,
            km,
            ascent_m: 1200.0,
            descent_m: 1100.0,
            moving_hours: 2.5,
        }
    }

    fn tag(name: &str, activities: Vec<(ActivityType, Sums)>) -> (TagSummary, TagFigures) {
        let figures = TagFigures {
            first: "2024-07-01".to_string(),
            last: "2024-07-09".to_string(),
            together: sums(activities.iter().map(|(_, s)| s.trips).sum(), 0.0),
            activities,
            longest_day: Some(DayRecord {
                date: "2024-07-04".to_string(),
                km: 92.0,
                trips: vec![(7, "Stelvio".to_string())],
            }),
        };
        let summary = TagSummary {
            name: name.to_string(),
            undated: 0,
            figures: Some(figures.clone()),
        };
        (summary, figures)
    }

    #[test]
    fn us78_one_tag_shows_its_dates_longest_day_and_each_activitys_figures() {
        let tags = vec![tag(
            "alps",
            vec![
                (ActivityType::Hiking, sums(3, 40.0)),
                (ActivityType::Cycling, sums(1, 60.0)),
            ],
        )];

        let html = render(move || rsx! { FiguresTable { tags: tags.clone(), colors: Vec::new() } });

        assert!(html.contains("alps"), "{html}");
        assert!(html.contains("1. Jul. 2024 – 9. Jul. 2024"), "{html}");
        assert!(html.contains("92.0 km"), "{html}");
        assert!(html.contains("4. Jul. 2024"), "{html}");
        assert!(html.contains("Stelvio"), "{html}");
        assert!(
            html.contains("Hiking") && html.contains("Cycling"),
            "{html}"
        );
        for label in [
            "Trips",
            "Days out",
            "Distance",
            "Ascent",
            "Descent",
            "Moving time",
        ] {
            assert!(html.contains(label), "{label} missing: {html}");
        }
        assert!(html.contains("40.0 km"), "{html}");
        assert!(html.contains("1100 m"), "{html}");
        assert!(html.contains("2:30 h"), "{html}");
        // More than one activity: their figures together beneath.
        assert!(html.contains("All activities"), "{html}");
    }

    #[test]
    fn us78_a_tag_of_one_activity_shows_no_together_group() {
        let tags = vec![tag("alps", vec![(ActivityType::Hiking, sums(3, 40.0))])];

        let html = render(move || rsx! { FiguresTable { tags: tags.clone(), colors: Vec::new() } });

        assert!(!html.contains("All activities"), "{html}");
    }

    #[test]
    fn us78_several_tags_are_columns_with_their_color_and_a_dash_for_what_one_lacks() {
        let tags = vec![
            tag("alps", vec![(ActivityType::Cycling, sums(8, 512.0))]),
            tag("norway", vec![(ActivityType::Hiking, sums(11, 140.0))]),
        ];

        let html = render(move || {
            rsx! { FiguresTable { tags: tags.clone(), colors: vec!["#eb6834", "#1baf7a"] } }
        });

        assert!(html.contains("alps") && html.contains("norway"), "{html}");
        assert!(
            html.contains("#eb6834") && html.contains("#1baf7a"),
            "{html}"
        );
        assert!(html.contains("512 km") && html.contains("140 km"), "{html}");
        assert!(html.contains(NOTHING), "{html}");
        assert!(html.contains("All activities"), "{html}");
    }
}
