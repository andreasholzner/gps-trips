//! The trip's photos on the detail screen (US-2/US-7): the gallery, the
//! markers that put them on the track map (US-3/US-4), and the control for
//! adding more after the import.

use dioxus::prelude::*;
use serde::Serialize;
use trip_archive_types::PhotoResponse;

use crate::api::{self, PhotoUpload};

/// A photo the map draws: where it is, what to show in its popup, and what
/// to call it. Prepared here rather than in the drawing script — the script
/// renders, Rust decides (ADR-0025) — which is also what makes the choice of
/// *which* photos appear (US-3/US-4) testable without a browser.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhotoMarker {
    pub lat: f64,
    pub lon: f64,
    pub thumbnail_url: String,
    pub name: String,
}

/// The markers for a trip's photos: one per photo that has a position,
/// however it was determined — read from EXIF (US-3) or interpolated from
/// the track by timestamp (US-4). A photo with neither is left off the map,
/// which is US-4's "left unplaced and not shown".
pub fn photo_markers(base_url: &str, photos: &[PhotoResponse]) -> Vec<PhotoMarker> {
    photos
        .iter()
        .filter_map(|photo| {
            Some(PhotoMarker {
                lat: photo.lat?,
                lon: photo.lon?,
                thumbnail_url: absolute(base_url, &photo.thumbnail_url),
                name: photo.original_name.clone(),
            })
        })
        .collect()
}

/// A photo URL the archive gave as a path, resolved against the archive it
/// came from. On the web `base_url` is the page's own origin and this changes
/// nothing that matters; on Android the app is not served from the archive at
/// all (US-16), and a bare path would resolve against the webview instead.
///
/// A `BlobStore` is free to hand back a URL of its own instead of a path
/// (ADR-0007 exists so a remote backend can), and that one is already
/// absolute — resolving it again would corrupt it.
pub fn absolute(base_url: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }
    format!("{base_url}{url}")
}

/// The gallery: every photo as a thumbnail (US-5 guarantees there is always
/// one to use — the full-size image stands in when none could be made).
///
/// `error` is what the archive said when the photos could not be read. It is
/// shown instead of the empty state, because "no photos yet" is a claim about
/// the trip and a failed fetch is not evidence for it.
#[component]
pub fn PhotoGallery(
    photos: Vec<PhotoResponse>,
    base_url: String,
    #[props(default)] error: Option<String>,
) -> Element {
    rsx! {
        h2 { "Photos" }
        if let Some(error) = error {
            p { class: "error", "Could not load the photos: {error}" }
        } else if photos.is_empty() {
            p { "No photos yet." }
        } else {
            div { class: "gallery",
                for photo in photos {
                    img {
                        key: "{photo.id}",
                        src: absolute(&base_url, &photo.thumbnail_url),
                        alt: "{photo.original_name}",
                    }
                }
            }
        }
    }
}

