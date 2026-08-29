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
use trip_archive_types::{Tag, TripSummary};

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

/// `GET /api/trips` — the filtered list (US-6/US-13). `query` is
/// `Filters::to_query`'s output, without a leading `?` (the same string the
/// SPA carries in its own URL); empty means the unfiltered list.
pub async fn list_trips(base_url: &str, query: String) -> Result<Vec<TripSummary>, ApiError> {
    let separator = if query.is_empty() { "" } else { "?" };
    get_json(format!("{base_url}/api/trips{separator}{query}")).await
}

/// `GET /api/tags` — every known tag, for the tag filter's choices (US-38)
/// and the bulk-tag suggestions (US-34).
pub async fn list_tags(base_url: &str) -> Result<Vec<Tag>, ApiError> {
    get_json(format!("{base_url}/api/tags")).await
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
        .map_err(|err| ApiError(format!("{url} unreachable: {err}")))?;

    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(ApiError(
            "one or more selected trips no longer exist; nothing was tagged".to_string(),
        ));
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError(if body.trim().is_empty() {
            format!("{url} returned {status}")
        } else {
            body
        }));
    }
    response
        .json()
        .await
        .map_err(|err| ApiError(format!("{url} returned unreadable JSON: {err}")))
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
