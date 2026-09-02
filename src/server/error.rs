use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use thiserror::Error;

use crate::models::ErrorResponse;

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
            AppError::Database(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::Storage(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::Komoot(e) => (StatusCode::BAD_GATEWAY, e.to_string()),
        }
    }
}

impl IntoResponse for AppError {
    /// Every refusal answers as JSON, like everything else the API says
    /// (ADR-0008). The status is the machine-readable half and the only
    /// thing a client branches on; the body carries the sentence.
    fn into_response(self) -> Response {
        let (status, message) = self.status_and_message();
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
