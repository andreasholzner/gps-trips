//! US-19 — the three requests that open, read and end a session.
//!
//! One resource, `/api/session`, in three methods (ADR-0008): `POST` to sign
//! in, `GET` to ask who the caller is, `DELETE` to sign out. The gate itself
//! — how a request's principal is resolved, and which routes may go without
//! one — lives in [`crate::server::auth`].

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use time::OffsetDateTime;

use crate::config;
use crate::models::{Identity, Login, Principal, Session};
use crate::server::{
    auth::{cleared_cookie, session_cookie, LoginOutcome},
    error::AppError,
    state::AppState,
};

/// POST `/api/session` — sign in with the one shared password (US-19).
///
/// The only route the gate lets through unauthenticated besides the SPA
/// bundle itself, for the obvious reason. The token comes back twice: as an
/// `HttpOnly` cookie, which is what the browser attaches to the photo
/// `<img src>` and GPX `<a href>` loads no header can reach, and in the body
/// for a client with no cookie store to put it in (US-16).
///
/// `429` rather than `401` once too many attempts have failed in a row, with
/// a `Retry-After` — and a correct password meets it too, which is what a
/// lockout means.
pub async fn handle_login(
    State(state): State<AppState>,
    Json(body): Json<Login>,
) -> Result<Response, AppError> {
    // Read once at the boundary, then passed on as a value (ADR-0012's
    // 2026-07-24 amendment).
    match state.auth.login(&body.password, OffsetDateTime::now_utc()) {
        LoginOutcome::Granted(session) => Ok(granted(session)),
        LoginOutcome::Refused => Err(AppError::Unauthorized),
        LoginOutcome::LockedOut(retry_after) => Err(AppError::RateLimited { retry_after }),
    }
}

/// GET `/api/session` — who the archive takes the caller to be (US-19).
///
/// Gated like everything else, deliberately: an anonymous caller gets the
/// gate's own `401`, so "am I signed in?" is one request whichever the
/// answer is, and the SPA needs no second way to ask.
///
/// The principal comes from the request's extensions, where the gate put it
/// — so this reads the same value every future authorization check will
/// (US-53), rather than deciding again for itself.
pub async fn handle_session(Extension(principal): Extension<Principal>) -> Json<Identity> {
    Json(Identity { principal })
}

/// DELETE `/api/session` — sign out (US-19).
///
/// Clears the cookie and nothing else: the token is signed rather than
/// stored (ADR-0010's amendment), so there is no server-side row to drop.
/// A `Bearer` client signs out by forgetting its token. Ending *every*
/// session at once is what rotating the password does.
pub async fn handle_logout() -> Response {
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, cleared_cookie())],
    )
        .into_response()
}

fn granted(session: Session) -> Response {
    let cookie = session_cookie(&session.token, config::auth::SESSION_TTL.unsigned_abs());
    ([(header::SET_COOKIE, cookie)], Json(session)).into_response()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
//
// The handlers are asserted through the real router in `tests/us19_auth.rs`,
// where a session's whole round trip — sign in, use the cookie, sign out —
// is one story rather than three isolated calls.
