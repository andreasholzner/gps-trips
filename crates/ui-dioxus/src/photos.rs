//! The trip's photos on the detail screen (US-2/US-7): the markers that put
//! them on the track map (US-3/US-4), what the gallery and the viewer show of
//! each (US-62), and the control for adding more after the import.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use time::UtcOffset;
use trip_archive_types::PhotoResponse;

use crate::api::{self, ApiClient, ApiError, PhotoUpload};
use crate::format;
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
    pub photos: Vec<PhotoView>,
}

/// A photo as the screen shows it — in a marker's popup, and in the viewer
/// either that popup or the gallery opens (US-62): the thumbnail, the image
/// at the size the archive holds it (ADR-0026's bounded copy), what to call
/// it, and when it was taken. Serialized because a marker's photos travel to
/// the map's drawing script, and back as the set to browse.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhotoView {
    pub thumbnail_url: String,
    pub url: String,
    pub name: String,
    pub caption: Option<String>,
}

/// Every photo as the viewer browses the gallery: all of them, in gallery
/// order (US-62).
pub fn photo_views(base_url: &str, photos: &[PhotoResponse]) -> Vec<PhotoView> {
    let with_date = captions_need_dates(photos);
    photos
        .iter()
        .map(|photo| view_of(base_url, photo, with_date))
        .collect()
}

fn view_of(base_url: &str, photo: &PhotoResponse, with_date: bool) -> PhotoView {
    PhotoView {
        thumbnail_url: absolute(base_url, &photo.thumbnail_url),
        url: absolute(base_url, &photo.url),
        name: photo.original_name.clone(),
        caption: caption(photo, with_date),
    }
}

/// When a photo was taken, in the zone it was taken in, through the same
/// rendering as the readout under the chart (US-62). `None` for a photo whose
/// EXIF named no time: no caption, rather than a dash that claims a missing
/// value.
pub fn caption(photo: &PhotoResponse, with_date: bool) -> Option<String> {
    let at = format::instant(photo.taken_at.as_deref()?)?;
    Some(format::clock(at, offset_of(photo), with_date))
}

/// Whether the photos were taken on more than one local date — then every
/// caption says which, and otherwise none repeats it (US-62, the readout's
/// rule applied to the photos themselves: the gallery has no track to ask).
pub fn captions_need_dates(photos: &[PhotoResponse]) -> bool {
    let mut dates = photos.iter().filter_map(|photo| {
        let at = format::instant(photo.taken_at.as_deref()?)?;
        Some(
            at.to_offset(offset_of(photo).unwrap_or(UtcOffset::UTC))
                .date(),
        )
    });
    match dates.next() {
        Some(first) => dates.any(|date| date != first),
        None => false,
    }
}

