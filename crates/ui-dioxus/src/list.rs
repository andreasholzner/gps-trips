//! The trip-list screen (US-41): browse, filter and tag trips. Each feature
//! slice moves its story's acceptance assertions over from the
//! server-rendered page (ADR-0012's migration rule).

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::TripKind;

use crate::api::{self, ApiClient};
use crate::bulk_tag::BulkTagPanel;
use crate::filter_bar::FilterBar;
use crate::filters::Filters;
use crate::format;
use crate::trip_table::TripTable;

/// The `filters` prop comes from the URL's query string (US-52), so opening
/// a bookmarked or reloaded link starts the screen already narrowed — the
/// same property the server-rendered page had for free.
#[component]
pub fn TripList(#[props(default)] filters: Filters) -> Element {
    let filters = use_signal(move || filters.clone());
    let archive = use_context::<Signal<ApiClient>>();
    // Keep the address bar in step with the live filters. `replace`, not
    // `push`: filtering as you type would otherwise leave one history entry
    // per keystroke between the owner and wherever they came from.
    //
    // Reading the signal inside the effect is what subscribes it; the URL is
    // written as a plain path so this does not depend on which router the
    // screen happens to be mounted in (the test harness has its own).
    use_effect(move || {
        navigator().replace(format!("/?{}", filters.read().to_query()));
    });
    // Re-runs whenever the filters or the configured archive change —
    // reading the signals inside the closure is the whole subscription.
    let mut trips = use_resource(move || async move {
        api::list_trips(&archive(), filters.read().to_query()).await
    });
    // The known tags: the tag filter's choices (US-38) and the bulk-tag
    // suggestions (US-34). A failure here costs those choices, not the list
    // — hence the separate resource and the fallback to none.
    let mut tag_resource = use_resource(move || async move { api::list_tags(&archive()).await });
    let all_tags = tag_resource
        .read_unchecked()
        .as_ref()
        .and_then(|tags| tags.clone().ok())
        .unwrap_or_default();
    // How many trips the tab holds in all (US-61), counted from the same rows
    // the list reads — only with every filter but `kind` dropped.
    //
    // Scoped to the tab by reading a memo of `kind` rather than the filters
    // themselves: subscribing to the whole signal would re-count the archive
    // on every typed character, to arrive at the same number each time.
    let kind = use_memo(move || filters.read().kind);
    let total = use_resource(move || async move {
        let all_of_this_tab = Filters {
            kind: kind(),
            ..Default::default()
        };
        api::list_trips(&archive(), all_of_this_tab.to_query()).await
    });
    // A failure here costs the count line, not the list — the same trade the
    // tags make above.
    let total = total
        .read_unchecked()
        .as_ref()
        .and_then(|trips| trips.as_ref().ok().map(Vec::len));
    // Which trips the bulk-tag panel will act on (US-34).
    let selected = use_signal(BTreeSet::new);
    let staged = use_signal(Vec::new);

    rsx! {
        h1 { "Trips" }
        FilterBar { filters, all_tags: all_tags.clone() }
        if let (Some(total), Some(Ok(shown))) = (total, trips.read_unchecked().as_ref()) {
            TripCounts { shown: shown.len(), total, kind: kind() }
        }
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

/// How many trips the list shows, and how many the tab holds in all (US-61).
/// A caption on the table rather than a heading: the numbers are context for
/// what is below, not an announcement of their own.
#[component]
fn TripCounts(shown: usize, total: usize, kind: TripKind) -> Element {
    // An empty tab is said better by `EmptyState`, in its own words — "0
    // recorded trips" would be a second, colder way of saying the same thing.
    if total == 0 {
        return rsx! {};
    }
    rsx! {
        p { class: "trip-counts", "{format::trip_counts(shown, total, kind)}" }
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
        import_gpx, import_sample, render, render_against_archive, serve_test_archive, tag_trip,
        ALPS_GPX,
    };
    use trip_archive_types::{KomootPrivacy, TripKind};

    // ── US-61's counts ───────────────────────────────────────────────────

    #[test]
    fn the_counts_line_reports_both_numbers() {
        let html = render(|| rsx! { TripCounts { shown: 1, total: 2, kind: TripKind::Recorded } });

        assert!(html.contains("1 of 2 recorded trips"), "{html}");
    }

    #[test]
    fn an_empty_tab_leaves_the_counts_to_the_empty_state() {
        // "0 recorded trips" would be a second, colder way of saying what
        // `EmptyState` already says in the owner's own terms.
        let html = render(|| rsx! { TripCounts { shown: 0, total: 0, kind: TripKind::Recorded } });

        assert!(!html.contains("trips"), "{html}");
    }

    // US-61 against a real server: a narrowed list says what it narrowed
    // from, counted from the rows the screen already reads.
    #[tokio::test]
    async fn a_narrowed_list_counts_itself_against_the_whole_tab() {
        let (archive, _dir) = serve_test_archive().await;
        import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
        import_sample(&archive, &[("name", "Inn Valley Ride")]).await;
        // On the other tab, so it counts towards neither number here.
        import_sample(&archive, &[("name", "Dream Route"), ("kind", "planned")]).await;

        let html = render_against_archive(
            &archive,
            || {
                rsx! { TripList { filters: Filters { q: "inn".to_string(), ..Default::default() } } }
            },
            // The list and the tab total are separate fetches landing in
            // either order: wait for the line that needs both.
            |html| html.contains("recorded trip"),
        )
        .await;

        assert!(html.contains("1 of 2 recorded trips"), "{html}");
    }

    // US-61: with nothing narrowing it, there is nothing to have narrowed
    // from, and the line says one number.
    #[tokio::test]
    async fn an_unnarrowed_list_counts_only_itself() {
        let (archive, _dir) = serve_test_archive().await;
        import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
        import_sample(&archive, &[("name", "Inn Valley Ride")]).await;

        let html = render_against_archive(
            &archive,
            || rsx! { TripList {} },
            |html| html.contains("recorded trip"),
        )
        .await;

        assert!(html.contains("2 recorded trips"), "{html}");
        assert!(!html.contains(" of "), "nothing narrowed it: {html}");
    }

    // US-61: the total follows the tab, and so does the noun.
    #[tokio::test]
    async fn the_counts_follow_the_selected_tab() {
        let (archive, _dir) = serve_test_archive().await;
        import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
        import_sample(&archive, &[("name", "Dream Route"), ("kind", "planned")]).await;

        let html = render_against_archive(
            &archive,
            || {
                rsx! { TripList { filters: Filters { kind: TripKind::Planned, ..Default::default() } } }
            },
            |html| html.contains("planned trip"),
        )
        .await;

        assert!(html.contains("1 planned trip"), "{html}");
    }

    // US-32 against a real server: the screen defaults to the Recorded tab,
    // so a planned trip stays off it.
    #[tokio::test]
    async fn the_list_screen_defaults_to_the_recorded_tab() {
        let (archive, _dir) = serve_test_archive().await;
        import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
        import_sample(&archive, &[("name", "Dream Route"), ("kind", "planned")]).await;

        let html = render_against_archive(
            &archive,
            || rsx! { TripList {} },
            |html| html.contains("Oslo Hills Walk"),
        )
        .await;

        assert!(!html.contains("Dream Route"), "{html}");
    }

    // US-32: the Planned tab shows exactly the other partition.
    #[tokio::test]
    async fn the_planned_tab_shows_only_planned_trips() {
        let (archive, _dir) = serve_test_archive().await;
        import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
        import_sample(&archive, &[("name", "Dream Route"), ("kind", "planned")]).await;

        let html = render_against_archive(
            &archive,
            || {
                rsx! { TripList { filters: Filters { kind: TripKind::Planned, ..Default::default() } } }
            },
            |html| html.contains("Dream Route"),
        )
        .await;

        assert!(!html.contains("Oslo Hills Walk"), "{html}");
    }

    // US-32: an empty Planned tab says so, not "no trips yet".
    #[tokio::test]
    async fn an_empty_planned_tab_reports_no_planned_trips() {
        let (archive, _dir) = serve_test_archive().await;
        import_sample(&archive, &[]).await;

        let html = render_against_archive(
            &archive,
            || {
                rsx! { TripList { filters: Filters { kind: TripKind::Planned, ..Default::default() } } }
            },
            |html| !html.contains("Loading"),
        )
        .await;

        assert!(html.contains("No planned trips yet."), "{html}");
    }

    // US-13 against a real server: the screen's query narrows the list to
    // matching trips.
    #[tokio::test]
    async fn the_list_screen_narrows_to_matching_trips() {
        let (archive, _dir) = serve_test_archive().await;
        import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
        import_sample(&archive, &[("name", "Inn Valley Ride")]).await;

        let html = render_against_archive(
            &archive,
            || {
                rsx! { TripList { filters: Filters { q: "inn".to_string(), ..Default::default() } } }
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
        let (archive, _dir) = serve_test_archive().await;
        import_sample(&archive, &[]).await;

        let html = render_against_archive(
            &archive,
            || {
                rsx! { TripList { filters: Filters { q: "nomatch".to_string(), ..Default::default() } } }
            },
            |html| !html.contains("Loading"),
        )
        .await;

        assert!(html.contains("No trips match your filters."), "{html}");
        assert!(!html.contains("No trips yet"), "{html}");
    }

    // US-38 against a real server: only trips carrying all chosen tags are
    // listed, and the known tags show up as filter choices.
    #[tokio::test]
    async fn the_list_screen_narrows_to_trips_with_all_chosen_tags() {
        let (archive, _dir) = serve_test_archive().await;
        let tagged = import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
        let partly = import_sample(&archive, &[("name", "Inn Valley Ride")]).await;
        tag_trip(&archive, tagged, "alpine").await;
        tag_trip(&archive, tagged, "summer").await;
        tag_trip(&archive, partly, "alpine").await;

        let html = render_against_archive(
            &archive,
            || {
                rsx! { TripList { filters: Filters {
                    tags: vec!["alpine".to_string(), "summer".to_string()],
                    ..Default::default()
                } } }
            },
            // The trips and the known tags are separate fetches that land in
            // either order: wait for both.
            |html| html.contains("Oslo Hills Walk") && html.contains("tag-choices"),
        )
        .await;

        assert!(
            !html.contains("Inn Valley Ride"),
            "one tag of two is not enough: {html}"
        );
        // The known tags are offered as choices, fetched from the server.
        assert!(html.contains("summer"), "{html}");
    }

    // ── US-14's region filter, moved off the server-rendered page (US-52) ──
    //
    // The rectangle is dragged in the browser layer; what belongs here is
    // what the region does to the list once chosen.

    #[tokio::test]
    async fn the_list_screen_narrows_to_trips_in_the_chosen_region() {
        let (archive, _dir) = serve_test_archive().await;
        import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
        import_gpx(&archive, ALPS_GPX, &[("name", "Inn Valley Ride")]).await;

        let html = render_against_archive(
            &archive,
            || {
                rsx! { TripList { filters: Filters {
                    bbox: "10.5,59.8,11.0,60.0".to_string(),
                    ..Default::default()
                } } }
            },
            |html| html.contains("Oslo Hills Walk"),
        )
        .await;

        assert!(!html.contains("Inn Valley Ride"), "{html}");
    }

    #[tokio::test]
    async fn a_different_region_shows_the_other_trip() {
        let (archive, _dir) = serve_test_archive().await;
        import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
        import_gpx(&archive, ALPS_GPX, &[("name", "Inn Valley Ride")]).await;

        let html = render_against_archive(
            &archive,
            || {
                rsx! { TripList { filters: Filters {
                    bbox: "11.2,47.1,11.6,47.4".to_string(),
                    ..Default::default()
                } } }
            },
            |html| html.contains("Inn Valley Ride"),
        )
        .await;

        assert!(!html.contains("Oslo Hills Walk"), "{html}");
    }

    #[tokio::test]
    async fn a_region_containing_no_trips_shows_the_filtered_empty_state() {
        // Not "No trips yet": the archive has trips, this region has none.
        let (archive, _dir) = serve_test_archive().await;
        import_sample(&archive, &[]).await;

        let html = render_against_archive(
            &archive,
            || {
                rsx! { TripList { filters: Filters {
                    bbox: "-30.0,30.0,-20.0,40.0".to_string(),
                    ..Default::default()
                } } }
            },
            |html| !html.contains("Loading"),
        )
        .await;

        assert!(html.contains("No trips match your filters."), "{html}");
        assert!(!html.contains("No trips yet"), "{html}");
    }

    #[tokio::test]
    async fn the_region_combines_with_the_other_filters_as_and() {
        // A trip inside the region but failing another filter is not listed.
        let (archive, _dir) = serve_test_archive().await;
        import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;

        let html = render_against_archive(
            &archive,
            || {
                rsx! { TripList { filters: Filters {
                    bbox: "10.5,59.8,11.0,60.0".to_string(),
                    q: "nomatch".to_string(),
                    ..Default::default()
                } } }
            },
            |html| !html.contains("Loading"),
        )
        .await;

        assert!(html.contains("No trips match your filters."), "{html}");
        assert!(!html.contains("Oslo Hills Walk"), "{html}");
    }

    // US-6 as the owner sees it: an imported trip appears on the list, with
    // its stats formatted — against a real server, seeded through the real
    // import API.
    #[tokio::test]
    async fn the_list_screen_shows_an_imported_trip() {
        let (archive, _dir) = serve_test_archive().await;
        import_sample(&archive, &[("activity_type", "hiking")]).await;

        let html = render_against_archive(
            &archive,
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
        let (archive, _dir) = serve_test_archive().await;

        let html = render_against_archive(
            &archive,
            || rsx! { TripList {} },
            |html| !html.contains("Loading"),
        )
        .await;

        assert!(html.contains("No trips yet"), "{html}");
    }
}
