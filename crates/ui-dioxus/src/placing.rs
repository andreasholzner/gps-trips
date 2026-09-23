//! Placing a photo on the map by hand (US-30): over the screen, a map of the
//! trip's track with the photo's current position on it, a tap to say where
//! the photo goes instead, and a warning first when that overwrites a
//! position the archive worked out itself.

use dioxus::prelude::*;
use trip_archive_types::{LocationSource, PhotoPlacement};

use crate::api::{self, ApiClient};
use crate::interop;
use crate::overlay::Overlay;
use crate::photos::PhotoView;
use crate::track;

/// What placing a photo by hand throws away, said before it happens — `None`
/// when there is nothing automatic to lose: a photo that was never placed,
/// or one the owner placed already.
pub fn overwrite_warning(source: LocationSource) -> Option<&'static str> {
    match source {
        LocationSource::Exif => {
            Some("This photo's position was read from its own GPS data. Saving replaces it.")
        }
        LocationSource::Interpolated => Some(
            "This photo's position was worked out from when it was taken along the track. \
             Saving replaces it.",
        ),
        LocationSource::Provided => {
            Some("This photo's position came from Komoot. Saving replaces it.")
        }
        LocationSource::None | LocationSource::Manual => None,
    }
}

/// A point picked on the map, as a position the archive accepts. Leaflet
/// reports a longitude off the ±180° range once the map has been panned
/// round the world, and a latitude a hair past a pole at the edge of its
/// projection; the place meant is the same, so it is wrapped and clamped
/// rather than refused.
///
/// A longitude already in range is left exactly as it is: wrapping it anyway
/// would cost it its last digits in the arithmetic.
pub fn picked([lat, lon]: [f64; 2]) -> PhotoPlacement {
    let lon = if (-180.0..=180.0).contains(&lon) {
        lon
    } else {
        (lon + 180.0).rem_euclid(360.0) - 180.0
    };
    PhotoPlacement {
        lat: lat.clamp(-90.0, 90.0),
        lon,
    }
}

