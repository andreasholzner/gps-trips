//! The trip-list screen (US-41): browse, filter and tag trips. Each feature
//! slice moves its story's acceptance assertions over from the
//! server-rendered page (ADR-0012's migration rule).

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, Tag, TripKind};

use crate::api;
use crate::bulk_tag::BulkTagPanel;
use crate::filters::Filters;
use crate::trip_table::TripTable;

/// `initial` seeds the filter state — the default for the app's own route;
/// tests (and later a deep link) start the screen pre-narrowed.
#[component]
pub fn TripList(#[props(default)] initial: Filters) -> Element {
    let filters = use_signal(move || initial.clone());
    let base_url = use_context::<Signal<String>>();
    // Re-runs whenever the filters or the configured archive change —
    // reading the signals inside the closure is the whole subscription.
    let mut trips = use_resource(move || async move {
        api::list_trips(&base_url(), filters.read().to_query()).await
    });
    // The known tags: the tag filter's choices (US-38) and the bulk-tag
    // suggestions (US-34). A failure here costs those choices, not the list
    // — hence the separate resource and the fallback to none.
    let mut tag_resource = use_resource(move || async move { api::list_tags(&base_url()).await });
    let all_tags = tag_resource
        .read_unchecked()
        .as_ref()
        .and_then(|tags| tags.clone().ok())
        .unwrap_or_default();
    // Which trips the bulk-tag panel will act on (US-34).
    let selected = use_signal(BTreeSet::new);
    let staged = use_signal(Vec::new);

    rsx! {
        h1 { "Trips" }
        KindTabs { filters }
        FilterPanel { filters }
        TagFilter { filters, all_tags: all_tags.clone() }
        BulkTagPanel {
            selected,
            staged,
            all_tags,
            // A new tag now exists and the trips carry it: both the
            // suggestions and a tag-filtered list are out of date.
            on_applied: move |_| {
                tag_resource.restart();
                trips.restart();
            },
        }
        match &*trips.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load trips: {err}" } },
            Some(Ok(trips)) if trips.is_empty() => rsx! { EmptyState { filters } },
            Some(Ok(trips)) => rsx! { TripTable { trips: trips.clone(), selected } },
        }
    }
}

/// The Recorded/Planned tabs (US-32). Switching tabs writes only `kind`
/// into the shared signal, so every other filter is kept — the same
/// guarantee the server-rendered page's tab forms give.
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

