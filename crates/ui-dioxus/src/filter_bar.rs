//! The trip list's filter controls (US-13/US-38/US-66), and the kinds it
//! lists (US-90, once US-32's tabs). US-14's region is
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
use trip_archive_types::{ActivityType, Tag};

use crate::activity_select::{choices, ActivitySelect, Choice};
use crate::date_range::{DateRange, Months};
use crate::filters::Filters;
use crate::kinds::Kind;
use crate::linked::RecordedTripChoice;
use crate::list_table::LineSwatch;

/// Everything the owner narrows the list with.
///
/// The split is by how often a filter is reached for, not by what it does:
/// the kinds, the name search and the activity are on the toolbar, always
/// visible; the dates, the distances, the tidy-up checkboxes (US-66) and
/// the tags sit behind "More filters", so the table starts near the top of a desktop screen
/// instead of below a stack of fieldsets (US-61).
///
/// The filters behind the disclosure are about trips alone: with no kind of
/// trip on (US-87, US-90) they are not rendered, and keep their values for
/// when the owner switches one back on. `matched` says how many of each kind
/// on the filters match, once read; `months` what the date slider draws
/// (US-110).
#[component]
pub fn FilterBar(
    filters: Signal<Filters>,
    all_tags: Vec<Tag>,
    #[props(default)] matched: Vec<(Kind, usize)>,
    months: Months,
) -> Element {
    let about_trips = filters.read().kinds.has_trips();
    rsx! {
        div { class: "filter-toolbar",
            KindToggles { filters, matched }
            label {
                "Search "
                input {
                    r#type: "search",
                    value: "{filters.read().q}",
                    oninput: move |event| filters.write().q = event.value(),
                }
            }
            ActivitySelect {
                id: "filter-activity",
                label: "Activity",
                value: filters.read().activity.map_or("", |activity| activity.as_str()),
                choices: choices(&[Choice::plain("", "— any —")]),
                onchange: move |value: String| {
                    filters.write().activity = value.parse::<ActivityType>().ok();
                },
            }
            RecordedTripSelect { filters }
            // Clearing is on the toolbar because it resets the filters behind
            // the disclosure too: a shared link that arrives narrowed by a
            // date must be clearable without first hunting for what narrowed
            // it.
            button {
                onclick: move |_| {
                    let cleared = filters.read().cleared();
                    filters.set(cleared);
                },
                "Clear filters"
            }
        }
        // Uncontrolled on purpose: the open state lives in the element, so it
        // survives the re-render every keystroke causes (US-61 — "stays open
        // once opened"). Binding `open` to a signal that only `ontoggle` sets
        // would leave the owner unable to close it again.
        if about_trips {
            details {
                summary { "More filters" }
                MoreFilters { filters, all_tags, months }
            }
        }
    }
}

/// Planned trips and collection tracks with or without a recorded trip
/// linked (US-99). Offered only while one of those kinds is on: recorded
/// trips are listed as they are, so it would narrow nothing.
#[component]
fn RecordedTripSelect(filters: Signal<Filters>) -> Element {
    if !filters.read().recorded_trip_applies() {
        return rsx! {};
    }
    rsx! {
        RecordedTripChoice {
            id: "filter-recorded-trip",
            value: filters.read().recorded_trip,
            onchange: move |chosen| filters.write().recorded_trip = chosen,
        }
    }
}

