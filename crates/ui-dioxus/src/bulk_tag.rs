//! Bulk-tagging the trips selected on the list screen (US-34): names are
//! staged first, then applied to every selected trip in one request.
//!
//! Staging is deliberately a pure function ([`stage`]) so the rules — the
//! confirm-before-creating-a-new-tag step (US-33) and what counts as a valid
//! name — are unit-tested without a browser. It reaches those rules through
//! the shared crate's `normalize_tag_name`, the same function the server
//! validates with, so a name this screen accepts is never one the server
//! rejects.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::{normalize_tag_name, Tag};

use crate::api::{self, ApiClient};

/// What typing a tag name into the panel should do.
#[derive(Debug, Clone, PartialEq)]
pub enum Staged {
    /// A tag that already exists — stage it as typed.
    Known(String),
    /// Valid, but no such tag yet: US-33's confirm-before-create step, so a
    /// typo can't quietly become a new tag.
    New(String),
    /// Already staged; nothing to do.
    Duplicate,
    /// Rejected by the shared normalizer, carrying its message.
    Invalid(String),
}

/// Decide what to do with a typed tag name, given the tags that exist and
/// the names already staged. Normalizes (trim, lowercase) exactly as the
/// server does.
pub fn stage(raw: &str, known: &[Tag], staged: &[String]) -> Staged {
    let name = match normalize_tag_name(raw) {
        Ok(name) => name,
        Err(message) => return Staged::Invalid(message),
    };
    if staged.contains(&name) {
        Staged::Duplicate
    } else if known.iter().any(|tag| tag.name == name) {
        Staged::Known(name)
    } else {
        Staged::New(name)
    }
}

/// The bulk-tag panel (US-34). Appears only once trips are selected; stages
/// names as removable chips and applies all of them to every selected trip
/// in one `POST /api/trips/tags`. `on_applied` lets the list refresh itself
/// once the request has succeeded.
#[component]
pub fn BulkTagPanel(
    selected: Signal<BTreeSet<i64>>,
    staged: Signal<Vec<String>>,
    all_tags: Vec<Tag>,
    on_applied: EventHandler<()>,
) -> Element {
    let mut typed = use_signal(String::new);
    // A new tag waiting for the owner's go-ahead (US-33).
    let mut awaiting_confirmation = use_signal(|| None::<String>);
    let mut message = use_signal(|| None::<String>);
    let archive = use_context::<Signal<ApiClient>>();

    let count = selected.read().len();
    if count == 0 {
        return rsx! {};
    }

    // Cloned out of the signal: the chips loop below writes back into it,
    // which a live read guard would deadlock on.
    let chips = staged.read().clone();
    let known = all_tags.clone();
    let mut add_typed = move || {
        let raw = typed();
        // Scoped so the read guard is gone before an arm writes back.
        let outcome = {
            let current = staged.read();
            stage(&raw, &known, &current)
        };
        match outcome {
            Staged::Known(name) => {
                staged.write().push(name);
                typed.set(String::new());
                message.set(None);
            }
            Staged::New(name) => awaiting_confirmation.set(Some(name)),
            Staged::Duplicate => typed.set(String::new()),
            Staged::Invalid(problem) => message.set(Some(problem)),
        }
    };

    rsx! {
        fieldset {
            legend { "Tag selected trips" }
            div { class: "chips",
                for name in chips {
                    span { key: "{name}", class: "chip",
                        "{name}"
                        button {
                            r#type: "button",
                            onclick: move |_| staged.write().retain(|staged| staged != &name),
                            "×"
                        }
                    }
                }
            }
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
                p {
                    "Tag \"{name}\" doesn't exist yet — create it?"
                    button {
                        r#type: "button",
                        onclick: move |_| {
                            staged.write().push(name.clone());
                            awaiting_confirmation.set(None);
                            typed.set(String::new());
                            message.set(None);
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

            button {
                r#type: "button",
                disabled: staged.read().is_empty(),
                onclick: move |_| {
                    let trip_ids: Vec<i64> = selected.read().iter().copied().collect();
                    let names = staged.read().clone();
                    spawn(async move {
                        match api::bulk_add_tags(&archive(), &trip_ids, &names).await {
                            Ok(_) => {
                                staged.write().clear();
                                selected.write().clear();
                                message.set(None);
                                on_applied.call(());
                            }
                            Err(err) => message.set(Some(err.to_string())),
                        }
                    });
                },
                "Apply to {count} selected"
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
    use std::collections::BTreeSet;
    use trip_archive_types::Tag;

    fn known_tags() -> Vec<Tag> {
        vec![Tag {
            id: 1,
            name: "alpine".to_string(),
        }]
    }

    #[test]
    fn an_existing_tag_stages_without_confirmation() {
        // Casing collapses the same way the server normalizes it (US-33).
        assert_eq!(
            stage("Alpine", &known_tags(), &[]),
            Staged::Known("alpine".to_string())
        );
    }

    #[test]
    fn an_unknown_tag_asks_before_creating_it() {
        // US-33/US-34: using a new tag creates it on demand, after
        // confirmation — the owner should never make a tag by typo.
        assert_eq!(
            stage("summer", &known_tags(), &[]),
            Staged::New("summer".to_string())
        );
    }

    #[test]
    fn a_tag_already_staged_is_not_staged_twice() {
        assert_eq!(
            stage("alpine", &known_tags(), &["alpine".to_string()]),
            Staged::Duplicate
        );
    }

    #[test]
    fn an_invalid_tag_name_is_rejected_before_it_is_sent() {
        // The same rules the server enforces (US-33/US-38), applied through
        // the shared normalizer rather than mirrored by hand — so a name the
        // client accepts can't be one the server 400s.
        assert!(matches!(
            stage("day trip", &known_tags(), &[]),
            Staged::Invalid(_)
        ));
        assert!(matches!(
            stage("day,trip", &known_tags(), &[]),
            Staged::Invalid(_)
        ));
        assert!(matches!(
            stage("   ", &known_tags(), &[]),
            Staged::Invalid(_)
        ));
    }

    #[test]
    fn the_panel_shows_the_staged_tags_and_how_many_trips_they_apply_to() {
        let html = render(|| {
            let staged = Signal::new(vec!["alpine".to_string(), "summer".to_string()]);
            let selected = Signal::new(BTreeSet::from([1_i64, 2, 3]));
            rsx! { BulkTagPanel { selected, staged, all_tags: known_tags(), on_applied: |_| {} } }
        });

        assert!(html.contains("alpine"), "{html}");
        assert!(html.contains("summer"), "{html}");
        assert!(html.contains("Apply to 3 selected"), "{html}");
    }

    #[test]
    fn the_panel_stays_out_of_the_way_until_trips_are_selected() {
        let html = render(|| {
            let staged = Signal::new(Vec::new());
            let selected = Signal::new(BTreeSet::new());
            rsx! { BulkTagPanel { selected, staged, all_tags: known_tags(), on_applied: |_| {} } }
        });

        assert!(!html.contains("Apply to"), "{html}");
    }
}
