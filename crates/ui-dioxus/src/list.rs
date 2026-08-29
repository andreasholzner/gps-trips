//! The trip-list screen (US-41): browse, filter and tag trips. Each feature
//! slice moves its story's acceptance assertions over from the
//! server-rendered page (ADR-0012's migration rule).

use dioxus::prelude::*;
use trip_archive_types::TripSummary;

use crate::api;

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

/// One row per trip (US-6). The row's stats, formatting and detail link grow
/// here in their own slices.
#[component]
fn TripTable(trips: Vec<TripSummary>) -> Element {
    rsx! {
        table {
            tbody {
                for trip in trips {
                    tr { key: "{trip.id}",
                        td { "{trip.name}" }
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
    use crate::test_support::{render, render_against_archive, serve_test_archive};
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
    // in, HTML string out, no server. US-6's full row content (stats,
    // formatting) grows here in its own slice.
    #[test]
    fn the_trip_table_shows_a_row_per_trip() {
        let trips = vec![a_trip(1, "Oslo Hills Walk"), a_trip(2, "Inn Valley Ride")];

        let html = render(move || rsx! { TripTable { trips: trips.clone() } });

        assert!(html.contains("Oslo Hills Walk"), "{html}");
        assert!(html.contains("Inn Valley Ride"), "{html}");
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
