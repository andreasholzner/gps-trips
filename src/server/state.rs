use sqlx::SqlitePool;

/// Shared server state threaded through Axum handlers via `State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
}
