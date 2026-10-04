//! The statistics screen's tables (US-77): the totals per activity, and the
//! records. They only lay out what `figures` added up.

use dioxus::prelude::*;
use trip_archive_types::ActivityType;

use super::figures::{DayRecord, RecordRow, Totals, TotalsRow, TripRecord};
use super::view::Measure;
use crate::activity_color::{self, Swatch};
use crate::Route;

/// What a cell with nothing in it shows — the dash the rest of the app uses.
const NOTHING: &str = "—";

/// The totals: a row per activity, a column per year or month, the row's
/// total and, with all activities, its share; all activities together at
/// the foot, named `together`.
#[component]
pub fn TotalsTable(totals: Totals, measure: Measure, together: &'static str) -> Element {
    // A ratio is no share of anything (US-80).
    let shares = totals.sum.is_some() && !measure.is_ratio();
    rsx! {
        div { class: "table-scroll",
            table { id: "stats-totals", class: "stats-table",
                thead {
                    tr {
                        th { scope: "col", "Activity" }
                        for column in totals.columns.iter() {
                            th { scope: "col", class: "num", "{column}" }
                        }
                        th { scope: "col", class: "num", "Total" }
                        if shares {
                            th { scope: "col", class: "num", "Share" }
                        }
                    }
                }
                tbody {
                    for row in totals.rows.iter() {
                        TotalsLine { row: row.clone(), measure, shares, together }
                    }
                }
                if let Some(sum) = totals.sum.clone() {
                    tfoot {
                        TotalsLine { row: sum, measure, shares, together }
                    }
                }
            }
        }
    }
}

#[component]
fn TotalsLine(row: TotalsRow, measure: Measure, shares: bool, together: &'static str) -> Element {
    let share = row
        .share
        .map_or_else(String::new, |share| format!("{:.0} %", share * 100.0));
    rsx! {
        tr {
            th { scope: "row", ActivityName { activity: row.activity, together } }
            for value in row.values.iter() {
                td { class: "num", "{cell(measure, *value)}" }
            }
            td { class: "num total", "{cell(measure, row.total)}" }
            if shares {
                td { class: "num", "{share}" }
            }
        }
    }
}

/// A value, or a dash for none — a column of zeros reads as noise.
fn cell(measure: Measure, value: f64) -> String {
    if value == 0.0 {
        NOTHING.to_string()
    } else {
        measure.format(value)
    }
}

/// An activity with its map color, or what the activities together are
/// called.
#[component]
fn ActivityName(activity: Option<ActivityType>, together: &'static str) -> Element {
    match activity {
        Some(activity) => rsx! {
            Swatch { color: activity_color::color(activity) }
            " {activity.label()}"
        },
        None => rsx! { "{together}" },
    }
}

/// The records: longest trip, most ascent and longest day, each linking to
/// its trips; overall (`together`) and per activity, or for the single
/// chosen activity alone.
#[component]
pub fn RecordsTable(rows: Vec<RecordRow>, together: &'static str) -> Element {
    rsx! {
        div { class: "table-scroll",
            table { id: "stats-records", class: "stats-table",
                thead {
                    tr {
                        th { scope: "col", "Activity" }
                        th { scope: "col", "Longest trip" }
                        th { scope: "col", "Most ascent" }
                        th { scope: "col", "Longest day" }
                    }
                }
                tbody {
                    for row in rows {
                        tr {
                            th { scope: "row", ActivityName { activity: row.activity, together } }
                            td { TripRecordCell { record: row.longest, measure: Measure::Distance } }
                            td { TripRecordCell { record: row.most_ascent, measure: Measure::Ascent } }
                            td { DayRecordCell { record: row.longest_day } }
                        }
                    }
                }
            }
        }
    }
}

/// A record's places, best first, as a numbered list.
#[component]
fn TripRecordCell(record: Vec<TripRecord>, measure: Measure) -> Element {
    if record.is_empty() {
        return rsx! { "{NOTHING}" };
    }
    rsx! {
        ol { class: "record-places",
            for place in record {
                li { key: "{place.id}",
                    span { class: "record-value", "{measure.format(place.value)}" }
                    " "
                    Link { to: Route::TripDetail { id: place.id }, "{place.name}" }
                }
            }
        }
    }
}

#[component]
fn DayRecordCell(record: Vec<DayRecord>) -> Element {
    if record.is_empty() {
        return rsx! { "{NOTHING}" };
    }
    rsx! {
        ol { class: "record-places",
            for day in record {
                li { key: "{day.date}", DayPlace { day } }
            }
        }
    }
}

