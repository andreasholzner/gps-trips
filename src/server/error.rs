use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

/// Errors surfaced to the HTTP layer.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Not found")]
    NotFound,

    /// US-26: a `PATCH`/`DELETE`/sync request that lost the race against an
    /// in-flight "Sync now" run (ADR-0021's concurrency guard).
    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    Import(#[from] ImportError),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Storage error: {0}")]
    Storage(#[from] std::io::Error),

    #[error("Komoot error: {0}")]
    Komoot(#[from] crate::server::komoot::KomootError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not found".to_string()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::Import(e) => (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()),
            AppError::Database(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::Storage(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::Komoot(e) => (StatusCode::BAD_GATEWAY, e.to_string()),
        };
        (status, body).into_response()
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
