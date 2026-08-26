//! The trip detail screen (US-7): the trip's stats, its track on an OSM map,
//! an elevation profile, and the photo gallery.
//!
//! The three data sources (metadata, track, photos) are fetched independently
//! so a slow or failing one never blanks the others — the same resilience the
//! server-rendered page gets from its separate `fetch` calls.

use dioxus::prelude::*;
use trip_archive_types::TripDetail as Trip;

use crate::api::{self, PhotoView};
use crate::format;
use crate::interop;
use crate::Route;

/// The ids the map and chart are drawn into. Both elements are rendered with
/// no children of their own and never re-rendered with any, so Leaflet and
/// uPlot own their subtrees outright.
const MAP_ID: &str = "trip-map";
const ELEVATION_ID: &str = "trip-elevation";

#[component]
pub fn TripDetail(id: i64) -> Element {
    let base_url = use_context::<Signal<String>>();
    let trip = use_resource(move || async move { api::trip(&base_url(), id).await });
    let photos = use_resource(move || async move { api::photos(&base_url(), id).await });
    let track = use_resource(move || async move { api::track(&base_url(), id).await });

    // Draws once the track has arrived *and* the containers below are in the
    // DOM. Both widgets guard against being drawn twice, so a re-render
    // caused by the photos resolving later cannot stack a second map.
    use_effect(move || {
        if let Some(Ok(track)) = &*track.read_unchecked() {
            interop::draw_map(MAP_ID, track.clone());
            interop::draw_elevation(ELEVATION_ID, track);
        }
    });

    rsx! {
        p { Link { to: Route::TripList {}, "← All trips" } }
        match &*trip.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load this trip: {err}" } },
            Some(Ok(trip)) => rsx! { TripHeader { trip: trip.clone() } },
        }

        h2 { "Track" }
        if let Some(Err(err)) = &*track.read_unchecked() {
            p { class: "error", "Could not load the track: {err}" }
        }
        div { id: MAP_ID, style: "height: 24rem;" }
        div { id: ELEVATION_ID, style: "margin-top: 1rem;" }

        h2 { "Photos" }
        match &*photos.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load the photos: {err}" } },
            Some(Ok(photos)) if photos.is_empty() => rsx! { p { "No photos for this trip." } },
            Some(Ok(photos)) => rsx! { Gallery { photos: photos.clone(), base_url: base_url() } },
        }
    }
}

#[component]
fn TripHeader(trip: Trip) -> Element {
    rsx! {
        h1 { "{trip.name}" }
        p { "Activity: {trip.activity_type.label()}" }
        ul {
            li { "Start: " {format::date(trip.start_time.as_deref())} }
            li { "Distance: " {format::km(trip.distance_m)} }
            li { "Ascent: " {format::metres(trip.ascent_m)} }
            li { "Descent: " {format::metres(trip.descent_m)} }
            li { "Duration: " {format::duration(trip.duration_secs)} }
        }
        // US-35's privacy is a linked tour's property; an unlinked trip has
        // none to show.
        if let Some(komoot) = &trip.komoot {
            p { "Komoot tour {komoot.tour_id}" }
        }
    }
}

/// The photos come back with server-relative URLs, which only resolve on the
/// web; on Android they have to be joined onto the configured archive.
#[component]
fn Gallery(photos: Vec<PhotoView>, base_url: String) -> Element {
    rsx! {
        div { style: "display: flex; flex-wrap: wrap; gap: 0.5rem;",
            for photo in photos {
                a { key: "{photo.id}", href: api::media_url(&base_url, &photo.url),
                    img {
                        src: api::media_url(&base_url, &photo.thumbnail_url),
                        alt: "{photo.original_name}",
                        style: "max-height: 8rem;",
                    }
                }
            }
        }
    }
}
