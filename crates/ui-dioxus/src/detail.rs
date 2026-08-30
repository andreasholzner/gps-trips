//! The trip-detail screen (US-42): relive and edit a single trip. Each
//! feature slice moves its story's acceptance assertions over from the
//! server-rendered page (ADR-0012's migration rule).

use dioxus::prelude::*;
// The screen is `TripDetail`; so is the shape it shows. Aliasing the data
// keeps both readable in one file.
use trip_archive_types::TripDetail as Trip;

use crate::api;
use crate::edit::EditTrip;
use crate::filters::Filters;
use crate::format;
use crate::interop;
use crate::photos::{self, AddPhotos, PhotoGallery, PhotoMarker};
use crate::track::{self, Track};
use crate::Route;
use trip_archive_types::PhotoResponse;

/// The screen. `id` comes from the route (`/trips/:id`), so a link, a
/// bookmark and a reload all land on the same trip.
#[component]
pub fn TripDetail(id: i64) -> Element {
    let base_url = use_context::<Signal<String>>();
    // `id` is a plain prop, not a signal: navigating from one trip to
    // another reuses this component's scope, and a resource that only
    // watched signals would keep showing the trip it first fetched.
    // `use_reactive` is what subscribes it to the prop.
    let mut trip_resource = use_resource(use_reactive!(|id| async move {
        api::get_trip(&base_url(), id).await
    }));
    // Fetched once for the two things that show them: the gallery, and the
    // markers on the track map (US-3/US-4). Adding photos restarts this, so
    // both are current.
    let mut photo_list = use_resource(use_reactive!(|id| async move {
        api::list_photos(&base_url(), id).await
    }));
    // The photos last read successfully, and which trip they belong to. A
    // restarted resource reads as pending, and showing that as "no photos"
    // would blank the gallery and pull every marker off the map for as long
    // as the refresh takes — so what is on screen stays there until there is
    // something newer. It is kept with its trip's id because this same
    // component shows the next trip too: without that, a trip whose stats
    // arrive before its photos would briefly wear the previous one's gallery
    // and plot its markers on the wrong track.
    let mut cached = use_signal(|| (id, Vec::<PhotoResponse>::new()));
    use_effect(move || {
        if let Some(Ok(latest)) = &*photo_list.read() {
            cached.set((id, latest.clone()));
        }
    });
    let photos = match cached() {
        (cached_id, photos) if cached_id == id => photos,
        _ => Vec::new(),
    };
    let photos_error = match &*photo_list.read_unchecked() {
        Some(Err(err)) => Some(err.to_string()),
        _ => None,
    };

    rsx! {
        // Back to the unfiltered list. Whatever the owner had narrowed it to
        // lives in the list's own URL (US-52), so the browser's Back button
        // is what restores that; this link is the way out when there is no
        // history behind the screen — a bookmark, or a shared link.
        nav { class: "elsewhere",
            Link { to: Route::TripList { filters: Filters::default() }, "← All trips" }
        }
        match &*trip_resource.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            // A trip that is simply gone — a stale bookmark, or the Back
            // button after a delete (US-9) — is an ordinary outcome, not a
            // fault, and reads as one.
            Some(Err(err)) if err.is_not_found() => rsx! {
                p { class: "error", "There is no such trip — it may have been deleted." }
            },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load this trip: {err}" } },
            Some(Ok(trip)) => rsx! {
                TripStats { trip: trip.clone() }
                EditTrip { trip: trip.clone(), on_saved: move |_| trip_resource.restart() }
                TrackSection { id, markers: photos::photo_markers(&base_url(), &photos) }
                PhotoGallery {
                    photos: photos.clone(),
                    base_url: base_url(),
                    error: photos_error.clone(),
                }
                AddPhotos { id, on_added: move |_| photo_list.restart() }
            },
        }
    }
}

/// The track on an OSM map and the elevation profile below it (US-7), from
/// the one fetch that carries both (ADR-0025).
///
/// A track that will not load costs the map and the chart, not the screen:
/// the stats, the gallery and the edit controls around it are unaffected.
#[component]
fn TrackSection(id: i64, markers: Vec<PhotoMarker>) -> Element {
    let base_url = use_context::<Signal<String>>();
    let track = use_resource(use_reactive!(|id| async move {
        api::get_track(&base_url(), id).await
    }));

    rsx! {
        match &*track.read_unchecked() {
            None => rsx! { p { "Loading the track…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load the track: {err}" } },
            Some(Ok(track)) => rsx! {
                TrackViews { track: track.clone(), markers: markers.clone() }
            },
        }
    }
}

