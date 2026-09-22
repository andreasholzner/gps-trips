//! The trip-detail screen (US-42): relive and edit a single trip. Each
//! feature slice moves its story's acceptance assertions over from the
//! server-rendered page (ADR-0012's migration rule).

use dioxus::prelude::*;

use crate::api::{self, ApiClient};
use crate::photos::{self, PhotoGallery};
use crate::trip_tags::TripTags;
use trip_archive_types::PhotoResponse;

mod actions;
mod stats;
mod track_views;

use actions::TripActions;
use stats::TripStats;
use track_views::TrackSection;

/// The screen. `id` comes from the route (`/trips/:id`), so a link, a
/// bookmark and a reload all land on the same trip.
#[component]
pub fn TripDetail(id: i64) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    // `id` is a plain prop, not a signal: navigating from one trip to
    // another reuses this component's scope, and a resource that only
    // watched signals would keep showing the trip it first fetched.
    // `use_reactive` is what subscribes it to the prop.
    // The id travels back with the answer. A resource keeps its previous
    // value while a new fetch is pending, so navigating from one trip to
    // another would otherwise render this trip's name and stats above
    // controls — delete among them — already aimed at the next trip's id.
    let mut trip_resource = use_resource(use_reactive!(|id| async move {
        (id, api::get_trip(&archive(), id).await)
    }));
    // Fetched once for the two things that show them: the gallery, and the
    // markers on the track map (US-3/US-4). Adding photos restarts this, so
    // both are current.
    //
    // The trip's id travels back with the answer, whichever way it went: a
    // resource keeps its last value while a new fetch is pending, so reading
    // the id from this scope instead would stamp one trip's photos — or one
    // trip's failure — with another trip's id.
    let mut photo_list = use_resource(use_reactive!(|id| async move {
        (id, api::list_photos(&archive(), id).await)
    }));
    // The photos last read successfully, and which trip they belong to. A
    // restarted resource reads as pending, and showing that as "no photos"
    // would blank the gallery and pull every marker off the map for as long
    // as the refresh takes — so what is on screen stays there until there is
    // something newer. It is kept with its trip's id because this same
    // component shows the next trip too: without that, a trip whose stats
    // arrive before its photos would briefly wear the previous one's gallery
    // and plot its markers on the wrong track.
    let mut cached = use_signal(|| (None::<i64>, Vec::<PhotoResponse>::new()));
    use_effect(move || {
        if let Some((read_for, Ok(photos))) = &*photo_list.read() {
            cached.set((Some(*read_for), photos.clone()));
        }
    });
    // `None` until something has been read for *this* trip: seeding the cache
    // with an empty list would make the gallery claim the trip has no photos
    // for as long as the first read takes.
    let photos = match cached() {
        (Some(cached_id), photos) if cached_id == id => Some(photos),
        _ => None,
    };
    // Only this trip's failure is this trip's news: the resource still holds
    // the previous one's answer until the new fetch lands.
    let photos_error = match &*photo_list.read_unchecked() {
        Some((read_for, Err(err))) if *read_for == id => Some(err.to_string()),
        _ => None,
    };

    rsx! {
        // The way back is the menu's "All trips" (US-60), on every screen
        // rather than one link per screen. Whatever the owner had narrowed
        // the list to lives in its own URL (US-52), so the browser's Back
        // button is still what restores *that* — the menu goes to the
        // unfiltered list, as this link did.
        match &*trip_resource.read_unchecked() {
            // Either nothing has been read yet, or what was read belongs to
            // the trip this screen was showing a moment ago.
            None => rsx! { p { "Loading…" } },
            Some((read_for, _)) if *read_for != id => rsx! { p { "Loading…" } },
            // A trip that is simply gone — a stale bookmark, or the Back
            // button after a delete (US-9) — is an ordinary outcome, not a
            // fault, and reads as one.
            Some((_, Err(err))) if err.is_not_found() => rsx! {
                p { class: "error", "There is no such trip — it may have been deleted." }
            },
            Some((_, Err(err))) => rsx! {
                p { class: "error", "Could not load this trip: {err}" }
            },
            Some((_, Ok(trip))) => rsx! {
                TripStats { trip: trip.clone() }
                TripTags { id }
                TrackSection {
                    id,
                    markers: photos::photo_markers(archive().base_url(), photos.as_deref().unwrap_or(&[])),
                }
                PhotoGallery {
                    photos: photos.clone(),
                    base_url: archive().base_url().to_string(),
                    error: photos_error.clone(),
                }
                TripActions {
                    trip: trip.clone(),
                    on_saved: move |_| trip_resource.restart(),
                    on_photos_added: move |_| photo_list.restart(),
                }
            },
        }
    }
}
// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{import_sample, render_against_archive, serve_test_archive};
    use trip_archive_types::ActivityType;

    // The whole screen against a real archive: nothing mocked (ADR-0012).
    #[tokio::test]
    async fn the_screen_loads_a_trip_from_the_archive() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[("activity_type", "hiking")]).await;

        let html = render_against_archive(
            &archive,
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
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;

        let html = render_against_archive(
            &archive,
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
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;

        let html = render_against_archive(
            &archive,
            move || rsx! { TripDetail { id } },
            |html| html.contains("Oslo Hills Walk"),
        )
        .await;

        assert!(html.contains(r#"id="edit-trip""#), "{html}");
        assert!(html.contains("Edit name / activity"), "{html}");
    }

    // The empty states are claims about the trip, and neither may be made
    // while the archive is still answering. This watches every intermediate
    // render, not just the last one — the defect it guards against is
    // invisible in the final HTML.
    #[tokio::test]
    async fn no_empty_state_is_claimed_while_the_archive_is_still_answering() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;
        crate::test_support::tag_trip(&archive, id, "alpine").await;
        api::add_photos(
            &archive,
            id,
            vec![crate::api::PhotoUpload {
                file_name: "later.jpg".to_string(),
                content_type: Some("image/jpeg".to_string()),
                bytes: b"\xFF\xD8\xFF-fake-jpeg".to_vec(),
            }],
        )
        .await
        .expect("upload");

        let claimed_empty = std::cell::Cell::new(false);
        render_against_archive(
            &archive,
            move || rsx! { TripDetail { id } },
            |html| {
                if html.contains("No tags yet") || html.contains("No photos yet") {
                    claimed_empty.set(true);
                }
                html.contains("alpine") && html.contains("later.jpg")
            },
        )
        .await;

        assert!(
            !claimed_empty.get(),
            "a trip with tags and photos must never be shown as having none"
        );
    }

    // US-21: the original bytes are one link away from the trip they were
    // imported for, addressed at the archive that stored them.
    #[tokio::test]
    async fn the_screen_links_to_the_original_gpx() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;

        let html = render_against_archive(
            &archive,
            move || rsx! { TripDetail { id } },
            |html| html.contains("Oslo Hills Walk"),
        )
        .await;

        assert!(
            html.contains(&format!("{}/api/trips/{id}/gpx", archive.base_url())),
            "{html}"
        );
        assert!(html.contains("Download original GPX"), "{html}");
    }

    // US-9: the screen offers the delete. That it asks first, and where it
    // goes afterwards, are `delete::`'s own tests and the browser layer's.
    #[tokio::test]
    async fn the_screen_offers_to_delete_the_trip() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;

        let html = render_against_archive(
            &archive,
            move || rsx! { TripDetail { id } },
            |html| html.contains("Oslo Hills Walk"),
        )
        .await;

        assert!(html.contains(r#"id="delete-trip""#), "{html}");
    }

    // US-33: the trip's tags, and the way to add one.
    #[tokio::test]
    async fn the_screen_shows_the_trips_tags() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;
        crate::test_support::tag_trip(&archive, id, "alpine").await;

        let html = render_against_archive(
            &archive,
            move || rsx! { TripDetail { id } },
            |html| html.contains("alpine"),
        )
        .await;

        // US-62: one line — the chips and a way to add one. The field and
        // its suggestions appear once adding is asked for, not before; that
        // they then offer every known tag is `trip_tags`' own test.
        assert!(html.contains(r#"id="add-tag""#), "{html}");
        assert!(!html.contains(r#"id="tag-input""#), "{html}");
        assert!(!html.contains("<datalist"), "{html}");
        assert!(!html.contains("<h2>Tags</h2>"), "{html}");
    }

    // US-7's gallery, and US-2's "photos can be added at a later time" as the
    // screen offers it. Which photos reach the *map* is `photos::` own unit
    // test — a marker is invisible to a rendered string.
    #[tokio::test]
    async fn the_screen_shows_the_trips_photos_and_offers_to_add_more() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;
        api::add_photos(
            &archive,
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
            &archive,
            move || rsx! { TripDetail { id } },
            |html| html.contains("later.jpg"),
        )
        .await;

        assert!(
            html.contains(&format!("{}/media/", archive.base_url())),
            "the gallery fetches the image from the archive it read: {html}"
        );
        assert!(html.contains("Add photos"), "{html}");
    }

    #[tokio::test]
    async fn a_trip_that_no_longer_exists_says_so_rather_than_loading_forever() {
        // A stale bookmark, or the Back button after a delete (US-9): the
        // screen must resolve to a readable message, not sit on "Loading…".
        let (archive, _dir) = serve_test_archive().await;

        let html = render_against_archive(
            &archive,
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
