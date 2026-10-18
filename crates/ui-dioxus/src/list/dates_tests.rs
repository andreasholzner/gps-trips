//! US-110 — the date slider's bars on the screen, against a real server
//! (ADR-0012): every trip of the kinds on, and the part of them the other
//! filters match — read without the dates while a date is set.

use super::*;
use crate::test_support::{import_sample, render_against_archive, serve_test_archive};
use trip_archive_types::ActivityType;

/// The month the sample trips are from (Jun 2024): the tallest bar, and the
/// only one with trips in it.
fn peak_bar(html: &str) -> &str {
    html.split(r#"<g class="bar"#)
        .skip(1)
        .find(|bar| bar.contains(r#"height="100""#))
        .unwrap_or_else(|| panic!("no bars: {html}"))
}

/// The height of a bar's matching part.
fn matching_part(bar: &str) -> &str {
    bar.split(r#"height=""#)
        .nth(2)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_default()
}

async fn two_trips() -> (ApiClient, tempfile::TempDir) {
    let (archive, dir) = serve_test_archive().await;
    import_sample(&archive, &[("name", "Walk"), ("activity_type", "hiking")]).await;
    import_sample(&archive, &[("name", "Ride"), ("activity_type", "cycling")]).await;
    (archive, dir)
}

#[tokio::test]
async fn us110_with_no_other_filter_set_the_whole_bar_matches() {
    let (archive, _dir) = two_trips().await;

    let html = render_against_archive(
        &archive,
        || rsx! { TripList {} },
        |html| html.contains("recorded trips") && html.contains(r#"height="100""#),
    )
    .await;

    assert_eq!(matching_part(peak_bar(&html)), "100", "{html}");
}

#[tokio::test]
async fn us110_the_lower_part_of_a_bar_is_what_the_other_filters_match() {
    let (archive, _dir) = two_trips().await;
    let cycling = Filters {
        activity: Some(ActivityType::Cycling),
        ..Default::default()
    };

    let html = render_against_archive(
        &archive,
        move || rsx! { TripList { filters: cycling.clone() } },
        |html| html.contains("1 of 2 recorded trips") && html.contains(r#"height="50""#),
    )
    .await;

    assert_eq!(matching_part(peak_bar(&html)), "50", "{html}");
}

// The list itself is narrowed by the date, so what the other filters match
// outside it comes from a read without the dates.
#[tokio::test]
async fn us110_with_a_date_set_the_bars_still_count_what_lies_outside_it() {
    let (archive, _dir) = two_trips().await;
    let later_rides = Filters {
        activity: Some(ActivityType::Cycling),
        from: "2025-01-01".to_string(),
        ..Default::default()
    };

    let html = render_against_archive(
        &archive,
        move || rsx! { TripList { filters: later_rides.clone() } },
        |html| html.contains(r#"height="50""#),
    )
    .await;

    let bar = peak_bar(&html);
    assert_eq!(matching_part(bar), "50", "{html}");
    assert!(
        bar.starts_with(r#"""#),
        "Jun 2024 lies before the handle: {html}"
    );
}
