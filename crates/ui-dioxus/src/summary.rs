//! The Summary screen (US-78): what the trips under a tag add up to, so a
//! multi-day vacation grouped by a tag (US-33) can be looked back on as a
//! whole — or several tags side by side, to compare one with another.
//!
//! The archive hands over the chosen tags' dated recorded trips once
//! (`GET /api/stats/tags`); [`figures`] adds them up and [`lines`] decides
//! the map's lines, so everything shown is unit-tested on the host. The
//! owner can share what is shown (US-82); its recipient sees the same body,
//! with each trip opening as a shared one.

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, Tag, TagSummaries};

use crate::activity_color::{ActivityLegend, Swatch};
use crate::api::{self, ApiClient};
use crate::interop::{self, OverviewEvent};
use crate::share::{ShareDialog, ShareTarget};
use crate::Route;

mod figures;
mod lines;
mod table;
mod trips;
mod view;

pub use view::SummaryView;

use figures::TagSummary;
use table::FiguresTable;
use trips::TripsByTag;

/// Who is looking at a summary, which decides where a trip opens: the
/// owner's trip page, or the shared trip's page of the share `token`
/// (US-82).
#[derive(Clone, Debug, PartialEq)]
pub enum Viewer {
    Owner,
    Share(String),
}

impl Viewer {
    pub fn trip_route(&self, id: i64) -> Route {
        match self {
            Self::Owner => Route::TripDetail { id },
            Self::Share(token) => Route::SharedTripDetail {
                token: token.clone(),
                id,
            },
        }
    }
}

/// `/summary` — the screen. The chosen tags come from the URL's query
/// string, and choosing or removing one navigates there (US-52's
/// mechanism), so a bookmark opens the summary as it was left.
#[component]
pub fn Summary(#[props(default)] view: SummaryView) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let known = use_resource(move || async move { api::list_tags(&archive()).await });
    let tags = view.tags.clone();
    // The tags travel back with their summary: a resource keeps its last
    // value while a new fetch is pending, and the old tags' figures must not
    // stand under the new tags' names.
    let summary = use_resource(use_reactive!(|tags| async move {
        api::tag_summaries(&archive(), &tags)
            .await
            .map(|summary| (tags, summary))
    }));
    let known: Vec<Tag> = match &*known.read_unchecked() {
        Some(Ok(tags)) => tags.clone(),
        _ => Vec::new(),
    };

    rsx! {
        div { class: "summary-heading",
            h1 { "Summary" }
            TagSearch { view: view.clone(), known }
        }
        div { class: "summary-chosen-row",
            ChosenTags { view: view.clone(), removable: true }
            if !view.tags.is_empty() {
                ShareSummary { tags: view.tags.clone() }
            }
        }
        if view.tags.is_empty() {
            p { class: "muted", "Choose a tag to see what its trips add up to." }
        } else {
            match &*summary.read_unchecked() {
                Some(Ok((for_tags, summary))) if *for_tags == view.tags => rsx! {
                    SummaryBody { summary: summary.clone(), viewer: Viewer::Owner }
                },
                Some(Err(err)) => rsx! {
                    p { class: "error", "Could not load the summary: {err}" }
                },
                _ => rsx! { p { "Loading…" } },
            }
        }
    }
}

/// Show `view` instead — `replace`, as the statistics controls do, so
/// trying tags out does not fill the history.
fn show(view: SummaryView) {
    navigator().replace(Route::Summary { view });
}

/// The search box offering the known tags, beside the heading.
#[component]
fn TagSearch(view: SummaryView, known: Vec<Tag>) -> Element {
    let mut typed = use_signal(String::new);
    let mut unknown = use_signal(|| None::<String>);
    let for_add = view.clone();
    let known_names: Vec<String> = known.iter().map(|tag| tag.name.clone()).collect();
    // Only a tag the archive knows is added: a name nobody has used would
    // only ever say it has no trips.
    let add = move |name: String| {
        let name = name.trim().to_lowercase();
        if name.is_empty() {
            return;
        }
        if known_names.contains(&name) {
            unknown.set(None);
            typed.set(String::new());
            show(for_add.with(&name));
        } else {
            unknown.set(Some(name));
        }
    };
    let mut on_change = add.clone();
    let mut on_submit = add;

    rsx! {
        div { class: "summary-search",
            form {
                onsubmit: move |event| {
                    event.prevent_default();
                    on_submit(typed());
                },
                input {
                    id: "summary-tag-input",
                    r#type: "search",
                    list: "summary-tag-suggestions",
                    placeholder: "Add a tag",
                    aria_label: "Add a tag",
                    value: "{typed}",
                    oninput: move |event| typed.set(event.value()),
                    // Picking a suggestion commits the field, which is the
                    // choice; typing alone does not, so a tag whose name
                    // starts another's is not chosen on the way.
                    onchange: move |event| on_change(event.value()),
                }
                datalist { id: "summary-tag-suggestions",
                    for tag in known.iter().filter(|tag| !view.tags.contains(&tag.name)) {
                        option { key: "{tag.id}", value: "{tag.name}" }
                    }
                }
            }
            if let Some(name) = unknown() {
                p { class: "error", "No tag is called “{name}”." }
            }
        }
    }
}