/// The Recorded, Planned and Collections toggles (US-90), each showing its
/// line style on the map and how many of its kind match. Toggling writes
/// only the kinds into the shared signal, so every filter is kept; the last
/// kind on cannot be switched off.
#[component]
fn KindToggles(filters: Signal<Filters>, matched: Vec<(Kind, usize)>) -> Element {
    let kinds = filters.read().kinds;
    rsx! {
        div { class: "kind-toggles", role: "group", aria_label: "Show",
            for kind in Kind::ALL {
                button {
                    key: "{kind.as_str()}",
                    r#type: "button",
                    class: "kind-toggle",
                    aria_pressed: "{kinds.contains(kind)}",
                    disabled: kinds.is_last(kind),
                    onclick: move |_| {
                        let toggled = filters.read().kinds.toggled(kind);
                        filters.write().kinds = toggled;
                    },
                    LineSwatch { kind }
                    "{kind.label()}"
                    // Empty when off rather than left out, so its room is
                    // kept and the toggle does not change size.
                    span { class: "kind-count",
                        if let Some((_, count)) = matched.iter().find(|(k, _)| *k == kind) {
                            "{count}"
                        }
                    }
                }
            }
        }
    }
}

/// What the disclosure holds. Rendered whether it is open or not, so a
/// filter arriving in a shared link (US-52) already shows its value the
/// moment the owner opens it.
#[component]
fn MoreFilters(filters: Signal<Filters>, all_tags: Vec<Tag>, months: Months) -> Element {
    rsx! {
        div { class: "filter-fields",
            DateRange { filters, months }
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
            // The trips still to tidy up (US-66).
            label { class: "filter-check",
                input {
                    r#type: "checkbox",
                    checked: filters.read().unnamed,
                    onchange: move |event| filters.write().unnamed = event.checked(),
                }
                "Unnamed"
            }
            label { class: "filter-check",
                input {
                    r#type: "checkbox",
                    checked: filters.read().unplaced,
                    onchange: move |event| filters.write().unplaced = event.checked(),
                }
                "Unplaced photos"
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
    use trip_archive_types::RecordedTripFilter;

    /// The date slider's months before any trip is read (US-110).
    fn no_months() -> Months {
        Months::of(
            time::macros::date!(2026 - 10 - 17),
            crate::kinds::Kinds::default(),
            None,
            &[],
        )
    }

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

    // The kinds (US-90): each offered with its line style, those on pressed,
    // the last one on not to be switched off. Actually clicking one is a real
    // event, which this layer cannot dispatch — the browser layer covers the
    // toggle itself, and that toggling keeps the other filters is
    // KindToggles writing only `kinds` into the shared signal.
    #[test]
    fn us90_the_kinds_are_offered_as_toggles_the_last_one_on_kept_on() {
        let html = render(|| {
            let filters = Signal::new(Filters::default());
            rsx! { KindToggles { filters, matched: vec![(Kind::Recorded, 214)] } }
        });

        let toggle = |label: &str| {
            html.split("<button")
                .find(|button| button.contains(label))
                .unwrap_or_else(|| panic!("no {label}: {html}"))
                .to_string()
        };
        let recorded = toggle("Recorded");
        assert!(recorded.contains(r#"aria-pressed="true""#), "{html}");
        assert!(recorded.contains("disabled"), "the last one on: {html}");
        assert!(recorded.contains("214"), "how many match: {html}");
        assert!(recorded.contains(r#"stroke-dasharray="none""#), "{html}");
        let planned = toggle("Planned");
        assert!(planned.contains(r#"aria-pressed="false""#), "{html}");
        assert!(!planned.contains("disabled"), "{html}");
        assert!(
            planned.contains(r#"class="kind-count"></span>"#),
            "an empty count keeps its size when off: {html}"
        );
        assert!(planned.contains(r#"stroke-dasharray="8 6""#), "{html}");
        assert!(toggle("Collections").contains(r#"stroke-dasharray="1 7""#));
    }

    // US-61: the three controls the owner reaches for constantly are on the
    // toolbar, and the kinds are part of it rather than a row of their own.
    #[test]
    fn the_toolbar_carries_the_kinds_the_search_and_the_activity() {
        let html = render(|| {
            let filters = Signal::new(Filters {
                q: "oslo".to_string(),
                ..Default::default()
            });
            rsx! { FilterBar { filters, all_tags: some_tags(), months: no_months() } }
        });

        let toolbar = html
            .split_once("filter-toolbar")
            .expect("no toolbar: {html}")
            .1
            .split_once("<details")
            .expect("no disclosure: {html}")
            .0;
        for expected in [
            "Recorded",
            "Planned",
            "Collections",
            "Search",
            "Activity",
            "Clear filters",
        ] {
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
            rsx! { FilterBar { filters, all_tags: some_tags(), months: no_months() } }
        });

        let disclosure = html.split_once("<details").expect("no disclosure").1;
        assert!(disclosure.contains("More filters"), "{html}");
        for expected in [
            "Dates",
            "Earliest month",
            "Latest month",
            "Min km",
            "Max km",
            "Unnamed",
            "Unplaced photos",
            "alpine",
        ] {
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
            rsx! { FilterBar { filters, all_tags: some_tags(), months: no_months() } }
        });

        assert!(html.contains("Jul. 2026 – today"), "{html}");
        assert!(html.contains("42"), "{html}");
        assert!(html.contains("checked"), "the chosen tag is ticked: {html}");
    }

    // US-66: the tidy-up filters arriving in a shared link are ticked. No
    // tags here, so the only boxes that can be checked are these two.
    #[test]
    fn the_tidy_up_filters_from_a_shared_link_are_ticked() {
        for (unnamed, unplaced) in [(true, false), (false, true)] {
            let html = render(move || {
                let filters = Signal::new(Filters {
                    unnamed,
                    unplaced,
                    ..Default::default()
                });
                rsx! { FilterBar { filters, all_tags: Vec::new(), months: no_months() } }
            });

            let ticked = html
                .split("<label")
                .filter(|label| label.contains("checked"));
            let ticked: Vec<_> = ticked.collect();
            assert_eq!(ticked.len(), 1, "{html}");
            let expected = if unnamed {
                "Unnamed"
            } else {
                "Unplaced photos"
            };
            assert!(ticked[0].contains(expected), "{html}");
        }
    }

    // US-99: the recorded trip filter is offered while it can narrow
    // something — planned trips or collection tracks are on — with its
    // choice from the URL selected.
    #[test]
    fn us99_the_recorded_trip_filter_is_offered_with_planned_trips_or_collections() {
        let recorded_only = render(|| {
            let filters = Signal::new(Filters::default());
            rsx! { FilterBar { filters, all_tags: some_tags(), months: no_months() } }
        });
        let planned = render(|| {
            let filters = Signal::new(Filters {
                kinds: crate::kinds::Kinds::default().toggled(Kind::Planned),
                recorded_trip: Some(RecordedTripFilter::Without),
                ..Default::default()
            });
            rsx! { FilterBar { filters, all_tags: some_tags(), months: no_months() } }
        });

        assert!(
            !recorded_only.contains("filter-recorded-trip"),
            "{recorded_only}"
        );
        let select = planned
            .split_once(r#"id="filter-recorded-trip""#)
            .expect("the filter")
            .1;
        assert!(
            select.contains(r#"<option value="without" selected=true>"#),
            "{planned}"
        );
    }

    // US-90: with collections alone the filters about trips alone are not
    // offered, and their values wait in the signal, unrendered; the search
    // and the activity narrow collections too.
    #[test]
    fn us90_with_collections_alone_the_trip_filters_are_hidden() {
        let html = render(|| {
            let filters = Signal::new(Filters {
                kinds: crate::kinds::Kinds::only(Kind::Collections),
                q: "alp".to_string(),
                from: "2026-07-01".to_string(),
                ..Default::default()
            });
            rsx! { FilterBar { filters, all_tags: some_tags(), months: no_months() } }
        });

        for expected in ["Collections", "Search", "alp", "Activity", "Clear filters"] {
            assert!(html.contains(expected), "missing {expected}: {html}");
        }
        for hidden in ["More filters", "Jul. 2026", "alpine"] {
            assert!(!html.contains(hidden), "{hidden} is about trips: {html}");
        }
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
