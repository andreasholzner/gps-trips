//! Opening, reading and ending a session (US-19) — the three calls that run
//! before, and around, every other one in [`super`].

use trip_archive_types::{Identity, Login, Session};

use super::{get_json, ok_or_error, ApiClient, ApiError};

/// `POST /api/session` — sign in with the one shared password (US-19).
///
/// The archive answers with the session *and* sets it as a cookie. On the
/// web the cookie is the credential and the returned token is spare; off it,
/// where there is no cookie store, the token is the only copy — so it is
/// what [`ApiClient::with_token`] is given.
///
/// A wrong password comes back as a `401` and too many wrong ones as a
/// `429`, both carrying the archive's own sentence, which is what the login
/// screen shows: "that is not the password" and "wait a quarter of an hour"
/// are different things to be told.
pub async fn login(archive: &ApiClient, password: &str) -> Result<Session, ApiError> {
    let url = archive.url("/api/session");
    let body = Login {
        password: password.to_string(),
    };
    let response = archive
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;

    ok_or_error(archive, &url, response)
        .await?
        .json()
        .await
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))
}

/// `GET /api/session` — who the archive takes this client to be (US-19).
///
/// The one call whose *failure* is the ordinary answer: a `401` here means
/// nobody is signed in, which is what [`ApiError::is_unauthorized`] is read
/// for. The gate answers it, so asking costs one request either way.
pub async fn session(archive: &ApiClient) -> Result<Identity, ApiError> {
    get_json(archive, archive.url("/api/session")).await
}

/// `DELETE /api/session` — sign out (US-19).
///
/// Clears the cookie on the archive's side. The client forgets its own token
/// separately: there is no server-side session to end, only a signature to
/// stop presenting.
pub async fn logout(archive: &ApiClient) -> Result<(), ApiError> {
    let url = archive.url("/api/session");
    let response = archive
        .delete(&url)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;

    ok_or_error(archive, &url, response).await?;
    Ok(())
}
