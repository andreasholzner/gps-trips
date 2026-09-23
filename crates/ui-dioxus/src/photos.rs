//! The trip's photos on the detail screen (US-2/US-7): the markers that put
//! them on the track map (US-3/US-4), what the gallery and the viewer show of
//! each (US-62), and the control for adding more after the import.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use time::UtcOffset;
use trip_archive_types::{LocationSource, PhotoResponse};

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
///
/// The viewer also offers to place the photo by hand (US-30), which needs to
/// know which photo it is, where it is now, and how that was decided.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhotoView {
    pub thumbnail_url: String,
    pub url: String,
    pub name: String,
    pub caption: Option<String>,
    pub id: i64,
    /// `[lat, lon]`, as the map scripts take positions.
    pub position: Option<[f64; 2]>,
    pub source: LocationSource,
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
        id: photo.id,
        position: photo.lat.zip(photo.lon).map(|(lat, lon)| [lat, lon]),
        source: photo.location_source,
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
// Split into photos/tests.rs to keep this file under the repo's 500-line cap.

#[cfg(test)]
mod tests;
