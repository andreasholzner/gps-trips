//! The tag summary's trips (US-78): every trip under the chosen tags, listed
//! per tag at the foot of the screen, each with its line's color on the map,
//! so a row and a line can be matched without touching either.

use dioxus::prelude::*;
use trip_archive_types::{StatsTrip, TagSummaries};

use super::{lines, Viewer};
use crate::activity_color::Swatch;
use crate::format;

/// One chosen tag's trips, oldest first, each with its line's color.
#[derive(Clone, Debug, PartialEq)]
pub struct TripGroup {
    pub name: String,
    /// The tag's own color, with several tags chosen.
    pub color: Option<&'static str>,
    pub trips: Vec<(StatsTrip, &'static str)>,
}

/// A group per chosen tag that has trips, in the order chosen. A trip under
/// several of them is listed under each.
pub fn groups(data: &TagSummaries) -> Vec<TripGroup> {
    let colors = lines::trip_colors(data);
    let several = data.tags.len() > 1;
    data.tags
        .iter()
        .enumerate()
        .map(|(index, tag)| TripGroup {
            name: tag.name.clone(),
            color: several.then(|| lines::tag_color(index)),
            trips: data
                .trips
                .iter()
                .zip(&colors)
                .filter(|(trip, _)| tag.trip_ids.contains(&trip.id))
                .map(|(trip, color)| (trip.clone(), *color))
                .collect(),
        })
        .filter(|group| !group.trips.is_empty())
        .collect()
}

/// The trips, under a heading per tag when there are several.
#[component]
pub fn TripsByTag(summary: TagSummaries, viewer: Viewer) -> Element {
    let groups = groups(&summary);
    let headed = groups.len() > 1;
    rsx! {
        section { id: "summary-trips", class: "stats-section",
            h2 { "Trips" }
            for group in groups {
                div { key: "{group.name}", class: "summary-trip-group",
                    if headed {
                        h3 {
                            if let Some(color) = group.color {
                                Swatch { color }
                                " "
                            }
                            "{group.name}"
                        }
                    }
                    TripRows { trips: group.trips, viewer: viewer.clone() }
                }
            }
        }
    }
}

#[component]
fn TripRows(trips: Vec<(StatsTrip, &'static str)>, viewer: Viewer) -> Element {
    rsx! {
        div { class: "table-scroll",
            table { class: "summary-trips",
                thead {
                    tr {
                        th { scope: "col", "Name" }
                        th { scope: "col", "Date" }
                        th { scope: "col", "Activity" }
                        th { scope: "col", class: "num", "Distance" }
                    }
                }
                tbody {
                    for (trip, color) in trips {
                        tr { key: "{trip.id}",
                            td { class: "summary-trip-name",
                                Swatch { color }
                                Link { to: viewer.trip_route(trip.id), "{trip.name}" }
                            }
                            td { {format::day(&trip.start_date)} }
                            td { "{trip.activity_type.label()}" }
                            td { class: "num", {format::km(trip.distance_m)} }
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
    use trip_archive_types::{ActivityType, TagTrips};

    fn trip(id: i64, start: &str) -> StatsTrip {
        StatsTrip {
            id,
            name: format!("Trip {id}"),
            activity_type: ActivityType::Hiking,
            start_date: start.to_string(),
            end_date: start.to_string(),
            distance_m: 12_300.0,
            ascent_m: None,
            descent_m: None,
            moving_secs: None,
        }
    }

    fn tag(name: &str, trip_ids: &[i64]) -> TagTrips {
        TagTrips {
            name: name.to_string(),
            trip_ids: trip_ids.to_vec(),
            undated: 0,
        }
    }

    fn data(tags: Vec<TagTrips>) -> TagSummaries {
        TagSummaries {
            tags,
            trips: vec![
                trip(1, "2024-07-01"),
                trip(2, "2024-07-02"),
                trip(3, "2024-08-10"),
            ],
        }
    }

    fn ids(group: &TripGroup) -> Vec<i64> {
        group.trips.iter().map(|(trip, _)| trip.id).collect()
    }

    #[test]
    fn us78_each_tag_lists_its_trips_oldest_first_and_a_shared_one_under_each() {
        let groups = groups(&data(vec![tag("norway", &[2, 3]), tag("alps", &[1, 2])]));

        let names: Vec<&str> = groups.iter().map(|group| group.name.as_str()).collect();
        assert_eq!(names, ["norway", "alps"]);
        assert_eq!(ids(&groups[0]), [2, 3]);
        assert_eq!(ids(&groups[1]), [1, 2]);
    }

    #[test]
    fn us78_a_row_wears_its_lines_color() {
        let data = data(vec![tag("norway", &[2, 3]), tag("alps", &[1, 2])]);

        let groups = groups(&data);

        assert_eq!(groups[0].color, Some(lines::tag_color(0)));
        // Trip 2 is drawn in the first chosen tag's color, under both tags.
        assert_eq!(groups[1].trips[1].1, lines::tag_color(0));
        assert_eq!(groups[1].trips[0].1, lines::tag_color(1));
    }

    #[test]
    fn us78_a_tag_without_trips_has_no_group() {
        let groups = groups(&data(vec![tag("alps", &[1]), tag("empty", &[])]));

        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn us78_the_rows_link_to_their_trips_with_date_activity_and_distance() {
        let data = data(vec![tag("alps", &[1, 2])]);

        let html =
            render(move || rsx! { TripsByTag { summary: data.clone(), viewer: Viewer::Owner } });

        assert!(html.contains(r#"href="/trips/1""#), "{html}");
        assert!(html.contains("Trip 2"), "{html}");
        assert!(html.contains("1. Jul. 2024"), "{html}");
        assert!(html.contains("Hiking"), "{html}");
        assert!(html.contains("12.30 km"), "{html}");
        // One tag: no heading repeating its name.
        assert!(!html.contains("<h3"), "{html}");
    }

    #[test]
    fn us78_several_tags_head_their_trips_with_their_name_and_color() {
        let data = data(vec![tag("norway", &[2, 3]), tag("alps", &[1, 2])]);

        let html =
            render(move || rsx! { TripsByTag { summary: data.clone(), viewer: Viewer::Owner } });

        assert_eq!(html.matches("<h3").count(), 2, "{html}");
        assert_eq!(html.matches(r#"href="/trips/2""#).count(), 2, "{html}");
        assert!(html.contains(lines::tag_color(1)), "{html}");
    }
}
