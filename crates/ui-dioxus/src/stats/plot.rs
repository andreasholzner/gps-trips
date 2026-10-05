//! The totals as a chart (US-77): the same figures the totals table lists,
//! decided here and only drawn by the script (ADR-0025).
//!
//! A measure that adds up across activities is drawn as bars stacked by
//! activity, so a column's height is the activities' total. One that does
//! not — a ratio (US-80, US-81), or days out, where two activities can
//! share a date — is drawn as a line per activity with the activities
//! together dashed over them, since stacking would add up what does not.

use serde::Serialize;

use super::figures::{Totals, TotalsRow};
use super::view::{Measure, StatsView};
use crate::activity_color;

/// What a chart shows.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Plot {
    pub kind: PlotKind,
    /// The column names: the years, or the months.
    pub labels: Vec<String>,
    /// What a column is, for the cursor's readout.
    pub column: &'static str,
    /// The value axis' label.
    pub axis: &'static str,
    /// In drawing order: for bars the top of the stack first, so each lower
    /// segment is drawn over the taller ones it sits under.
    pub series: Vec<PlotSeries>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlotKind {
    Bars,
    Lines,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlotSeries {
    pub label: &'static str,
    /// The activity's color; `None` for the activities together, which are
    /// drawn in the text color.
    pub color: Option<&'static str>,
    /// Where each column's mark reaches — for bars the top of the segment,
    /// so the stack below included; `None` where there is nothing.
    pub drawn: Vec<Option<f64>>,
    /// Each column's own value as the table shows it, for the readout.
    pub shown: Vec<String>,
}

/// The chart of `totals`, whose rows and foot are named as the table names
/// them (`together` for the foot).
pub fn plot(totals: &Totals, view: &StatsView, together: &'static str) -> Plot {
    let measure = view.measure;
    let stacks = totals.rows.len() == 1 || adds_up(measure);
    let series = if stacks {
        let mut below = vec![0.0; totals.columns.len()];
        let mut series: Vec<PlotSeries> = totals
            .rows
            .iter()
            .map(|row| {
                let drawn = row
                    .values
                    .iter()
                    .zip(&mut below)
                    .map(|(value, below)| {
                        *below += value;
                        (*value != 0.0).then_some(*below)
                    })
                    .collect();
                series(row, measure, together, drawn)
            })
            .collect();
        series.reverse();
        series
    } else {
        totals
            .rows
            .iter()
            .chain(&totals.sum)
            .map(|row| {
                let drawn = row
                    .values
                    .iter()
                    .map(|value| (*value != 0.0).then_some(*value))
                    .collect();
                series(row, measure, together, drawn)
            })
            .collect()
    };
    Plot {
        kind: if stacks {
            PlotKind::Bars
        } else {
            PlotKind::Lines
        },
        labels: totals.columns.clone(),
        column: if view.year.is_some() { "Month" } else { "Year" },
        axis: measure.axis_label(),
        series,
    }
}

/// Whether the activities' values add up to theirs together.
fn adds_up(measure: Measure) -> bool {
    !measure.is_ratio() && measure != Measure::DaysOut
}

fn series(
    row: &TotalsRow,
    measure: Measure,
    together: &'static str,
    drawn: Vec<Option<f64>>,
) -> PlotSeries {
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
    fn us77_a_sum_is_stacked_by_activity_with_the_top_of_the_stack_drawn_first() {
        let plot = plot(
            &two_activities(),
            &view(Measure::Distance, None),
            "All activities",
        );

        assert_eq!(plot.kind, PlotKind::Bars);
        assert_eq!(plot.labels, ["2024", "2025"]);
        assert_eq!(plot.column, "Year");
        assert_eq!(plot.axis, "Distance (km)");
        let labels: Vec<_> = plot.series.iter().map(|series| series.label).collect();
        assert_eq!(labels, ["Cycling", "Hiking"], "no series for the sum");
        // Cycling sits on hiking; a column without hiking has no segment.
        assert_eq!(plot.series[0].drawn, [Some(90.0), Some(10.0)]);
        assert_eq!(plot.series[1].drawn, [Some(60.0), None]);
        assert_eq!(plot.series[1].color, Some(activity_color::color(Hiking)));
        // The readout shows each activity's own value, as the table does.
        assert_eq!(plot.series[0].shown, ["30.0 km", "10.0 km"]);
        assert_eq!(plot.series[1].shown, ["60.0 km", "—"]);
    }

    #[test]
    fn us80_a_ratio_is_a_line_per_activity_and_one_for_them_together() {
        let plot = plot(
            &two_activities(),
            &view(Measure::AverageSpeed, None),
            "All activities",
        );

        assert_eq!(plot.kind, PlotKind::Lines);
        let labels: Vec<_> = plot.series.iter().map(|series| series.label).collect();
        assert_eq!(labels, ["Hiking", "Cycling", "All activities"]);
        assert_eq!(plot.series[0].drawn, [Some(60.0), None], "unstacked");
        assert_eq!(plot.series[2].color, None, "drawn in the text color");
    }

    #[test]
    fn us77_days_out_do_not_stack_since_two_activities_can_share_a_date() {
        let plot = plot(
            &two_activities(),
            &view(Measure::DaysOut, Some(2024)),
            "Chosen activities",
        );

        assert_eq!(plot.kind, PlotKind::Lines);
        assert_eq!(plot.column, "Month");
        assert_eq!(plot.series.last().unwrap().label, "Chosen activities");
    }

    #[test]
    fn us77_a_single_activity_is_bars_whatever_the_measure() {
        let totals = Totals {
            columns: vec!["2024".into()],
            rows: vec![row(Some(ActivityType::Unknown), vec![5.0])],
            sum: None,
        };

        let plot = plot(
            &totals,
            &view(Measure::AverageSpeed, None),
            "All activities",
        );

        assert_eq!(plot.kind, PlotKind::Bars);
        assert_eq!(plot.series.len(), 1);
        assert_eq!(plot.series[0].label, "Unspecified");
        assert_eq!(plot.series[0].drawn, [Some(5.0)]);
    }
}
