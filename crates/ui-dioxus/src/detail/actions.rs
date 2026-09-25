//! The occasional controls, in one quiet row at the foot of the detail screen
//! (US-62): edit, add photos, share (US-53), download the original GPX, delete. Each used to
//! be a full-width primary button, with the add-photos form open besides; now
//! nothing is open until it is asked for.

use dioxus::prelude::*;
use trip_archive_types::TripDetail as Trip;

use crate::api::{self, ApiClient};
use crate::delete::DeleteTrip;
use crate::edit::EditTrip;
use crate::photos::AddPhotos;
use crate::share::ShareForm;

/// The row, and the add-photos or share form under it once asked for. `on_saved` and
/// `on_photos_added` tell the screen what to re-read.
#[component]
pub fn TripActions(
    trip: Trip,
    on_saved: EventHandler<()>,
    on_photos_added: EventHandler<()>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let id = trip.id;
    let mut adding_photos = use_signal(|| false);
    let mut sharing = use_signal(|| false);
    // The router shows the next trip through this same scope; a form opened
    // for one trip must not stay open, aimed at the next.
    use_effect(use_reactive!(|id| {
        let _ = id;
        adding_photos.set(false);
        sharing.set(false);
    }));

    rsx! {
        div { class: "trip-actions",
            EditTrip { trip, on_saved }
            button {
                id: "add-photos",
                r#type: "button",
                class: "quiet",
                onclick: move |_| adding_photos.toggle(),
                if adding_photos() { "Cancel adding" } else { "Add photos" }
            }
            button {
                id: "share-trip",
                r#type: "button",
                class: "quiet",
                onclick: move |_| sharing.toggle(),
                if sharing() { "Cancel sharing" } else { "Share" }
            }
            a { class: "quiet", href: api::original_gpx_url(&archive(), id), "Download original GPX" }
            DeleteTrip { id }
        }
        if adding_photos() {
            AddPhotos { id, on_added: move |_| on_photos_added.call(()) }
        }
        if sharing() {
            ShareForm { trip_ids: vec![id] }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{a_trip, render};

    #[test]
    fn the_occasional_controls_share_one_row() {
        let trip = a_trip("Oslo Hills Walk");

        let html = render(move || {
            rsx! {
                TripActions { trip: trip.clone(), on_saved: move |_| {}, on_photos_added: move |_| {} }
            }
        });

        let row = html
            .split(r#"class="trip-actions""#)
            .nth(1)
            .expect("the row")
            .to_string();
        for control in [
            r#"id="edit-trip""#,
            r#"id="add-photos""#,
            r#"id="share-trip""#,
            "/gpx",
            r#"id="delete-trip""#,
        ] {
            assert!(row.contains(control), "{control} in the row: {html}");
        }
    }

    #[test]
    fn no_form_is_open_until_it_is_asked_for() {
        let trip = a_trip("Oslo Hills Walk");

        let html = render(move || {
            rsx! {
                TripActions { trip: trip.clone(), on_saved: move |_| {}, on_photos_added: move |_| {} }
            }
        });

        assert!(!html.contains("add-photos-input"), "{html}");
        assert!(!html.contains("edit-trip-form"), "{html}");
    }

    #[test]
    fn deleting_stays_distinct_from_the_rest() {
        // Quiet like the others, but not disguised as one of them (US-9).
        let trip = a_trip("Oslo Hills Walk");

        let html = render(move || {
            rsx! {
                TripActions { trip: trip.clone(), on_saved: move |_| {}, on_photos_added: move |_| {} }
            }
        });

        assert!(html.contains(r#"class="quiet danger""#), "{html}");
    }
}
