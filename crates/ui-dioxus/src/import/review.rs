//! The review of files added to a collection, or making one of several
//! (US-88): every track and waypoint of the files with the action the
//! archive proposes, which the owner may change, and what the collection
//! holds and none of the files does, kept unless the owner removes it.
//! The archive proposes and the owner decides (ADR-0029): each change goes
//! back to the archive, which proposes again — a decision on an earlier file
//! changes what a later one is proposed — and nothing changes before the
//! owner confirms.
//!
//! The rules are plain functions — what a change makes of the decisions,
//! what a confirmation sends, how a match is described — so they are
//! checked without a browser; the component is wiring.

use dioxus::prelude::*;
use trip_archive_types::{
    CollectionImportReview, ConfirmCollectionImport, HeldItem, ImportAction, ImportDecisions,
    ItemDecision, ReviewItem,
};

use crate::api::{self, ApiClient};
use crate::collection::{CollectionFields, CollectionForm};
use crate::Route;

/// What the screen says when the archive no longer has the upload: swept
/// after a long pause, or lost to a restart.
const EXPIRED: &str = "That upload has expired — choose the files again.";

/// Which of the files' items a decision is about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Items {
    Tracks,
    Waypoints,
}

impl Items {
    fn id(self) -> &'static str {
        match self {
            Items::Tracks => "track",
            Items::Waypoints => "waypoint",
        }
    }
}

/// The decisions with the owner's choice for one file item, replacing any
/// earlier one for it.
pub fn decide(
    decisions: &ImportDecisions,
    items: Items,
    file: usize,
    index: usize,
    action: ImportAction,
) -> ImportDecisions {
    let mut decided = decisions.clone();
    let list = match items {
        Items::Tracks => &mut decided.tracks,
        Items::Waypoints => &mut decided.waypoints,
    };
    list.retain(|item| (item.file, item.index) != (file, index));
    list.push(ItemDecision {
        file,
        index,
        action,
    });
    decided
}

/// The decisions with a held track or waypoint kept or removed.
pub fn set_removed(
    decisions: &ImportDecisions,
    items: Items,
    id: i64,
    remove: bool,
) -> ImportDecisions {
    let mut decided = decisions.clone();
    let list = match items {
        Items::Tracks => &mut decided.remove_tracks,
        Items::Waypoints => &mut decided.remove_waypoints,
    };
    list.retain(|held| *held != id);
    if remove {
        list.push(id);
    }
    decided
}

/// What a confirmation sends: the decisions, what the collection held when
/// the owner decided, and a new collection's details.
pub fn confirmation(
    review: &CollectionImportReview,
    decisions: &ImportDecisions,
    form: &CollectionForm,
) -> ConfirmCollectionImport {
    let new = review.collection_id.is_none();
    let name = form.name.trim();
    ConfirmCollectionImport {
        decisions: decisions.clone(),
        held_digest: review.held_digest.clone(),
        name: (new && !name.is_empty()).then(|| name.to_string()),
        description: new.then(|| form.description.trim().to_string()),
        activity_type: new.then(|| form.activity.clone()),
    }
}

/// A new collection's details as the first file suggests them.
pub fn form_of(review: &CollectionImportReview) -> CollectionForm {
    CollectionForm {
        name: review.suggested_name.clone(),
        description: review.suggested_description.clone(),
        activity: String::new(),
    }
}

/// What an item matches, in words: one the collection holds, or one an
/// earlier file brings.
pub fn matched_text(item: &ReviewItem, files: &[String]) -> String {
    let Some(matched) = &item.matched else {
        return "—".to_string();
    };
    let name = matched.name.as_deref().unwrap_or("unnamed");
    match matched.file.and_then(|file| files.get(file)) {
        Some(file) => format!("{name} from {file}"),
        None => format!("{name} in the collection"),
    }
}

fn action_label(action: ImportAction, proposed: ImportAction) -> String {
    if action == proposed {
        format!("{} (proposed)", action.label())
    } else {
        action.label().to_string()
    }
}

