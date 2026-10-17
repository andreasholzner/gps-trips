//! The trip's photos as a gallery on the detail screen (US-7), every one by
//! its thumbnail (US-5), those still to place marked (US-106).

use dioxus::prelude::*;
use trip_archive_types::PhotoResponse;

use crate::photos::{absolute, caption, captions_need_dates};

/// A map pin (`mdiMapMarker`, Material Design Icons by Pictogrammers,
/// Apache-2.0, `@mdi/js` 7.4.47); the badge strikes it out (US-106).
const PIN: &str = "M12,11.5A2.5,2.5 0 0,1 9.5,9A2.5,2.5 0 0,1 12,6.5A2.5,2.5 0 0,1 14.5,9A2.5,2.5 0 0,1 12,11.5M12,2A7,7 0 0,0 5,9C5,14.25 12,22 12,22C12,22 19,14.25 19,9A7,7 0 0,0 12,2Z";

/// The gallery: every photo as a thumbnail (US-5 guarantees there is always
/// one to use — the full-size image stands in when none could be made),
/// captioned with when it was taken (US-62). A thumbnail is a button:
/// `on_open` is told which photo to open the viewer on. A photo the server
/// says needs placing (US-106) carries a crossed-out pin in its corner,
/// inside the button so it names the button too.
///
/// `photos` is `None` until the trip's photos have been read — distinct from
/// an empty list, because "no photos yet" is a claim about the trip and a
/// read still in flight is not evidence for it. `error` is what the archive
/// said when that read failed, and is shown alongside whatever is already on
/// screen rather than replacing it.
#[component]
pub fn PhotoGallery(
    photos: Option<Vec<PhotoResponse>>,
    base_url: String,
    #[props(default)] error: Option<String>,
    #[props(default)] on_open: EventHandler<usize>,
) -> Element {
    rsx! {
        h2 { "Photos" }
        if let Some(error) = error.clone() {
            p { class: "error", "Could not load the photos: {error}" }
        }
        match photos {
            None if error.is_none() => rsx! { p { "Loading the photos…" } },
            None => rsx! {},
            Some(photos) if photos.is_empty() => rsx! {
                if error.is_none() {
                    p { "No photos yet." }
                }
            },
            Some(photos) => {
                let with_date = captions_need_dates(&photos);
                rsx! {
                    div { class: "gallery",
                        for (index, photo) in photos.into_iter().enumerate() {
                            figure { key: "{photo.id}",
                                button {
                                    r#type: "button",
                                    title: if photo.needs_placing {
                                        "Open {photo.original_name} — needs placing"
                                    } else {
                                        "Open {photo.original_name}"
                                    },
                                    onclick: move |_| on_open.call(index),
                                    img {
                                        src: absolute(&base_url, &photo.thumbnail_url),
                                        alt: "{photo.original_name}",
                                    }
                                    if photo.needs_placing {
                                        NeedsPlacing {}
                                    }
                                }
                                if let Some(caption) = caption(&photo, with_date) {
                                    figcaption { "{caption}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The badge of a photo still to place: a crossed-out map pin on a light
/// disc. The stroke under the strike, in the disc's color, keeps it apart
/// from the pin.
#[component]
fn NeedsPlacing() -> Element {
    rsx! {
        span { class: "needs-placing", role: "img", aria_label: "needs placing",
            svg { view_box: "0 0 24 24", "aria-hidden": "true",
                path { d: PIN, fill: "currentColor" }
                line { class: "gap", x1: "3", y1: "3", x2: "21", y2: "21" }
                line { x1: "3", y1: "3", x2: "21", y2: "21" }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{a_photo, render};

    #[test]
    fn the_gallery_shows_every_photo_by_its_thumbnail() {
        let photos = vec![
            a_photo(1, "first.jpg", None),
            a_photo(2, "second.jpg", Some((59.91, 10.75))),
        ];

        let html = render(move || {
            rsx! {
                PhotoGallery {
                    photos: Some(photos.clone()),
                    base_url: "http://archive.test".to_string(),
                }
            }
        });

        assert!(
            html.contains("http://archive.test/media/trips/1/thumb-first.jpg"),
            "{html}"
        );
        assert!(html.contains("thumb-second.jpg"), "{html}");
        // The name is the alt text: a gallery of unlabelled images is no use
        // to a screen reader.
        assert!(html.contains(r#"alt="first.jpg""#), "{html}");
    }

    #[test]
    fn a_trip_with_no_photos_says_so() {
        let html = render(|| {
            rsx! { PhotoGallery { photos: Some(Vec::new()), base_url: String::new() } }
        });

        assert!(html.contains("No photos yet"), "{html}");
    }

    #[test]
    fn a_gallery_that_could_not_be_read_says_so_instead_of_claiming_emptiness() {
        // "No photos yet" is a claim about the trip; a failed fetch is not
        // evidence for it.
        let html = render(|| {
            rsx! {
                PhotoGallery {
                    photos: Some(Vec::new()),
                    base_url: String::new(),
                    error: Some("the archive is unreachable".to_string()),
                }
            }
        });

        assert!(html.contains("the archive is unreachable"), "{html}");
        assert!(!html.contains("No photos yet"), "{html}");
    }

    #[test]
    fn a_gallery_that_failed_to_refresh_keeps_what_is_already_on_screen() {
        // The refresh after an upload can fail; the photos already read are
        // still true, and blanking them would be a second, invented loss.
        let photos = vec![a_photo(1, "first.jpg", None)];

        let html = render(move || {
            rsx! {
                PhotoGallery {
                    photos: Some(photos.clone()),
                    base_url: String::new(),
                    error: Some("the archive is unreachable".to_string()),
                }
            }
        });

        assert!(html.contains("the archive is unreachable"), "{html}");
        assert!(html.contains("thumb-first.jpg"), "{html}");
    }

    #[test]
    fn a_gallery_that_has_not_been_read_yet_claims_nothing_about_the_trip() {
        // "No photos yet" is a claim; a read still in flight is not evidence
        // for it, any more than a failed one is.
        let html = render(|| {
            rsx! { PhotoGallery { photos: None, base_url: String::new() } }
        });

        assert!(!html.contains("No photos yet"), "{html}");
        assert!(html.contains("Loading"), "{html}");
    }

    // ── US-62: when each was taken, and a way to look at it properly ─────

    fn taken(photo: PhotoResponse, at: &str) -> PhotoResponse {
        PhotoResponse {
            taken_at: Some(at.to_string()),
            taken_offset_secs: Some(7200),
            ..photo
        }
    }

    #[test]
    fn each_thumbnail_is_captioned_with_when_it_was_taken() {
        let photos = vec![
            taken(a_photo(1, "first.jpg", None), "2024-06-01T08:15:00Z"),
            a_photo(2, "untimed.jpg", None),
        ];

        let html = render(move || {
            rsx! { PhotoGallery { photos: Some(photos.clone()), base_url: String::new() } }
        });

        assert!(
            html.contains("<figcaption>10:15 (+02:00)</figcaption>"),
            "{html}"
        );
        assert_eq!(
            html.matches("<figcaption").count(),
            1,
            "no caption for a photo whose EXIF named no time: {html}"
        );
    }

    #[test]
    fn each_thumbnail_opens_the_photo() {
        let photos = vec![
            a_photo(1, "first.jpg", None),
            a_photo(2, "second.jpg", None),
        ];

        let html = render(move || {
            rsx! { PhotoGallery { photos: Some(photos.clone()), base_url: String::new() } }
        });

        assert_eq!(
            html.matches(r#"<button type="button""#).count(),
            2,
            "{html}"
        );
        assert!(html.contains(r#"title="Open first.jpg""#), "{html}");
    }

    // ── US-106: the photos still to place ────────────────────────────────

    fn to_place(photo: PhotoResponse) -> PhotoResponse {
        PhotoResponse {
            needs_placing: true,
            ..photo
        }
    }

    #[test]
    fn us106_a_photo_to_place_is_badged_and_named_as_needing_placing() {
        let photos = vec![
            to_place(a_photo(1, "unplaced.jpg", None)),
            a_photo(2, "placed.jpg", Some((59.91, 10.75))),
        ];

        let html = render(move || {
            rsx! { PhotoGallery { photos: Some(photos.clone()), base_url: String::new() } }
        });

        assert_eq!(
            html.matches(r#"class="needs-placing""#).count(),
            1,
            "{html}"
        );
        // Inside the thumbnail's button, so a screen reader names the button
        // by it too.
        let unplaced = html.find("thumb-unplaced.jpg").unwrap();
        let badge = html.find(r#"class="needs-placing""#).unwrap();
        let placed = html.find("thumb-placed.jpg").unwrap();
        assert!(unplaced < badge && badge < placed, "{html}");
        assert!(html.contains(r#"aria-label="needs placing""#), "{html}");
        assert!(
            html.contains(r#"title="Open unplaced.jpg — needs placing""#),
            "{html}"
        );
        assert!(html.contains(r#"title="Open placed.jpg""#), "{html}");
    }

    #[test]
    fn us106_a_gallery_with_nothing_to_place_has_no_badge() {
        let photos = vec![
            a_photo(1, "first.jpg", Some((59.91, 10.75))),
            taken(a_photo(2, "second.jpg", None), "2024-06-01T08:15:00Z"),
        ];

        let html = render(move || {
            rsx! { PhotoGallery { photos: Some(photos.clone()), base_url: String::new() } }
        });

        assert!(!html.contains("needs-placing"), "{html}");
        assert!(!html.contains("needs placing"), "{html}");
    }
}
