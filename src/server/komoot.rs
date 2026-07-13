//! Komoot client seam (ADR-0021).
//!
//! Wraps the unofficial Komoot API. Endpoint URLs, request/response shapes,
//! and the auth mechanism (HTTP Basic per request, not a session — kept
//! behind the single `authed_get` seam below so a later switch to
//! Komoot's cookie-based session auth touches one function, not every call
//! site) are documented in `docs/komoot-api.md`; this module is the
//! implementation of that protocol, not its source of truth.
//!
//! `KomootClient` is mocked in tests wherever it's used (ADR-0012 — the
//! network is one of the few things this project mocks). US-27
//! (`komoot_check`) is the exception: it has no automated tests of its own,
//! since running it against a real Komoot account is itself the acceptance
//! check for this story.

use serde::Deserialize;
use thiserror::Error;

const BASE_URL: &str = "https://api.komoot.de";

/// Errors from talking to the Komoot API.
#[derive(Debug, Error)]
pub enum KomootError {
    #[error("Komoot rejected the credentials")]
    Unauthorized,

    #[error("Komoot returned an unexpected status {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },

    #[error("Network error talking to Komoot: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Could not decode Komoot's response: {0}")]
    Decode(#[from] serde_json::Error),
}

/// A Komoot tour as listed by `list_tours` — enough to decide whether to
/// import it (US-22) without a separate per-tour metadata call: the list
/// response already embeds each tour's `name`/`sport`/`date` in full
/// (confirmed against a real response; see `docs/komoot-api.md`).
#[derive(Debug, Clone, Deserialize)]
pub struct KomootTourSummary {
    #[serde(deserialize_with = "id_as_string")]
    pub id: String,
    pub name: String,
    /// Komoot's sport string (e.g. `"mtb"`, `"hike"`) — mapped to this app's
    /// `ActivityType` by `komoot_sport::map_sport`.
    pub sport: String,
    /// RFC-3339-ish timestamp as Komoot sends it, kept as-is (parsed by the
    /// caller only if/when needed).
    pub date: String,
    pub distance: f64,
}

/// Komoot's tour `id` has been observed as either a JSON string or number;
/// accept both rather than let an incidental type change break parsing.
fn id_as_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IdValue {
        Str(String),
        Num(i64),
    }
    Ok(match IdValue::deserialize(deserializer)? {
        IdValue::Str(s) => s,
        IdValue::Num(n) => n.to_string(),
    })
}

/// One photo attached to a tour, from `GET /v007/tours/{id}/cover_images/`
/// (`docs/komoot-api.md`). `src` is a **templated** CloudFront URL —
/// `resolve_photo_url` fills in `{width}`/`{height}`/`{crop}` before the
/// bytes themselves can be fetched (`fetch_photo_bytes`). `location` is
/// `None` on the rare photo Komoot has no GPS for (US-22's placement
/// pipeline falls back to this app's own EXIF/interpolation for those).
#[derive(Debug, Clone, Deserialize)]
pub struct KomootPhoto {
    #[serde(deserialize_with = "id_as_string")]
    pub id: String,
    pub src: String,
    pub location: Option<KomootLocation>,
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KomootLocation {
    pub lat: f64,
    pub lng: f64,
}

/// Fill a [`KomootPhoto::src`] template's `{width}`/`{height}`/`{crop}`
/// placeholders with concrete values. `crop` must be encoded as the literal
/// string `true`/`false` — CloudFront rejects `1`/`0` with `400 Bad Request`
/// (confirmed against the real API; see `docs/komoot-api.md`).
pub fn resolve_photo_url(template: &str, width: u32, height: u32, crop: bool) -> String {
    template
        .replace("{width}", &width.to_string())
        .replace("{height}", &height.to_string())
        .replace("{crop}", if crop { "true" } else { "false" })
}

#[derive(Deserialize)]
struct LoginResponse {
    username: String,
}

#[derive(Deserialize)]
struct ToursResponse {
    // HAL APIs conventionally omit `_embedded` entirely for an empty
    // collection (e.g. an account with no recorded tours, or a page past
    // the last one) rather than sending `{"tours": []}` — `default` treats
    // a missing key the same as an explicitly empty one instead of failing
    // to parse.
    #[serde(rename = "_embedded", default)]
    embedded: Embedded,
}

#[derive(Deserialize, Default)]
struct Embedded {
    tours: Vec<KomootTourSummary>,
}

#[derive(Deserialize)]
struct CoverImagesResponse {
    // Same reasoning as `ToursResponse::embedded` — most tours have zero
    // photos, and a HAL API is expected to drop `_embedded` for that case.
    #[serde(rename = "_embedded", default)]
    embedded: CoverImagesEmbedded,
}

#[derive(Deserialize, Default)]
struct CoverImagesEmbedded {
    items: Vec<KomootPhoto>,
}

/// Talks to Komoot. Real implementation: [`KomootHttpClient`].
pub trait KomootClient: Send + Sync {
    /// Validates credentials and returns the Komoot username, which
    /// [`list_tours`](Self::list_tours) needs to build its URL.
    fn login(&self) -> Result<String, KomootError>;

