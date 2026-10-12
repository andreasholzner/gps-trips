//! The trip-list screen (US-41): browse, filter and tag trips — and, on its
//! Collections tab, find the collections (US-87). Each feature slice moves
//! its story's acceptance assertions over from the server-rendered page
//! (ADR-0012's migration rule).

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::{CollectionSummary, TripSummary, VERSION};

use crate::api::{self, ApiClient};
use crate::bulk_activity::BulkActivityPanel;
use crate::bulk_tag::BulkTagPanel;
use crate::collection_table::CollectionTable;
use crate::filter_bar::FilterBar;
use crate::filters::{Filters, ListTab};
use crate::format;
use crate::map_items::Mapped;
use crate::pager::{self, Pager};
use crate::region::RegionFilter;
use crate::share::ShareSelectedPanel;
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
    let mut trips = use_trips(filters);
    let collections = use_collections(filters);
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
    // the list reads — only with every filter but the tab dropped. On the
    // Collections tab, how many collections there are (US-87).
    //
    // Scoped to the tab by reading a memo of it rather than the filters
    // themselves: subscribing to the whole signal would re-count the archive
    // on every typed character, to arrive at the same number each time.
    //
    // Collections are counted by the archive rather than listed and counted
    // here: a collection comes with all its tracks' marks.
    let tab = use_memo(move || filters.read().tab);
    let total = use_resource(move || async move {
        let tab = tab();
        let all_of_this_tab = Filters {
            tab,
            ..Default::default()
        };
        let total = match all_of_this_tab.trip_query() {
            Some(query) => api::list_trips(&archive(), query).await.map(|t| t.len()),
            None => api::count_collections(&archive()).await,
        };
        (tab, total)
    });
    // A failure here costs the count line, not the list — the same trade the
    // tags make above.
    let total = for_tab(total.read_unchecked().as_ref(), tab())
        .and_then(|total| total.as_ref().ok().copied());
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
    // What the tab lists, once read for it — `None` until then, so a list
    // read for the tab before is neither shown nor counted.
    let listed: Option<Result<Listed, api::ApiError>> = match tab() {
        ListTab::Collections => for_tab(
            collections
                .read_unchecked()
                .as_ref()
                .and_then(Option::as_ref),
            tab(),
        )
        .map(|read| read.clone().map(Listed::Collections)),
        ListTab::Recorded | ListTab::Planned => for_tab(
            trips.read_unchecked().as_ref().and_then(Option::as_ref),
            tab(),
        )
        .map(|read| read.clone().map(Listed::Trips)),
    };
    let rows = listed.as_ref().and_then(|listed| listed.as_ref().ok());
    // Every trip the filters match, or every track of the listed
    // collections (US-87), for the map (US-63, US-73) — all of them, not the
    // page the table shows.
    let mapped = rows.map(Listed::mapped);
    let shown = rows.map(Listed::len);
    // Whether a deploy has happened since this page loaded (US-68). Until
    // the server answers, or if it cannot, nothing says so.
    let server_version = use_resource(move || async move { api::server_version(&archive()).await });
    let outdated = is_outdated(
        VERSION,
        server_version
            .read_unchecked()
            .as_ref()
            .and_then(|version| version.as_deref().ok()),
    );

    rsx! {
        div { class: "list-heading",
            h1 { "Trips" }
            VersionLabel { outdated }
        }
        FilterBar { filters, all_tags: all_tags.clone() }
        RegionFilter { filters, mapped }
        if let (Some(total), Some(shown)) = (total, shown) {
            ListCounts { shown, total, tab: tab(), page: page() }
        }
        // What to do with the selected trips: tag them (US-34), give them
        // all one activity (US-63), or share them (US-53). Trips alone: on
        // the Collections tab the selection waits, as the filters do.
        if tab() != ListTab::Collections {
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
            ShareSelectedPanel { selected }
        }
        }
        match listed {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => {
                let what = if tab() == ListTab::Collections { "collections" } else { "trips" };
                rsx! { p { class: "error", "Could not load {what}: {err}" } }
            }
            Some(Ok(listed)) if listed.len() == 0 => rsx! { EmptyState { filters } },
            // Select-all acts on this page's rows alone, while `selected`
            // lives here and so outlives paging: trips from several pages can
            // be acted on together (US-34, US-63).
            Some(Ok(Listed::Trips(trips))) => rsx! {
                TripTable { trips: trips[pager::page_range(page(), trips.len())].to_vec(), selected }
                Pager { page, len: trips.len() }
            },
            Some(Ok(Listed::Collections(collections))) => rsx! {
                CollectionTable { collections: collections[pager::page_range(page(), collections.len())].to_vec() }
                Pager { page, len: collections.len() }
            },
        }
    }
}

