//! US-19 — the shared-password gate: one secret, no accounts, and a session
//! the browser carries by itself ([ADR-0010]'s 2026-09-02 amendment).
//!
//! The shape of this module follows three things that amendment settled:
//!
//! * **The credential is a cookie**, because photos load as `<img src>` and
//!   the GPX download is an `<a href>` — plain URL loads that can carry no
//!   `Authorization` header, so the credential has to be one the browser
//!   attaches on its own. `Bearer` is accepted as a second form of the same
//!   token for the client that has no cookie store: the Android app's native
//!   `reqwest` (US-16), and the host-target tests standing in for it.
//! * **The gate resolves a [`Principal`], not a boolean.** Every request
//!   carries `Owner` or `Anonymous` in its extensions; refusing the
//!   anonymous ones is a separate step, which is what leaves room for
//!   US-53's share links without rewriting this.
//! * **The session is signed, not stored.** No sessions table: the token
//!   carries its own expiry and a signature over it, under a key derived
//!   from the password. Rotating the password changes the key, which is what
//!   revokes every session that exists.
//!
//! [ADR-0010]: ../../docs/adr/0010-single-user-optional-auth.md

use std::sync::{Arc, Mutex};

use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, Method},
    middleware::Next,
    response::{IntoResponse, Response},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use time::OffsetDateTime;

use crate::config;
use crate::models::{Principal, Session};
use crate::server::{error::AppError, state::AppState};

type HmacSha256 = Hmac<Sha256>;

/// Domain separation for the two things HMAC is used for here, so a value
/// signed as one can never be read as the other.
const KEY_CONTEXT: &[u8] = b"trip-archive/session-key/v1";
const TOKEN_CONTEXT: &[u8] = b"trip-archive/session-token/v1/owner.";

/// The server was asked to start without a usable shared password.
///
/// Deliberately fatal and deliberately without an exemption: an archive on
/// the public internet (ADR-0023) that boots unauthenticated because an
/// environment variable was forgotten is the failure US-19 exists to
/// prevent, and a local-development escape hatch reachable in production
/// would be the same failure with an excuse.
#[derive(Debug, thiserror::Error)]
#[error(
    "{} is not set, or is empty — the archive refuses to start unauthenticated (US-19)",
    config::auth::PASSWORD_ENV_VAR
)]
pub struct MissingPassword;

/// The gate's own state: the key every session is signed with, and the
/// consecutive-failure counter that rate-limits logins.
///
/// Cloned into every request as part of [`AppState`], hence the `Arc` around
/// the counter — the key is 32 bytes and copies.
#[derive(Clone)]
pub struct Auth {
    key: [u8; 32],
    failures: Arc<Mutex<LoginFailures>>,
}

/// What a login attempt came to.
#[derive(Debug, PartialEq)]
pub enum LoginOutcome {
    Granted(Session),
    /// The password was wrong.
    Refused,
    /// Too many consecutive failures; no password is accepted until the wait
    /// has passed, a correct one included — that is what the lockout means.
    LockedOut(std::time::Duration),
}

/// A token that verified: who it says the caller is, and when it stops
/// saying so.
#[derive(Debug, PartialEq)]
pub struct VerifiedSession {
    pub principal: Principal,
    pub expires_at: OffsetDateTime,
}

impl Auth {
    /// The gate for `password`. Rejects an empty or whitespace-only secret:
    /// it is indistinguishable from none, and none is what this refuses to
    /// run without.
    pub fn new(password: &str) -> Result<Self, MissingPassword> {
        if password.trim().is_empty() {
            return Err(MissingPassword);
        }
        Ok(Self {
            key: derive_key(password),
            failures: Arc::new(Mutex::new(LoginFailures::default())),
        })
    }

    /// The gate configured from the environment — `main`'s entry point, and
    /// the last thing that happens before the archive would otherwise be
    /// reachable. An unset variable and an empty one are the same answer.
    pub fn from_env() -> Result<Self, MissingPassword> {
        Self::new(&std::env::var(config::auth::PASSWORD_ENV_VAR).unwrap_or_default())
    }

    /// Attempt a login. `now` arrives as a value rather than being read here
    /// (ADR-0012's 2026-07-24 amendment), so the lockout is testable without
    /// waiting fifteen minutes.
    pub fn login(&self, password: &str, now: OffsetDateTime) -> LoginOutcome {
        let mut failures = self.failures();
        if let Some(wait) = failures.locked_for(now) {
            return LoginOutcome::LockedOut(wait);
        }
        if !self.password_matches(password) {
            failures.register_failure(now);
            return LoginOutcome::Refused;
        }
        failures.reset();
        LoginOutcome::Granted(self.mint(now))
    }

