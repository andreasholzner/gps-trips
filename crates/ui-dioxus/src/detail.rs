//! The trip-detail screen (US-42): relive and edit a single trip. Each
//! feature slice moves its story's acceptance assertions over from the
//! server-rendered page (ADR-0012's migration rule).

use dioxus::prelude::*;
// The screen is `TripDetail`; so is the shape it shows. Aliasing the data
// keeps both readable in one file.
use trip_archive_types::TripDetail as Trip;

use crate::api;
use crate::filters::Filters;
use crate::format;
use crate::Route;

/// The screen. `id` comes from the route (`/trips/:id`), so a link, a
/// bookmark and a reload all land on the same trip.
#[component]
pub fn TripDetail(id: i64) -> Element {
    let base_url = use_context::<Signal<String>>();
    // `id` is a plain prop, not a signal: navigating from one trip to
    // another reuses this component's scope, and a resource that only
    // watched signals would keep showing the trip it first fetched.
    // `use_reactive` is what subscribes it to the prop.
    let trip = use_resource(use_reactive!(|id| async move {
        api::get_trip(&base_url(), id).await
    }));

    rsx! {
        // Back to the unfiltered list. Whatever the owner had narrowed it to
        // lives in the list's own URL (US-52), so the browser's Back button
        // is what restores that; this link is the way out when there is no
        // history behind the screen — a bookmark, or a shared link.
        nav { class: "elsewhere",
            Link { to: Route::TripList { filters: Filters::default() }, "← All trips" }
        }
        match &*trip.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            // A trip that is simply gone — a stale bookmark, or the Back
            // button after a delete (US-9) — is an ordinary outcome, not a
            // fault, and reads as one.
            Some(Err(err)) if err.is_not_found() => rsx! {
                p { class: "error", "There is no such trip — it may have been deleted." }
            },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load this trip: {err}" } },
            Some(Ok(trip)) => rsx! { TripStats { trip: trip.clone() } },
        }
    }
}

/// The trip's name and its stats — every one of them computed at import and
/// never entered by hand (US-8); this screen only reports them.
#[component]
fn TripStats(trip: Trip) -> Element {
    rsx! {
        h1 { id: "trip-name", "{trip.name}" }
        p {
            "Activity: "
            span { id: "trip-activity", "{trip.activity_type.label()}" }
        }
        dl { class: "stats",
            dt { "Start" }
            dd { {format::or_dash(trip.start_time.as_deref())} }
            dt { "Distance" }
            dd { {format::km(trip.distance_m)} }
            dt { "Ascent" }
            dd { {format::metres(trip.ascent_m)} }
            dt { "Descent" }
            dd { {format::metres(trip.descent_m)} }
            dt { "Duration" }
            dd { {format::duration(trip.duration_secs)} }
            // US-4 places a photo without GPS by matching its timestamp to
            // the track, in this timezone — so the screen says which one it
            // assumed, and a photo in an odd place is explicable.
            dt { "Photo timestamp timezone" }
            dd { {format::or_dash(trip.tz_name.as_deref())} }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{import_sample, render, render_against_archive, serve_test_archive};
    use trip_archive_types::ActivityType;

    /// A trip as the detail endpoint returns it, for the component-level
    /// tests — no server needed to assert what a screen shows.
    fn a_trip(name: &str) -> Trip {
        Trip {
            id: 1,
            name: name.to_string(),
            activity_type: ActivityType::Hiking,
            tz_name: Some("Europe/Oslo".to_string()),
            start_time: Some("2026-07-11T09:30:00Z".to_string()),
            end_time: Some("2026-07-11T13:15:00Z".to_string()),
            distance_m: 12_345.0,
            ascent_m: Some(410.0),
            descent_m: Some(395.0),
            duration_secs: Some(13_500),
            min_lat: Some(59.9),
            min_lon: Some(10.7),
            max_lat: Some(60.0),
            max_lon: Some(10.8),
            komoot: None,
        }
    }

    // US-7: the trip's own numbers, around the map and the gallery that
    // follow in later phases. US-8 computed them at import; the screen
    // reports them.
    #[test]
    fn the_screen_shows_the_trips_name_and_computed_stats() {
        let trip = a_trip("Oslo Hills Walk");

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert!(html.contains("Oslo Hills Walk"), "{html}");
        assert!(html.contains(ActivityType::Hiking.label()), "{html}");
        assert!(html.contains("12.35 km"), "{html}");
        assert!(html.contains("410 m"), "{html}");
        assert!(html.contains("395 m"), "{html}");
        assert!(html.contains("03:45:00"), "{html}");
        assert!(html.contains("2026-07-11T09:30:00Z"), "{html}");
        // US-4 places photos by the trip's assumed timezone; the screen says
        // which one it is, so a photo in the wrong place is explicable.
        assert!(html.contains("Europe/Oslo"), "{html}");
    }

    #[test]
    fn a_trip_missing_optional_stats_shows_dashes_not_blanks() {
        let trip = Trip {
            start_time: None,
            ascent_m: None,
            descent_m: None,
            duration_secs: None,
            tz_name: None,
            ..a_trip("Bare Trip")
        };

        let html = render(move || rsx! { TripStats { trip: trip.clone() } });

        assert!(html.contains("Bare Trip"), "{html}");
        assert!(html.contains("—"), "{html}");
    }

    // The whole screen against a real archive: nothing mocked (ADR-0012).
    #[tokio::test]
    async fn the_screen_loads_a_trip_from_the_archive() {
        let (base_url, _dir) = serve_test_archive().await;
        let id = import_sample(&base_url, &[("activity_type", "hiking")]).await;

        let html = render_against_archive(
            &base_url,
            move || rsx! { TripDetail { id } },
            |html| html.contains("Oslo Hills Walk"),
        )
        .await;

        assert!(html.contains(ActivityType::Hiking.label()), "{html}");
        // SAMPLE_GPX's own track, measured at import (US-8).
        assert!(html.contains(" km"), "{html}");
    }

    #[tokio::test]
    async fn a_trip_that_no_longer_exists_says_so_rather_than_loading_forever() {
        // A stale bookmark, or the Back button after a delete (US-9): the
        // screen must resolve to a readable message, not sit on "Loading…".
        let (base_url, _dir) = serve_test_archive().await;

        let html = render_against_archive(
            &base_url,
            || rsx! { TripDetail { id: 9_999 } },
            |html| !html.contains("Loading"),
        )
        .await;

        assert!(
            html.contains("no such trip"),
            "the owner is told the trip is gone: {html}"
        );
    }
}