/// Sharing the summary of the tags shown (US-82), in the same dialog as
/// sharing trips.
#[component]
fn ShareSummary(tags: Vec<String>) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        button {
            id: "share-summary",
            r#type: "button",
            class: "quiet",
            onclick: move |_| open.set(true),
            "Share…"
        }
        if open() {
            ShareDialog { target: ShareTarget::Tags(tags), on_close: move |_| open.set(false) }
        }
    }
}

/// The chosen tags as chips — each in its map color when there are several,
/// and removable on the owner's screen.
#[component]
pub fn ChosenTags(view: SummaryView, removable: bool) -> Element {
    let several = view.tags.len() > 1;
    rsx! {
        div { class: "chips", id: "summary-chosen",
            for (index, tag) in view.tags.iter().enumerate() {
                span { key: "{tag}", class: "chip",
                    if several {
                        Swatch { color: lines::tag_color(index) }
                        " "
                    }
                    "{tag}"
                    if removable {
                        " "
                        button {
                            r#type: "button",
                            title: "Remove {tag}",
                            onclick: {
                                let without = view.without(tag);
                                move |_| show(without.clone())
                            },
                            "×"
                        }
                    }
                }
            }
        }
    }
}

/// Everything under the tags, once their trips are in.
#[component]
pub fn SummaryBody(summary: TagSummaries, viewer: Viewer) -> Element {
    let summaries = figures::summaries(&summary);
    let several = summaries.len() > 1;
    let mut with_trips = Vec::new();
    let mut colors = Vec::new();
    for (index, tag) in summaries.iter().enumerate() {
        if let Some(figures) = &tag.figures {
            with_trips.push((tag.clone(), figures.clone()));
            if several {
                colors.push(lines::tag_color(index));
            }
        }
    }

    rsx! {
        for tag in summaries.iter().filter(|tag| tag.figures.is_none()) {
            p { key: "{tag.name}", class: "muted summary-empty",
                "No recorded trips are tagged {tag.name}."
            }
        }
        for tag in summaries.iter().filter(|tag| tag.undated > 0) {
            p { key: "undated-{tag.name}", class: "muted summary-undated", {undated(tag)} }
        }
        if !with_trips.is_empty() {
            SummaryMap { summary: summary.clone(), viewer: viewer.clone() }
            FiguresTable { tags: with_trips, colors, viewer: viewer.clone() }
            TripsByTag { summary: summary.clone(), viewer }
        }
    }
}

/// What the screen says about a tag's trips it could not count.
fn undated(tag: &TagSummary) -> String {
    match tag.undated {
        1 => format!(
            "One recorded trip tagged {} has no dates and is not counted.",
            tag.name
        ),
        count => format!(
            "{count} recorded trips tagged {} have no dates and are not counted.",
            tag.name
        ),
    }
}

/// Every trip under the chosen tags on one map, as a share's overview map
/// draws them (US-53); clicking one opens that trip as `viewer` sees it.
/// With one tag, which color is which activity sits under it (US-75); with
/// several, each tag's color sits beside its name instead.
#[component]
fn SummaryMap(summary: TagSummaries, viewer: Viewer) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut shown = use_signal(Vec::<ActivityType>::new);
    let one_tag = summary.tags.len() == 1;
    let _draw = use_resource(use_reactive!(|summary, viewer| async move {
        let ids: Vec<i64> = summary.trips.iter().map(|trip| trip.id).collect();
        // One that cannot be read is absent from the answer and leaves its
        // line off the map; a request that fails outright leaves the map
        // without lines, and the figures still stand.
        let tracks = api::list_tracks(&archive(), &ids)
            .await
            .unwrap_or_else(|err| {
                dioxus::logger::tracing::error!("could not read the tracks: {err}");
                Vec::new()
            });
        shown.set(
            summary
                .trips
                .iter()
                .filter(|trip| tracks.iter().any(|track| track.id == trip.id))
                .map(|trip| trip.activity_type)
                .collect(),
        );
        let mut map = interop::start_overview_map(lines::lines(&summary, &tracks));
        while let Ok(event) = map.recv::<OverviewEvent>().await {
            if let OverviewEvent::Open(id) = event {
                navigator().push(viewer.trip_route(id));
            }
        }
    }));
    rsx! {
        div { id: "overview-map", class: "overview-map" }
        if one_tag {
            ActivityLegend { shown: shown() }
        }
    }
}

#[cfg(test)]
mod tests;
