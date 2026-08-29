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
use trip_archive_types::TripSummary;

/// A failed API call, already reduced to what the UI shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ApiError(pub String);

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
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

/// `GET /api/trips` — the filtered list (US-6/US-13); `query` is the
/// `?`-prefixed query string, or empty for the unfiltered list.
pub async fn list_trips(base_url: &str, query: String) -> Result<Vec<TripSummary>, ApiError> {
    get_json(format!("{base_url}/api/trips{query}")).await
}