/// Adding photos to a trip that already exists (US-2). The files are read in
/// the browser and posted to the same multipart endpoint the import form uses
/// (ADR-0004); `on_added` tells the screen to re-read its photos.
#[component]
pub fn AddPhotos(id: i64, on_added: EventHandler<()>) -> Element {
    let base_url = use_context::<Signal<String>>();
    let mut chosen = use_signal(Vec::<PhotoUpload>::new);
    let mut status = use_signal(|| None::<String>);

    rsx! {
        form {
            class: "add-photos",
            onsubmit: move |event| async move {
                event.prevent_default();
                let photos = chosen.read().clone();
                if photos.is_empty() {
                    status.set(Some("Choose one or more photos first.".to_string()));
                    return;
                }
                match api::add_photos(&base_url(), id, photos).await {
                    Ok(()) => {
                        chosen.take();
                        status.set(None);
                        on_added.call(());
                    }
                    // The selection is kept on purpose: the owner presses the
                    // button again rather than picking every file a second
                    // time.
                    Err(err) => status.set(Some(format!("Could not add the photos: {err}"))),
                }
            },
            input {
                r#type: "file",
                accept: "image/*",
                multiple: true,
                // Reading the bytes is the browser's job and it is async, so
                // the chosen files are held here until the owner submits.
                onchange: move |event: FormEvent| async move {
                    // Whatever was staged belongs to the previous selection;
                    // it must not survive a pick that then fails to read, or
                    // the button would upload files the input no longer names.
                    chosen.take();
                    let mut uploads = Vec::new();
                    for file in event.files() {
                        match file.read_bytes().await {
                            Ok(bytes) => uploads.push(PhotoUpload {
                                file_name: file.name(),
                                content_type: file.content_type(),
                                bytes: bytes.to_vec(),
                            }),
                            Err(err) => {
                                status.set(Some(format!("Could not read {}: {err}", file.name())));
                                return;
                            }
                        }
                    }
                    chosen.set(uploads);
                },
            }
            button { r#type: "submit", "Add photos" }
        }
        if let Some(message) = status() {
            p { class: "error", "{message}" }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;
    use trip_archive_types::LocationSource;

    fn a_photo(id: i64, name: &str, at: Option<(f64, f64)>) -> PhotoResponse {
        PhotoResponse {
            id,
            trip_id: 1,
            original_name: name.to_string(),
            content_type: Some("image/jpeg".to_string()),
            byte_len: 1024,
            created_at: "2026-07-11T09:30:00Z".to_string(),
            url: format!("/media/trips/1/{name}"),
            thumbnail_url: format!("/media/trips/1/thumb-{name}"),
            lat: at.map(|(lat, _)| lat),
            lon: at.map(|(_, lon)| lon),
            location_source: match at {
                Some(_) => LocationSource::Exif,
                None => LocationSource::None,
            },
        }
    }

    // US-3 and US-4: a photo is on the map when it has a position, however it
    // got one. US-4's unplaced photo — outside the track's time range — has
    // none, and is left off rather than guessed at.
    #[test]
    fn only_photos_with_a_position_become_map_markers() {
        let photos = vec![
            a_photo(1, "geotagged.jpg", Some((59.91, 10.75))),
            a_photo(2, "unplaced.jpg", None),
        ];

        let markers = photo_markers("http://archive.test", &photos);

        assert_eq!(markers.len(), 1, "{markers:?}");
        assert_eq!(markers[0].lat, 59.91);
        assert_eq!(markers[0].lon, 10.75);
        assert_eq!(markers[0].name, "geotagged.jpg");
    }

    #[test]
    fn a_photos_url_is_resolved_against_the_archive_it_came_from() {
        // The archive serves photo blobs under a path of its own; on Android
        // the app is not served from that origin at all (US-16), so the URL
        // the markers and the gallery use is made absolute here rather than
        // left for a webview to resolve against itself.
        assert_eq!(
            absolute("http://archive.test", "/media/trips/1/a.jpg"),
            "http://archive.test/media/trips/1/a.jpg"
        );
        // The web build fetches from its own origin, which is what `base_url`
        // already is; an empty one leaves the path alone.
        assert_eq!(absolute("", "/media/trips/1/a.jpg"), "/media/trips/1/a.jpg");
        // A blob store that serves its own URLs (ADR-0007) has already
        // resolved it; resolving it again would corrupt it.
        assert_eq!(
            absolute("http://archive.test", "https://bucket.example/a.jpg"),
            "https://bucket.example/a.jpg"
        );
    }

    #[test]
    fn the_gallery_shows_every_photo_by_its_thumbnail() {
        let photos = vec![
            a_photo(1, "first.jpg", None),
            a_photo(2, "second.jpg", Some((59.91, 10.75))),
        ];

        let html = render(move || {
            rsx! { PhotoGallery { photos: photos.clone(), base_url: "http://archive.test".to_string() } }
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
            rsx! { PhotoGallery { photos: Vec::new(), base_url: String::new() } }
        });

        assert!(html.contains("No photos yet"), "{html}");
    }

    // US-2's other half, on the screen: photos can be added at a later time.
    // Choosing files and clicking are real events (the browser layer); that
    // the control is offered at all is assertable here.
    #[test]
    fn a_gallery_that_could_not_be_read_says_so_instead_of_claiming_emptiness() {
        // "No photos yet" is a claim about the trip; a failed fetch is not
        // evidence for it.
        let html = render(|| {
            rsx! {
                PhotoGallery {
                    photos: Vec::new(),
                    base_url: String::new(),
                    error: Some("the archive is unreachable".to_string()),
                }
            }
        });

        assert!(html.contains("the archive is unreachable"), "{html}");
        assert!(!html.contains("No photos yet"), "{html}");
    }

    #[test]
    fn the_screen_offers_a_way_to_add_photos_later() {
        let html = render(|| {
            rsx! { AddPhotos { id: 1, on_added: move |_| {} } }
        });

        assert!(html.contains(r#"type="file""#), "{html}");
        assert!(html.contains("multiple"), "{html}");
        assert!(html.contains("Add photos"), "{html}");
    }
}