    /// Lists **recorded** tours (`type=tour_recorded`) owned by `username`,
    /// one page at a time: `limit` caps the page size, `page` selects which
    /// page (0-based; `None` means page 0). A page shorter than `limit`
    /// (including empty) is the last page — callers that need every tour
    /// (US-22) loop until then. Komoot's planned routes (`type=tour_planned`
    /// — future/unrecorded routes, not something the owner has actually
    /// done) are filtered out server-side; this app archives recorded trips
    /// only (confirmed against the real API: `docs/komoot-api.md`).
    fn list_tours(
        &self,
        username: &str,
        limit: Option<u32>,
        page: Option<u32>,
    ) -> Result<Vec<KomootTourSummary>, KomootError>;

    /// Fetches a tour's track as raw GPX bytes
    /// (`GET /v007/tours/{tour_id}.gpx`).
    fn get_tour_gpx(&self, tour_id: &str) -> Result<Vec<u8>, KomootError>;

    /// Lists a tour's photos, one page at a time (`GET
    /// /v007/tours/{tour_id}/cover_images/`, despite the name — not just a
    /// designated cover picture; see `docs/komoot-api.md`). Same
    /// pagination contract as [`list_tours`](Self::list_tours): `limit`
    /// caps the page size, `page` selects which page, and a short
    /// (including empty) page is the last one — callers that need every
    /// photo loop until then, the same way `list_all_tours` does for
    /// tours.
    fn get_tour_photos(
        &self,
        tour_id: &str,
        limit: Option<u32>,
        page: Option<u32>,
    ) -> Result<Vec<KomootPhoto>, KomootError>;

    /// Fetches a photo's bytes from its already-resolved (placeholders
    /// filled in via `resolve_photo_url`) CloudFront URL. Deliberately takes
    /// no auth — sending Komoot's Basic Auth credentials to a third-party
    /// CloudFront host would leak them well beyond `api.komoot.de`, and the
    /// resolved URL needs none (confirmed against the real API).
    fn fetch_photo_bytes(&self, resolved_url: &str) -> Result<Vec<u8>, KomootError>;
}

/// Real [`KomootClient`], talking to the live Komoot API over HTTP Basic
/// Auth (see module docs).
pub struct KomootHttpClient {
    email: String,
    password: String,
    debug: bool,
    http: reqwest::blocking::Client,
}

impl KomootHttpClient {
    pub fn new(email: String, password: String, debug: bool) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("reqwest client with a fixed timeout always builds");
        Self {
            email,
            password,
            debug,
            http,
        }
    }

    /// Send an authenticated `GET`, before the response body is read —
    /// shared by `authed_get`/`authed_get_bytes` so the request-building
    /// (auth header, query params) can't drift between the text and binary
    /// paths.
    fn send_authed(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<reqwest::blocking::Response, KomootError> {
        Ok(self
            .http
            .get(url)
            .basic_auth(&self.email, Some(&self.password))
            .query(query)
            .send()?)
    }

    /// Turn a response's status into a [`KomootError`]: `FORBIDDEN` means
    /// bad credentials, any other non-2xx is `UnexpectedStatus`. Shared by
    /// every method that talks to Komoot so this mapping can't drift
    /// between them; `body` is computed lazily since it's only needed on
    /// the error path.
    fn map_status(
        status: reqwest::StatusCode,
        body: impl FnOnce() -> String,
    ) -> Result<(), KomootError> {
        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(KomootError::Unauthorized);
        }
        if !status.is_success() {
            return Err(KomootError::UnexpectedStatus {
                status: status.as_u16(),
                body: body(),
            });
        }
        Ok(())
    }

    /// The one seam every authenticated Komoot request goes through:
    /// attaches auth, optionally logs the raw response for `--debug`, and
    /// turns non-success statuses into [`KomootError`].
    fn authed_get(&self, url: &str, query: &[(&str, &str)]) -> Result<String, KomootError> {
        let response = self.send_authed(url, query)?;
        let status = response.status();
        let body = response.text()?;

        if self.debug {
            eprintln!("[komoot debug] GET {url} -> {status}\n{body}");
        }

        Self::map_status(status, || body.clone())?;
        Ok(body)
    }

    /// Like `authed_get`, but for binary responses (the `.gpx` endpoint):
    /// returns the raw bytes rather than decoding them as UTF-8 text.
    fn authed_get_bytes(&self, url: &str, query: &[(&str, &str)]) -> Result<Vec<u8>, KomootError> {
        let response = self.send_authed(url, query)?;
        let status = response.status();
        let bytes = response.bytes()?;

        if self.debug {
            eprintln!(
                "[komoot debug] GET {url} -> {status} ({} bytes)",
                bytes.len()
            );
        }

        Self::map_status(status, || String::from_utf8_lossy(&bytes).into_owned())?;
        Ok(bytes.to_vec())
    }
}

