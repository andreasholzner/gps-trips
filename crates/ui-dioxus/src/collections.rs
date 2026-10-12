//! The Collections screen (US-97): every collection on a page of its own, so
//! the owner gets to one without going through the trip list.
//!
//! The archive hands over every collection at once (`GET /api/collections`);
//! the filter and the paging work over that list, as the Tags screen's do
//! (US-83) — a collection comes with all its tracks' marks, so asking again
//! per keystroke would read them all again.

use dioxus::prelude::*;
use trip_archive_types::CollectionSummary;

use crate::api::{self, ApiClient};
use crate::collection_table::{CollectionTable, NO_COLLECTIONS};
use crate::name_filter::NameFilter;
use crate::pager::{self, Pager};
use crate::Route;

/// The collections whose names contain `query`, ignoring case and the
/// spaces around it. Unlike tags' names (US-33), collections' keep their
/// case, so both sides are lowercased.
pub fn matching(collections: &[CollectionSummary], query: &str) -> Vec<CollectionSummary> {
    let query = query.trim().to_lowercase();
    collections
        .iter()
        .filter(|collection| collection.name.to_lowercase().contains(&query))
        .cloned()
        .collect()
}

/// `/collections` — the screen. The filter comes from the URL's query
/// string, and typing navigates there (US-52's mechanism).
#[component]
pub fn Collections(#[props(default)] view: NameFilter) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let collections =
        use_resource(move || async move { api::list_collections(&archive(), String::new()).await });

    rsx! {
        h1 { "Collections" }
        CollectionFilter { query: view.q.clone() }
        match &*collections.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load collections: {err}" } },
            Some(Ok(collections)) => rsx! {
                CollectionList { collections: matching(collections, &view.q), query: view.q.clone() }
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

/// The matching collections, 50 a page. Back to the first page whenever the
/// filter changes, as the trip list goes back when its filters do (US-63).
#[component]
pub fn CollectionList(collections: Vec<CollectionSummary>, query: String) -> Element {
    let mut page = use_signal(|| 0_usize);
    use_effect(use_reactive!(|query| {
        let _ = query;
        page.set(0);
    }));

    if collections.is_empty() {
        let empty = if query.trim().is_empty() {
            NO_COLLECTIONS.to_string()
        } else {
            format!("No collection contains “{}”.", query.trim())
        };
        return rsx! { p { class: "muted", "{empty}" } };
    }
    let shown = collections[pager::page_range(page(), collections.len())].to_vec();
    rsx! {
        CollectionTable { collections: shown }
        Pager { page, len: collections.len() }
    }
}

#[cfg(test)]
mod tests;
