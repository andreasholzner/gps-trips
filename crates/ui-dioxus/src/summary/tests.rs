//! The Summary screen against a real server (ADR-0012). What the figures
//! are is `figures`' tests'; here, that the screen shows them for the tags
//! it was opened with. Choosing a tag and the map's lines are the browser
//! layer's (`tests/browser/summary.spec.mjs`).

use super::*;
use crate::test_support::{
    import_gpx, import_sample, render_against_archive, serve_test_archive, tag_trip,
};

const UNTIMED_GPX: &[u8] = include_bytes!("../../../../tests/fixtures/untimed.gpx");

fn view(tags: &[&str]) -> SummaryView {
    tags.iter()
        .fold(SummaryView::default(), |view, tag| view.with(tag))
}

#[tokio::test]
async fn us78_without_a_tag_the_screen_asks_for_one_and_offers_the_known_tags() {
    let (archive, _dir) = serve_test_archive().await;
    let id = import_sample(&archive, &[]).await;
    tag_trip(&archive, id, "alps").await;

    let html = render_against_archive(
        &archive,
        || rsx! { Summary {} },
        |html| html.contains(r#"value="alps""#),
    )
    .await;

    assert!(html.contains("Choose a tag"), "{html}");
    assert!(html.contains(r#"id="summary-tag-input""#), "{html}");
    assert!(!html.contains("summary-figures"), "{html}");
}

#[tokio::test]
async fn us78_one_tag_shows_its_figures_its_map_and_what_was_left_out() {
    let (archive, _dir) = serve_test_archive().await;
    let walk = import_sample(
        &archive,
        &[("name", "Oslo Hills Walk"), ("activity_type", "hiking")],
    )
    .await;
    let ride = import_sample(
        &archive,
        &[("name", "Oslo Ride"), ("activity_type", "cycling")],
    )
    .await;
    let undated = import_gpx(&archive, UNTIMED_GPX, &[]).await;
    let elsewhere = import_sample(&archive, &[("name", "Elsewhere")]).await;
    for id in [walk, ride, undated] {
        tag_trip(&archive, id, "alps").await;
    }
    tag_trip(&archive, elsewhere, "norway").await;

    let html = render_against_archive(
        &archive,
        || rsx! { Summary { view: view(&["alps"]) } },
        |html| html.contains("summary-figures"),
    )
    .await;

    assert!(html.contains(r#"id="summary-chosen""#), "{html}");
    assert!(html.contains("1. Jun. 2024"), "{html}");
    assert!(
        html.contains("Hiking") && html.contains("Cycling"),
        "{html}"
    );
    assert!(html.contains("All activities"), "{html}");
    assert!(html.contains("Longest day"), "{html}");
    assert!(html.contains("Oslo Hills Walk"), "{html}");
    assert!(!html.contains("Elsewhere"), "{html}");
    assert!(html.contains(r#"id="overview-map""#), "{html}");
    assert!(
        html.contains("One recorded trip tagged alps has no dates"),
        "{html}"
    );
}

#[tokio::test]
async fn us78_a_tag_without_recorded_trips_says_so_instead_of_showing_zeros() {
    let (archive, _dir) = serve_test_archive().await;
    let planned = import_sample(&archive, &[("kind", "planned")]).await;
    tag_trip(&archive, planned, "someday").await;

    let html = render_against_archive(
        &archive,
        || rsx! { Summary { view: view(&["someday"]) } },
        |html| html.contains("summary-empty"),
    )
    .await;

    assert!(
        html.contains("No recorded trips are tagged someday."),
        "{html}"
    );
    assert!(!html.contains("summary-figures"), "{html}");
    assert!(!html.contains(r#"id="overview-map""#), "{html}");
}

#[tokio::test]
async fn us78_several_tags_stand_side_by_side_each_in_its_color() {
    let (archive, _dir) = serve_test_archive().await;
    let walk = import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
    let ride = import_sample(&archive, &[("name", "Oslo Ride")]).await;
    tag_trip(&archive, walk, "alps").await;
    tag_trip(&archive, ride, "norway").await;

    let html = render_against_archive(
        &archive,
        || rsx! { Summary { view: view(&["alps", "norway"]) } },
        |html| html.contains("summary-figures"),
    )
    .await;

    let header = html
        .split("<thead>")
        .nth(1)
        .and_then(|rest| rest.split("</thead>").next())
        .expect("a header row");
    assert!(
        header.contains("alps") && header.contains("norway"),
        "{header}"
    );
    assert!(header.contains(lines::tag_color(0)), "{header}");
    assert!(header.contains(lines::tag_color(1)), "{header}");
}
