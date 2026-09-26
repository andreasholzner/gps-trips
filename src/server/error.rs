use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use thiserror::Error;

use crate::models::ErrorResponse;

/// The answer to a database or storage failure; the detail goes to the log.
const SERVER_FAILURE_MSG: &str =
    "The archive could not complete this request; the details are in its log";

/// Errors surfaced to the HTTP layer.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Not found")]
    NotFound,

    /// US-19: the request carried no session the archive recognises, and the
    /// route it asked for is not on the gate's allowlist.
    #[error("Authentication required")]
    Unauthorized,

    /// US-19: too many consecutive failed logins. Carries how long the
    /// caller has to wait, which travels back as `Retry-After` — a lockout
    /// nobody can see the end of is a lockout nobody can act on.
    #[error("Too many failed sign-in attempts; try again in {} seconds", .retry_after.as_secs())]
    RateLimited { retry_after: std::time::Duration },

    /// US-26: a `PATCH`/`DELETE`/sync request that lost the race against an
    /// in-flight "Sync now" run (ADR-0021's concurrency guard).
    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    Import(#[from] ImportError),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// A bug rather than a bad request: state the archive itself wrote and
    /// then could not read back (US-12's parked parse is the only such state
    /// today). Reported as a 500 because nothing the caller does differently
    /// would help.
    #[error("{0}")]
    Internal(String),

    #[error("Storage error: {0}")]
    Storage(#[from] std::io::Error),

    #[error("Komoot error: {0}")]
    Komoot(#[from] crate::server::komoot::KomootError),
}

impl AppError {
    /// The status this is reported as, and the archive's own account of it.
    ///
    /// The sentence is what the owner reads — the screens show it verbatim —
    /// so it is the message each variant was already answering with, not its
    /// `Display` form: `Display` prefixes some variants for a log line's
    /// benefit ("Database error: …"), which is a different audience.
    fn status_and_message(&self) -> (StatusCode, String) {
        match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not found".to_string()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::RateLimited { .. } => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::Import(e) => (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()),
            // What failed inside the archive — paths, queries — is for its
            // log, not for whoever made the request; a share's recipient can
            // meet these too.
            AppError::Database(_) | AppError::Storage(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                SERVER_FAILURE_MSG.to_string(),
            ),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::Komoot(e) => (StatusCode::BAD_GATEWAY, e.to_string()),
        }
    }
}

impl IntoResponse for AppError {
    /// Every refusal answers as JSON, like everything else the API says
    /// (ADR-0008). The status is the machine-readable half and the only
    /// thing a client branches on; the body carries the sentence.
    ///
    /// A failure on the archive's side is logged here, with its detail: the
    /// one place every handler's error passes through, and otherwise the
    /// only trace of it would be the answer the caller got.
    fn into_response(self) -> Response {
        let (status, message) = self.status_and_message();
        if status.is_server_error() {
            tracing::error!(status = status.as_u16(), "{self}");
        }
        let body = Json(ErrorResponse::new(message));
        match self {
            // The one refusal that says when to come back (US-19).
            AppError::RateLimited { retry_after } => (
                status,
                [(header::RETRY_AFTER, retry_after.as_secs().to_string())],
                body,
            )
                .into_response(),
            _ => (status, body).into_response(),
        }
    }
}

/// Domain errors from the GPX import pipeline.
#[derive(Debug, Error)]
pub enum ImportError {
    #[error("Failed to parse GPX: {0}")]
    Parse(String),

    #[error("GPX file contains no tracks")]
    NoTrack,

    #[error("Track has no points")]
    NoPoints,
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Answer `error` as the router would, and return the response's status
    /// and body alongside what was logged while doing it.
    async fn answer(error: AppError) -> (StatusCode, String, String) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let writer = {
            let log = Arc::clone(&log);
            move || Writer(Arc::clone(&log))
        };
        let subscriber = tracing_subscriber::fmt()
            .with_writer(writer)
            .with_ansi(false)
            .finish();
        let response = tracing::subscriber::with_default(subscriber, || error.into_response());
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let logged = String::from_utf8(log.lock().unwrap().clone()).unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap(), logged)
    }

    struct Writer(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Writer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_server_failure_is_logged_with_its_detail_and_answered_without_it() {
        let detail = "/data/photos/trips/1: permission denied";
        let (status, body, logged) = answer(AppError::Storage(std::io::Error::other(detail))).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(
            logged.contains("ERROR") && logged.contains(detail),
            "{logged}"
        );
        assert!(!body.contains(detail), "{body}");
        assert!(body.contains("error"), "still a JSON refusal: {body}");
    }

    #[tokio::test]
    async fn a_database_failure_is_logged_and_kept_out_of_the_answer() {
        let (status, body, logged) = answer(AppError::Database(sqlx::Error::PoolTimedOut)).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(logged.contains("ERROR"), "{logged}");
        assert!(!body.to_lowercase().contains("pool"), "{body}");
    }

    #[tokio::test]
    async fn a_refusal_the_caller_caused_is_not_logged_as_an_error() {
        let (status, body, logged) = answer(AppError::BadRequest("no trips".into())).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.contains("no trips"), "{body}");
        assert!(!logged.contains("ERROR"), "{logged}");
    }

    #[tokio::test]
    async fn a_komoot_failure_is_logged_and_still_named_to_the_owner() {
        // US-25: the sync's failure has to say what went wrong.
        let (status, body, logged) = answer(AppError::Komoot(
            crate::server::komoot::KomootError::Unauthorized,
        ))
        .await;

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert!(logged.contains("ERROR"), "{logged}");
        assert!(body.contains("rejected the credentials"), "{body}");
    }
}