impl KomootClient for KomootHttpClient {
    fn login(&self) -> Result<String, KomootError> {
        let url = format!("{BASE_URL}/v006/account/email/{}/", self.email);
        let body = self.authed_get(&url, &[])?;
        let parsed: LoginResponse = serde_json::from_str(&body)?;
        Ok(parsed.username)
    }

    fn list_tours(
        &self,
        username: &str,
        limit: Option<u32>,
        page: Option<u32>,
    ) -> Result<Vec<KomootTourSummary>, KomootError> {
        let url = format!("{BASE_URL}/v007/users/{username}/tours/");
        let limit_str = limit.map(|l| l.to_string());
        let page_str = page.map(|p| p.to_string());
        let mut query = vec![("type", "tour_recorded")];
        if let Some(l) = &limit_str {
            query.push(("limit", l.as_str()));
        }
        if let Some(p) = &page_str {
            query.push(("page", p.as_str()));
        }
        let body = self.authed_get(&url, &query)?;
        let parsed: ToursResponse = serde_json::from_str(&body)?;
        Ok(parsed.embedded.tours)
    }

    fn get_tour_gpx(&self, tour_id: &str) -> Result<Vec<u8>, KomootError> {
        let url = format!("{BASE_URL}/v007/tours/{tour_id}.gpx");
        self.authed_get_bytes(&url, &[])
    }

    fn get_tour_photos(
        &self,
        tour_id: &str,
        limit: Option<u32>,
        page: Option<u32>,
    ) -> Result<Vec<KomootPhoto>, KomootError> {
        let url = format!("{BASE_URL}/v007/tours/{tour_id}/cover_images/");
        let limit_str = limit.map(|l| l.to_string());
        let page_str = page.map(|p| p.to_string());
        let mut query = Vec::new();
        if let Some(l) = &limit_str {
            query.push(("limit", l.as_str()));
        }
        if let Some(p) = &page_str {
            query.push(("page", p.as_str()));
        }
        let body = self.authed_get(&url, &query)?;
        let parsed: CoverImagesResponse = serde_json::from_str(&body)?;
        Ok(parsed.embedded.items)
    }

    fn fetch_photo_bytes(&self, resolved_url: &str) -> Result<Vec<u8>, KomootError> {
        let response = self.http.get(resolved_url).send()?;
        let status = response.status();
        let bytes = response.bytes()?;
        Self::map_status(status, || String::from_utf8_lossy(&bytes).into_owned())?;
        Ok(bytes.to_vec())
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
//
// `KomootHttpClient` itself talks to the real Komoot API and has no
// automated tests of its own (see module docs, and US-27/`komoot_check`) —
// only pure, I/O-free logic is unit-tested here. `KomootClient` consumers
// (e.g. `komoot_sync`) get a hand-rolled test-double `impl KomootClient`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_photo_url_fills_in_width_height_and_crop() {
        let template = "https://cdn.example/photo?width={width}&height={height}&crop={crop}";
        assert_eq!(
            resolve_photo_url(template, 800, 600, true),
            "https://cdn.example/photo?width=800&height=600&crop=true"
        );
    }

    #[test]
    fn resolve_photo_url_encodes_crop_as_the_literal_words_not_1_or_0() {
        // Regression guard: CloudFront rejects crop=1/crop=0 with a 400
        // ("crop must be true or false"), confirmed against the real API.
        let template = "https://cdn.example/photo?crop={crop}";
        assert_eq!(
            resolve_photo_url(template, 1, 1, false),
            "https://cdn.example/photo?crop=false"
        );
    }

    // Regression guard: HAL APIs conventionally omit `_embedded` entirely
    // for an empty collection rather than sending an explicit empty one —
    // these must parse as empty, not error out and halt a sync.

    #[test]
    fn tours_response_parses_with_embedded_present() {
        let json = r#"{"_embedded": {"tours": []}}"#;
        let parsed: ToursResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.embedded.tours.is_empty());
    }

    #[test]
    fn tours_response_parses_as_empty_when_embedded_is_missing() {
        let json = r#"{}"#;
        let parsed: ToursResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.embedded.tours.is_empty());
    }

    #[test]
    fn cover_images_response_parses_with_embedded_present() {
        let json = r#"{"_embedded": {"items": []}}"#;
        let parsed: CoverImagesResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.embedded.items.is_empty());
    }

    #[test]
    fn cover_images_response_parses_as_empty_when_embedded_is_missing() {
        let json = r#"{}"#;
        let parsed: CoverImagesResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.embedded.items.is_empty());
    }
}
