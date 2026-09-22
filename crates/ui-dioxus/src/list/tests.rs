//! The trip-list screen's tests: its components rendered with props in
//! hand, and the whole screen against a real server (ADR-0012). The events —
//! typing, dragging, paging — are the browser layer's
//! (`tests/browser/trip_list.spec.mjs`).

use super::*;
use crate::test_support::{
    import_gpx, import_sample, render, render_against_archive, serve_test_archive, tag_trip,
    ALPS_GPX,
};
use trip_archive_types::{KomootPrivacy, TripKind};

// ── US-61's counts ───────────────────────────────────────────────────

#[test]
fn the_counts_line_reports_both_numbers() {
    let html =
        render(|| rsx! { TripCounts { shown: 1, total: 2, kind: TripKind::Recorded, page: 0 } });

    assert!(html.contains("1 of 2 recorded trips"), "{html}");
}

#[test]
fn a_list_longer_than_a_page_says_which_rows_are_shown() {
    // US-63: the owner's place, beside the counts it is part of.
    let html = render(|| {
        rsx! { TripCounts { shown: 120, total: 312, kind: TripKind::Recorded, page: 2 } }
    });

    assert!(
        html.contains("120 of 312 recorded trips · showing 101–120"),
        "{html}"
    );
}

#[test]
fn a_list_that_fits_on_a_page_needs_no_place() {
    let html = render(|| {
        rsx! { TripCounts { shown: 50, total: 50, kind: TripKind::Recorded, page: 0 } }
    });

    assert!(!html.contains("showing"), "{html}");
}

// US-63 against a real server: the table shows a page of the list, and the
// pager says which.
#[tokio::test]
async fn a_long_list_is_shown_a_page_at_a_time() {
    let (archive, _dir) = serve_test_archive().await;
    for n in 0..51 {
        import_sample(&archive, &[("name", &format!("Trip {n:02}"))]).await;
    }

    let html = render_against_archive(
        &archive,
        || rsx! { TripList {} },
        |html| html.contains("recorded trips"),
    )
    .await;

    assert_eq!(
        html.matches("<tr").count(),
        51,
        "a header and 50 rows: {html}"
    );
    assert!(html.contains("51 recorded trips · showing 1–50"), "{html}");
    assert!(html.contains("Page 1 of 2"), "{html}");
}

#[test]
fn an_empty_tab_leaves_the_counts_to_the_empty_state() {
    // "0 recorded trips" would be a second, colder way of saying what
    // `EmptyState` already says in the owner's own terms.
    let html =
        render(|| rsx! { TripCounts { shown: 0, total: 0, kind: TripKind::Recorded, page: 0 } });

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
