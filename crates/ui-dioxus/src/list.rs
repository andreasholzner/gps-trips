//! The trip-list screen (US-41): browse, filter and tag trips. Each feature
//! slice moves its story's acceptance assertions over from the
//! server-rendered page (ADR-0012's migration rule).

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::TripKind;

use crate::api::{self, ApiClient};
use crate::bulk_activity::BulkActivityPanel;
use crate::bulk_tag::BulkTagPanel;
use crate::filter_bar::FilterBar;
use crate::filters::Filters;
use crate::format;
use crate::heat;
use crate::pager::{self, Pager};
use crate::region::RegionFilter;
use crate::trip_table::TripTable;

/// The `filters` prop comes from the URL's query string (US-52), so opening
/// a bookmarked or reloaded link starts the screen already narrowed — the
/// same property the server-rendered page had for free.
#[component]
pub fn TripList(#[props(default)] filters: Filters) -> Element {
    let filters = use_signal(move || filters.clone());
    let archive = use_context::<Signal<ApiClient>>();
    // Keep the address bar in step with the live filters. `replace`, not
    // `push`: filtering as you type would otherwise leave one history entry
    // per keystroke between the owner and wherever they came from.
    //
    // Reading the signal inside the effect is what subscribes it; the URL is
    // written as a plain path so this does not depend on which router the
    // screen happens to be mounted in (the test harness has its own).
    use_effect(move || {
        navigator().replace(format!("/?{}", filters.read().to_query()));
    });
    // Re-runs whenever the filters or the configured archive change —
    // reading the signals inside the closure is the whole subscription.
    let mut trips = use_resource(move || async move {
        api::list_trips(&archive(), filters.read().to_query()).await
    });
    // The known tags: the tag filter's choices (US-38) and the bulk-tag
    // suggestions (US-34). A failure here costs those choices, not the list
    // — hence the separate resource and the fallback to none.
    let mut tag_resource = use_resource(move || async move { api::list_tags(&archive()).await });
    let all_tags = tag_resource
        .read_unchecked()
        .as_ref()
        .and_then(|tags| tags.clone().ok())
        .unwrap_or_default();
    // How many trips the tab holds in all (US-61), counted from the same rows
    // the list reads — only with every filter but `kind` dropped.
    //
    // Scoped to the tab by reading a memo of `kind` rather than the filters
    // themselves: subscribing to the whole signal would re-count the archive
    // on every typed character, to arrive at the same number each time.
    let kind = use_memo(move || filters.read().kind);
    let total = use_resource(move || async move {
        let all_of_this_tab = Filters {
            kind: kind(),
            ..Default::default()
        };
        api::list_trips(&archive(), all_of_this_tab.to_query()).await
    });
    // A failure here costs the count line, not the list — the same trade the
    // tags make above.
    let total = total
        .read_unchecked()
        .as_ref()
        .and_then(|trips| trips.as_ref().ok().map(Vec::len));
    // Which trips the bulk panels will act on (US-34, US-63).
    let selected = use_signal(BTreeSet::new);
    let staged = use_signal(Vec::new);
    // Which page of the list the table shows (US-63). Back to the first
    // whenever the filters change — page 4 of a list narrowed to nine rows
    // would be a blank table — and kept out of the URL: US-52's contract is
    // about what the list *is*, and a page is where the owner is inside it.
    let mut page = use_signal(|| 0_usize);
    use_effect(move || {
        filters.read();
        page.set(0);
    });
    // Every trip the filters match, as the map's heat marks (US-63) — all of
    // them, not the page the table shows.
    let marks = match &*trips.read_unchecked() {
        Some(Ok(trips)) => Some(heat::marks(trips)),
        _ => None,
    };

    rsx! {
        h1 { "Trips" }
        FilterBar { filters, all_tags: all_tags.clone() }
        RegionFilter { filters, marks }
        if let (Some(total), Some(Ok(shown))) = (total, trips.read_unchecked().as_ref()) {
            TripCounts { shown: shown.len(), total, kind: kind(), page: page() }
        }
        // What to do with the selected trips: tag them (US-34), or give
        // them all one activity (US-63).
        div { class: "bulk-panels",
            BulkTagPanel {
                selected,
                staged,
                all_tags,
                // A new tag now exists and the trips carry it: both the
                // suggestions and a tag-filtered list are out of date.
                on_applied: move |_| {
                    tag_resource.restart();
                    trips.restart();
                },
            }
            // An activity-filtered list may no longer hold the trips.
            BulkActivityPanel { selected, on_applied: move |_| trips.restart() }
        }
        match &*trips.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load trips: {err}" } },
            Some(Ok(trips)) if trips.is_empty() => rsx! { EmptyState { filters } },
            // Select-all acts on this page's rows alone, while `selected`
            // lives here and so outlives paging: trips from several pages can
            // be acted on together (US-34, US-63).
            Some(Ok(trips)) => rsx! {
                TripTable { trips: trips[pager::page_range(page(), trips.len())].to_vec(), selected }
                Pager { page, len: trips.len() }
            },
        }
    }
}

/// How many trips the list shows, and how many the tab holds in all (US-61),
/// and — once the list is longer than a page — which of them this page shows
/// (US-63). A caption on the table rather than a heading: the numbers are
/// context for what is below, not an announcement of their own.
#[component]
fn TripCounts(shown: usize, total: usize, kind: TripKind, page: usize) -> Element {
    // An empty tab is said better by `EmptyState`, in its own words — "0
    // recorded trips" would be a second, colder way of saying the same thing.
    if total == 0 {
        return rsx! {};
    }
    let counts = format::trip_counts(shown, total, kind);
    let line = if pager::page_count(shown) > 1 {
        format!(
            "{counts} · {}",
            format::page_place(pager::page_range(page, shown))
        )
    } else {
        counts
    };
    rsx! {
        p { class: "trip-counts", "{line}" }
    }
}

/// Tells "nothing imported yet" apart from "nothing matches the filters" —
/// the same distinction the server-rendered page draws (US-13).
#[component]
fn EmptyState(filters: Signal<Filters>) -> Element {
    let filters = filters.read();
    if filters.any_set() {
        rsx! { p { "No trips match your filters." } }
    } else if filters.kind == TripKind::Planned {
        rsx! { p { "No planned trips yet." } }
    } else {
        rsx! { p { "No trips yet. Import your first trip." } }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
// Split into list/tests.rs to keep this file under the repo's 500-line cap.

#[cfg(test)]
mod tests;