/// What a tab lists: trips, or collections (US-87).
enum Listed {
    Trips(Vec<TripSummary>),
    Collections(Vec<CollectionSummary>),
}

impl Listed {
    fn len(&self) -> usize {
        match self {
            Listed::Trips(trips) => trips.len(),
            Listed::Collections(collections) => collections.len(),
        }
    }

    fn mapped(&self) -> Mapped {
        match self {
            Listed::Trips(trips) => Mapped::trips(trips),
            Listed::Collections(collections) => Mapped::collections(collections),
        }
    }
}

/// What was read, if it was read for `tab`. A resource keeps its last
/// answer while it reads anew, so right after a tab switch it still holds
/// the tab before's.
fn for_tab<T>(read: Option<&(ListTab, T)>, tab: ListTab) -> Option<&T> {
    read.filter(|(read_for, _)| *read_for == tab)
        .map(|(_, read)| read)
}

/// What a list resource holds: what it read, and for which tab — or `None`
/// while its tab is not showing, so it asks the archive for nothing.
type ListResource<T> = Resource<Option<(ListTab, Result<Vec<T>, api::ApiError>)>>;

/// Every trip `filters` match. Re-runs whenever the filters or the
/// configured archive change — reading the signals inside the closure is the
/// whole subscription.
fn use_trips(filters: Signal<Filters>) -> ListResource<TripSummary> {
    let archive = use_context::<Signal<ApiClient>>();
    use_resource(move || async move {
        // Read into a local, not inline in the call: a temporary read guard
        // would live across the `.await`, and every keystroke's write during
        // the request would panic on the borrowed signal.
        let (tab, query) = {
            let filters = filters.read();
            (filters.tab, filters.trip_query()?)
        };
        Some((tab, api::list_trips(&archive(), query).await))
    })
}

/// Every collection `filters` match on the Collections tab (US-87).
fn use_collections(filters: Signal<Filters>) -> ListResource<CollectionSummary> {
    let archive = use_context::<Signal<ApiClient>>();
    use_resource(move || async move {
        let query = {
            let filters = filters.read();
            (filters.tab == ListTab::Collections).then(|| filters.collection_query())?
        };
        Some((
            ListTab::Collections,
            api::list_collections(&archive(), query).await,
        ))
    })
}

/// Whether this page was built from another version than the server's
/// (US-68). An unknown server version is no evidence of a mismatch.
fn is_outdated(client: &str, server: Option<&str>) -> bool {
    server.is_some_and(|server| server != client)
}

/// The version this page was built with (US-68): muted like the toolbar's
/// "Clear filters", red once the server runs another one — which a reload
/// fetches, since the app shell is never cached (US-67).
#[component]
fn VersionLabel(outdated: bool) -> Element {
    let class = if outdated {
        "app-version outdated"
    } else {
        "app-version"
    };
    rsx! {
        span { class, "{VERSION}" }
    }
}

/// How many trips — or collections (US-87) — the list shows, and how many
/// the tab holds in all (US-61), and — once the list is longer than a page —
/// which of them this page shows (US-63). A caption on the table rather than
/// a heading: the numbers are context for what is below, not an
/// announcement of their own.
#[component]
fn ListCounts(shown: usize, total: usize, tab: ListTab, page: usize) -> Element {
    // An empty tab is said better by `EmptyState`, in its own words — "0
    // recorded trips" would be a second, colder way of saying the same thing.
    if total == 0 {
        return rsx! {};
    }
    let counts = format::list_counts(shown, total, tab);
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
    match (filters.tab, filters.any_set()) {
        (ListTab::Collections, true) => rsx! { p { "No collections match your filters." } },
        (ListTab::Collections, false) => {
            rsx! { p { "No collections yet. Import a GPX file as a collection." } }
        }
        (_, true) => rsx! { p { "No trips match your filters." } },
        (ListTab::Planned, false) => rsx! { p { "No planned trips yet." } },
        (ListTab::Recorded, false) => rsx! { p { "No trips yet. Import your first trip." } },
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
// Split into list/tests.rs to keep this file under the repo's 500-line cap.

#[cfg(test)]
mod collections_tests;
#[cfg(test)]
mod tests;
