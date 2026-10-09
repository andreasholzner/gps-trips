//! The Tags screen (US-83): every tag in one place, so the owner can reach a
//! tag's summary, clear out tags no longer used and set one up before any
//! trip carries it.
//!
//! The archive hands over every tag at once (`GET /api/tags/overview`); the
//! filter and the paging work over that list, as the trip list's paging does
//! (US-63). Deleting, renaming (US-85) or creating a tag reads the list
//! again, since deleting one can change what the others' shares are and a
//! renamed one moves to its new alphabetical place.

use dioxus::prelude::*;
use trip_archive_types::{TagOverview, TagShare};

use crate::api::{self, ApiClient};
use crate::pager::{self, Pager};
use crate::summary::SummaryView;
use crate::Route;

mod rename;
mod view;

use rename::RenameTag;
pub use view::TagsView;

/// The tags whose names contain `query`, ignoring case and the spaces around
/// it — names are stored lowercased (US-33), so lowercasing the query is all
/// that takes.
pub fn matching(tags: &[TagOverview], query: &str) -> Vec<TagOverview> {
    let query = query.trim().to_lowercase();
    tags.iter()
        .filter(|tag| tag.name.contains(&query))
        .cloned()
        .collect()
}

/// What a share is called here: its label, or its tags without one.
fn share_name(share: &TagShare) -> String {
    match &share.label {
        Some(label) => format!("“{label}”"),
        None => share.tags.join(", "),
    }
}

/// What deleting `tag` does to `share`: narrowed to its other tags, or
/// stopped if it names no other.
fn consequence(share: &TagShare, tag: &str) -> String {
    let rest: Vec<&str> = share
        .tags
        .iter()
        .map(String::as_str)
        .filter(|name| *name != tag)
        .collect();
    let name = share_name(share);
    if rest.is_empty() {
        format!("The share {name} is stopped.")
    } else {
        format!("The share {name} is narrowed to {}.", rest.join(", "))
    }
}

/// The confirmation's question: which tag, what it comes off, and that it is
/// final.
pub fn delete_question(tag: &TagOverview) -> String {
    let trips = match tag.trip_count {
        0 => "No trip carries it.".to_string(),
        1 => "It comes off its one trip; no trip is deleted.".to_string(),
        count => format!("It comes off its {count} trips; no trip is deleted."),
    };
    format!(
        "Delete the tag “{}”? {trips} This cannot be undone.",
        tag.name
    )
}

/// `/tags` — the screen. The filter comes from the URL's query string, and
/// typing navigates there (US-52's mechanism).
#[component]
pub fn Tags(#[props(default)] view: TagsView) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut tags = use_resource(move || async move { api::tag_overview(&archive()).await });
    let reload = move |_| tags.restart();

    rsx! {
        h1 { "Tags" }
        TagFilter { query: view.q.clone() }
        match &*tags.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load the tags: {err}" } },
            Some(Ok(tags)) => rsx! {
                TagTable { tags: matching(tags, &view.q), query: view.q.clone(), on_changed: reload }
            },
        }
        CreateTag { on_created: reload }
    }
}

/// The field narrowing the table as the owner types. `replace`, as the
/// Summary screen's controls do, so typing does not fill the history.
#[component]
fn TagFilter(query: String) -> Element {
    rsx! {
        input {
            id: "tag-filter",
            r#type: "search",
            placeholder: "Find a tag",
            aria_label: "Find a tag",
            value: "{query}",
            oninput: move |event| {
                navigator().replace(Route::Tags { view: TagsView { q: event.value() } });
            },
        }
    }
}

/// The matching tags, 50 a page. Back to the first page whenever the filter
/// changes, as the trip list goes back when its filters do (US-63).
#[component]
pub fn TagTable(tags: Vec<TagOverview>, query: String, on_changed: EventHandler<()>) -> Element {
    let mut page = use_signal(|| 0_usize);
    use_effect(use_reactive!(|query| {
        let _ = query;
        page.set(0);
    }));

    if tags.is_empty() {
        let empty = if query.trim().is_empty() {
            "No tags yet.".to_string()
        } else {
            format!("No tag contains “{}”.", query.trim())
        };
        return rsx! { p { class: "muted", "{empty}" } };
    }
    let shown = tags[pager::page_range(page(), tags.len())].to_vec();
    rsx! {
        table { class: "tags",
            tbody {
                for tag in shown {
                    TagRow { key: "{tag.id}", tag, on_changed }
                }
            }
        }
        Pager { page, len: tags.len() }
    }
}

