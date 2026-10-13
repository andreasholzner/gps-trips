//! The trip-list screen (US-41): browse, filter and tag trips — and find the
//! collections (US-87) — listed and mapped together, chosen by kind (US-90).
//! Each feature slice moves its story's acceptance assertions over from the
//! server-rendered page (ADR-0012's migration rule).

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::{CollectionSummary, TripKind, TripSummary, VERSION};

use crate::api::{self, ApiClient};
use crate::collection_table::NO_COLLECTIONS;
use crate::filter_bar::FilterBar;
use crate::filters::Filters;
use crate::format;
use crate::kinds::{Kind, Kinds};
use crate::list_table::{self, ListTable};
use crate::map_items::Mapped;
use crate::pager::{self, Pager};
use crate::picked::{self, Picked};
use crate::region::RegionFilter;
use crate::selection_bar::SelectionBar;
use crate::share::Selected;

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
    let mut collections = use_collections(filters);
    let kinds = use_memo(move || filters.read().kinds);
    // The known tags: the tag filter's choices (US-38) and the bulk-tag
    // suggestions (US-34). A failure here costs those choices, not the list
    // — hence the separate resource and the fallback to none.
    let mut tag_resource = use_resource(move || async move { api::list_tags(&archive()).await });
    let all_tags = tag_resource
        .read_unchecked()
        .as_ref()
        .and_then(|tags| tags.clone().ok())
        .unwrap_or_default();
    // How many of each kind on there are in all (US-61), counted with every
    // filter dropped: the trips of both kinds from one unfiltered read, the
    // collections by the archive — a listed collection comes with its
    // tracks' marks.
    //
    // Each read only when its side is switched on: scoped by memos rather
    // than the filters themselves, so neither a typed character nor
    // toggling the other side re-counts the archive, to arrive at the same
    // numbers each time.
    let has_trips = use_memo(move || kinds().has_trips());
    let has_collections = use_memo(move || kinds().has_collections());
    let trip_count = use_resource(move || async move {
        if !has_trips() {
            return None;
        }
        Some(
            api::list_trips(&archive(), String::new())
                .await
                .map(|trips| trip_totals(&trips)),
        )
    });
    let collection_count = use_resource(move || async move {
        if !has_collections() {
            return None;
        }
        Some(api::count_collections(&archive()).await)
    });
    // A failure here costs the count line, not the list — the same trade the
    // tags make above.
    let totals = totals(
        kinds(),
        trip_count
            .read_unchecked()
            .clone()
            .flatten()
            .and_then(Result::ok),
        collection_count
            .read_unchecked()
            .clone()
            .flatten()
            .and_then(Result::ok),
    );
    // What is selected (US-90): trips, collections and collection tracks,
    // across pages. Turning a kind off drops what it listed.
    let mut picked = use_signal(BTreeSet::<Picked>::new);
    use_effect(move || {
        let kinds = kinds();
        picked.with_mut(|picked| picked::retain_kinds(picked, kinds));
    });
    // The names staged to tag the selection with (US-34, US-100).
    let staged = use_signal(Vec::new);
    // The collections whose tracks are folded out: none to begin with.
    let expanded = use_signal(BTreeSet::<i64>::new);
    // Which page of the list the table shows (US-63). Back to the first
    // whenever the filters change — page 4 of a list narrowed to nine rows
    // would be a blank table — and kept out of the URL: US-52's contract is
    // about what the list *is*, and a page is where the owner is inside it.
    let mut page = use_signal(|| 0_usize);
    use_effect(move || {
        filters.read();
        page.set(0);
    });
    // What the kinds on list, once all of it is read for them — `None`
    // until then, so a list read for other kinds is neither shown nor
    // counted.
    let listed = listed(
        kinds(),
        read_for(
            trips.read_unchecked().as_ref().and_then(Option::as_ref),
            &kinds().trips_only(),
        )
        .cloned(),
        collections.read_unchecked().clone().flatten(),
    );
    let rows = listed.as_ref().and_then(|listed| listed.as_ref().ok());
    // Everything that matches, for the map (US-63, US-73) — all of it, not
    // the page the table shows.
    let mapped = rows.map(Listed::mapped);
    let matched = rows.map(|rows| rows.matched(kinds())).unwrap_or_default();
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
        FilterBar { filters, all_tags: all_tags.clone(), matched: matched.clone() }
        RegionFilter { filters, mapped }
        if let (Some(rows), Some(totals)) = (rows, totals) {
            ListCounts { counts: counts(&matched, &totals), rows: rows.len(), page: page() }
        }
        // What to do with the selection: tag it (US-34/100), share it
        // (US-53/94) or set the trips' activity (US-63).
        SelectionBar {
            picked,
            staged,
            all_tags,
            on_applied: move |of: Selected| {
                // A new tag may now exist: the suggestions are out of date.
                tag_resource.restart();
                // A tag- or activity-filtered list may no longer hold them.
                match of {
                    Selected::Trips => trips.restart(),
                    Selected::Collections | Selected::CollectionTracks => collections.restart(),
                }
            },
        }
        match listed {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load the list: {err}" } },
            Some(Ok(listed)) if listed.len() == 0 => rsx! { EmptyState { filters } },
            // Select-all acts on this page's rows alone, while `picked` lives
            // here and so outlives paging: rows from several pages can be
            // acted on together (US-34, US-63).
            Some(Ok(listed)) => {
                let (trip_rows, collection_rows) =
                    list_table::page_rows(page(), listed.trips.len(), listed.collections.len());
                rsx! {
                    ListTable {
                        trips: listed.trips[trip_rows].to_vec(),
                        collections: listed.collections[collection_rows].to_vec(),
                        picked,
                        expanded,
                    }
                    Pager { page, len: listed.len() }
                }
            }
        }
    }
}

