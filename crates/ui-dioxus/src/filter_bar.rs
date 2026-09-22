//! The trip list's filter controls (US-13/US-32/US-38). US-14's region is
//! drawn on the map above the table ([`crate::region`], US-63).
//!
//! Split out of `list.rs` by US-61, which reshaped a stack of four full-width
//! fieldsets into one toolbar and a disclosure: the controls are a concern of
//! their own, the same way the table and the bulk-tag panel are, and the
//! screen module was near the project's file-length cap.
//!
//! Nothing here decides what a filter *means* — that is [`Filters`], which
//! stays pure and Dioxus-free. These components only show its fields and
//! write back into the one shared signal.

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, Tag, TripKind};

use crate::filters::Filters;

/// Everything the owner narrows the list with.
///
/// The split is by how often a filter is reached for, not by what it does:
/// the tab, the name search and the activity are on the toolbar, always
/// visible; the dates, the distances and the tags sit behind
/// "More filters", so the table starts near the top of a desktop screen
/// instead of below a stack of fieldsets (US-61).
#[component]
pub fn FilterBar(filters: Signal<Filters>, all_tags: Vec<Tag>) -> Element {
    rsx! {
        div { class: "filter-toolbar",
            KindTabs { filters }
            label {
                "Search "
                input {
                    r#type: "search",
                    value: "{filters.read().q}",
                    oninput: move |event| filters.write().q = event.value(),
                }
            }
            label {
                "Activity "
                select {
                    value: filters.read().activity.map_or("", |activity| activity.as_str()),
                    onchange: move |event| {
                        filters.write().activity = event.value().parse::<ActivityType>().ok();
                    },
                    option { value: "", "— any —" }
                    for activity in ActivityType::SELECTABLE {
                        option { key: "{activity}", value: activity.as_str(), "{activity.label()}" }
                    }
                }
            }
            // Clearing is on the toolbar because it resets the filters behind
            // the disclosure too: a shared link that arrives narrowed by a
            // date must be clearable without first hunting for what narrowed
            // it.
            button {
                onclick: move |_| {
                    // Clearing keeps the tab the owner is on.
                    let kind = filters.read().kind;
                    filters.set(Filters { kind, ..Default::default() });
                },
                "Clear filters"
            }
        }
        // Uncontrolled on purpose: the open state lives in the element, so it
        // survives the re-render every keystroke causes (US-61 — "stays open
        // once opened"). Binding `open` to a signal that only `ontoggle` sets
        // would leave the owner unable to close it again.
        details {
            summary { "More filters" }
            MoreFilters { filters, all_tags }
        }
    }
}

/// The Recorded/Planned tabs (US-32). Switching tabs writes only `kind`
/// into the shared signal, so every other filter is kept — the same
/// guarantee the server-rendered page's tab forms gave.
#[component]
fn KindTabs(filters: Signal<Filters>) -> Element {
    rsx! {
        nav { class: "tabs",
            for kind in TripKind::ALL {
                button {
                    key: "{kind}",
                    disabled: filters.read().kind == kind,
                    onclick: move |_| filters.write().kind = kind,
                    "{kind.label()}"
                }
            }
        }
    }
}

/// What the disclosure holds. Rendered whether it is open or not, so a
/// filter arriving in a shared link (US-52) already shows its value the
/// moment the owner opens it.
#[component]
fn MoreFilters(filters: Signal<Filters>, all_tags: Vec<Tag>) -> Element {
    rsx! {
        div { class: "filter-fields",
            label {
                "From "
                input {
                    r#type: "date",
                    value: "{filters.read().from}",
                    oninput: move |event| filters.write().from = event.value(),
                }
            }
            label {
                "To "
                input {
                    r#type: "date",
                    value: "{filters.read().to}",
                    oninput: move |event| filters.write().to = event.value(),
                }
            }
            label {
                "Min km "
                input {
                    r#type: "number",
                    min: "0",
                    value: "{filters.read().min_dist}",
                    oninput: move |event| filters.write().min_dist = event.value(),
                }
            }
            label {
                "Max km "
                input {
                    r#type: "number",
                    min: "0",
                    value: "{filters.read().max_dist}",
                    oninput: move |event| filters.write().max_dist = event.value(),
                }
            }
        }
        TagFilter { filters, all_tags }
    }
}

