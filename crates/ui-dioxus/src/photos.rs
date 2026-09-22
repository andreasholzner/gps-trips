//! The trip's photos on the detail screen (US-2/US-7): the gallery, the
//! markers that put them on the track map (US-3/US-4), and the control for
//! adding more after the import.

use dioxus::prelude::*;
use serde::Serialize;
use trip_archive_types::PhotoResponse;

use crate::api::{self, ApiClient, ApiError, PhotoUpload};
use crate::import::batches;
use crate::interop;

/// One marker on the map: where it is, and every photo it stands for
/// (US-57). Prepared here rather than in the drawing script — the script
/// renders, Rust decides (ADR-0025) — which is also what makes the choice of
/// *which* photos appear (US-3/US-4) and which of them share a marker
/// testable without a browser.
///
/// `photos` is never empty, and its first entry is the seed that fixed the
/// marker's position. The script reads the count off it rather than being
/// sent one, so there is no second number that can disagree with the list.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhotoMarker {
    pub lat: f64,
    pub lon: f64,
    pub photos: Vec<MarkerPhoto>,
}

/// A photo inside a marker's popup: the thumbnail to show and what to call
/// it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MarkerPhoto {
    pub thumbnail_url: String,
    pub name: String,
}

/// How close two photos must be to share a marker (US-57).
const GROUPING_RADIUS_M: f64 = 20.0;

/// The mean Earth radius, as `geo` uses it for the haversine distances the
/// server computes — so the two agree about what a metre is.
const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// The markers for a trip's photos: one per group of photos taken at the
/// same place, from those that have a position, however it was determined —
/// read from EXIF (US-3) or interpolated from the track by timestamp (US-4).
/// A photo with neither is left off the map, which is US-4's "left unplaced
/// and not shown".
///
/// Grouping is seed-anchored (US-57): the first photo of a group fixes its
/// position, and a later one joins only if it is within [`GROUPING_RADIUS_M`]
/// *of that seed*. That is deterministic in the order the API lists them, and
/// it bounds a group at the threshold — merging transitively would chain
/// photos 15 m apart into a group far wider than the marker claims.
pub fn photo_markers(base_url: &str, photos: &[PhotoResponse]) -> Vec<PhotoMarker> {
    let mut markers: Vec<PhotoMarker> = Vec::new();
    for photo in photos {
        let (Some(lat), Some(lon)) = (photo.lat, photo.lon) else {
            continue;
        };
        let here = MarkerPhoto {
            thumbnail_url: absolute(base_url, &photo.thumbnail_url),
            name: photo.original_name.clone(),
        };
        match markers.iter_mut().find(|marker| {
            metres_between((marker.lat, marker.lon), (lat, lon)) <= GROUPING_RADIUS_M
        }) {
            Some(marker) => marker.photos.push(here),
            None => markers.push(PhotoMarker {
                lat,
                lon,
                photos: vec![here],
            }),
        }
    }
    markers
}

