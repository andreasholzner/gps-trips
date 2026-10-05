//! The totals as a chart (US-77): the same figures the totals table lists,
//! decided here and only drawn by the script (ADR-0025) — a line per
//! activity, and one for the activities together, dashed, when there are
//! several.

use serde::Serialize;

use super::figures::{Totals, TotalsRow};
use super::view::{Measure, StatsView};
use crate::activity_color;

/// What a chart shows.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Plot {
    /// The column names: the years, or the months.
    pub labels: Vec<String>,
    /// What a column is, for the cursor's readout.
    pub column: &'static str,
    /// The value axis' label.
    pub axis: &'static str,
    pub series: Vec<PlotSeries>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlotSeries {
    pub label: &'static str,
    /// The activity's color; `None` for the activities together, which are
    /// drawn in the text color.
    pub color: Option<&'static str>,
    /// Each column's point; `None` where there is none — a ratio with
    /// nothing to divide by — so the line breaks there.
    pub drawn: Vec<Option<f64>>,
    /// Each column's value as the table shows it, for the readout.
    pub shown: Vec<String>,
}

/// The chart of `totals`, whose rows and foot are named as the table names
/// them (`together` for the foot). One activity needs no line for the
/// activities together: it would be its own line again.
pub fn plot(totals: &Totals, view: &StatsView, together: &'static str) -> Plot {
    let measure = view.measure;
    let sum = totals.sum.as_ref().filter(|_| totals.rows.len() > 1);
    Plot {
        labels: totals.columns.clone(),
        column: if view.year.is_some() { "Month" } else { "Year" },
        axis: measure.axis_label(),
        series: totals
            .rows
            .iter()
            .chain(sum)
            .map(|row| series(row, measure, together))
            .collect(),
    }
}

fn series(row: &TotalsRow, measure: Measure, together: &'static str) -> PlotSeries {
    // A ratio's 0 is no figure (US-80); any other measure's is a real one.
    let drawn = row
        .values
        .iter()
        .map(|value| (!measure.is_ratio() || *value != 0.0).then_some(*value))
        .collect();
    PlotSeries {
        label: row.activity.map_or(together, activity_color::label),
        color: row.activity.map(activity_color::color),
        drawn,
        shown: row
            .values
            .iter()
            .map(|value| super::tables::cell(measure, *value))
            .collect(),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use trip_archive_types::ActivityType::{self, Cycling, Hiking};

    fn row(activity: Option<ActivityType>, values: Vec<f64>) -> TotalsRow {
        TotalsRow {
            activity,
            total: values.iter().sum(),
            values,
            share: None,
        }
    }

    fn two_activities() -> Totals {
        Totals {
            columns: vec!["2024".into(), "2025".into()],
            rows: vec![
                row(Some(Hiking), vec![60.0, 0.0]),
                row(Some(Cycling), vec![30.0, 10.0]),
            ],
            sum: Some(row(None, vec![90.0, 10.0])),
        }
    }

    fn view(measure: Measure, year: Option<i32>) -> StatsView {
        StatsView {
            measure,
            year,
            ..StatsView::default()
        }
    }

    #[test]
    fn us77_the_totals_are_a_line_per_activity_and_one_for_them_together() {
        let plot = plot(
            &two_activities(),
            &view(Measure::Distance, None),
            "All activities",
        );

        assert_eq!(plot.labels, ["2024", "2025"]);
        assert_eq!(plot.column, "Year");
        assert_eq!(plot.axis, "Distance (km)");
        let labels: Vec<_> = plot.series.iter().map(|series| series.label).collect();
        assert_eq!(labels, ["Hiking", "Cycling", "All activities"]);
        // A year without hiking is a real nothing: the line drops to zero.
        assert_eq!(plot.series[0].drawn, [Some(60.0), Some(0.0)]);
        assert_eq!(plot.series[2].drawn, [Some(90.0), Some(10.0)]);
        assert_eq!(plot.series[0].color, Some(activity_color::color(Hiking)));
        assert_eq!(plot.series[2].color, None, "drawn in the text color");
        // The readout shows the values as the table does.
        assert_eq!(plot.series[0].shown, ["60.0 km", "—"]);
        assert_eq!(plot.series[1].shown, ["30.0 km", "10.0 km"]);
    }

    #[test]
    fn us80_a_ratio_with_nothing_to_divide_by_leaves_a_gap() {
        let plot = plot(
            &two_activities(),
            &view(Measure::AverageSpeed, None),
            "All activities",
        );

        assert_eq!(plot.series[0].drawn, [Some(60.0), None]);
    }

    #[test]
    fn us77_one_year_has_a_point_per_month() {
        let plot = plot(
            &two_activities(),
            &view(Measure::DaysOut, Some(2024)),
            "Chosen activities",
        );

        assert_eq!(plot.column, "Month");
        assert_eq!(plot.series.last().unwrap().label, "Chosen activities");
    }

    #[test]
    fn us77_a_single_activity_is_its_line_alone() {
        let totals = Totals {
            columns: vec!["2024".into()],
            rows: vec![row(Some(ActivityType::Unknown), vec![5.0])],
            // All activities, of which there is only the one.
            sum: Some(row(None, vec![5.0])),
        };

        let plot = plot(&totals, &view(Measure::Distance, None), "All activities");

        assert_eq!(plot.series.len(), 1);
        assert_eq!(plot.series[0].label, "Unspecified");
        assert_eq!(plot.series[0].drawn, [Some(5.0)]);
    }
}