/// The overlay, for `photo` of trip `id`. `on_placed` is told once the
/// archive has stored the new position; `on_close` closes it unchanged.
#[component]
pub fn PlacePhoto(
    id: i64,
    photo: PhotoView,
    on_placed: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    // Read again here rather than handed down: the detail screen's track
    // lives inside its map section, and this is opened rarely.
    let track = use_resource(move || async move { api::get_track(&archive(), id).await });
    let mut chosen = use_signal(|| None::<PhotoPlacement>);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let photo_id = photo.id;
    let current = photo.position;

    rsx! {
        Overlay { label: "Place {photo.name}", class: "placing", on_close,
            div { class: "panel",
                h2 { "Place {photo.name}" }
                p { "Tap the map where the photo was taken." }
                match &*track.read_unchecked() {
                    None => rsx! { p { "Loading the track…" } },
                    // The track is only a guide: without it the photo can
                    // still be placed, on the tiles alone.
                    Some(Err(err)) => rsx! {
                        p { class: "error", "Could not load the track: {err}" }
                        PlaceMap { points: Vec::new(), current, on_pick: move |at| chosen.set(Some(picked(at))) }
                    },
                    Some(Ok(track)) => rsx! {
                        PlaceMap {
                            points: track::polyline(track),
                            current,
                            on_pick: move |at| chosen.set(Some(picked(at))),
                        }
                    },
                }
                if let Some(warning) = overwrite_warning(photo.source) {
                    p { id: "place-warning", class: "warning", "{warning}" }
                }
                if let Some(message) = error() {
                    p { class: "error", "Could not place the photo: {message}" }
                }
                div { class: "form-actions",
                    button {
                        id: "place-save",
                        r#type: "button",
                        disabled: chosen().is_none() || saving(),
                        onclick: move |_| async move {
                            let Some(at) = chosen() else { return };
                            // One request per press: a second would race the
                            // first to the same row.
                            if saving() {
                                return;
                            }
                            saving.set(true);
                            error.set(None);
                            match api::place_photo(&archive(), id, photo_id, at).await {
                                Ok(_) => on_placed.call(()),
                                Err(err) => error.set(Some(err.to_string())),
                            }
                            saving.set(false);
                        },
                        "Save"
                    }
                    button {
                        r#type: "button",
                        class: "secondary",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                }
            }
        }
    }
}

/// The map's container. Rendered empty: Leaflet owns this subtree (ADR-0025).
/// Mounted once per placement, with the values it draws, so its script runs
/// exactly once; every pick comes back on the same channel.
#[component]
fn PlaceMap(
    points: Vec<[f64; 2]>,
    current: Option<[f64; 2]>,
    on_pick: EventHandler<[f64; 2]>,
) -> Element {
    use_future(move || {
        let points = points.clone();
        async move {
            let mut map = interop::start_place_map(points, current);
            while let Ok(at) = map.recv::<[f64; 2]>().await {
                on_pick.call(at);
            }
        }
    });

    rsx! { div { id: "place-map", class: "place-map" } }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::PhotoUpload;
    use crate::test_support::{import_sample, render_against_archive, serve_test_archive};

    fn a_view(source: LocationSource) -> PhotoView {
        PhotoView {
            thumbnail_url: "http://archive.test/media/thumb-a.jpg".to_string(),
            url: "http://archive.test/media/a.jpg".to_string(),
            name: "a.jpg".to_string(),
            caption: None,
            id: 1,
            position: Some([59.91, 10.75]),
            source,
        }
    }

    #[test]
    fn every_automatic_position_is_warned_about_before_it_is_replaced() {
        for source in [
            LocationSource::Exif,
            LocationSource::Interpolated,
            LocationSource::Provided,
        ] {
            let warning = overwrite_warning(source).expect("a warning");
            assert!(warning.contains("replaces"), "{source:?}: {warning}");
        }
        // Each says what would be lost, not the same sentence three times.
        assert_ne!(
            overwrite_warning(LocationSource::Exif),
            overwrite_warning(LocationSource::Interpolated)
        );
        assert!(overwrite_warning(LocationSource::Provided)
            .unwrap()
            .contains("Komoot"));
    }

    #[test]
    fn nothing_is_warned_about_when_nothing_automatic_is_lost() {
        assert_eq!(overwrite_warning(LocationSource::None), None);
        assert_eq!(overwrite_warning(LocationSource::Manual), None);
    }

    #[test]
    fn a_pick_is_kept_as_it_was_made_on_the_first_copy_of_the_world() {
        assert_eq!(
            picked([59.95, 10.8]),
            PhotoPlacement {
                lat: 59.95,
                lon: 10.8
            }
        );
    }

    #[test]
    fn a_pick_on_a_map_panned_round_the_world_lands_where_it_was_meant() {
        let at = picked([59.91, 370.75]);
        assert!((at.lon - 10.75).abs() < 1e-9, "{at:?}");
        let at = picked([59.91, -349.25]);
        assert!((at.lon - 10.75).abs() < 1e-9, "{at:?}");
        assert_eq!(picked([90.4, 0.0]).lat, 90.0);
        assert_eq!(picked([-90.4, 0.0]).lat, -90.0);
    }

    async fn placing(archive: &ApiClient, photo: PhotoView) -> String {
        let id = import_sample(archive, &[]).await;
        render_against_archive(
            archive,
            move || {
                rsx! {
                    PlacePhoto { id, photo: photo.clone(), on_placed: move |_| {}, on_close: move |_| {} }
                }
            },
            |html| html.contains("place-map"),
        )
        .await
    }

    #[tokio::test]
    async fn an_automatic_position_is_warned_about_on_the_map_it_is_replaced_on() {
        let (archive, _dir) = serve_test_archive().await;

        let html = placing(&archive, a_view(LocationSource::Exif)).await;

        assert!(html.contains("Place a.jpg"), "{html}");
        assert!(html.contains(r#"id="place-warning""#), "{html}");
        assert!(html.contains("GPS"), "{html}");
        // The map's container is Leaflet's, so Dioxus leaves it empty.
        assert!(
            html.contains(r#"<div id="place-map" class="place-map"></div>"#),
            "{html}"
        );
    }

    #[tokio::test]
    async fn an_unplaced_photo_is_placed_without_a_warning() {
        let (archive, _dir) = serve_test_archive().await;
        let photo = PhotoView {
            position: None,
            ..a_view(LocationSource::None)
        };

        let html = placing(&archive, photo).await;

        assert!(!html.contains("place-warning"), "{html}");
    }

    #[tokio::test]
    async fn nothing_is_saved_until_a_point_is_picked() {
        let (archive, _dir) = serve_test_archive().await;

        let html = placing(&archive, a_view(LocationSource::Exif)).await;

        let save = html.split(r#"id="place-save""#).nth(1).expect("the button");
        let save = &save[..save.find('>').unwrap()];
        assert!(save.contains("disabled"), "{html}");
    }

    // The request the Save button makes, against a real archive.
    #[tokio::test]
    async fn a_placed_photo_is_stored_where_it_was_put() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;
        api::add_photos(
            &archive,
            id,
            vec![PhotoUpload {
                file_name: "later.jpg".to_string(),
                content_type: Some("image/jpeg".to_string()),
                bytes: b"\xFF\xD8\xFF-fake-jpeg".to_vec(),
            }],
        )
        .await
        .expect("upload");
        let photo = api::list_photos(&archive, id).await.expect("photos")[0].id;

        let placed = api::place_photo(&archive, id, photo, picked([59.95, 10.8]))
            .await
            .expect("placed");

        assert_eq!(placed.location_source, LocationSource::Manual);
        let stored = &api::list_photos(&archive, id).await.expect("photos")[0];
        assert_eq!((stored.lat, stored.lon), (Some(59.95), Some(10.8)));
        assert_eq!(stored.location_source, LocationSource::Manual);
    }

    #[tokio::test]
    async fn a_photo_the_trip_does_not_have_is_not_placed() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;

        let err = api::place_photo(&archive, id, 9_999, picked([59.95, 10.8]))
            .await
            .expect_err("no such photo");

        assert!(err.is_not_found(), "{err}");
    }
}
