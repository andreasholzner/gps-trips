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
use trip_archive_types::{
    ConfirmImport, ImportedTrip, PhotoResponse, StagedImport, Tag, TripDetail, TripSummary,
};

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

    ok_or_error(&url, response).await?;
    Ok(())
}

/// The response, or the archive's own account of why not.
///
/// Every write below ends the same way — a status check, the server's own
/// sentence where it wrote one, and the request named where it did not — so
/// it is written once here rather than at each call site. `from_status`
/// keeps the code for the screens that read it: a 404 that means "already
/// gone", a 409 that means "not now" (US-26).
async fn ok_or_error(
    url: &str,
    response: reqwest::Response,
) -> Result<reqwest::Response, ApiError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    Err(ApiError::from_status(
        status,
        readable_body(body).unwrap_or_else(|| format!("{url} returned {status}")),
    ))
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

/// `DELETE /api/trips/:id` — delete a trip and its photo blobs (US-9). A
/// Komoot-sourced trip is also queued for deletion on Komoot by the next
/// sync (US-24), which is the archive's business, not this call's.
///
/// The archive's own words come back for a refusal — notably the 409 while a
/// "Sync now" run is in flight (US-26), which is a "not now", not a failure.
pub async fn delete_trip(base_url: &str, id: i64) -> Result<(), ApiError> {
    let url = format!("{base_url}/api/trips/{id}");
    let response = reqwest::Client::new()
        .delete(&url)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;

    ok_or_error(&url, response).await?;
    Ok(())
}

/// The address the original uploaded GPX is downloaded from (US-21). A plain
/// link, not a fetch: the archive answers with the bytes it stored and a
/// `Content-Disposition` naming the file, so the browser saves it — reading
/// it into wasm first would only take a detour through memory.
pub fn original_gpx_url(base_url: &str, id: i64) -> String {
    format!("{base_url}/api/trips/{id}/gpx")
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

    ok_or_error(&url, response).await?;
    Ok(())
}

/// `GET /api/trips/:id/tags` — the tags on one trip (US-33).
pub async fn list_trip_tags(base_url: &str, id: i64) -> Result<Vec<Tag>, ApiError> {
    get_json(format!("{base_url}/api/trips/{id}/tags")).await
}

/// The `POST /api/trips/:id/tags` body (US-33): the name as typed. The
/// archive normalizes it — trimmed and lowercased — and creates the tag if it
/// is new, so casing never makes a second one.
#[derive(Serialize)]
struct AddTag<'a> {
    name: &'a str,
}

/// `POST /api/trips/:id/tags` — tag a trip, creating the tag if this is the
/// first use of the name (US-33). The archive's own words come back for a
/// name it refuses (one with a space or a comma), so the owner reads the rule
/// rather than a status code.
pub async fn add_trip_tag(base_url: &str, id: i64, name: &str) -> Result<Tag, ApiError> {
    let url = format!("{base_url}/api/trips/{id}/tags");
    let response = reqwest::Client::new()
        .post(&url)
        .json(&AddTag { name })
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;

    ok_or_error(&url, response)
        .await?
        .json()
        .await
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))
}

/// `DELETE /api/trips/:id/tags/:tag_id` — take a tag off a trip (US-33). The
/// tag itself stays in the archive, unused but suggestible again later.
pub async fn remove_trip_tag(base_url: &str, id: i64, tag_id: i64) -> Result<(), ApiError> {
    let url = format!("{base_url}/api/trips/{id}/tags/{tag_id}");
    let response = reqwest::Client::new()
        .delete(&url)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;

    ok_or_error(&url, response).await?;
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
    ok_or_error(&url, response)
        .await?
        .json()
        .await
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))
}

/// `POST /api/import/staged` — phase one of the import (US-12): hand the
/// archive the chosen GPX and get back what the confirm step prefills itself
/// with, the suggested `YYYY-mm-dd` name among it.
///
/// **Creates no trip.** Until [`confirm_import`] runs there is nothing in the
/// archive, which is what lets the screen offer a name for the owner to edit
/// before anything is committed — and what makes walking away free.
///
/// The type is stated rather than taken from the picker: unlike a photo
/// (`add_photos`), the archive reads this field's bytes as GPX whatever the
/// picker called it, and a browser reports nothing useful for `.gpx` anyway.
pub async fn stage_gpx(
    base_url: &str,
    file_name: &str,
    bytes: Vec<u8>,
) -> Result<StagedImport, ApiError> {
    let url = format!("{base_url}/api/import/staged");
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(file_name.to_string())
        .mime_str("application/gpx+xml")
        .map_err(|err| ApiError::new(format!("{file_name} could not be sent: {err}")))?;

    let response = reqwest::Client::new()
        .post(&url)
        .multipart(reqwest::multipart::Form::new().part("gpx", part))
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;

    ok_or_error(&url, response)
        .await?
        .json()
        .await
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))
}

/// `POST /api/import/staged/:id/confirm` — phase two (US-12): the owner's
/// name, activity type (US-11), kind (US-31) and timezone turn the staged
/// parse into a trip, whose id comes back.
///
/// Two refusals mean different things to the screen and are told apart by
/// status. A 404 is the parse being gone — already confirmed, cancelled, or
/// swept — and the screen re-stages the bytes it still holds rather than
/// making the owner pick the file again. Anything else is a field the archive
/// would not take, reported in its own words with the parse still waiting, so
/// the owner fixes the field instead of repeating the upload.
pub async fn confirm_import(
    base_url: &str,
    staging_id: i64,
    confirm: &ConfirmImport,
) -> Result<i64, ApiError> {
    let url = format!("{base_url}/api/import/staged/{staging_id}/confirm");
    let response = reqwest::Client::new()
        .post(&url)
        .json(confirm)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;

    let imported: ImportedTrip = ok_or_error(&url, response)
        .await?
        .json()
        .await
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))?;
    Ok(imported.id)
}

/// `DELETE /api/import/staged/:id` — the owner picked a different file
/// (US-12). Saying so now keeps the archive from holding an upload nobody
/// wants until the sweeper gets to it.
///
/// Navigating away sends nothing: there is no hook that reliably runs on the
/// way out, so those parses wait for the sweeper.
pub async fn cancel_staged_import(base_url: &str, staging_id: i64) -> Result<(), ApiError> {
    let url = format!("{base_url}/api/import/staged/{staging_id}");
    let response = reqwest::Client::new()
        .delete(&url)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;

    ok_or_error(&url, response).await?;
    Ok(())
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