/// The two widgets themselves, once there is a track to draw. Split from the
/// fetch above so each starts its script exactly once, when it mounts with
/// the values it draws — never on a re-render of the screen around it.
#[component]
fn TrackViews(track: Track, markers: Vec<PhotoMarker>) -> Element {
    let points = track::polyline(&track);
    let series = track::elevation_series(&track);

    rsx! {
        TrackMap { points, markers }
        if let Some((distance_km, elevation_m)) = series {
            ElevationChart { distance_km, elevation_m }
        }
    }
}

/// The map container. Rendered empty and never given children: Leaflet owns
/// this subtree from the moment it initialises (ADR-0025).
#[component]
fn TrackMap(points: Vec<[f64; 2]>, markers: Vec<PhotoMarker>) -> Element {
    // Redrawn whenever the line changes, not only when the component first
    // mounts: the router shows a different trip through this same component,
    // and a plain `use_future` would leave the previous trip's track on the
    // map. The script is written to be drawn into twice (`interop::track`).
    use_future(use_reactive!(|points, markers| async move {
        let mut map = interop::start_track_map(points, markers);
        // The handle is the channel: hold it until this draw is superseded or
        // the screen goes away, or a dropped handle takes the payload with it
        // before the script has read it. Nothing is sent back yet — photo
        // markers (US-3/US-4) make this a real loop.
        let _: Result<(), _> = map.recv().await;
    }));

    rsx! { div { id: "track-map", class: "track-map" } }
}

/// The elevation chart's container, on the same terms as the map's.
#[component]
fn ElevationChart(distance_km: Vec<f64>, elevation_m: Vec<f64>) -> Element {
    use_future(use_reactive!(|distance_km, elevation_m| async move {
        let mut chart = interop::start_elevation_chart(distance_km, elevation_m);
        let _: Result<(), _> = chart.recv().await;
    }));

    rsx! { div { id: "elevation", class: "elevation" } }
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

    // US-7's map and chart. `document::eval` does nothing headless, so what
    // this layer can assert is the wiring: both containers exist, and both
    // are empty — Leaflet and uPlot own those subtrees outright and Dioxus
    // must never render children into them (ADR-0025). That they actually
    // draw is the browser layer's business (ADR-0012's 2026-08-26b rule).
    #[tokio::test]
    async fn the_screen_gives_the_map_and_the_chart_a_container_of_their_own() {
        let (base_url, _dir) = serve_test_archive().await;
        let id = import_sample(&base_url, &[]).await;

        let html = render_against_archive(
            &base_url,
            move || rsx! { TripDetail { id } },
            |html| html.contains("track-map"),
        )
        .await;

        assert!(
            html.contains(r#"<div id="track-map" class="track-map"></div>"#),
            "an empty map container: {html}"
        );
        assert!(
            html.contains(r#"<div id="elevation" class="elevation"></div>"#),
            "an empty chart container — the fixture track has elevations: {html}"
        );
    }

    // US-15: the screen offers the edit, and saving it re-reads the trip.
    // What the form sends is `edit::changes`' own business; that it is
    // reachable from the screen at all is this one's.
    #[tokio::test]
    async fn the_screen_offers_the_edit_form() {
        let (base_url, _dir) = serve_test_archive().await;
        let id = import_sample(&base_url, &[]).await;

        let html = render_against_archive(
            &base_url,
            move || rsx! { TripDetail { id } },
            |html| html.contains("Oslo Hills Walk"),
        )
        .await;

        assert!(html.contains(r#"id="edit-trip""#), "{html}");
        assert!(html.contains("Edit name / activity"), "{html}");
    }

    // US-7's gallery, and US-2's "photos can be added at a later time" as the
    // screen offers it. Which photos reach the *map* is `photos::` own unit
    // test — a marker is invisible to a rendered string.
    #[tokio::test]
    async fn the_screen_shows_the_trips_photos_and_offers_to_add_more() {
        let (base_url, _dir) = serve_test_archive().await;
        let id = import_sample(&base_url, &[]).await;
        api::add_photos(
            &base_url,
            id,
            vec![crate::api::PhotoUpload {
                file_name: "later.jpg".to_string(),
                content_type: Some("image/jpeg".to_string()),
                bytes: b"\xFF\xD8\xFF-fake-jpeg".to_vec(),
            }],
        )
        .await
        .expect("upload");

        let html = render_against_archive(
            &base_url,
            move || rsx! { TripDetail { id } },
            |html| html.contains("later.jpg"),
        )
        .await;

        assert!(
            html.contains(&format!("{base_url}/media/")),
            "the gallery fetches the image from the archive it read: {html}"
        );
        assert!(html.contains("Add photos"), "{html}");
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
