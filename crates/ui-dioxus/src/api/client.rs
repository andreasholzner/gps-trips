//! Where the archive is, and what proves who is asking (US-19) — the handle
//! every call in [`super`] takes, and the one place a request is built.

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
        }
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
