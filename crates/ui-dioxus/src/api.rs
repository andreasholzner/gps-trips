//! The JSON API client (ADR-0008). Response shapes come straight from
//! `trip-archive-types` (ADR-0015's 2026-08-28 amendment), so nothing is
//! described twice.
//!
//! Every call takes a `base_url`: the page's own origin on the web (the SPA
//! is served by the server it queries, but `reqwest` — unlike a browser
//! `fetch` wrapper — rejects relative URLs outright), and later the address
//! the owner configured on Android (US-16), where there is no origin to be
//! relative to.
//!
//! `reqwest` rather than a browser-only client: it compiles to `fetch` under
//! wasm and to a native client on Android, which is the whole reason the two
//! platforms can share this file (ADR-0024).

use serde::de::DeserializeOwned;
use serde::Serialize;
use trip_archive_types::{PhotoResponse, Tag, TripDetail, TripSummary};

use crate::track::Track;

/// A failed API call, already reduced to what the UI shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ApiError {
    message: String,
    /// The status the archive answered with, where it answered at all —
    /// `None` for a request that never got a response, or an answer that
    /// could not be read. Screens branch on it only where the status means
    /// something to the owner rather than to a programmer: a trip that is
    /// simply gone (404), or an edit refused while a sync runs (409, US-26).
    status: Option<u16>,
}

impl ApiError {
    /// A failure with no status behind it.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: None,
        }
    }

    /// A failure the archive answered with, keeping the status for the few
    /// screens that read it.
    fn from_status(status: reqwest::StatusCode, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: Some(status.as_u16()),
        }
    }

    /// Whether the archive answered "no such thing" — for the screens where
    /// that is an ordinary outcome and not a fault.
    pub fn is_not_found(&self) -> bool {
        self.status == Some(reqwest::StatusCode::NOT_FOUND.as_u16())
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

async fn get_json<T: DeserializeOwned>(url: String) -> Result<T, ApiError> {
    let response = reqwest::get(&url)
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ApiError::from_status(
            status,
            format!("{url} returned {status}"),
        ));
    }
    response
        .json::<T>()
        .await
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))
}

/// `GET /api/trips` — the filtered list (US-6/US-13). `query` is
/// `Filters::to_query`'s output, without a leading `?` (the same string the
/// SPA carries in its own URL); empty means the unfiltered list.
pub async fn list_trips(base_url: &str, query: String) -> Result<Vec<TripSummary>, ApiError> {
    let separator = if query.is_empty() { "" } else { "?" };
    get_json(format!("{base_url}/api/trips{separator}{query}")).await
}

/// `GET /api/trips/:id` — one trip's metadata for the detail screen (US-7).
/// A 404 travels back as such (`ApiError::is_not_found`), because a trip the
/// owner has deleted is an ordinary thing to ask for and the screen says so
/// in its own words.
pub async fn get_trip(base_url: &str, id: i64) -> Result<TripDetail, ApiError> {
    get_json(format!("{base_url}/api/trips/{id}")).await
}

/// `GET /api/trips/:id/track.geojson` — the track geometry (ADR-0003). One
/// fetch feeds both the map and the elevation chart: the geometry and the
/// chart's series travel together in the same blob (ADR-0025).
pub async fn get_track(base_url: &str, id: i64) -> Result<Track, ApiError> {
    get_json(format!("{base_url}/api/trips/{id}/track.geojson")).await
}

/// `GET /api/tags` — every known tag, for the tag filter's choices (US-38)
/// and the bulk-tag suggestions (US-34).
pub async fn list_tags(base_url: &str) -> Result<Vec<Tag>, ApiError> {
    get_json(format!("{base_url}/api/tags")).await
}

/// `GET /api/trips/:id/photos` — the trip's photos (US-2/US-7), each already
/// carrying the URLs to fetch its image and its thumbnail (US-5).
pub async fn list_photos(base_url: &str, id: i64) -> Result<Vec<PhotoResponse>, ApiError> {
    get_json(format!("{base_url}/api/trips/{id}/photos")).await
}

