//! The statistics screen (US-77): what the owner did per activity and over
//! the years. Three controls pick the period, the measure and the activity;
//! under them sit the totals, as a chart over their table, and the records.
//!
//! The archive hands over every dated recorded trip once
//! (`GET /api/stats/trips`); [`figures`] adds them up for whatever the
//! controls pick, so changing one asks nothing of the archive.

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, StatsTrips};

use crate::activity_icon::ActivityIcon;
use crate::api::{self, ApiClient};
use crate::interop;

pub(crate) mod figures;
mod plot;
mod tables;
mod view;

pub use view::{activity_order, Measure, StatsView};

pub(crate) use plot::Plot;
use tables::{RecordsTable, TotalsTable};

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
    let plot = plot::plot(&totals, &current, together);

    rsx! {
        Controls { view: current.clone(), years, activities }
        if stats.undated > 0 {
            p { class: "muted", id: "stats-undated", "{undated(stats.undated)}" }
        }
        section { class: "stats-section",
            h2 { "{totals_heading}" }
            TotalsChart { plot }
            details { class: "stats-table-details",
                summary { "Table" }
                TotalsTable { totals, measure, together }
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
                if let [one] = view.activities[..] {
                    ActivityIcon { activity: one }
                    " "
                }
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
            ActivityIcon { activity }
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

/// The totals as a chart. The container is Dioxus-empty; uPlot owns it
/// (ADR-0025). Redrawn whenever the figures change, and the handle is held
/// so the script keeps its channel until it has drawn.
#[component]
fn TotalsChart(plot: Plot) -> Element {
    let mut handle = use_signal(|| None::<document::Eval>);
    use_effect(use_reactive!(|plot| {
        handle.set(Some(interop::draw_stats_plot(plot)));
    }));
    rsx! { div { id: "stats-plot", class: "stats-chart" } }
}

#[cfg(test)]
mod tests;
