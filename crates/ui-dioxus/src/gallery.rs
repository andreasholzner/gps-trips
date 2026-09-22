//! The trip's photos as a gallery on the detail screen (US-7), every one by
//! its thumbnail (US-5).

use dioxus::prelude::*;
use trip_archive_types::PhotoResponse;

use crate::photos::absolute;

/// The gallery: every photo as a thumbnail (US-5 guarantees there is always
/// one to use — the full-size image stands in when none could be made).
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
            Some(photos) => rsx! {
                div { class: "gallery",
                    for photo in photos {
                        img {
                            key: "{photo.id}",
                            src: absolute(&base_url, &photo.thumbnail_url),
                            alt: "{photo.original_name}",
                        }
                    }
                }
            },
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
}