/// One photo on its way to the archive: the name the file was chosen under,
/// which is stored as its `original_name`, the type the browser reported for
/// it, and its bytes.
#[derive(Clone)]
pub struct PhotoUpload {
    pub file_name: String,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// `POST /api/trips/:id/photos` — attach photos to a trip that already exists
/// (US-2: photos can be added at a later time). The same multipart endpoint
/// the import form posts to (ADR-0004), issued from the SPA.
///
/// A part whose content type the file picker did not report, or did not
/// report as a `type/subtype`, is sent without one rather than not at all —
/// one oddly described file must not take the rest of the batch down with it,
/// and the archive already treats the type as optional: it reads the image's
/// own bytes to make a thumbnail (US-5) and serves it back under a type
/// derived from the stored key's extension.
///
/// The endpoint answers `204`, so this reports the upload's own outcome. It
/// used to redirect to the server-rendered detail page, which meant reading
/// that page's status instead — a browser `fetch` gives no way to decline a
/// redirect — and would have started reporting 404 the moment US-42 retired
/// the page. US-43's import wants the same treatment.
pub async fn add_photos(base_url: &str, id: i64, photos: Vec<PhotoUpload>) -> Result<(), ApiError> {
    let url = format!("{base_url}/api/trips/{id}/photos");
    let mut form = reqwest::multipart::Form::new();
    for photo in photos {
        let name = photo.file_name.clone();
        let content_type = usable_content_type(photo.content_type.as_deref());
        let mut part = reqwest::multipart::Part::bytes(photo.bytes).file_name(photo.file_name);
        if let Some(content_type) = content_type {
            part = part
                .mime_str(content_type)
                .map_err(|err| ApiError::new(format!("{name} has an unusable type: {err}")))?;
        }
        form = form.part("photos", part);
    }

    let response = reqwest::Client::new()
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError::from_status(
            status,
            readable_body(body).unwrap_or_else(|| format!("{url} returned {status}")),
        ));
    }
    Ok(())
}

/// A content type worth attaching to a part: `type/subtype`, both halves
/// present and made of the characters a MIME type is allowed to contain.
///
/// A web file picker reports either that or an empty string, but the Android
/// target's is a different implementation (ADR-0024), so what arrives is
/// checked rather than assumed.
fn usable_content_type(content_type: Option<&str>) -> Option<&str> {
    /// RFC 9110's token characters, minus the alphanumerics tested separately.
    const TOKEN_PUNCTUATION: &str = "!#$%&'*+-.^_`|~";
    let is_token = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || TOKEN_PUNCTUATION.contains(c))
    };

    let content_type = content_type?;
    let (kind, subtype) = content_type.split_once('/')?;
    (is_token(kind) && is_token(subtype)).then_some(content_type)
}

/// The server's own words, where it answered with words. The API's own
/// errors are a plain sentence the owner can act on; a redirect that landed
/// on an error *page* answers in HTML, which belongs in no error line.
fn readable_body(body: String) -> Option<String> {
    let body = body.trim();
    (!body.is_empty() && !body.starts_with('<')).then(|| body.to_string())
}

/// The `PATCH /api/trips/:id` body (US-15/US-35). Every field is optional and
/// an omitted one is left unchanged — which is why only what the owner
/// actually changed is sent: an edit of the name alone must not write back a
/// stale activity type that another tab, or a Komoot sync, changed after this
/// screen loaded.
#[derive(Default, Serialize)]
pub struct TripEdit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The wire value, not the label; empty resets the trip to `Unknown`,
    /// the same as importing without choosing one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_type: Option<String>,
    /// US-35: the linked Komoot tour's privacy, pushed to Komoot by the next
    /// sync. Only a settable value is accepted; `unknown` is a state Komoot
    /// put the tour in, never one to choose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_status: Option<String>,
}

impl TripEdit {
    /// Whether anything is actually being changed — an edit of nothing is
    /// worth no request.
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.activity_type.is_none() && self.privacy_status.is_none()
    }
}

/// `PATCH /api/trips/:id` — save an edit (US-15/US-35). The archive's own
/// words come back for a rejected one (a blank name, an unrecognized
/// activity, a privacy on a trip that never came from Komoot), so the owner
/// reads what is wrong rather than a status code — and for a 409 while a
/// "Sync now" run is in flight (US-26).
pub async fn edit_trip(base_url: &str, id: i64, edit: &TripEdit) -> Result<(), ApiError> {
    let url = format!("{base_url}/api/trips/{id}");
    let response = reqwest::Client::new()
        .patch(&url)
        .json(edit)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError::from_status(
            status,
            readable_body(body).unwrap_or_else(|| format!("{url} returned {status}")),
        ));
    }
    Ok(())
}

/// The `POST /api/trips/tags` body (US-34): every name applied to every
/// trip, in one request.
#[derive(Serialize)]
struct BulkAddTags<'a> {
    trip_ids: &'a [i64],
    names: &'a [String],
}