/// What the kinds on list: the matching trips, newest first, and the
/// matching collections by name (US-90).
#[derive(Clone, Debug, Default, PartialEq)]
struct Listed {
    trips: Vec<TripSummary>,
    collections: Vec<CollectionSummary>,
}

impl Listed {
    /// The rows the table pages over: trips and collections, never the
    /// tracks inside a group.
    fn len(&self) -> usize {
        self.trips.len() + self.collections.len()
    }

    fn mapped(&self) -> Mapped {
        Mapped::of(&self.trips, &self.collections)
    }

    /// How many of each kind on match.
    fn matched(&self, kinds: Kinds) -> Vec<(Kind, usize)> {
        kinds
            .iter()
            .map(|kind| {
                let count = match kind.trip_kind() {
                    Some(trip_kind) => self
                        .trips
                        .iter()
                        .filter(|t| t.trip_kind == trip_kind)
                        .count(),
                    None => self.collections.len(),
                };
                (kind, count)
            })
            .collect()
    }
}

/// The list, once every part the kinds need is read: the trips with a kind
/// of trip on, the collections with collections on. A failure in either is
/// the list's.
fn listed(
    kinds: Kinds,
    trips: Option<Result<Vec<TripSummary>, api::ApiError>>,
    collections: Option<Result<Vec<CollectionSummary>, api::ApiError>>,
) -> Option<Result<Listed, api::ApiError>> {
    let trips = if kinds.has_trips() {
        trips?
    } else {
        Ok(Vec::new())
    };
    let collections = if kinds.has_collections() {
        collections?
    } else {
        Ok(Vec::new())
    };
    Some(trips.and_then(|trips| collections.map(|collections| Listed { trips, collections })))
}

/// How many of each kind on there are in all, whatever the filters.
#[derive(Clone, Debug, PartialEq)]
struct Totals(Vec<(Kind, usize)>);

/// How many trips of each kind there are in all.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TripTotals {
    recorded: usize,
    planned: usize,
}

/// The trips counted by kind, from one read of them all.
fn trip_totals(trips: &[TripSummary]) -> TripTotals {
    let planned = trips
        .iter()
        .filter(|t| t.trip_kind == TripKind::Planned)
        .count();
    TripTotals {
        recorded: trips.len() - planned,
        planned,
    }
}

/// The totals of the kinds on — or `None` until every side they need is
/// counted.
fn totals(kinds: Kinds, trips: Option<TripTotals>, collections: Option<usize>) -> Option<Totals> {
    kinds
        .iter()
        .map(|kind| {
            let total = match kind {
                Kind::Recorded => trips?.recorded,
                Kind::Planned => trips?.planned,
                Kind::Collections => collections?,
            };
            Some((kind, total))
        })
        .collect::<Option<Vec<_>>>()
        .map(Totals)
}

