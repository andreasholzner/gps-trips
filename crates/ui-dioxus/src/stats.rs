//! The statistics screen (US-77): what the owner did per activity and over
//! the years. Three controls pick the period, the measure and the activity;
//! under them sit the totals, the running total through the year, and the
//! records.
//!
//! The archive hands over every dated recorded trip once
//! (`GET /api/stats/trips`); [`figures`] adds them up for whatever the
//! controls pick, so changing one asks nothing of the archive.

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, StatsTrips};

use crate::activity_color::{self, Swatch};
use crate::api::{self, ApiClient};
use crate::{format, interop};

pub(crate) mod figures;
mod tables;
mod view;
pub(crate) mod year_colors;

pub use view::{activity_order, Measure, StatsView};

use figures::{Running, MONTHS};
use tables::{RecordsTable, TotalsTable};

/// What the charts are drawn in with all activities together — the
/// elevation profile's blue, which no activity color is close to.
const ACCENT: &str = "#3367d6";

/// `/stats` — the screen. The `view` comes from the URL's query string, and
/// a control changes it by navigating there (US-52's mechanism): the URL is
/// the one place the view lives, so a bookmark opens it as it was left.
#[component]
pub fn Statistics(#[props(default)] view: StatsView) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    // Fetched once: the controls only change how the same trips are added up.
    let stats = use_resource(move || async move { api::stats_trips(&archive()).await });

    rsx! {
        h1 { "Statistics" }
        match &*stats.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load the statistics: {err}" } },
            Some(Ok(stats)) => rsx! { StatsBody { stats: stats.clone(), view } },
        }
    }
}

/// Everything under the heading, once the trips are in.
#[component]
fn StatsBody(stats: StatsTrips, view: StatsView) -> Element {
    let current = view.clone();
    let everything = figures::dated(&stats.trips, &[]);
    if everything.is_empty() {
        return rsx! { p { "No recorded trips with dates yet — the statistics count those." } };
    }
    let years = figures::years(&everything);
    let activities = figures::activities(&everything);
    let trips = figures::dated(&stats.trips, &current.activities);
    let totals = figures::totals(&trips, &current);
    let records = figures::records(&trips, &current);
    let running = figures::parse_date(&stats.today)
        .map(|today| figures::running(&trips, current.measure, current.year, today));
    let color = current.single().map_or(ACCENT, activity_color::color);
    // What the foot of the totals and the first row of the records add up.
    let together = if current.activities.is_empty() {
        "All activities"
    } else {
        "Chosen activities"
    };
    let measure = current.measure;
    let totals_heading = match current.year {
        Some(year) => format!("{} per month in {year}", measure.label()),
        None => format!("{} per year", measure.label()),
    };
    let bars = current
        .single()
        .and(totals.rows.first())
        .map(|row| row.values.clone());

    rsx! {
        Controls { view: current.clone(), years, activities }
        if stats.undated > 0 {
            p { class: "muted", id: "stats-undated", "{undated(stats.undated)}" }
        }
        section { class: "stats-section",
            h2 { "{totals_heading}" }
            if let Some(values) = bars {
                BarsChart { labels: totals.columns.clone(), values, color, label: measure.axis_label() }
            }
            TotalsTable { totals, measure, together }
        }
        if let Some(running) = running {
            section { class: "stats-section",
                h2 { "{measure.label()} through the year" }
                RunningSection { running, measure, color }
            }
        }
        section { class: "stats-section",
            h2 { "Records" }
            RecordsTable { rows: records, together }
        }
    }
}

/// What the screen says about the trips it could not count.
fn undated(count: u32) -> String {
    match count {
        1 => "One recorded trip has no dates and is not counted.".to_string(),
        count => format!("{count} recorded trips have no dates and are not counted."),
    }
}

/// Show `view` instead — `replace`, so stepping through the measures does
/// not fill the history.
fn show(view: StatsView) {
    navigator().replace(crate::Route::Statistics { view });
}

/// The period, the measure and the activity.
#[component]
fn Controls(view: StatsView, years: Vec<i32>, activities: Vec<ActivityType>) -> Element {
    let year = view.year.map_or_else(String::new, |year| year.to_string());
    let (for_year, for_measure) = (view.clone(), view.clone());
    rsx! {
        div { class: "stats-controls",
            label {
                "Period "
                select {
                    id: "stats-period",
                    value: "{year}",
                    onchange: move |event| {
                        show(StatsView { year: event.value().parse().ok(), ..for_year.clone() });
                    },
                    option { value: "", selected: view.year.is_none(), "All years" }
                    for year in years {
                        option {
                            key: "{year}",
                            value: "{year}",
                            selected: view.year == Some(year),
                            "{year}"
                        }
                    }
                }
            }
            label {
                "Measure "
                select {
                    id: "stats-measure",
                    value: view.measure.as_str(),
                    onchange: move |event| {
                        show(StatsView {
                            measure: event.value().parse().unwrap_or_default(),
                            ..for_measure.clone()
                        });
                    },
                    for measure in Measure::ALL {
                        option {
                            key: "{measure.as_str()}",
                            value: measure.as_str(),
                            selected: view.measure == measure,
                            "{measure.label()}"
                        }
                    }
                }
            }
            ActivityPicker { view: view.clone(), activities }
        }
    }
}