/// `POST /api/trips/tags` — tag every selected trip with every staged name
/// (US-34). All-or-nothing: if any id no longer exists the server tags
/// nothing, which this reports as such rather than as a bare 404. Other
/// failures carry the server's own message (an invalid name, US-33's rules)
/// so the owner reads what is wrong instead of a status code.
pub async fn bulk_add_tags(
    base_url: &str,
    trip_ids: &[i64],
    names: &[String],
) -> Result<Vec<Tag>, ApiError> {
    let url = format!("{base_url}/api/trips/tags");
    let response = reqwest::Client::new()
        .post(&url)
        .json(&BulkAddTags { trip_ids, names })
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;

    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(ApiError::from_status(
            status,
            "one or more selected trips no longer exist; nothing was tagged",
        ));
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError::from_status(
            status,
            readable_body(body).unwrap_or_else(|| format!("{url} returned {status}")),
        ));
    }
    response
        .json()
        .await
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
//
// The client against a real server, no mocks: these carry US-34's
// acceptance criteria that live in the request rather than in the screen.
// Driving the same call from a click belongs to the browser layer.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{import_sample, serve_test_archive};
    use trip_archive_types::ActivityType;

    #[tokio::test]
    async fn bulk_tagging_applies_every_tag_to_every_selected_trip() {
        let (base_url, _dir) = serve_test_archive().await;
        let first = import_sample(&base_url, &[("name", "Oslo Hills Walk")]).await;
        let second = import_sample(&base_url, &[("name", "Inn Valley Ride")]).await;

        bulk_add_tags(
            &base_url,
            &[first, second],
            &["alpine".to_string(), "summer".to_string()],
        )
        .await
        .expect("bulk tag");

        let names: Vec<String> = list_tags(&base_url)
            .await
            .expect("tags")
            .into_iter()
            .map(|tag| tag.name)
            .collect();
        assert!(names.contains(&"alpine".to_string()), "{names:?}");
        assert!(names.contains(&"summer".to_string()), "{names:?}");
        // Both trips carry both tags: filtering on both lists both.
        let both = list_trips(&base_url, "tags=alpine,summer".to_string())
            .await
            .expect("filtered list");
        assert_eq!(both.len(), 2, "{both:?}");
    }

    /// JPEG magic plus padding: enough for the archive to store and serve,
    /// and undecodable, which the thumbnail step already tolerates (US-5).
    const FAKE_JPEG: &[u8] = b"\xFF\xD8\xFF-fake-jpeg";

    // US-2's "photos can be added at a later time", from the SPA. Uploading
    // is a request, not a screen behaviour, so it belongs here — only the
    // file picker itself needs a browser (ADR-0012).
    // ── US-15: editing a trip's name and activity type ───────────────────
    //
    // The acceptance criterion is that the new values are saved, so these
    // read them back over the API. Which fields a form sends is the screen's
    // business, and tested there.

    #[tokio::test]
    async fn an_edited_name_and_activity_type_are_saved() {
        let (base_url, _dir) = serve_test_archive().await;
        let id = import_sample(&base_url, &[]).await;

        edit_trip(
            &base_url,
            id,
            &TripEdit {
                name: Some("Renamed Trip".to_string()),
                activity_type: Some("cycling".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("edit");

        let trip = get_trip(&base_url, id).await.expect("trip");
        assert_eq!(trip.name, "Renamed Trip");
        assert_eq!(trip.activity_type, ActivityType::Cycling);
    }

    #[tokio::test]
    async fn editing_one_field_leaves_the_other_alone() {
        // The reason only changed fields are sent: an omitted field must not
        // be written back from what this screen happened to load with.
        let (base_url, _dir) = serve_test_archive().await;
        let id = import_sample(&base_url, &[("activity_type", "hiking")]).await;

        edit_trip(
            &base_url,
            id,
            &TripEdit {
                name: Some("Only Name Changed".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("edit");

        let trip = get_trip(&base_url, id).await.expect("trip");
        assert_eq!(trip.name, "Only Name Changed");
        assert_eq!(trip.activity_type, ActivityType::Hiking);
    }

    #[tokio::test]
    async fn a_blank_name_is_refused_in_the_archives_own_words() {
        let (base_url, _dir) = serve_test_archive().await;
        let id = import_sample(&base_url, &[]).await;

        let err = edit_trip(
            &base_url,
            id,
            &TripEdit {
                name: Some("   ".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect_err("a blank name is not a name");

        assert!(err.to_string().contains("name"), "{err}");
        assert_eq!(
            get_trip(&base_url, id).await.expect("trip").name,
            "Oslo Hills Walk",
            "a refused edit changes nothing"
        );
    }

    #[tokio::test]
    async fn a_privacy_on_a_trip_that_never_came_from_komoot_is_refused() {
        // US-35: privacy belongs to the linked tour, so an unlinked trip has
        // none to change — and the screen offers no picker for one.
        let (base_url, _dir) = serve_test_archive().await;
        let id = import_sample(&base_url, &[]).await;

        let err = edit_trip(
            &base_url,
            id,
            &TripEdit {
                privacy_status: Some("public".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect_err("there is no tour to make public");

        assert!(!err.to_string().is_empty(), "the archive says why");
    }

    #[test]
    fn only_a_content_type_worth_attaching_is_attached() {
        assert_eq!(usable_content_type(Some("image/jpeg")), Some("image/jpeg"));
        assert_eq!(
            usable_content_type(Some("image/svg+xml")),
            Some("image/svg+xml")
        );
        // What a web picker reports for a file it cannot classify, and what
        // another platform's might.
        assert_eq!(usable_content_type(Some("")), None);
        assert_eq!(usable_content_type(Some("image")), None);
        assert_eq!(usable_content_type(Some("image/")), None);
        assert_eq!(usable_content_type(Some("image/jpeg; charset=utf-8")), None);
        assert_eq!(usable_content_type(None), None);
    }

    #[test]
    fn only_a_servers_own_words_reach_the_owner() {
        assert_eq!(
            readable_body("tag names cannot contain spaces".to_string()),
            Some("tag names cannot contain spaces".to_string())
        );
        assert_eq!(readable_body("   ".to_string()), None);
        // An error page, not a message: the caller falls back to naming the
        // request and its status.
        assert_eq!(readable_body("<!DOCTYPE html><h1>oh no".to_string()), None);
    }

    #[tokio::test]
    async fn a_photo_added_after_the_import_joins_the_trips_photos() {
        let (base_url, _dir) = serve_test_archive().await;
        let id = import_sample(&base_url, &[]).await;

        add_photos(
            &base_url,
            id,
            vec![PhotoUpload {
                file_name: "later.jpg".to_string(),
                content_type: Some("image/jpeg".to_string()),
                bytes: FAKE_JPEG.to_vec(),
            }],
        )
        .await
        .expect("upload");

        let photos = list_photos(&base_url, id).await.expect("photos");
        assert_eq!(photos.len(), 1, "{photos:?}");
        assert_eq!(photos[0].original_name, "later.jpg");
        // US-5: the gallery is handed a thumbnail URL either way — the
        // full-size one stands in when no thumbnail could be made, as here.
        assert!(!photos[0].thumbnail_url.is_empty(), "{photos:?}");
    }

    #[tokio::test]
    async fn photos_added_later_accumulate_rather_than_replace() {
        let (base_url, _dir) = serve_test_archive().await;
        let id = import_sample(&base_url, &[]).await;

        for name in ["a.jpg", "b.jpg"] {
            add_photos(
                &base_url,
                id,
                vec![PhotoUpload {
                    file_name: name.to_string(),
                    content_type: Some("image/jpeg".to_string()),
                    bytes: FAKE_JPEG.to_vec(),
                }],
            )
            .await
            .expect("upload");
        }

        assert_eq!(list_photos(&base_url, id).await.expect("photos").len(), 2);
    }

    #[tokio::test]
    async fn adding_a_photo_to_a_trip_that_is_gone_says_so() {
        let (base_url, _dir) = serve_test_archive().await;

        let err = add_photos(
            &base_url,
            9_999,
            vec![PhotoUpload {
                file_name: "later.jpg".to_string(),
                content_type: Some("image/jpeg".to_string()),
                bytes: FAKE_JPEG.to_vec(),
            }],
        )
        .await
        .expect_err("there is no such trip to add to");

        assert!(err.is_not_found(), "{err}");
    }

    #[tokio::test]
    async fn a_selection_holding_a_vanished_trip_tags_nothing_at_all() {
        // US-34's all-or-nothing rule: the whole request 404s and no tag is
        // created or linked, so a stale selection can't half-apply.
        let (base_url, _dir) = serve_test_archive().await;
        let existing = import_sample(&base_url, &[]).await;

        let err = bulk_add_tags(&base_url, &[existing, 9_999], &["alpine".to_string()])
            .await
            .expect_err("a vanished trip must fail the request");

        assert!(
            err.to_string().contains("no longer exist"),
            "the message must say nothing was tagged: {err}"
        );
        assert_eq!(list_tags(&base_url).await.expect("tags"), Vec::new());
    }

    #[tokio::test]
    async fn an_invalid_tag_name_is_reported_readably_and_tags_nothing() {
        let (base_url, _dir) = serve_test_archive().await;
        let trip = import_sample(&base_url, &[]).await;

        let err = bulk_add_tags(&base_url, &[trip], &["day trip".to_string()])
            .await
            .expect_err("a name with a space must be rejected");

        // The server's own wording, not a status code the owner must decode.
        assert!(err.to_string().contains("cannot contain spaces"), "{err}");
        assert_eq!(list_tags(&base_url).await.expect("tags"), Vec::new());
    }
}