/// The review with the archive behind it: each change asks for the
/// proposals again, the confirmation ends on the collection's page, and
/// cancelling — or an upload the archive no longer has — goes back to the
/// picker through `on_leave`, with what to tell the owner.
#[component]
pub(super) fn ReviewFlow(
    initial: CollectionImportReview,
    on_leave: EventHandler<Option<String>>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut review = use_signal(|| initial.clone());
    let mut decisions = use_signal(ImportDecisions::default);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let decide = move |decided: ImportDecisions| async move {
        decisions.set(decided.clone());
        busy.set(true);
        error.set(None);
        let staging_id = review.read().staging_id;
        match api::review_collection_import(&archive(), staging_id, &decided).await {
            Ok(proposed) => review.set(proposed),
            Err(err) if err.is_not_found() => on_leave.call(Some(EXPIRED.to_string())),
            Err(err) => error.set(Some(format!("Could not review the change: {err}"))),
        }
        busy.set(false);
    };

    let confirm = move |form: CollectionForm| async move {
        if busy() {
            return;
        }
        busy.set(true);
        error.set(None);
        let current = review();
        let sent = confirmation(&current, &decisions.read(), &form);
        match api::confirm_collection_import(&archive(), current.staging_id, &sent).await {
            Ok(id) => {
                navigator().push(Route::Collection { id });
            }
            Err(err) if err.is_not_found() => on_leave.call(Some(EXPIRED.to_string())),
            // The collection changed under the review: show the new one.
            Err(err) if err.is_conflict() => {
                let decided = decisions();
                match api::review_collection_import(&archive(), current.staging_id, &decided).await
                {
                    Ok(proposed) => review.set(proposed),
                    Err(again) => {
                        dioxus::logger::tracing::error!("could not review again: {again}")
                    }
                }
                error.set(Some(format!("{err}. The review above is up to date.")));
            }
            Err(err) => error.set(Some(format!("Could not import these files: {err}"))),
        }
        busy.set(false);
    };

    let cancel = move |_| async move {
        let staging_id = review.read().staging_id;
        // Left to the sweeper if the archive cannot be told.
        let _ = api::cancel_collection_import(&archive(), staging_id).await;
        on_leave.call(None);
    };

    rsx! {
        ReviewStep {
            review: review(),
            decisions: decisions(),
            error: error(),
            busy: busy(),
            on_decide: decide,
            on_confirm: confirm,
            on_cancel: cancel,
        }
    }
}

/// The review step. `busy` while the archive works on a change or the
/// confirmation, when nothing can be changed.
#[component]
pub(super) fn ReviewStep(
    review: CollectionImportReview,
    decisions: ImportDecisions,
    #[props(default)] error: Option<String>,
    #[props(default)] busy: bool,
    on_decide: EventHandler<ImportDecisions>,
    on_confirm: EventHandler<CollectionForm>,
    on_cancel: EventHandler<()>,
) -> Element {
    let form = use_signal(|| form_of(&review));
    let new = review.collection_id.is_none();
    let files = review.files.join(", ");
    let replace_all = review.replace_all;
    let decisions_for_toggle = decisions.clone();

    rsx! {
        form {
            id: "review-import",
            onsubmit: move |event| {
                event.prevent_default();
                on_confirm.call(form.read().clone());
            },
            p { id: "review-files", "From {files}." }
            if new {
                CollectionFields { form }
            } else {
                p {
                    label {
                        input {
                            id: "import-replace-all",
                            r#type: "checkbox",
                            checked: replace_all,
                            disabled: busy,
                            onchange: move |event: FormEvent| {
                                on_decide.call(ImportDecisions {
                                    replace_all: event.checked(),
                                    ..decisions_for_toggle.clone()
                                });
                            },
                        }
                        "Replace the whole collection: remove everything it holds and import "
                        "the files' contents; its details stay"
                    }
                }
            }
            ReviewTable {
                items: Items::Tracks,
                rows: review.tracks.clone(),
                files: review.files.clone(),
                decisions: decisions.clone(),
                busy,
                on_decide,
            }
            ReviewTable {
                items: Items::Waypoints,
                rows: review.waypoints.clone(),
                files: review.files.clone(),
                decisions: decisions.clone(),
                busy,
                on_decide,
            }
            if !review.held_tracks.is_empty() || !review.held_waypoints.is_empty() {
                h2 { "In the collection, in none of the files" }
                HeldTable {
                    items: Items::Tracks,
                    rows: review.held_tracks.clone(),
                    decisions: decisions.clone(),
                    busy,
                    on_decide,
                }
                HeldTable {
                    items: Items::Waypoints,
                    rows: review.held_waypoints.clone(),
                    decisions: decisions.clone(),
                    busy,
                    on_decide,
                }
            }
            button {
                id: "import-confirm",
                r#type: "submit",
                disabled: busy,
                if busy {
                    "Working…"
                } else if new {
                    "Import collection"
                } else {
                    "Add to collection"
                }
            }
            button {
                id: "import-cancel",
                r#type: "button",
                disabled: busy,
                onclick: move |_| on_cancel.call(()),
                "Cancel"
            }
        }
        if let Some(error) = error {
            p { class: "error", "{error}" }
        }
    }
}

