//! One shared trip, as its recipient sees it (US-53): the detail screen's
//! stats, map, elevation profile and photos, and the GPX to download —
//! without the tags, the controls, or placing a photo by hand.

use dioxus::prelude::*;
use trip_archive_types::ShareOverview;

use super::{as_trip_detail, title};
use crate::api::{self, ApiClient};
use crate::detail::{TrackSection, TripStats};
use crate::gallery::PhotoGallery;
use crate::photos::{self, PhotoView};
use crate::viewer::PhotoViewer;
use crate::Route;

/// The trip `id` of the share `overview` describes. The share's title sits
/// above it; a way back to the share's list only where there is a list.
#[component]
pub fn SharedTripView(token: String, id: i64, overview: ShareOverview) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    // The id travels back with each answer, as on the owner's detail screen:
    // the router shows the next trip through this same scope, and a resource
    // keeps its last value while the next fetch is pending.
    let trip = use_resource(use_reactive!(|id| async move {
        (id, api::get_shared_trip(&archive(), id).await)
    }));
    let photo_list = use_resource(use_reactive!(|id| async move {
        (id, api::list_photos(&archive(), id).await)
    }));
    let mut viewing = use_signal(|| None::<(Vec<PhotoView>, usize)>);
    use_effect(use_reactive!(|id| {
        let _ = id;
        viewing.set(None);
    }));

    let many = overview.trips.len() > 1;
    let heading = title(&overview).to_string();
    let base_url = archive().base_url().to_string();
    let (photos, photos_error) = match &*photo_list.read_unchecked() {
        Some((read_for, Ok(photos))) if *read_for == id => (Some(photos.clone()), None),
        Some((read_for, Err(err))) if *read_for == id => (None, Some(err.to_string())),
        _ => (None, None),
    };

    rsx! {
        p { class: "share-title",
            if many {
                Link { to: Route::Shared { token: token.clone() }, "{heading}" }
            } else {
                "{heading}"
            }
        }
        match &*trip.read_unchecked() {
            Some((read_for, Ok(trip))) if *read_for == id => rsx! {
                TripStats { trip: as_trip_detail(trip) }
                TrackSection {
                    id,
                    markers: photos::photo_markers(&base_url, photos.as_deref().unwrap_or(&[])),
                    on_open_photos: move |opened| viewing.set(Some(opened)),
                }
                PhotoGallery {
                    photos: photos.clone(),
                    base_url: base_url.clone(),
                    error: photos_error,
                    on_open: {
                        let views = photos::photo_views(&base_url, photos.as_deref().unwrap_or(&[]));
                        move |start| viewing.set(Some((views.clone(), start)))
                    },
                }
                div { class: "trip-actions",
                    a { class: "quiet", href: api::original_gpx_url(&archive(), id), "Download GPX" }
                }
                if let Some((photos, start)) = viewing() {
                    PhotoViewer { photos, start, on_close: move |_| viewing.set(None) }
                }
            },
            Some((read_for, Err(err))) if *read_for == id && err.is_not_found() => rsx! {
                p { class: "error", "This trip is not part of what was shared." }
            },
            Some((read_for, Err(err))) if *read_for == id => rsx! {
                p { class: "error", "Could not load this trip: {err}" }
            },
            _ => rsx! { p { "Loading…" } },
        }
    }
}