/// The filter form (US-13). Every input writes straight into the shared
/// `Filters` signal, so the list re-queries as the owner types — no submit
/// button, and no separate "pending vs. applied" copy of the state.
#[component]
fn FilterPanel(filters: Signal<Filters>) -> Element {
    rsx! {
        fieldset {
            legend { "Filter" }
            div { class: "filter-fields",
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
            button {
                onclick: move |_| {
                    // Clearing keeps the tab the owner is on.
                    let kind = filters.read().kind;
                    filters.set(Filters { kind, ..Default::default() });
                },
                "Clear filters"
            }
        }
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

/// Tells "nothing imported yet" apart from "nothing matches the filters" —
/// the same distinction the server-rendered page draws (US-13).
#[component]
fn EmptyState(filters: Signal<Filters>) -> Element {
    let filters = filters.read();
    if filters.any_set() {
        rsx! { p { "No trips match your filters." } }
    } else if filters.kind == TripKind::Planned {
        rsx! { p { "No planned trips yet." } }
    } else {
        rsx! { p { "No trips yet. Import your first trip." } }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        import_sample, render, render_against_archive, serve_test_archive, tag_trip,
    };
    use trip_archive_types::{KomootPrivacy, Tag, TripKind};

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

    // US-32 against a real server: the screen defaults to the Recorded tab,
    // so a planned trip stays off it.
    #[tokio::test]
    async fn the_list_screen_defaults_to_the_recorded_tab() {
        let (base_url, _dir) = serve_test_archive().await;
        import_sample(&base_url, &[("name", "Oslo Hills Walk")]).await;
        import_sample(&base_url, &[("name", "Dream Route"), ("kind", "planned")]).await;

        let html = render_against_archive(
            &base_url,
            || rsx! { TripList {} },
            |html| html.contains("Oslo Hills Walk"),
        )
        .await;

        assert!(!html.contains("Dream Route"), "{html}");
    }

    // US-32: the Planned tab shows exactly the other partition.
    #[tokio::test]
    async fn the_planned_tab_shows_only_planned_trips() {
        let (base_url, _dir) = serve_test_archive().await;
        import_sample(&base_url, &[("name", "Oslo Hills Walk")]).await;
        import_sample(&base_url, &[("name", "Dream Route"), ("kind", "planned")]).await;

        let html = render_against_archive(
            &base_url,
            || {
                rsx! { TripList { initial: Filters { kind: TripKind::Planned, ..Default::default() } } }
            },
            |html| html.contains("Dream Route"),
        )
        .await;

        assert!(!html.contains("Oslo Hills Walk"), "{html}");
    }

    // US-32: an empty Planned tab says so, not "no trips yet".
    #[tokio::test]
    async fn an_empty_planned_tab_reports_no_planned_trips() {
        let (base_url, _dir) = serve_test_archive().await;
        import_sample(&base_url, &[]).await;

        let html = render_against_archive(
            &base_url,
            || {
                rsx! { TripList { initial: Filters { kind: TripKind::Planned, ..Default::default() } } }
            },
            |html| !html.contains("Loading"),
        )
        .await;

        assert!(html.contains("No planned trips yet."), "{html}");
    }

    // The filter form itself (US-13): every dimension the owner can narrow
    // by has its input, pre-filled with the current filter state.
    #[test]
    fn the_filter_panel_offers_every_dimension_prefilled() {
        let html = render(|| {
            let filters = Signal::new(Filters {
                q: "oslo".to_string(),
                ..Default::default()
            });
            rsx! { FilterPanel { filters } }
        });

        for label in ["Search", "Activity", "From", "To", "Min km", "Max km"] {
            assert!(html.contains(label), "missing {label}: {html}");
        }
        assert!(html.contains("oslo"), "{html}");
        // The activity picker offers every selectable activity.
        assert!(html.contains("Kayaking"), "{html}");
    }

    // US-13 against a real server: the screen's query narrows the list to
    // matching trips.
    #[tokio::test]
    async fn the_list_screen_narrows_to_matching_trips() {
        let (base_url, _dir) = serve_test_archive().await;
        import_sample(&base_url, &[("name", "Oslo Hills Walk")]).await;
        import_sample(&base_url, &[("name", "Inn Valley Ride")]).await;

        let html = render_against_archive(
            &base_url,
            || {
                rsx! { TripList { initial: Filters { q: "inn".to_string(), ..Default::default() } } }
            },
            |html| html.contains("Inn Valley Ride"),
        )
        .await;

        assert!(!html.contains("Oslo Hills Walk"), "{html}");
    }

    // US-13: a filter that matches nothing is told apart from an archive
    // with nothing in it.
    #[tokio::test]
    async fn a_filter_matching_nothing_says_so() {
        let (base_url, _dir) = serve_test_archive().await;
        import_sample(&base_url, &[]).await;

        let html = render_against_archive(
            &base_url,
            || {
                rsx! { TripList { initial: Filters { q: "nomatch".to_string(), ..Default::default() } } }
            },
            |html| !html.contains("Loading"),
        )
        .await;

        assert!(html.contains("No trips match your filters."), "{html}");
        assert!(!html.contains("No trips yet"), "{html}");
    }

    // The tag filter (US-38): one checkbox per known tag, the chosen ones
    // checked. Toggling is a real event — browser layer.
    #[test]
    fn the_tag_filter_offers_every_known_tag_with_chosen_ones_checked() {
        let all_tags = vec![
            Tag {
                id: 1,
                name: "alpine".to_string(),
            },
            Tag {
                id: 2,
                name: "summer".to_string(),
            },
        ];

        let html = render(move || {
            let filters = Signal::new(Filters {
                tags: vec!["alpine".to_string()],
                ..Default::default()
            });
            rsx! { TagFilter { filters, all_tags: all_tags.clone() } }
        });

        assert!(html.contains("alpine"), "{html}");
        assert!(html.contains("summer"), "{html}");
        assert!(html.contains("checked"), "{html}");
    }

    // US-38 against a real server: only trips carrying all chosen tags are
    // listed, and the known tags show up as filter choices.
    #[tokio::test]
    async fn the_list_screen_narrows_to_trips_with_all_chosen_tags() {
        let (base_url, _dir) = serve_test_archive().await;
        let tagged = import_sample(&base_url, &[("name", "Oslo Hills Walk")]).await;
        let partly = import_sample(&base_url, &[("name", "Inn Valley Ride")]).await;
        tag_trip(&base_url, tagged, "alpine").await;
        tag_trip(&base_url, tagged, "summer").await;
        tag_trip(&base_url, partly, "alpine").await;

        let html = render_against_archive(
            &base_url,
            || {
                rsx! { TripList { initial: Filters {
                    tags: vec!["alpine".to_string(), "summer".to_string()],
                    ..Default::default()
                } } }
            },
            |html| html.contains("Oslo Hills Walk"),
        )
        .await;

        assert!(
            !html.contains("Inn Valley Ride"),
            "one tag of two is not enough: {html}"
        );
        // The known tags are offered as choices, fetched from the server.
        assert!(html.contains("summer"), "{html}");
    }

    // US-6 as the owner sees it: an imported trip appears on the list, with
    // its stats formatted — against a real server, seeded through the real
    // import API.
    #[tokio::test]
    async fn the_list_screen_shows_an_imported_trip() {
        let (base_url, _dir) = serve_test_archive().await;
        import_sample(&base_url, &[("activity_type", "hiking")]).await;

        let html = render_against_archive(
            &base_url,
            || rsx! { TripList {} },
            |html| !html.contains("Loading"),
        )
        .await;

        assert!(html.contains("Oslo Hills Walk"), "{html}");
        assert!(html.contains(" km"), "{html}");
        assert!(html.contains("Hiking"), "{html}");
        // US-35: an imported trip never came from Komoot, so its privacy
        // cell is a dash rather than a claim.
        assert!(html.contains("Privacy"), "{html}");
        assert!(!html.contains(KomootPrivacy::Public.label()), "{html}");
    }

    // Exemplar for the whole-screen layer (ADR-0012, 2026-08-26a): the fetch
    // and the empty state the owner actually sees, against a real server —
    // nothing is mocked.
    #[tokio::test]
    async fn the_list_screen_reports_an_empty_archive() {
        let (base_url, _dir) = serve_test_archive().await;

        let html = render_against_archive(
            &base_url,
            || rsx! { TripList {} },
            |html| !html.contains("Loading"),
        )
        .await;

        assert!(html.contains("No trips yet"), "{html}");
    }
}
