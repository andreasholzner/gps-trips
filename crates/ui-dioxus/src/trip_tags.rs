//! Tagging a trip from its detail screen (US-33): the tags it carries, the
//! ones the archive already knows, and creating a new one on demand.

use dioxus::prelude::*;
use trip_archive_types::{normalize_tag_name, Tag};

use crate::api;

/// Whether using this name would create a tag that does not exist yet — the
/// question US-33 wants confirmed before it happens.
///
/// Compared after normalization, because that is what the archive stores: a
/// name typed in another casing joins the tag that exists, and confirming its
/// "creation" would be a lie. A name the archive would refuse outright is not
/// a new tag either; the archive says why when it is submitted.
pub fn is_new_name(known: &[Tag], typed: &str) -> bool {
    let Ok(name) = normalize_tag_name(typed) else {
        return false;
    };
    !known.iter().any(|tag| tag.name == name)
}

/// The tags section: what the trip carries, and a way to add more.
#[component]
pub fn TripTags(id: i64) -> Element {
    let base_url = use_context::<Signal<String>>();
    // The trip's id travels back with its tags: a resource keeps its last
    // value while a new fetch is pending, so reading the id from this scope
    // instead would let one trip's tags be cached under another's.
    let mut on_trip = use_resource(use_reactive!(|id| async move {
        api::list_trip_tags(&base_url(), id)
            .await
            .map(|tags| (id, tags))
    }));
    // Every tag the archive knows: the suggestions, and what decides whether
    // a typed name is new. Restarted with the trip's own tags, because using
    // a new name adds to both.
    let mut known = use_resource(move || async move { api::list_tags(&base_url()).await });
    let suggestions = known
        .read_unchecked()
        .as_ref()
        .and_then(|tags| tags.clone().ok())
        .unwrap_or_default();

    let mut typed = use_signal(String::new);
    let mut confirming = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    // The router shows the next trip through this same scope, so a half-typed
    // name — or worse, a confirmation waiting to be answered — would carry
    // over and land on a trip the owner never opened it for.
    use_effect(use_reactive!(|id| {
        let _ = id;
        typed.set(String::new());
        confirming.set(None);
        error.set(None);
    }));

    // Adding the name as typed, once it is either known or confirmed. The
    // request is spawned rather than returned: this is called from a plain
    // click handler, which would drop a future rather than run it.
    let add = use_callback(move |name: String| {
        spawn(async move {
            match api::add_trip_tag(&base_url(), id, &name).await {
                Ok(_) => {
                    typed.set(String::new());
                    error.set(None);
                    on_trip.restart();
                    known.restart();
                }
                Err(err) => error.set(Some(err.to_string())),
            }
            confirming.set(None);
        });
    });

    // The tags last read, kept with the trip they belong to: a re-read after
    // adding or removing one can fail, and blanking the chips would be a
    // second, invented loss on top of it.
    let mut cached = use_signal(|| (id, Vec::<Tag>::new()));
    use_effect(move || {
        if let Some(Ok(latest)) = &*on_trip.read() {
            cached.set(latest.clone());
        }
    });
    let on_this_trip = match cached() {
        (cached_id, tags) if cached_id == id => Some(tags),
        _ => None,
    };
    let load_error = match &*on_trip.read_unchecked() {
        Some(Err(err)) => Some(err.to_string()),
        _ => None,
    };

    rsx! {
        h2 { "Tags" }
        if let Some(message) = load_error {
            p { class: "error", "Could not load the tags: {message}" }
        }
        match on_this_trip {
            None => rsx! { p { "Loading the tags…" } },
            Some(tags) => rsx! {
                TagChips {
                    tags,
                    on_remove: move |tag_id| async move {
                        match api::remove_trip_tag(&base_url(), id, tag_id).await {
                            Ok(()) => {
                                error.set(None);
                                on_trip.restart();
                            }
                            Err(err) => error.set(Some(err.to_string())),
                        }
                    },
                }
            },
        }
        if let Some(name) = confirming() {
            ConfirmNewTag {
                name: name.clone(),
                on_confirm: move |_| add.call(name.clone()),
                on_cancel: move |_| confirming.set(None),
            }
        } else {
            TagInput {
                suggestions,
                value: typed(),
                on_input: move |value| typed.set(value),
                on_submit: move |_| {
                    let name = typed();
                    if name.trim().is_empty() {
                        return;
                    }
                    // A name nobody has used is created only after the owner
                    // says so (US-33); a known one is applied straight away.
                    if is_new_name(&known.read_unchecked().as_ref()
                        .and_then(|tags| tags.clone().ok())
                        .unwrap_or_default(), &name)
                    {
                        confirming.set(Some(name));
                    } else {
                        add.call(name);
                    }
                },
            }
        }
        if let Some(message) = error() {
            p { class: "error", "{message}" }
        }
    }
}