/// The distance in metres between two `(lat, lon)` positions, as an
/// equirectangular approximation: the latitudes are close enough here that
/// treating the patch between them as flat is sub-metre accurate, which is
/// well inside what a 20 m threshold needs — and not a reason to pull `geo`
/// into a crate that does not depend on it.
fn metres_between((lat_a, lon_a): (f64, f64), (lat_b, lon_b): (f64, f64)) -> f64 {
    let mid_lat = ((lat_a + lat_b) / 2.0).to_radians();
    let x = (lon_b - lon_a).to_radians() * mid_lat.cos();
    let y = (lat_b - lat_a).to_radians();
    EARTH_RADIUS_M * x.hypot(y)
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

/// An upload that stopped part-way: how many photos the archive already
/// holds, and why the rest did not arrive.
#[derive(Debug)]
pub struct PartialUpload {
    pub uploaded: usize,
    pub error: ApiError,
}

/// Add `photos` to trip `id` a batch at a time ([`batches`]), reporting the
/// running count after each request. Every screen that uploads photos goes
/// through here, so none sends them all at once: the archive holds a whole
/// request in memory while it stores the photos (US-54).
pub async fn upload_in_batches(
    archive: &ApiClient,
    id: i64,
    photos: Vec<PhotoUpload>,
    mut on_batch: impl FnMut(usize),
) -> Result<(), PartialUpload> {
    let mut uploaded = 0;
    for batch in batches(photos) {
        let sending = batch.len();
        api::add_photos(archive, id, batch)
            .await
            .map_err(|error| PartialUpload { uploaded, error })?;
        uploaded += sending;
        on_batch(uploaded);
    }
    Ok(())
}

/// Adding photos to a trip that already exists (US-2). The files are read in
/// the browser and posted to the same multipart endpoint the import form uses
/// (ADR-0004); `on_added` tells the screen to re-read its photos.
#[component]
pub fn AddPhotos(id: i64, on_added: EventHandler<()>) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
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
                match upload_in_batches(&archive(), id, photos, |_| {}).await {
                    Ok(()) => {
                        chosen.take();
                        // Nothing else can empty a file input, and one still
                        // naming uploaded files contradicts the button, which
                        // would answer "choose one or more photos first".
                        interop::clear_photo_picker().await;
                        status.set(None);
                        on_added.call(());
                    }
                    // What did not arrive stays selected on purpose: the owner
                    // presses the button again rather than picking every file a
                    // second time — and what did arrive must not go up twice.
                    Err(PartialUpload { uploaded, error }) => {
                        chosen.write().drain(..uploaded);
                        if uploaded > 0 {
                            on_added.call(());
                        }
                        status.set(Some(format!("Could not add the photos: {error}")));
                    }
                }
            },
            input {
                id: "add-photos-input",
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
    use crate::test_support::{import_sample, render, serve_test_archive};
    use trip_archive_types::LocationSource;

    fn a_photo(id: i64, name: &str, at: Option<(f64, f64)>) -> PhotoResponse {
        PhotoResponse {
            id,
            trip_id: 1,
            original_name: name.to_string(),
            content_type: Some("image/jpeg".to_string()),
            byte_len: 1024,
            created_at: "2026-07-11T09:30:00Z".to_string(),
            taken_at: None,
            taken_offset_secs: None,
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
        assert_eq!(markers[0].photos.len(), 1);
        assert_eq!(markers[0].photos[0].name, "geotagged.jpg");
        assert_eq!(
            markers[0].photos[0].thumbnail_url,
            "http://archive.test/media/trips/1/thumb-geotagged.jpg"
        );
    }

    // ── US-57: every photo taken at the same place is reachable ──────────

    /// A latitude `metres` north of `lat` — one degree of latitude is the
    /// same distance everywhere, so this needs no longitude.
    fn north_of(lat: f64, metres: f64) -> f64 {
        lat + metres / (EARTH_RADIUS_M * std::f64::consts::PI / 180.0)
    }

    #[test]
    fn photos_taken_at_the_same_place_share_one_marker_that_says_how_many() {
        // What US-4's interpolation produces routinely: several photos
        // snapped onto one track point, which used to stack markers so that
        // only the topmost could be clicked.
        let photos = vec![
            a_photo(1, "first.jpg", Some((59.91, 10.75))),
            a_photo(2, "second.jpg", Some((59.91, 10.75))),
            a_photo(3, "third.jpg", Some((north_of(59.91, 5.0), 10.75))),
        ];

        let markers = photo_markers("http://archive.test", &photos);

        assert_eq!(markers.len(), 1, "{markers:?}");
        // The seed fixes the position, and the photos keep the order the API
        // listed them in.
        assert_eq!(markers[0].lat, 59.91);
        assert_eq!(
            markers[0]
                .photos
                .iter()
                .map(|photo| photo.name.as_str())
                .collect::<Vec<_>>(),
            ["first.jpg", "second.jpg", "third.jpg"]
        );
    }

    #[test]
    fn a_photo_further_than_twenty_metres_away_keeps_its_own_marker() {
        let photos = vec![
            a_photo(1, "here.jpg", Some((59.91, 10.75))),
            a_photo(2, "over-there.jpg", Some((north_of(59.91, 25.0), 10.75))),
        ];

        let markers = photo_markers("http://archive.test", &photos);

        assert_eq!(markers.len(), 2, "{markers:?}");
        assert_eq!(markers[0].photos[0].name, "here.jpg");
        assert_eq!(markers[1].photos[0].name, "over-there.jpg");
    }

    #[test]
    fn grouping_is_anchored_on_the_seed_rather_than_chaining_along_a_line() {
        // Three photos 15 m apart in a line. Merging transitively would put
        // all three in one group 30 m wide — wider than the threshold the
        // marker claims. Each photo is measured against its group's seed, so
        // the third opens a group of its own.
        let photos = vec![
            a_photo(1, "a.jpg", Some((59.91, 10.75))),
            a_photo(2, "b.jpg", Some((north_of(59.91, 15.0), 10.75))),
            a_photo(3, "c.jpg", Some((north_of(59.91, 30.0), 10.75))),
        ];

        let markers = photo_markers("http://archive.test", &photos);

        assert_eq!(markers.len(), 2, "{markers:?}");
        assert_eq!(
            markers[0]
                .photos
                .iter()
                .map(|photo| photo.name.as_str())
                .collect::<Vec<_>>(),
            ["a.jpg", "b.jpg"]
        );
        assert_eq!(markers[1].photos[0].name, "c.jpg");
    }

    #[test]
    fn distance_is_metres_on_the_ground_in_both_directions() {
        // Sub-metre at this threshold is all the grouping needs, which is why
        // it is an equirectangular approximation computed here rather than a
        // reason to pull `geo` into this crate.
        let oslo = (59.91, 10.75);

        let north = metres_between(oslo, (north_of(59.91, 15.0), 10.75));
        assert!((north - 15.0).abs() < 0.01, "{north}");

        // A degree of longitude shortens with the cosine of the latitude:
        // ~55.6 m per 0.001° at 59.91°N, not the ~111 m it would be at the
        // equator.
        let east = metres_between(oslo, (59.91, 10.751));
        assert!((east - 55.7).abs() < 0.5, "{east}");
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

    // ── US-54: no single request carries every chosen photo ────────────

    fn uploads(count: usize) -> Vec<PhotoUpload> {
        (0..count)
            .map(|i| PhotoUpload {
                file_name: format!("{i}.jpg"),
                content_type: Some("image/jpeg".to_string()),
                bytes: b"\xFF\xD8\xFF-fake-jpeg".to_vec(),
            })
            .collect()
    }

    #[tokio::test]
    async fn photos_added_to_a_trip_travel_in_batches_and_all_arrive() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;
        let mut reported = Vec::new();

        upload_in_batches(&archive, id, uploads(20), |done| reported.push(done))
            .await
            .expect("upload");

        // One report per request: 20 photos, `import::batches`' 8 to a request.
        assert_eq!(reported, vec![8, 16, 20]);
        assert_eq!(
            api::list_photos(&archive, id).await.expect("photos").len(),
            20
        );
    }

    #[tokio::test]
    async fn a_failed_batch_says_how_many_photos_made_it_before_it() {
        let (archive, _dir) = serve_test_archive().await;

        let failed = upload_in_batches(&archive, 9_999, uploads(3), |_| {})
            .await
            .expect_err("there is no such trip to add to");

        assert_eq!(failed.uploaded, 0);
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
