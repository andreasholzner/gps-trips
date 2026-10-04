//! The statistics screen against a real server (ADR-0012). What the figures
//! are is `figures`' tests'; here, that the screen shows them for the view
//! it was opened with. Changing a control and the charts' canvases are the
//! browser layer's (`tests/browser/statistics.spec.mjs`).

use super::*;
use crate::test_support::{import_gpx, import_sample, render_against_archive, serve_test_archive};

const UNTIMED_GPX: &[u8] = include_bytes!("../../../../tests/fixtures/untimed.gpx");

#[tokio::test]
async fn us77_the_screen_shows_the_totals_the_running_total_and_the_records() {
    let (archive, _dir) = serve_test_archive().await;
    import_sample(
        &archive,
        &[("name", "Oslo Hills Walk"), ("activity_type", "hiking")],
    )
    .await;
    import_sample(
        &archive,
        &[("name", "Oslo Ride"), ("activity_type", "cycling")],
    )
    .await;
    import_gpx(&archive, UNTIMED_GPX, &[]).await;

    let html = render_against_archive(
        &archive,
        || rsx! { Statistics {} },
        |html| html.contains("stats-records"),
    )
    .await;

    // The controls, at their defaults.
    assert!(html.contains(r#"id="stats-period""#), "{html}");
    assert!(html.contains("All years"), "{html}");
    assert!(html.contains(">2024<"), "{html}");
    assert!(html.contains(r#"id="stats-measure""#), "{html}");
    assert!(html.contains("Days out"), "{html}");
    assert!(html.contains("All activities"), "{html}");
    // The totals: a row per activity with its share, and their sum.
    assert!(html.contains("Distance per year"), "{html}");
    assert!(html.contains(r#"id="stats-totals""#), "{html}");
    assert!(
        html.contains("Hiking") && html.contains("Cycling"),
        "{html}"
    );
    assert!(html.contains("50 %"), "{html}");
    // With all activities, a table rather than bars.
    assert!(!html.contains(r#"id="stats-bars""#), "{html}");
    // The running total.
    assert!(html.contains(r#"id="stats-headline""#), "{html}");
    assert!(html.contains(r#"id="stats-running""#), "{html}");
    // The records, linking to their trips.
    assert!(html.contains("Longest day"), "{html}");
    assert!(html.contains("Oslo Hills Walk"), "{html}");
    // The trip with no dates is accounted for.
    assert!(html.contains("One recorded trip has no dates"), "{html}");
}

#[tokio::test]
async fn us77_a_chosen_activity_and_year_narrow_every_figure() {
    let (archive, _dir) = serve_test_archive().await;
    import_sample(
        &archive,
        &[("name", "Oslo Hills Walk"), ("activity_type", "hiking")],
    )
    .await;
    import_sample(
        &archive,
        &[("name", "Oslo Ride"), ("activity_type", "cycling")],
    )
    .await;

    let html = render_against_archive(
        &archive,
        || {
            rsx! {
                Statistics {
                    view: StatsView {
                        year: Some(2024),
                        measure: Measure::MovingTime,
                        activities: vec![ActivityType::Hiking],
                    },
                }
            }
        },
        |html| html.contains("stats-records"),
    )
    .await;

    assert!(html.contains("Moving time per month in 2024"), "{html}");
    assert!(html.contains(">Jun<"), "{html}");
    // The hike's hour, on its own scale, as bars.
    assert!(html.contains("1:00 h"), "{html}");
    assert!(html.contains(r#"id="stats-bars""#), "{html}");
    assert!(!html.contains("Share"), "{html}");
    assert!(!html.contains("Oslo Ride"), "{html}");
}

#[tokio::test]
async fn us77_an_archive_without_recorded_trips_says_so() {
    let (archive, _dir) = serve_test_archive().await;
    import_sample(&archive, &[("kind", "planned")]).await;

    let html = render_against_archive(
        &archive,
        || rsx! { Statistics {} },
        |html| !html.contains("Loading"),
    )
    .await;

    assert!(html.contains("No recorded trips with dates yet"), "{html}");
}

#[tokio::test]
async fn us77_several_chosen_activities_are_compared_in_the_table_without_bars() {
    let (archive, _dir) = serve_test_archive().await;
    import_sample(
        &archive,
        &[("name", "Oslo Hills Walk"), ("activity_type", "hiking")],
    )
    .await;
    import_sample(
        &archive,
        &[("name", "Oslo Ride"), ("activity_type", "cycling")],
    )
    .await;
    import_sample(
        &archive,
        &[("name", "Oslo Paddle"), ("activity_type", "kayaking")],
    )
    .await;

    let html = render_against_archive(
        &archive,
        || {
            rsx! {
                Statistics {
                    view: StatsView {
                        activities: vec![ActivityType::Hiking, ActivityType::Kayaking],
                        ..StatsView::default()
                    },
                }
            }
        },
        |html| html.contains("stats-records"),
    )
    .await;

    // The closed picker names what is chosen.
    assert!(html.contains("Hiking, Kayaking"), "{html}");
    assert!(!html.contains(r#"id="stats-bars""#), "{html}");
    assert!(html.contains("Chosen activities"), "{html}");
    assert!(html.contains("50 %"), "{html}");
    assert!(!html.contains("Oslo Ride"), "{html}");
}

#[test]
fn us77_the_closed_picker_says_what_is_chosen() {
    use ActivityType::{Cycling, Hiking, Kayaking};

    assert_eq!(picked(&[]), "All activities");
    assert_eq!(picked(&[Hiking]), "Hiking");
    assert_eq!(picked(&[Hiking, Cycling]), "Hiking, Cycling");
    assert_eq!(picked(&[Hiking, Cycling, Kayaking]), "3 activities");
}

#[tokio::test]
async fn us80_average_speed_is_offered_and_shown_without_shares() {
    let (archive, _dir) = serve_test_archive().await;
    import_sample(
        &archive,
        &[("name", "Oslo Hills Walk"), ("activity_type", "hiking")],
    )
    .await;

    let html = render_against_archive(
        &archive,
        || {
            rsx! {
                Statistics {
                    view: StatsView { measure: Measure::AverageSpeed, ..StatsView::default() },
                }
            }
        },
        |html| html.contains("stats-records"),
    )
    .await;

    assert!(html.contains(r#"value="average_speed""#), "{html}");
    assert!(html.contains("Average speed per year"), "{html}");
    assert!(html.contains(" km/h"), "{html}");
    assert!(!html.contains("Share"), "{html}");
}