/// One removable chip per tag.
#[component]
fn TagChips(tags: Vec<Tag>, on_remove: EventHandler<i64>) -> Element {
    rsx! {
        if tags.is_empty() {
            p { "No tags yet." }
        } else {
            div { class: "chips",
                for tag in tags {
                    span { key: "{tag.id}", class: "chip",
                        "{tag.name} "
                        // A glyph, like the staged chips on the list screen
                        // — with the name spelled out for anyone who cannot
                        // see which chip it belongs to.
                        button {
                            r#type: "button",
                            onclick: move |_| on_remove.call(tag.id),
                            title: "Remove {tag.name}",
                            "×"
                        }
                    }
                }
            }
        }
    }
}

/// The add-a-tag field, with every known tag offered as the owner types.
#[component]
fn TagInput(
    suggestions: Vec<Tag>,
    value: String,
    on_input: EventHandler<String>,
    on_submit: EventHandler<()>,
) -> Element {
    rsx! {
        form {
            onsubmit: move |event| {
                event.prevent_default();
                on_submit.call(());
            },
            input {
                id: "tag-input",
                list: "tag-suggestions",
                placeholder: "add a tag",
                value: "{value}",
                oninput: move |event| on_input.call(event.value()),
            }
            datalist { id: "tag-suggestions",
                for tag in suggestions {
                    option { key: "{tag.id}", value: "{tag.name}" }
                }
            }
            button { r#type: "submit", "Add tag" }
        }
    }
}

/// The confirmation US-33 asks for before a name nobody has used becomes a
/// tag. In the page rather than a browser dialog: the decision stays in Rust,
/// it behaves the same in the Android webview, and it can be tested without
/// one.
#[component]
fn ConfirmNewTag(
    name: String,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        p { class: "confirm",
            "Create a new tag \"{name}\"? "
            button { r#type: "button", onclick: move |_| on_confirm.call(()), "Create it" }
            button { r#type: "button", onclick: move |_| on_cancel.call(()), "Cancel" }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    fn tags(names: &[&str]) -> Vec<Tag> {
        names
            .iter()
            .enumerate()
            .map(|(index, name)| Tag {
                id: index as i64 + 1,
                name: name.to_string(),
            })
            .collect()
    }

    #[test]
    fn a_name_the_archive_already_knows_is_not_a_new_tag() {
        let known = tags(&["alpine", "summer"]);

        assert!(!is_new_name(&known, "alpine"));
        // Names are normalized before they are compared, so casing and stray
        // spaces cannot make an existing tag look new — and confirm the
        // creation of one that already exists.
        assert!(!is_new_name(&known, "  ALPINE "));
    }

    #[test]
    fn a_name_nobody_has_used_is_a_new_tag() {
        assert!(is_new_name(&tags(&["alpine"]), "winter"));
    }

    #[test]
    fn a_name_the_archive_would_refuse_is_not_treated_as_new() {
        // It is not a tag at all: the archive rejects it, and asking whether
        // to create it would be asking the wrong question.
        assert!(!is_new_name(&tags(&["alpine"]), "day trip"));
        assert!(!is_new_name(&tags(&["alpine"]), "   "));
    }

    #[test]
    fn every_tag_on_the_trip_is_shown_and_can_be_taken_off() {
        let html = render(move || {
            rsx! {
                TagChips {
                    tags: tags(&["alpine", "summer"]),
                    on_remove: move |_| {},
                }
            }
        });

        assert!(html.contains("alpine"), "{html}");
        assert!(html.contains("summer"), "{html}");
        assert_eq!(
            html.matches("<button").count(),
            2,
            "one way off the trip per tag: {html}"
        );
    }

    #[test]
    fn a_trip_with_no_tags_says_so() {
        let html = render(|| {
            rsx! { TagChips { tags: Vec::new(), on_remove: move |_| {} } }
        });

        assert!(html.contains("No tags yet"), "{html}");
    }

    #[test]
    fn the_known_tags_are_offered_as_suggestions_while_typing() {
        // US-33: "the detail view suggests existing tags as the owner types."
        let html = render(move || {
            rsx! {
                TagInput {
                    suggestions: tags(&["alpine", "summer"]),
                    value: String::new(),
                    on_input: move |_| {},
                    on_submit: move |_| {},
                }
            }
        });

        assert!(html.contains("<datalist"), "{html}");
        assert!(html.contains(r#"value="alpine""#), "{html}");
        assert!(html.contains(r#"value="summer""#), "{html}");
    }

    #[test]
    fn creating_a_tag_nobody_has_used_is_confirmed_first() {
        // US-33: "using a new tag creates the tag on-demand after
        // confirmation." The confirmation is in the page, not a browser
        // dialog, so it works the same in the Android webview — and can be
        // asserted here.
        let html = render(|| {
            rsx! {
                ConfirmNewTag {
                    name: "winter".to_string(),
                    on_confirm: move |_| {},
                    on_cancel: move |_| {},
                }
            }
        });

        assert!(html.contains("winter"), "{html}");
        assert!(html.contains("Create"), "{html}");
        assert!(html.contains("Cancel"), "{html}");
        // Asked, not asserted: whether the archive knows the name is read
        // from a list that may itself have failed to load, so the prompt
        // never claims the tag does not exist.
        assert!(!html.contains("exists yet"), "{html}");
    }
}