    /// A fresh session, good for [`config::auth::SESSION_TTL`].
    pub fn mint(&self, now: OffsetDateTime) -> Session {
        let expires_at = now + config::auth::SESSION_TTL;
        Session {
            token: self.sign(expires_at.unix_timestamp()),
            expires_at: format_rfc3339(expires_at),
        }
    }

    /// Read a token: `None` unless the signature is ours *and* the expiry it
    /// carries is still in the future.
    pub fn verify(&self, token: &str, now: OffsetDateTime) -> Option<VerifiedSession> {
        let (expiry, signature) = token.split_once('.')?;
        let expires_at_unix: i64 = expiry.parse().ok()?;
        // Constant time, and false for a signature of the wrong length —
        // `subtle` answers "not equal" rather than short-circuiting on it.
        if !bool::from(
            signature
                .as_bytes()
                .ct_eq(signature_hex(&self.key, expires_at_unix).as_bytes()),
        ) {
            return None;
        }
        let expires_at = OffsetDateTime::from_unix_timestamp(expires_at_unix).ok()?;
        (expires_at > now).then_some(VerifiedSession {
            principal: Principal::Owner,
            expires_at,
        })
    }

    /// Whether `attempt` is the shared password — compared as derived keys
    /// rather than as strings, so the archive holds no copy of the secret in
    /// memory and the comparison leaks neither its content nor its length.
    fn password_matches(&self, attempt: &str) -> bool {
        bool::from(derive_key(attempt).ct_eq(&self.key))
    }

    fn sign(&self, expires_at_unix: i64) -> String {
        format!(
            "{expires_at_unix}.{}",
            signature_hex(&self.key, expires_at_unix)
        )
    }

    /// The failure counter, recovering the count if some earlier panic
    /// poisoned the lock: the state behind it is one integer and a deadline,
    /// and locking the owner out permanently is a worse answer than reading
    /// a counter written by a thread that panicked elsewhere.
    fn failures(&self) -> std::sync::MutexGuard<'_, LoginFailures> {
        self.failures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The signing key: HMAC used as a key-derivation step, so the password
/// itself never has to be kept and rotating it invalidates every token
/// signed under the old one.
fn derive_key(password: &str) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(KEY_CONTEXT).expect("HMAC accepts any key length");
    mac.update(password.as_bytes());
    mac.finalize().into_bytes().into()
}

/// The hex signature over a token's expiry. The whole token is the expiry
/// plus this, so nothing about the session is secret — only unforgeable.
fn signature_hex(key: &[u8; 32], expires_at_unix: i64) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(TOKEN_CONTEXT);
    mac.update(expires_at_unix.to_string().as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// RFC-3339 UTC, the representation ADR-0009 uses for every timestamp that
/// leaves this process.
fn format_rfc3339(at: OffsetDateTime) -> String {
    at.format(&time::format_description::well_known::Rfc3339)
        .expect("an OffsetDateTime always formats as RFC-3339")
}

/// Consecutive failed logins, and the deadline they earned.
///
/// Global rather than per client address: there is one user, so a global
/// counter locks nobody else out, and it needs no decision about whether to
/// trust a platform proxy's `X-Forwarded-For`.
#[derive(Default)]
struct LoginFailures {
    consecutive: u32,
    locked_until: Option<OffsetDateTime>,
}

impl LoginFailures {
    /// How long logins stay refused, or `None` if they are not.
    fn locked_for(&self, now: OffsetDateTime) -> Option<std::time::Duration> {
        self.locked_until
            .filter(|until| *until > now)
            .map(|until| (until - now).unsigned_abs())
    }

    fn register_failure(&mut self, now: OffsetDateTime) {
        self.consecutive += 1;
        if self.consecutive >= config::auth::LOGIN_FAILURE_LIMIT {
            self.locked_until = Some(now + config::auth::LOGIN_LOCKOUT);
            // Cleared with the lockout, so surviving one takes another full
            // run of failures rather than a single attempt.
            self.consecutive = 0;
        }
    }

    fn reset(&mut self) {
        self.consecutive = 0;
        self.locked_until = None;
    }
}

// ── The cookie ───────────────────────────────────────────────────────────────

/// `Set-Cookie` for a session that should last `max_age`.
///
/// `HttpOnly` so no script can read it, `Secure` because the deployed
/// instance is HTTPS-only (ADR-0023/US-49) — browsers treat `localhost` and
/// `127.0.0.1` as secure contexts, so the development loop still works —
/// and `SameSite=Lax`, which is this design's CSRF answer: it withholds the
/// cookie from the cross-site form POSTs that the multipart import and photo
/// uploads would otherwise be targets for.
pub fn session_cookie(token: &str, max_age: std::time::Duration) -> String {
    format!(
        "{}={token}; Max-Age={}; Path=/; HttpOnly; Secure; SameSite=Lax",
        config::auth::COOKIE_NAME,
        max_age.as_secs()
    )
}

/// `Set-Cookie` that ends the session — signing out (US-19).
pub fn cleared_cookie() -> String {
    format!(
        "{}=; Max-Age=0; Path=/; HttpOnly; Secure; SameSite=Lax",
        config::auth::COOKIE_NAME
    )
}

/// The session token from the `Cookie` header, if it holds one.
fn cookie_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            (name.trim() == config::auth::COOKIE_NAME).then(|| value.trim())
        })
}

