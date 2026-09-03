//! Where the archive is, and what proves who is asking (US-19) — the handle
//! every call in [`super`] takes, and the one place a request is built.

use dioxus::prelude::*;

/// Where the archive is, and what proves who is asking (US-19).
///
/// The **base URL** is the page's own origin on the web — the SPA is served
/// by the server it queries, but `reqwest`, unlike a browser `fetch`
/// wrapper, rejects relative URLs outright — and on Android the address the
/// owner configured (US-16), where there is no origin to be relative to.
///
/// The **token** is `None` in the browser, where the session travels as an
/// `HttpOnly` cookie the page cannot read and has no need to: the browser
/// attaches it by itself, which is the whole reason ADR-0010's amendment
/// made the session a cookie. It is `Some` where there is no cookie store —
/// the Android app's native client, and the host-target tests that stand in
/// for it — and then rides every request as `Authorization: Bearer`, the
/// second form of the same token.
#[derive(Clone, Default, PartialEq)]
pub struct ApiClient {
    base_url: String,
    token: Option<String>,
    /// Set when the archive stops recognising this client (US-19).
    ///
    /// Rotating the password is the only revocation the design has
    /// (ADR-0010's amendment: the session is signed, not stored), so a
    /// session ending *mid-use* is an ordinary event, not an edge case. It
    /// arrives as a `401` on whatever the current screen happened to fetch
    /// next — so it is noticed here, where every response already passes,
    /// rather than by each screen remembering to look. The one that forgot
    /// would strand the owner on an error line with no way back to the login
    /// screen, and an Android app (US-16) has no reload to fall back on.
    ///
    /// `None` for a client nobody is watching, which is every client in a
    /// test that is about something else.
    refused: Option<Signal<bool>>,
}

/// The token is redacted: this client is held for the life of the app and
/// passed through every screen, so a `{:?}` of it anywhere — a log line, a
/// panic message, a props dump — would be a live credential in plain sight.
/// Whether one is held is the part worth seeing.
impl std::fmt::Debug for ApiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiClient")
            .field("base_url", &self.base_url)
            .field("signed_in", &self.token.is_some())
            .finish()
    }
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            token: None,
            refused: None,
        }
    }

    /// The same archive, reporting to `refused` when it stops recognising
    /// this client. Set once by the app that owns the login screen.
    pub fn reporting_refusals_to(self, refused: Signal<bool>) -> Self {
        Self {
            refused: Some(refused),
            ..self
        }
    }

    /// Note what the archive answered, so a lost session is seen by whoever
    /// can act on it. Only a refusal counts: a trip that is simply gone
    /// (404), or an edit refused while a sync runs (409), are ordinary
    /// outcomes and must not send the owner back to the login screen.
    pub(super) fn note(&self, status: reqwest::StatusCode) {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            if let Some(mut refused) = self.refused {
                refused.set(true);
            }
        }
    }

    /// The token this client carries, for a test that needs to build a second
    /// client against the same session.
    #[cfg(test)]
    pub fn token_for_test(&self) -> Option<String> {
        self.token.clone()
    }

    /// The same archive, reached with a token in hand.
    pub fn with_token(self, token: impl Into<String>) -> Self {
        Self {
            token: Some(token.into()),
            ..self
        }
    }

    /// The archive's origin, for the two things handed to the browser as a
    /// URL rather than fetched: the GPX download `<a href>` and a photo's
    /// `<img src>`.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub(super) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// A request already carrying the session, where this client holds one.
    pub(super) fn request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        let request = reqwest::Client::new().request(method, url);
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    pub(super) fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::GET, url)
    }

    pub(super) fn post(&self, url: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::POST, url)
    }

    pub(super) fn patch(&self, url: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::PATCH, url)
    }

    pub(super) fn delete(&self, url: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::DELETE, url)
    }
}
