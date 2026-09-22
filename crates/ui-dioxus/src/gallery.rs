//! The trip's photos as a gallery on the detail screen (US-7), every one by
//! its thumbnail (US-5).

use dioxus::prelude::*;
use trip_archive_types::PhotoResponse;

use crate::photos::{absolute, caption, captions_need_dates};

/// The gallery: every photo as a thumbnail (US-5 guarantees there is always
/// one to use — the full-size image stands in when none could be made),
/// captioned with when it was taken (US-62). A thumbnail is a button:
/// `on_open` is told which photo to open the viewer on.
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
                                    title: "Open {photo.original_name}",
                                    onclick: move |_| on_open.call(index),
                                    img {
                                        src: absolute(&base_url, &photo.thumbnail_url),
                                        alt: "{photo.original_name}",
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
}