/// The session token from `Authorization: Bearer …`, the same token in the
/// form a client without a cookie store can send (US-16).
fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("Bearer").then(|| token.trim())
}

// ── The gate ─────────────────────────────────────────────────────────────────

/// Where a request's credential came from — the cookie is the only one worth
/// refreshing, since a `Bearer` client keeps its own token.
#[derive(Clone, Copy, PartialEq)]
enum Credential {
    Cookie,
    Bearer,
}

/// Whether a path is reachable without a session. **Deny by default**: this
/// answers `false` for everything not named here, so a route added later is
/// protected without anyone remembering to protect it.
///
/// What is allowlisted, and why each one has to be:
///
/// * `POST /api/session` — the way in; gating it would gate itself.
/// * `/app…` — the SPA bundle. Code, not data, and it has to load before it
///   can render a login screen.
/// * the four bookmark redirects (`/`, `/trips/:id`, `/import`,
///   `/komoot/sync`) — they answer with a `Location` and nothing else, and
///   they are what lets a bookmark made before US-42/43/44 land on a login
///   screen instead of a JSON body in the address bar.
fn is_public(method: &Method, path: &str) -> bool {
    if method == Method::POST {
        return path == "/api/session";
    }
    // `HEAD` is a `GET` without the body, and axum answers it from the same
    // handler — so a route reachable one way is reachable the other, and the
    // browsers, link previews and platform health checks that ask this way
    // reach the front door rather than a 401.
    if method != Method::GET && method != Method::HEAD {
        return false;
    }
    match path {
        "/app" | "/" | "/import" | "/komoot/sync" => true,
        _ => {
            path.starts_with("/app/")
                || path
                    .strip_prefix("/trips/")
                    .is_some_and(|id| !id.is_empty() && !id.contains('/'))
        }
    }
}

/// The gate, layered over the whole router.
///
/// Resolves the principal for every request and puts it in the extensions,
/// refuses the anonymous ones outside the allowlist, and slides a valid
/// session forward once it is more than halfway through its life.
pub async fn gate(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    // Read once at the boundary and passed onward as a value — the pattern
    // ADR-0012's 2026-07-24 amendment settled on instead of a clock trait.
    let now = OffsetDateTime::now_utc();
    let (principal, source, expires_at) = match resolve(&state.auth, request.headers(), now) {
        Some((session, source)) => (session.principal, Some(source), Some(session.expires_at)),
        None => (Principal::Anonymous, None, None),
    };

    if principal == Principal::Anonymous && !is_public(request.method(), request.uri().path()) {
        return AppError::Unauthorized.into_response();
    }

    request.extensions_mut().insert(principal);
    let mut response = next.run(request).await;

    // The sliding half of the lifetime: a cookie past the halfway mark comes
    // back renewed, so a phone in any kind of regular use never meets the
    // login screen. A `Bearer` client keeps its own token and gets a fresh
    // one by logging in again.
    //
    // Never over a handler that has already spoken about the session, which
    // is the whole of `/api/session`: appending a renewal after signing out
    // hands the browser a clear *and* a fresh cookie, and the later one wins
    // — so pressing "Sign out" on a session old enough to be renewed would
    // silently keep the owner signed in.
    if source == Some(Credential::Cookie) && !sets_session_cookie(&response) {
        if let Some(expires_at) = expires_at {
            if expires_at - now < config::auth::SESSION_REFRESH_AFTER {
                let renewed = state.auth.mint(now);
                if let Ok(cookie) =
                    session_cookie(&renewed.token, config::auth::SESSION_TTL.unsigned_abs()).parse()
                {
                    response.headers_mut().append(header::SET_COOKIE, cookie);
                }
            }
        }
    }
    response
}

/// Whether a response already carries a session cookie of its own.
fn sets_session_cookie(response: &Response) -> bool {
    let prefix = format!("{}=", config::auth::COOKIE_NAME);
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .any(|value| value.as_bytes().starts_with(prefix.as_bytes()))
}

/// The principal a request's headers establish: the cookie first, then
/// `Bearer`.
fn resolve(
    auth: &Auth,
    headers: &HeaderMap,
    now: OffsetDateTime,
) -> Option<(VerifiedSession, Credential)> {
    if let Some(session) = cookie_token(headers).and_then(|token| auth.verify(token, now)) {
        return Some((session, Credential::Cookie));
    }
    let session = bearer_token(headers).and_then(|token| auth.verify(token, now))?;
    Some((session, Credential::Bearer))
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
