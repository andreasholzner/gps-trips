//! The trip-list screen (US-41): browse, filter and tag trips. Each feature
//! slice moves its story's acceptance assertions over from the
//! server-rendered page (ADR-0012's migration rule).

use dioxus::prelude::*;
use trip_archive_types::TripSummary;

use crate::api;
use crate::format;

#[component]
pub fn TripList() -> Element {
    let base_url = use_context::<Signal<String>>();
    // Re-runs whenever the configured archive changes — reading the signal
    // inside the closure is the whole subscription.
    let trips =
        use_resource(move || async move { api::list_trips(&base_url(), String::new()).await });

    rsx! {
        h1 { "Trips" }
        match &*trips.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load trips: {err}" } },
            Some(Ok(trips)) if trips.is_empty() => rsx! { p { "No trips yet. Import your first trip." } },
            Some(Ok(trips)) => rsx! { TripTable { trips: trips.clone() } },
        }
    }
}

/// One row per trip: name, activity, date, distance, ascent, duration
/// (US-6). The name becomes a link to the trip's detail screen when US-42
/// builds it; until then the row is display-only.
#[component]
fn TripTable(trips: Vec<TripSummary>) -> Element {
    rsx! {
        table {
            thead {
                tr {
                    th { "Trip" }
                    th { "Activity" }
                    th { "Date" }
                    th { "Distance" }
                    th { "Ascent" }
                    th { "Duration" }
                }
            }
            tbody {
                for trip in trips {
                    tr { key: "{trip.id}",
                        td { "{trip.name}" }
                        td { "{trip.activity_type.label()}" }
                        td { {format::date(trip.start_time.as_deref())} }
                        td { {format::km(trip.distance_m)} }
                        td { {format::metres(trip.ascent_m)} }
                        td { {format::duration(trip.duration_secs)} }
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
    use crate::test_support::{import_sample, render, render_against_archive, serve_test_archive};
    use trip_archive_types::{ActivityType, TripKind};

    fn a_trip(id: i64, name: &str) -> TripSummary {
        TripSummary {
            id,
            name: name.to_string(),
            activity_type: ActivityType::Hiking,
            start_time: Some("2026-07-11T09:30:00Z".to_string()),
            distance_m: 12_345.0,
            ascent_m: Some(410.0),
            duration_secs: Some(3_725),
            trip_kind: TripKind::Recorded,
            privacy_status: None,
        }
    }

    // Exemplar for the component-render layer (ADR-0012, 2026-08-26a): props
    // in, HTML string out, no server. US-6: each trip's name, date, distance,
    // ascent and duration — asserting the formatting the screen is
    // responsible for, not the raw values.
    #[test]
    fn the_trip_table_shows_a_row_per_trip() {
        let trips = vec![a_trip(1, "Oslo Hills Walk"), a_trip(2, "Inn Valley Ride")];

        let html = render(move || rsx! { TripTable { trips: trips.clone() } });

        assert!(html.contains("Oslo Hills Walk"), "{html}");
        assert!(html.contains("Inn Valley Ride"), "{html}");
        assert!(html.contains("2026-07-11"), "{html}");
        assert!(html.contains("12.35 km"), "{html}");
        assert!(html.contains("410 m"), "{html}");
        assert!(html.contains("01:02:05"), "{html}");
        assert!(html.contains("Hiking"), "{html}");
    }

    #[test]
    fn a_trip_missing_optional_stats_shows_dashes_not_blanks() {
        let trips = vec![TripSummary {
            start_time: None,
            ascent_m: None,
            duration_secs: None,
            ..a_trip(1, "Bare Trip")
        }];

        let html = render(move || rsx! { TripTable { trips: trips.clone() } });

        assert!(html.contains("—"), "{html}");
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