/// The tag filter (US-38): one checkbox per known tag; only trips carrying
/// all checked tags are listed. Nothing renders while the archive has no
/// tags at all — an empty fieldset would only raise the question of what
/// belongs in it.
#[component]
fn TagFilter(filters: Signal<Filters>, all_tags: Vec<Tag>) -> Element {
    if all_tags.is_empty() {
        return rsx! {};
    }
    rsx! {
        fieldset {
            legend { "Tags" }
            div { class: "tag-choices",
            for tag in all_tags {
                label {
                    input {
                        r#type: "checkbox",
                        checked: filters.read().tags.contains(&tag.name),
                        onchange: {
                            let name = tag.name.clone();
                            move |event: FormEvent| {
                                let mut filters = filters.write();
                                if event.checked() {
                                    if !filters.tags.contains(&name) {
                                        filters.tags.push(name.clone());
                                    }
                                } else {
                                    filters.tags.retain(|chosen| chosen != &name);
                                }
                            }
                        },
                    }
                    "{tag.name}"
                }
            }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    fn some_tags() -> Vec<Tag> {
        vec![
            Tag {
                id: 1,
                name: "alpine".to_string(),
            },
            Tag {
                id: 2,
                name: "summer".to_string(),
            },
        ]
    }

    // The Recorded/Planned tabs (US-32): both tabs offered, the active one
    // marked. Actually clicking a tab is a real event, which this layer
    // cannot dispatch — the browser layer covers the switch itself, and
    // that switching keeps the other filters is KindTabs writing only
    // `kind` into the shared signal.
    #[test]
    fn the_tabs_offer_recorded_and_planned_with_the_active_one_marked() {
        let html = render(|| {
            let filters = Signal::new(Filters::default());
            rsx! { KindTabs { filters } }
        });

        assert!(html.contains("Recorded"), "{html}");
        assert!(html.contains("Planned"), "{html}");
        assert!(
            html.contains("disabled"),
            "the active tab is not clickable: {html}"
        );
    }

    // US-61: the three controls the owner reaches for constantly are on the
    // toolbar, and the tabs are part of it rather than a row of their own.
    #[test]
    fn the_toolbar_carries_the_tabs_the_search_and_the_activity() {
        let html = render(|| {
            let filters = Signal::new(Filters {
                q: "oslo".to_string(),
                ..Default::default()
            });
            rsx! { FilterBar { filters, all_tags: some_tags() } }
        });

        let toolbar = html
            .split_once("filter-toolbar")
            .expect("no toolbar: {html}")
            .1
            .split_once("<details")
            .expect("no disclosure: {html}")
            .0;
        for expected in ["Recorded", "Planned", "Search", "Activity", "Clear filters"] {
            assert!(toolbar.contains(expected), "missing {expected}: {html}");
        }
        assert!(toolbar.contains("oslo"), "the search is prefilled: {html}");
        // The activity picker offers every selectable activity.
        assert!(toolbar.contains("Kayaking"), "{html}");
    }

    // US-61: the rest is behind the disclosure, which is what keeps the table
    // near the top of the screen.
    #[test]
    fn the_occasional_filters_sit_behind_the_disclosure() {
        let html = render(|| {
            let filters = Signal::new(Filters::default());
            rsx! { FilterBar { filters, all_tags: some_tags() } }
        });

        let disclosure = html.split_once("<details").expect("no disclosure").1;
        assert!(disclosure.contains("More filters"), "{html}");
        for expected in ["From", "To", "Min km", "Max km", "alpine"] {
            assert!(disclosure.contains(expected), "missing {expected}: {html}");
        }
        // Closed to begin with: the table starts near the top on an ordinary
        // list view.
        assert!(
            !disclosure.starts_with(" open"),
            "the disclosure starts closed: {html}"
        );
    }

    // US-61 carrying US-52: a filter that arrives in a shared link is hidden,
    // not lost — its value is rendered inside the closed disclosure and is
    // there the moment the owner opens it.
    #[test]
    fn a_hidden_filter_from_a_shared_link_still_shows_its_value() {
        let html = render(|| {
            let filters = Signal::new(Filters {
                from: "2026-07-01".to_string(),
                max_dist: "42".to_string(),
                tags: vec!["alpine".to_string()],
                ..Default::default()
            });
            rsx! { FilterBar { filters, all_tags: some_tags() } }
        });

        assert!(html.contains("2026-07-01"), "{html}");
        assert!(html.contains("42"), "{html}");
        assert!(html.contains("checked"), "the chosen tag is ticked: {html}");
    }

    // The tag filter (US-38): one checkbox per known tag, the chosen ones
    // checked. Toggling is a real event — browser layer.
    #[test]
    fn the_tag_filter_offers_every_known_tag_with_chosen_ones_checked() {
        let html = render(move || {
            let filters = Signal::new(Filters {
                tags: vec!["alpine".to_string()],
                ..Default::default()
            });
            rsx! { TagFilter { filters, all_tags: some_tags() } }
        });

        assert!(html.contains("alpine"), "{html}");
        assert!(html.contains("summer"), "{html}");
        assert!(html.contains("checked"), "{html}");
    }

    // An archive with no tags at all offers no tag filter — an empty
    // fieldset would only raise the question of what belongs in it.
    #[test]
    fn an_archive_without_tags_offers_no_tag_filter() {
        let html = render(|| {
            let filters = Signal::new(Filters::default());
            rsx! { TagFilter { filters, all_tags: Vec::new() } }
        });

        assert!(!html.contains("tag-choices"), "{html}");
    }
}