fn offset_of(photo: &PhotoResponse) -> Option<UtcOffset> {
    UtcOffset::from_whole_seconds(photo.taken_offset_secs?).ok()
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
    // Dated or not by the trip's photos as a whole, as the gallery's are, so
    // a photo reads the same wherever it is opened from.
    let with_date = captions_need_dates(photos);
    let mut markers: Vec<PhotoMarker> = Vec::new();
    for photo in photos {
        let (Some(lat), Some(lon)) = (photo.lat, photo.lon) else {
            continue;
        };
        let here = view_of(base_url, photo, with_date);
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

/// A photo tapped in a marker's popup, as the map's script reports it: which
/// marker, and where in its group (US-62).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct PopupTap {
    pub marker: usize,
    pub photo: usize,
}

/// The set a popup tap opens the viewer on — that marker's photos, in the
/// popup's order — and where in it to start. `None` for a tap that names a
/// marker or a photo the map no longer has.
pub fn tapped(markers: &[PhotoMarker], tap: PopupTap) -> Option<(Vec<PhotoView>, usize)> {
    let group = &markers.get(tap.marker)?.photos;
    (tap.photo < group.len()).then(|| (group.clone(), tap.photo))
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
            button { r#type: "submit", "Upload" }
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
    use crate::test_support::{a_photo, import_sample, render, serve_test_archive};

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

    // ── US-62: when each photo was taken, and looking at it properly ─────

    fn taken(photo: PhotoResponse, at: &str, offset_secs: Option<i32>) -> PhotoResponse {
        PhotoResponse {
            taken_at: Some(at.to_string()),
            taken_offset_secs: offset_secs,
            ..photo
        }
    }

    #[test]
    fn a_photo_is_captioned_with_when_it_was_taken_where_it_was_taken() {
        let photo = taken(
            a_photo(1, "a.jpg", None),
            "2024-06-01T08:15:00Z",
            Some(7200),
        );

        assert_eq!(caption(&photo, false).as_deref(), Some("10:15 (+02:00)"));
        assert_eq!(
            caption(&photo, true).as_deref(),
            Some("1 Jun 10:15 (+02:00)")
        );
    }

    #[test]
    fn a_photo_whose_zone_is_unknown_is_captioned_in_labelled_utc() {
        let photo = taken(a_photo(1, "a.jpg", None), "2024-06-01T08:15:00Z", None);

        assert_eq!(caption(&photo, false).as_deref(), Some("08:15 UTC"));
    }

    #[test]
    fn a_photo_with_no_capture_time_has_no_caption() {
        // Silence rather than a dash: a dash claims a missing value.
        assert_eq!(caption(&a_photo(1, "a.jpg", None), false), None);
    }

    #[test]
    fn captions_carry_the_date_only_when_the_photos_span_more_than_one() {
        let morning = taken(
            a_photo(1, "a.jpg", None),
            "2024-06-01T08:15:00Z",
            Some(7200),
        );
        let evening = taken(
            a_photo(2, "b.jpg", None),
            "2024-06-01T20:15:00Z",
            Some(7200),
        );
        // 22:30 UTC is already the 2nd at +02:00: dates are the local ones.
        let midnight = taken(
            a_photo(3, "c.jpg", None),
            "2024-06-01T22:30:00Z",
            Some(7200),
        );
        let untimed = a_photo(4, "d.jpg", None);

        assert!(!captions_need_dates(&[
            morning.clone(),
            evening.clone(),
            untimed
        ]));
        assert!(captions_need_dates(&[morning, midnight]));
    }

    #[test]
    fn the_viewer_is_given_every_photo_in_gallery_order_at_full_size() {
        let photos = vec![
            taken(
                a_photo(1, "first.jpg", None),
                "2024-06-01T08:15:00Z",
                Some(7200),
            ),
            a_photo(2, "second.jpg", Some((59.91, 10.75))),
        ];

        let views = photo_views("http://archive.test", &photos);

        assert_eq!(
            views,
            vec![
                PhotoView {
                    thumbnail_url: "http://archive.test/media/trips/1/thumb-first.jpg".to_string(),
                    url: "http://archive.test/media/trips/1/first.jpg".to_string(),
                    name: "first.jpg".to_string(),
                    caption: Some("10:15 (+02:00)".to_string()),
                },
                PhotoView {
                    thumbnail_url: "http://archive.test/media/trips/1/thumb-second.jpg".to_string(),
                    url: "http://archive.test/media/trips/1/second.jpg".to_string(),
                    name: "second.jpg".to_string(),
                    caption: None,
                },
            ]
        );
    }

    #[test]
    fn a_marker_carries_what_the_viewer_needs_for_its_photos() {
        // A tap in the popup opens the viewer on that marker's photos, so
        // each one travels with its full-size URL and its caption.
        let photos = vec![taken(
            a_photo(1, "here.jpg", Some((59.91, 10.75))),
            "2024-06-01T08:15:00Z",
            Some(7200),
        )];

        let markers = photo_markers("http://archive.test", &photos);

        assert_eq!(
            markers[0].photos,
            photo_views("http://archive.test", &photos)
        );
    }

    #[test]
    fn a_tap_in_a_popup_opens_that_markers_photos_at_the_one_tapped() {
        // The popup's set, in the popup's order — not the trip's (US-62).
        let photos = vec![
            a_photo(1, "alone.jpg", Some((59.91, 10.75))),
            a_photo(2, "first.jpg", Some((60.5, 11.0))),
            a_photo(3, "second.jpg", Some((60.5, 11.0))),
        ];
        let markers = photo_markers("", &photos);

        let (set, start) = tapped(
            &markers,
            PopupTap {
                marker: 1,
                photo: 1,
            },
        )
        .unwrap();

        assert_eq!(
            set.iter()
                .map(|photo| photo.name.as_str())
                .collect::<Vec<_>>(),
            ["first.jpg", "second.jpg"]
        );
        assert_eq!(start, 1);
    }

    #[test]
    fn a_tap_on_a_marker_the_map_no_longer_has_opens_nothing() {
        // The tap and the markers arrive on different paths: a popup drawn
        // for the previous photos can answer after they have changed.
        let markers = photo_markers("", &[a_photo(1, "a.jpg", Some((59.91, 10.75)))]);

        assert_eq!(
            tapped(
                &markers,
                PopupTap {
                    marker: 3,
                    photo: 0
                }
            ),
            None
        );
        assert_eq!(
            tapped(
                &markers,
                PopupTap {
                    marker: 0,
                    photo: 4
                }
            ),
            None
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

    // US-2's other half, on the screen: photos can be added at a later time.
    // Choosing files and clicking are real events (the browser layer); that
    // the control is offered at all is assertable here.
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
        // The action row's "Add photos" is what opens this; the form's own
        // button says what it does next (US-62).
        assert!(html.contains("Upload"), "{html}");
    }
}