/// The activities, as a dropdown of checkboxes: none ticked is all of them,
/// one ticked draws its bars, several are compared in the table. A native
/// `select multiple` is a list box, not a dropdown, and holds a phone's
/// whole screen.
///
/// Open/closed is state, and a backdrop behind the open list closes it on a
/// click outside, as the header menu does (US-60). Each tick navigates to
/// the new view; the list stays open for the next one.
#[component]
fn ActivityPicker(view: StatsView, activities: Vec<ActivityType>) -> Element {
    let mut open = use_signal(|| false);
    let summary = picked(&view.activities);
    let all = view.clone();
    rsx! {
        div {
            class: "stats-activities",
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    open.set(false);
                }
            },
            span { class: "stats-control-label", "Activity" }
            button {
                r#type: "button",
                id: "stats-activity",
                class: "stats-activity-toggle",
                aria_haspopup: "true",
                aria_expanded: "{open()}",
                aria_controls: "stats-activity-list",
                onclick: move |_| open.toggle(),
                "{summary}"
            }
            if open() {
                div { class: "stats-backdrop", onclick: move |_| open.set(false) }
                fieldset { id: "stats-activity-list", class: "stats-activity-list",
                    label {
                        input {
                            r#type: "checkbox",
                            checked: view.activities.is_empty(),
                            onchange: move |_| {
                                show(StatsView { activities: Vec::new(), ..all.clone() });
                            },
                        }
                        "All activities"
                    }
                    for activity in activities {
                        ActivityChoice { key: "{activity}", view: view.clone(), activity }
                    }
                }
            }
        }
    }
}

/// One activity's checkbox, with its map color.
#[component]
fn ActivityChoice(view: StatsView, activity: ActivityType) -> Element {
    let checked = view.activities.contains(&activity);
    rsx! {
        label {
            input {
                r#type: "checkbox",
                value: activity.as_str(),
                checked,
                onchange: move |event: FormEvent| show(view.toggled(activity, event.checked())),
            }
            Swatch { color: activity_color::color(activity) }
            " {activity.label()}"
        }
    }
}

/// What the closed picker says is chosen.
fn picked(activities: &[ActivityType]) -> String {
    match activities {
        [] => "All activities".to_string(),
        [one] => one.label().to_string(),
        [first, second] => format!("{}, {}", first.label(), second.label()),
        several => format!("{} activities", several.len()),
    }
}

/// The current year so far against the year before by the same date, over
/// every year's line.
#[component]
fn RunningSection(running: Running, measure: Measure, color: &'static str) -> Element {
    let years: Vec<i32> = running.years.iter().map(|(year, _)| *year).collect();
    let series: Vec<Vec<Option<f64>>> = running
        .years
        .iter()
        .map(|(_, values)| values.clone())
        .collect();
    // A ratio has no figure before its year's first trip (US-80).
    let shown = |value: Option<f64>| {
        value.map_or_else(|| format::or_dash(None), |value| measure.format(value))
    };
    rsx! {
        p { id: "stats-headline",
            "This year so far: "
            strong { {shown(running.this_year)} }
            " — last year by the same date: "
            strong { {shown(running.last_year)} }
        }
        RunningChart {
            years,
            series,
            highlighted: running.highlighted,
            color,
            label: measure.axis_label(),
        }
    }
}

/// The chosen activity's totals as bars. The container is Dioxus-empty;
/// uPlot owns it (ADR-0025). Redrawn whenever the figures change, and the
/// handle is held so the script keeps its channel until it has drawn.
#[component]
fn BarsChart(
    labels: Vec<String>,
    values: Vec<f64>,
    color: &'static str,
    label: &'static str,
) -> Element {
    let mut handle = use_signal(|| None::<document::Eval>);
    use_effect(use_reactive!(|labels, values, color, label| {
        handle.set(Some(interop::draw_stats_bars(labels, values, color, label)));
    }));
    rsx! { div { id: "stats-bars", class: "stats-chart" } }
}

/// Every year's running total as a line, on the same terms as [`BarsChart`].
#[component]
fn RunningChart(
    years: Vec<i32>,
    series: Vec<Vec<Option<f64>>>,
    highlighted: i32,
    color: &'static str,
    label: &'static str,
) -> Element {
    let mut handle = use_signal(|| None::<document::Eval>);
    use_effect(use_reactive!(|years, series, highlighted, color, label| {
        let newest = years.iter().copied().max().unwrap_or(highlighted);
        let colors = years
            .iter()
            .map(|year| year_colors::year_color(*year, newest))
            .collect();
        handle.set(Some(interop::draw_stats_running(interop::RunningView {
            colors,
            years,
            series,
            highlighted,
            color,
            label,
            day_labels: figures::day_labels(),
            month_starts: figures::month_starts(),
            month_labels: MONTHS.to_vec(),
        })));
    }));
    rsx! { div { id: "stats-running", class: "stats-chart" } }
}

#[cfg(test)]
mod tests;
