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

/// Just enough of a Komoot tour to count/identify it; not the full tour
/// model later stories (US-22/US-23) will need for import.
#[derive(Debug, Deserialize)]
pub struct KomootTourSummary {
    #[serde(deserialize_with = "id_as_string")]
    pub id: String,
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

#[derive(Deserialize)]
struct LoginResponse {
    username: String,
}

#[derive(Deserialize)]
struct ToursResponse {
    #[serde(rename = "_embedded")]
    embedded: Embedded,
}

#[derive(Deserialize)]
struct Embedded {
    tours: Vec<KomootTourSummary>,
}

/// Talks to Komoot. Real implementation: [`KomootHttpClient`].
pub trait KomootClient: Send + Sync {
    /// Validates credentials and returns the Komoot username, which
    /// [`list_tours`](Self::list_tours) needs to build its URL.
    fn login(&self) -> Result<String, KomootError>;

    /// Lists tours owned by `username`, capped at `limit` if given.
    fn list_tours(
        &self,
        username: &str,
        limit: Option<u32>,
    ) -> Result<Vec<KomootTourSummary>, KomootError>;
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

    /// The one seam every Komoot request goes through: attaches auth,
    /// optionally logs the raw response for `--debug`, and turns
    /// non-success statuses into [`KomootError`].
    fn authed_get(&self, url: &str, query: &[(&str, &str)]) -> Result<String, KomootError> {
        let response = self
            .http
            .get(url)
            .basic_auth(&self.email, Some(&self.password))
            .query(query)
            .send()?;
        let status = response.status();
        let body = response.text()?;

        if self.debug {
            eprintln!("[komoot debug] GET {url} -> {status}\n{body}");
        }

        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(KomootError::Unauthorized);
        }
        if !status.is_success() {
            return Err(KomootError::UnexpectedStatus {
                status: status.as_u16(),
                body,
            });
        }
        Ok(body)
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
    ) -> Result<Vec<KomootTourSummary>, KomootError> {
        let url = format!("{BASE_URL}/v007/users/{username}/tours/");
        let limit_str = limit.map(|l| l.to_string());
        let mut query = Vec::new();
        if let Some(l) = &limit_str {
            query.push(("limit", l.as_str()));
        }
        let body = self.authed_get(&url, &query)?;
        let parsed: ToursResponse = serde_json::from_str(&body)?;
        Ok(parsed.embedded.tours)
    }
}
