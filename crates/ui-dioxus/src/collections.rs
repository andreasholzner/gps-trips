//! The Collections screen (US-97): every collection on a page of its own, so
//! the owner gets to one without going through the trip list.
//!
//! The archive hands over every collection at once (`GET /api/collections`);
//! the filter and the paging work over that list, as the Tags screen's do
//! (US-83) — a collection comes with all its tracks' marks, so asking again
//! per keystroke would read them all again. For the same reason the list is
//! taken out of the answer once, and a keystroke copies only the page shown.
//!
//! Collections are selected here to be shared (US-94) or tagged (US-100)
//! together; the selection outlives the filter and the paging, as the trip
//! list's does.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::CollectionSummary;

use crate::api::{self, ApiClient, BulkTagged};
use crate::bulk_tag::BulkTagPanel;
use crate::collection_table::{CollectionTable, NO_COLLECTIONS};
use crate::name_filter::NameFilter;
use crate::pager::{self, Pager};
use crate::share::{Selected, ShareSelectedPanel};
use crate::Route;

/// The collections whose names contain `query`, ignoring case and the
/// spaces around it. Unlike tags' names (US-33), collections' keep their
/// case, so both sides are lowercased.
pub fn matching<'a>(
    collections: &'a [CollectionSummary],
    query: &str,
) -> Vec<&'a CollectionSummary> {
    let query = query.trim().to_lowercase();
    collections
        .iter()
        .filter(|collection| collection.name.to_lowercase().contains(&query))
        .collect()
}

/// `/collections` — the screen. The filter comes from the URL's query
/// string, and typing navigates there (US-52's mechanism).
#[component]
pub fn Collections(#[props(default)] view: NameFilter) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let collections =
        use_resource(move || async move { api::list_collections(&archive(), String::new()).await });
    // Copied once per answer, not once per keystroke.
    let all = use_memo(move || match &*collections.read() {
        Some(Ok(collections)) => collections.clone(),
        _ => Vec::new(),
    });
    let selected = use_signal(BTreeSet::new);
    // The tags suggested when tagging the selection.
    let mut tags = use_resource(move || async move { api::list_tags(&archive()).await });
    let all_tags = match &*tags.read_unchecked() {
        Some(Ok(tags)) => tags.clone(),
        _ => Vec::new(),
    };

    let ids: Vec<i64> = selected.read().iter().copied().collect();
    rsx! {
        h1 { "Collections" }
        CollectionFilter { query: view.q.clone() }
        div { class: "bulk-panels",
            BulkTagPanel {
                ids: ids.clone(),
                all_tags,
                of: BulkTagged::Collections,
                // A new tag may now exist: the suggestions are out of date.
                on_added: move |_| tags.restart(),
            }
            ShareSelectedPanel { ids, of: Selected::Collections }
        }
        match &*collections.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load collections: {err}" } },
            Some(Ok(_)) => rsx! {
                CollectionList { collections: all, query: view.q.clone(), selected }
            },
        }
    }
}

/// The field narrowing the table as the owner types. `replace`, as the Tags
/// screen's does, so typing does not fill the history.
#[component]
fn CollectionFilter(query: String) -> Element {
    rsx! {
        input {
            id: "collection-filter",
            r#type: "search",
            placeholder: "Find a collection",
            aria_label: "Find a collection",
            value: "{query}",
            oninput: move |event| {
                navigator().replace(Route::Collections { view: NameFilter { q: event.value() } });
            },
        }
    }
}

/// The collections `query` matches, 50 a page. Back to the first page
/// whenever the filter changes, as the trip list goes back when its filters
/// do (US-63). Select-all acts on the page's rows alone.
#[component]
pub fn CollectionList(
    collections: ReadSignal<Vec<CollectionSummary>>,
    query: String,
    selected: Signal<BTreeSet<i64>>,
) -> Element {
    let mut page = use_signal(|| 0_usize);
    use_effect(use_reactive!(|query| {
        let _ = query;
        page.set(0);
    }));

    let all = collections.read();
    let matches = matching(&all, &query);
    if matches.is_empty() {
        let empty = if query.trim().is_empty() {
            NO_COLLECTIONS.to_string()
        } else {
            format!("No collection contains “{}”.", query.trim())
        };
        return rsx! { p { class: "muted", "{empty}" } };
    }
    let len = matches.len();
    let shown: Vec<CollectionSummary> = matches[pager::page_range(page(), len)]
        .iter()
        .map(|&collection| collection.clone())
        .collect();
    rsx! {
        CollectionTable { collections: shown, selected }
        Pager { page, len }
    }
}

#[cfg(test)]
mod tests;