/// One tag: its name — linking to its summary when it has one — and its
/// trips, the share icon when a summary share names it, renaming it in place
/// (US-85) and deleting it.
#[component]
fn TagRow(tag: TagOverview, on_changed: EventHandler<()>) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut renaming = use_signal(|| false);
    let mut arming = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let id = tag.id;
    let shared = (!tag.shares.is_empty()).then(|| {
        let names: Vec<String> = tag.shares.iter().map(share_name).collect();
        format!("Shared in {}", names.join("; "))
    });

    rsx! {
        tr { id: "tag-{id}",
            td {
                if renaming() {
                    RenameTag {
                        id,
                        name: tag.name.clone(),
                        on_renamed: move |_| {
                            renaming.set(false);
                            on_changed.call(());
                        },
                        on_cancel: move |_| renaming.set(false),
                    }
                } else {
                    // Without a recorded trip its summary would be empty (US-78).
                    if tag.recorded_trip_count > 0 {
                        Link {
                            to: Route::Summary { view: SummaryView::of(&tag.name) },
                            "{tag.name}"
                        }
                    } else {
                        "{tag.name}"
                    }
                    " ({tag.trip_count})"
                    if let Some(title) = shared {
                        " "
                        Link {
                            to: Route::Shares {},
                            class: "tag-shared",
                            title: "{title}",
                            aria_label: "{title}",
                            "🔗"
                        }
                    }
                }
            }
            td { class: "tag-actions",
                button {
                    r#type: "button",
                    class: "quiet",
                    disabled: renaming() || arming(),
                    onclick: move |_| {
                        error.set(None);
                        renaming.set(true);
                    },
                    "Rename"
                }
                " "
                button {
                    r#type: "button",
                    class: "quiet danger",
                    disabled: renaming() || arming(),
                    onclick: move |_| {
                        error.set(None);
                        arming.set(true);
                    },
                    "Delete"
                }
            }
        }
        if arming() {
            tr {
                td { colspan: 2,
                    ConfirmDeleteTag {
                        tag: tag.clone(),
                        busy: deleting(),
                        on_confirm: move |_| async move {
                            // One delete per confirmation: a second would
                            // answer 404 for the tag the first one deleted.
                            if deleting() {
                                return;
                            }
                            deleting.set(true);
                            match api::delete_tag(&archive(), id).await {
                                Ok(()) => on_changed.call(()),
                                Err(err) => error.set(Some(err.to_string())),
                            }
                            arming.set(false);
                            deleting.set(false);
                        },
                        on_cancel: move |_| arming.set(false),
                    }
                }
            }
        }
        if let Some(message) = error() {
            tr {
                td { colspan: 2, class: "error", "Could not delete the tag: {message}" }
            }
        }
    }
}

/// The confirmation, in the page rather than a browser dialog, as deleting a
/// trip's is: the tag, what it comes off, and what becomes of each summary
/// share naming it.
#[component]
pub fn ConfirmDeleteTag(
    tag: TagOverview,
    #[props(default)] busy: bool,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "confirm",
            p { {delete_question(&tag)} }
            if !tag.shares.is_empty() {
                ul {
                    for share in tag.shares.iter() {
                        li { key: "{share.id}", {consequence(share, &tag.name)} }
                    }
                }
            }
            p {
                button {
                    r#type: "button",
                    class: "danger",
                    disabled: busy,
                    onclick: move |_| on_confirm.call(()),
                    "Delete it"
                }
                " "
                button { r#type: "button", onclick: move |_| on_cancel.call(()), "Cancel" }
            }
        }
    }
}

/// Creating a tag carrying no trips yet, below the table. The archive
/// normalizes and validates the name and says why it refuses one.
#[component]
fn CreateTag(on_created: EventHandler<()>) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut typed = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);

    rsx! {
        form {
            class: "create-tag",
            onsubmit: move |event| async move {
                event.prevent_default();
                match api::create_tag(&archive(), &typed()).await {
                    Ok(_) => {
                        typed.set(String::new());
                        error.set(None);
                        on_created.call(());
                    }
                    Err(err) => error.set(Some(err.to_string())),
                }
            },
            input {
                id: "new-tag-name",
                placeholder: "New tag",
                aria_label: "New tag",
                value: "{typed}",
                oninput: move |event| typed.set(event.value()),
            }
            button { r#type: "submit", "Create tag" }
        }
        if let Some(message) = error() {
            p { class: "error", id: "create-tag-error", "Could not create the tag: {message}" }
        }
    }
}

#[cfg(test)]
mod tests;