/// The files' tracks, or their waypoints, each with its action.
#[component]
fn ReviewTable(
    items: Items,
    rows: Vec<ReviewItem>,
    files: Vec<String>,
    decisions: ImportDecisions,
    busy: bool,
    on_decide: EventHandler<ImportDecisions>,
) -> Element {
    if rows.is_empty() {
        return rsx! {};
    }
    let (heading, unnamed) = match items {
        Items::Tracks => ("Tracks", "Unnamed track"),
        Items::Waypoints => ("Waypoints", "Unnamed waypoint"),
    };
    rsx! {
        h2 { "{heading}" }
        div { class: "table-scroll",
            table { id: "review-{items.id()}s",
                thead {
                    tr {
                        th { scope: "col", "{heading}" }
                        th { scope: "col", "File" }
                        th { scope: "col", "Matches" }
                        th { scope: "col", "Action" }
                    }
                }
                tbody {
                    for item in rows {
                        tr { key: "{item.file}-{item.index}",
                            td { {item.name.clone().unwrap_or_else(|| unnamed.to_string())} }
                            td { {files.get(item.file).cloned().unwrap_or_default()} }
                            td { {matched_text(&item, &files)} }
                            td {
                                select {
                                    id: "{items.id()}-{item.file}-{item.index}",
                                    aria_label: "Action",
                                    disabled: busy,
                                    onchange: {
                                        let decisions = decisions.clone();
                                        let (file, index) = (item.file, item.index);
                                        move |event: FormEvent| {
                                            if let Some(action) = ImportAction::parse(&event.value()) {
                                                on_decide.call(decide(&decisions, items, file, index, action));
                                            }
                                        }
                                    },
                                    for choice in item.choices.clone() {
                                        option {
                                            value: choice.as_str(),
                                            selected: choice == item.action,
                                            {action_label(choice, item.proposed)}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What the collection holds and none of the files does: kept or removed.
#[component]
fn HeldTable(
    items: Items,
    rows: Vec<HeldItem>,
    decisions: ImportDecisions,
    busy: bool,
    on_decide: EventHandler<ImportDecisions>,
) -> Element {
    if rows.is_empty() {
        return rsx! {};
    }
    let (heading, unnamed) = match items {
        Items::Tracks => ("Track", "Unnamed track"),
        Items::Waypoints => ("Waypoint", "Unnamed waypoint"),
    };
    rsx! {
        div { class: "table-scroll",
            table { id: "review-held-{items.id()}s",
                thead {
                    tr {
                        th { scope: "col", "{heading}" }
                        th { scope: "col", "Action" }
                    }
                }
                tbody {
                    for held in rows {
                        tr { key: "{held.id}",
                            td { {held.name.clone().unwrap_or_else(|| unnamed.to_string())} }
                            td {
                                select {
                                    id: "held-{items.id()}-{held.id}",
                                    aria_label: "Action",
                                    disabled: busy,
                                    onchange: {
                                        let decisions = decisions.clone();
                                        let id = held.id;
                                        move |event: FormEvent| {
                                            let remove = event.value() == "remove";
                                            on_decide.call(set_removed(&decisions, items, id, remove));
                                        }
                                    },
                                    option { value: "keep", selected: !held.remove, "Keep (proposed)" }
                                    option { value: "remove", selected: held.remove, "Remove" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
#[path = "review_tests.rs"]
mod tests;
