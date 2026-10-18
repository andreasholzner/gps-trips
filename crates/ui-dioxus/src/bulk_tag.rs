//! Bulk-tagging the trips selected on the list screen (US-34), or the
//! collections selected on either list of them (US-100): Add tags everything
//! selected with the typed name in one request — at once for a tag that
//! exists, after a confirmation for a new one (US-112).
//!
//! Deciding which is deliberately a pure function ([`classify`]) so the
//! rules — the confirm-before-creating-a-new-tag step (US-33) and what counts
//! as a valid name — are unit-tested without a browser. It reaches those
//! rules through the shared crate's `normalize_tag_name`, the same function
//! the server validates with, so a name this screen accepts is never one the
//! server rejects.

use dioxus::prelude::*;
use trip_archive_types::{normalize_tag_name, Tag};

use crate::api::{self, ApiClient, BulkTagged};

/// What adding a typed tag name should do.
#[derive(Debug, Clone, PartialEq)]
pub enum Typed {
    /// A tag that already exists — add it as typed.
    Known(String),
    /// Valid, but no such tag yet: US-33's confirm-before-create step, so a
    /// typo can't quietly become a new tag.
    New(String),
    /// Rejected by the shared normalizer, carrying its message.
    Invalid(String),
}

/// Decide what to do with a typed tag name, given the tags that exist.
/// Normalizes (trim, lowercase) exactly as the server does.
pub fn classify(raw: &str, known: &[Tag]) -> Typed {
    let name = match normalize_tag_name(raw) {
        Ok(name) => name,
        Err(message) => return Typed::Invalid(message),
    };
    if known.iter().any(|tag| tag.name == name) {
        Typed::Known(name)
    } else {
        Typed::New(name)
    }
}

/// The bulk-tag panel (US-34). Appears only once something is selected —
/// trips, or collections (US-100) as `of` says. Each tag is added to
/// everything selected as soon as it is added (US-112); the selection stays,
/// so the next tag goes onto the same items. `on_added` lets the list refresh
/// what a new tag changed.
#[component]
pub fn BulkTagPanel(
    ids: Vec<i64>,
    all_tags: Vec<Tag>,
    of: BulkTagged,
    on_added: EventHandler<()>,
) -> Element {
    let mut typed = use_signal(String::new);
    // A new tag waiting for the owner's go-ahead (US-33).
    let mut awaiting_confirmation = use_signal(|| None::<String>);
    let mut message = use_signal(|| None::<String>);
    // What the last add did, until the selection changes under it.
    let mut added = use_signal(|| None::<String>);
    use_effect(use_reactive!(|ids| {
        let _ = ids;
        added.set(None);
    }));
    let archive = use_context::<Signal<ApiClient>>();

    // Adding a name, once it is either known or confirmed. The request is
    // spawned rather than returned: this is called from a plain click
    // handler, which would drop a future rather than run it.
    let add = use_callback({
        let ids = ids.clone();
        move |name: String| {
            let ids = ids.clone();
            spawn(async move {
                match api::bulk_add_tags(&archive(), of, &ids, std::slice::from_ref(&name)).await {
                    Ok(_) => {
                        typed.set(String::new());
                        message.set(None);
                        added.set(Some(format!("Added \"{name}\" to {} selected.", ids.len())));
                        on_added.call(());
                    }
                    Err(err) => message.set(Some(err.to_string())),
                }
            });
        }
    });

    if ids.is_empty() {
        return rsx! {};
    }

    let known = all_tags.clone();
    let mut add_typed = move || match classify(&typed(), &known) {
        Typed::Known(name) => {
            awaiting_confirmation.set(None);
            add(name);
        }
        Typed::New(name) => awaiting_confirmation.set(Some(name)),
        Typed::Invalid(problem) => message.set(Some(problem)),
    };

    rsx! {
        fieldset {
            legend { "Tag selected {of.plural()}" }
            datalist { id: "bulk-tag-suggestions",
                for tag in all_tags.clone() {
                    option { key: "{tag.id}", value: "{tag.name}" }
                }
            }
            div { class: "tag-entry",
                input {
                    r#type: "text",
                    list: "bulk-tag-suggestions",
                    placeholder: "add a tag",
                    value: "{typed}",
                    oninput: move |event| typed.set(event.value()),
                }
                button { r#type: "button", onclick: move |_| add_typed(), "Add" }
            }

            if let Some(name) = awaiting_confirmation() {
                p { class: "confirm",
                    "Tag \"{name}\" doesn't exist yet — create it?"
                    button {
                        r#type: "button",
                        onclick: move |_| {
                            awaiting_confirmation.set(None);
                            add(name.clone());
                        },
                        "Create"
                    }
                    button {
                        r#type: "button",
                        onclick: move |_| awaiting_confirmation.set(None),
                        "Cancel"
                    }
                }
            }

            if let Some(added) = added() {
                p { class: "added", "{added}" }
            }
            if let Some(message) = message() {
                p { class: "error", "{message}" }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;
    use trip_archive_types::Tag;

    fn known_tags() -> Vec<Tag> {
        vec![Tag {
            id: 1,
            name: "alpine".to_string(),
        }]
    }

    #[test]
    fn an_existing_tag_is_added_without_confirmation() {
        // Casing collapses the same way the server normalizes it (US-33).
        assert_eq!(
            classify("Alpine", &known_tags()),
            Typed::Known("alpine".to_string())
        );
    }

    #[test]
    fn an_unknown_tag_asks_before_creating_it() {
        // US-33/US-34: using a new tag creates it on demand, after
        // confirmation — the owner should never make a tag by typo.
        assert_eq!(
            classify("summer", &known_tags()),
            Typed::New("summer".to_string())
        );
    }

    #[test]
    fn an_invalid_tag_name_is_rejected_before_it_is_sent() {
        // The same rules the server enforces (US-33/US-38), applied through
        // the shared normalizer rather than mirrored by hand — so a name the
        // client accepts can't be one the server 400s.
        assert!(matches!(classify("day trip", &known_tags()), Typed::Invalid(_)));
        assert!(matches!(classify("day,trip", &known_tags()), Typed::Invalid(_)));
        assert!(matches!(classify("   ", &known_tags()), Typed::Invalid(_)));
    }

    // US-112: Add tags the selection itself; there is no separate apply step.
    #[test]
    fn us112_the_panel_adds_with_no_apply_step() {
        let html = render(|| {
            let ids = vec![1_i64, 2, 3];
            rsx! { BulkTagPanel { ids, all_tags: known_tags(), of: BulkTagged::Trips, on_added: |_| {} } }
        });

        assert!(html.contains("Tag selected trips"), "{html}");
        assert!(html.contains(">Add<"), "{html}");
        assert!(!html.contains("Apply"), "{html}");
    }

    #[test]
    fn us100_the_panel_says_it_tags_the_selected_collections() {
        let html = render(|| {
            let ids = vec![1_i64, 2];
            rsx! {
                BulkTagPanel {
                    ids,
                    all_tags: known_tags(),
                    of: BulkTagged::Collections,
                    on_added: |_| {},
                }
            }
        });

        assert!(html.contains("Tag selected collections"), "{html}");
    }

    #[test]
    fn the_panel_stays_out_of_the_way_until_trips_are_selected() {
        let html = render(|| {
            let ids = Vec::<i64>::new();
            rsx! { BulkTagPanel { ids, all_tags: known_tags(), of: BulkTagged::Trips, on_added: |_| {} } }
        });

        assert!(!html.contains("Tag selected"), "{html}");
    }
}