/// Each kind's line in the counts: how many match out of how many there
/// are. A kind with none at all is left out — "0 planned trips" says
/// nothing the toggle does not.
fn counts(matched: &[(Kind, usize)], totals: &Totals) -> Vec<(Kind, usize, usize)> {
    totals
        .0
        .iter()
        .filter(|(_, total)| *total > 0)
        .map(|&(kind, total)| {
            let shown = matched
                .iter()
                .find(|(k, _)| *k == kind)
                .map_or(0, |(_, shown)| *shown);
            (kind, shown, total)
        })
        .collect()
}

/// What was read, if it was read for `key`. A resource keeps its last answer
/// while it reads anew, so right after a toggle it still holds what the
/// kinds before asked for.
fn read_for<'a, K: PartialEq, T>(read: Option<&'a (K, T)>, key: &K) -> Option<&'a T> {
    read.filter(|(read_for, _)| read_for == key)
        .map(|(_, read)| read)
}

/// What a list resource read.
type Read<T> = Result<Vec<T>, api::ApiError>;

/// Every trip `filters` match, with a kind of trip on, and the kinds of trip
/// it was read for. Re-runs only when the trip query changes — not when the
/// collections are toggled, nor for a filter that is not the trips' — by
/// reading a memo of that query rather than the filters themselves.
fn use_trips(filters: Signal<Filters>) -> Resource<Option<(Option<Kinds>, Read<TripSummary>)>> {
    let archive = use_context::<Signal<ApiClient>>();
    let query = use_memo(move || {
        let filters = filters.read();
        Some((filters.kinds.trips_only(), filters.trip_query()?))
    });
    use_resource(move || async move {
        let (read_for, query) = query()?;
        Some((read_for, api::list_trips(&archive(), query).await))
    })
}

/// Every collection `filters` match, with collections on (US-87, US-90).
/// Re-runs only when the collections' own query changes, not for a filter
/// about trips alone.
fn use_collections(filters: Signal<Filters>) -> Resource<Option<Read<CollectionSummary>>> {
    let archive = use_context::<Signal<ApiClient>>();
    let query = use_memo(move || {
        let filters = filters.read();
        filters
            .kinds
            .has_collections()
            .then(|| filters.collection_query())
    });
    use_resource(move || async move {
        let query = query()?;
        Some(api::list_collections(&archive(), query).await)
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

/// How many of each kind on the list shows, and how many there are in all
/// (US-61, US-90), and — once the list is longer than a page — which of its
/// rows this page shows (US-63). A caption on the table rather than a
/// heading: the numbers are context for what is below, not an announcement
/// of their own.
#[component]
fn ListCounts(counts: Vec<(Kind, usize, usize)>, rows: usize, page: usize) -> Element {
    // An empty list is said better by `EmptyState`, in its own words — "0
    // recorded trips" would be a second, colder way of saying the same thing.
    if counts.is_empty() {
        return rsx! {};
    }
    let counts = counts
        .iter()
        .map(|&(kind, shown, total)| format::list_counts(shown, total, kind))
        .collect::<Vec<_>>()
        .join(" · ");
    let line = if pager::page_count(rows) > 1 {
        format!(
            "{counts} · {}",
            format::page_place(pager::page_range(page, rows))
        )
    } else {
        counts
    };
    rsx! {
        p { class: "trip-counts", "{line}" }
    }
}

/// Tells "nothing imported yet" apart from "nothing matches the filters" —
/// the same distinction the server-rendered page draws (US-13) — in the
/// words of the kinds on.
#[component]
fn EmptyState(filters: Signal<Filters>) -> Element {
    let filters = filters.read();
    let kinds = filters.kinds;
    let collections_only = kinds == Kinds::only(Kind::Collections);
    match (filters.any_set(), kinds.has_trips(), collections_only) {
        (true, _, true) => rsx! { p { "No collections match your filters." } },
        (true, true, _) if !kinds.has_collections() => {
            rsx! { p { "No trips match your filters." } }
        }
        (true, _, _) => rsx! { p { "Nothing matches your filters." } },
        (false, _, true) => rsx! { p { "{NO_COLLECTIONS}" } },
        (false, _, _)
            if kinds.trip_kind() == Some(TripKind::Planned) && !kinds.has_collections() =>
        {
            rsx! { p { "No planned trips yet." } }
        }
        (false, _, _) => rsx! { p { "No trips yet. Import your first trip." } },
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
// Split into list/tests.rs to keep this file under the repo's 500-line cap.

#[cfg(test)]
mod collections_tests;
#[cfg(test)]
mod kinds_tests;
#[cfg(test)]
mod tests;
