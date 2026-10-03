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

/// The sum row's name.
const ALL_ACTIVITIES: &str = "All activities";

/// The totals: a row per activity, a column per year or month, the row's
/// total and, with all activities, its share; all activities together at
/// the foot.
#[component]
pub fn TotalsTable(totals: Totals, measure: Measure) -> Element {
    let shares = totals.sum.is_some();
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
                        TotalsLine { row: row.clone(), measure, shares }
                    }
                }
                if let Some(sum) = totals.sum.clone() {
                    tfoot {
                        TotalsLine { row: sum, measure, shares }
                    }
                }
            }
        }
    }
}

#[component]
fn TotalsLine(row: TotalsRow, measure: Measure, shares: bool) -> Element {
    let share = row
        .share
        .map_or_else(String::new, |share| format!("{:.0} %", share * 100.0));
    rsx! {
        tr {
            th { scope: "row", ActivityName { activity: row.activity } }
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

/// An activity with its map color, or the sum row's name.
#[component]
fn ActivityName(activity: Option<ActivityType>) -> Element {
    match activity {
        Some(activity) => rsx! {
            Swatch { color: activity_color::color(activity) }
            " {activity.label()}"
        },
        None => rsx! { "{ALL_ACTIVITIES}" },
    }
}

/// The records: longest trip, most ascent and longest day, each linking to
/// its trips; overall and per activity, or for the chosen activity alone.
#[component]
pub fn RecordsTable(rows: Vec<RecordRow>) -> Element {
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
                            th { scope: "row", ActivityName { activity: row.activity } }
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

#[component]
fn TripRecordCell(record: Option<TripRecord>, measure: Measure) -> Element {
    let Some(record) = record else {
        return rsx! { "{NOTHING}" };
    };
    rsx! {
        span { class: "record-value", "{measure.format(record.value)}" }
        " "
        Link { to: Route::TripDetail { id: record.id }, "{record.name}" }
    }
}

#[component]
fn DayRecordCell(record: Option<DayRecord>) -> Element {
    let Some(record) = record else {
        return rsx! { "{NOTHING}" };
    };
    let count = record.trips.len();
    rsx! {
        span { class: "record-value", "{Measure::Distance.format(record.km)}" }
        " on {record.date}: "
        for (index, (id, name)) in record.trips.into_iter().enumerate() {
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
            rsx! { TotalsTable { totals: totals.clone(), measure: Measure::Distance } }
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
            rsx! { TotalsTable { totals: totals.clone(), measure: Measure::Trips } }
        });

        assert!(!html.contains("Share"), "{html}");
        assert!(!html.contains("All activities"), "{html}");
    }

    #[test]
    fn us77_each_record_links_to_its_trips() {
        let rows = vec![RecordRow {
            activity: None,
            longest: Some(TripRecord {
                id: 7,
                name: "Big Ride".into(),
                value: 123.0,
            }),
            most_ascent: None,
            longest_day: Some(DayRecord {
                date: "2024-03-10".into(),
                km: 150.0,
                trips: vec![(7, "Big Ride".into()), (8, "Evening Walk".into())],
            }),
        }];

        let html = render(move || rsx! { RecordsTable { rows: rows.clone() } });

        assert!(html.contains("123 km"), "{html}");
        assert!(html.contains(r#"href="/trips/7""#), "{html}");
        assert!(html.contains(r#"href="/trips/8""#), "{html}");
        assert!(html.contains("150 km"), "{html}");
        assert!(html.contains("2024-03-10"), "{html}");
        assert!(html.contains("Evening Walk"), "{html}");
        assert!(html.contains(NOTHING), "no ascent record: {html}");
    }
}