/// One date: its distance, the date, and the trips started on it.
#[component]
fn DayPlace(day: DayRecord) -> Element {
    let count = day.trips.len();
    rsx! {
        span { class: "record-value", "{Measure::Distance.format(day.km)}" }
        " on {day.date}: "
        for (index, (id, name)) in day.trips.into_iter().enumerate() {
            Link { key: "{id}", to: Route::TripDetail { id }, "{name}" }
            if index + 1 < count {
                ", "
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    fn row(activity: Option<ActivityType>, values: Vec<f64>, share: Option<f64>) -> TotalsRow {
        let total = values.iter().sum();
        TotalsRow {
            activity,
            values,
            total,
            share,
        }
    }

    #[test]
    fn us77_the_totals_table_has_a_row_per_activity_its_share_and_the_sum() {
        let totals = Totals {
            columns: vec!["2024".into(), "2025".into()],
            rows: vec![
                row(Some(ActivityType::Hiking), vec![60.0, 0.0], Some(0.6)),
                row(Some(ActivityType::Cycling), vec![30.0, 10.0], Some(0.4)),
            ],
            sum: Some(row(None, vec![90.0, 10.0], None)),
        };

        let html = render(move || {
            rsx! { TotalsTable { totals: totals.clone(), measure: Measure::Distance, together: "All activities" } }
        });

        assert!(html.contains(r#"id="stats-totals""#), "{html}");
        assert!(html.contains(">2024<") && html.contains(">2025<"), "{html}");
        assert!(
            html.contains("Hiking") && html.contains("Cycling"),
            "{html}"
        );
        assert!(html.contains("60.0 km"), "{html}");
        assert!(html.contains("60 %") && html.contains("40 %"), "{html}");
        assert!(html.contains("All activities"), "{html}");
        assert!(html.contains("<tfoot>"), "{html}");
        // An empty cell is a dash, not "0.0 km".
        assert!(!html.contains(">0.0 km<"), "{html}");
        assert!(html.contains(NOTHING), "{html}");
    }

    #[test]
    fn us77_one_activitys_totals_have_no_share_column_and_no_sum() {
        let totals = Totals {
            columns: vec!["2024".into()],
            rows: vec![row(Some(ActivityType::Hiking), vec![2.0], None)],
            sum: None,
        };

        let html = render(move || {
            rsx! { TotalsTable { totals: totals.clone(), measure: Measure::Trips, together: "All activities" } }
        });

        assert!(!html.contains("Share"), "{html}");
        assert!(!html.contains("All activities"), "{html}");
    }

    #[test]
    fn us80_average_speeds_have_no_share_column_but_keep_their_sum() {
        let totals = Totals {
            columns: vec!["2024".into()],
            rows: vec![
                row(Some(ActivityType::Hiking), vec![5.0], None),
                row(Some(ActivityType::Cycling), vec![30.0], None),
            ],
            sum: Some(row(None, vec![40.0 / 3.0], None)),
        };

        let html = render(move || {
            rsx! { TotalsTable { totals: totals.clone(), measure: Measure::AverageSpeed, together: "All activities" } }
        });

        assert!(!html.contains("Share"), "{html}");
        assert!(html.contains("All activities"), "{html}");
        assert!(html.contains("13.3 km/h"), "{html}");
    }

    fn place(id: i64, name: &str, value: f64) -> TripRecord {
        TripRecord {
            id,
            name: name.into(),
            value,
        }
    }

    #[test]
    fn us77_each_record_lists_its_places_in_order_linking_to_their_trips() {
        let rows = vec![RecordRow {
            activity: None,
            longest: vec![
                place(7, "Big Ride", 123.0),
                place(9, "Long Walk", 45.0),
                place(8, "Evening Walk", 12.0),
            ],
            most_ascent: Vec::new(),
            longest_day: vec![DayRecord {
                date: "2024-03-10".into(),
                km: 150.0,
                trips: vec![(7, "Big Ride".into()), (8, "Evening Walk".into())],
            }],
        }];

        let html = render(
            move || rsx! { RecordsTable { rows: rows.clone(), together: "Chosen activities" } },
        );

        assert_eq!(html.matches("<ol").count(), 2, "{html}");
        let first = html.find("Big Ride").unwrap();
        let second = html.find("Long Walk").unwrap();
        let third = html.find("Evening Walk").unwrap();
        assert!(first < second && second < third, "best first: {html}");
        assert!(
            html.contains("123 km") && html.contains("45.0 km"),
            "{html}"
        );
        assert!(html.contains(r#"href="/trips/9""#), "{html}");
        assert!(html.contains("150 km</span> on 2024-03-10"), "{html}");
        assert!(html.contains(NOTHING), "no ascent record: {html}");
        assert!(html.contains("Chosen activities"), "{html}");
    }
}
