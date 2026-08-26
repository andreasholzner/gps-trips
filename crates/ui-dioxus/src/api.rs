//! The JSON API client (ADR-0008). Every response type except one comes
//! straight from `trip-archive-types`, so no shape is described twice.
//!
//! The exception is `PhotoView`: the server's photo endpoint answers with
//! `PhotoResponse` — `Photo` plus the per-request `url`/`thumbnail_url`
//! (ADR-0015) — which lives in `http.rs` and is therefore not shareable.
//! Mirroring it here is the one duplication in this crate, and a finding for
//! the spike write-up rather than something to paper over.
//!
//! Every call takes a `base_url`: empty on the web (the SPA is served by the
//! server it queries, so relative URLs are correct), and the address the
//! owner configured on Android, where there is no such origin to be relative
//! to. See `settings.rs`.
//!
//! `reqwest` rather than a browser-only client: it compiles to `fetch` under
//! wasm and to a native client on Android, which is the whole reason the two
//! platforms can share this file.

use serde::de::DeserializeOwned;
use serde::Deserialize;
use trip_archive_types::{LocationSource, TripDetail, TripSummary};

/// A failed API call, already reduced to what the UI shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ApiError(pub String);

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One photo as the gallery needs it — see the module note on why this type
/// is mirrored rather than shared.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PhotoView {
    pub id: i64,
    pub original_name: String,
    pub url: String,
    pub thumbnail_url: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub location_source: LocationSource,
}

async fn get_json<T: DeserializeOwned>(url: String) -> Result<T, ApiError> {
    let response = reqwest::get(&url)
        .await
        .map_err(|err| ApiError(format!("{url} unreachable: {err}")))?;
    if !response.status().is_success() {
        return Err(ApiError(format!("{url} returned {}", response.status())));
    }
    response
        .json::<T>()
        .await
        .map_err(|err| ApiError(format!("{url} returned unreadable JSON: {err}")))
}

/// `GET /api/trips` — the filtered list (US-13); `query` is `Filters::to_query`.
pub async fn list_trips(base_url: &str, query: String) -> Result<Vec<TripSummary>, ApiError> {
    get_json(format!("{base_url}/api/trips{query}")).await
}

/// `GET /api/trips/:id` — one trip's metadata (US-16).
pub async fn trip(base_url: &str, id: i64) -> Result<TripDetail, ApiError> {
    get_json(format!("{base_url}/api/trips/{id}")).await
}

/// `GET /api/trips/:id/photos` — the gallery (US-2/US-5).
pub async fn photos(base_url: &str, id: i64) -> Result<Vec<PhotoView>, ApiError> {
    get_json(format!("{base_url}/api/trips/{id}/photos")).await
}

/// `GET /api/trips/:id/track.geojson` — the track (ADR-0003), handed to the
/// map and elevation chart as-is. Kept as raw `serde_json::Value`: the
/// interop layer passes it straight to Leaflet, which understands GeoJSON
/// natively, so parsing it into Rust structs here would only be to
/// re-serialize it a moment later.
pub async fn track(base_url: &str, id: i64) -> Result<serde_json::Value, ApiError> {
    get_json(format!("{base_url}/api/trips/{id}/track.geojson")).await
}

/// An absolute URL for a path the server handed us (a photo's `url` /
/// `thumbnail_url`, which are always server-relative). On the web this is the
/// path unchanged; on Android it has to be resolved against the configured
/// archive, or the `<img>` would point at the app's own internal origin.
pub fn media_url(base_url: &str, path: &str) -> String {
    format!("{base_url}{path}")
}
