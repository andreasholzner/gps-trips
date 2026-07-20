//! Centralized configuration defaults.
//!
//! These are rarely-changed values that don't warrant a settings file or env
//! var of their own (beyond the two paths that are already env-overridable —
//! see `server::paths`) — kept here as one place to find and adjust them,
//! instead of scattered as inline literals across the modules that use them.

/// Storage & filesystem layout (ADR-0002, ADR-0007, ADR-0016; US-10).
pub mod storage {
    /// Env var overriding the data directory (DB + photo blobs). See
    /// `server::paths::data_dir`.
    pub const DATA_DIR_ENV_VAR: &str = "TRIP_ARCHIVE_DATA_DIR";
    /// Default data directory when `DATA_DIR_ENV_VAR` isn't set (the `cargo
    /// run` dev workflow).
    pub const DEFAULT_DATA_DIR: &str = "./data";
    /// Env var overriding the vendored static-assets directory. See
    /// `server::paths::assets_dir`.
    pub const ASSETS_DIR_ENV_VAR: &str = "TRIP_ARCHIVE_ASSETS_DIR";
    /// SQLite database filename, under the data directory.
    pub const DB_FILENAME: &str = "trip-archive.db";
    /// Photo blob subdirectory name, under the data directory (ADR-0007).
    pub const BLOBS_SUBDIR: &str = "photos";
}

/// HTTP server networking (US-10: single-user, laptop-local; deployment
/// topology is otherwise deferred per ADR-0014).
pub mod server {
    /// Address the HTTP server binds to.
    pub const BIND_ADDR: &str = "127.0.0.1:3000";
}

/// Komoot sync (US-27, ADR-0021). Auth details: `docs/komoot-api.md`.
pub mod komoot {
    /// Env var holding the Komoot account email, read by the `komoot_check`
    /// (and later `komoot_backfill`) binaries.
    pub const EMAIL_ENV_VAR: &str = "KOMOOT_EMAIL";
    /// Env var holding the Komoot account password.
    pub const PASSWORD_ENV_VAR: &str = "KOMOOT_PASSWORD";

    /// Minimum spacing between consecutive *authenticated* Komoot API
    /// requests (`KomootHttpClient`'s throttle, `server::komoot::rate_limit`,
    /// US-23/ADR-0021) — applied inside `KomootClient` itself so every call
    /// site (the small "Sync now" and the large historical
    /// `komoot_backfill`) gets it automatically. Does not apply to
    /// `fetch_photo_bytes`, which hits a public, unauthenticated CloudFront
    /// URL, not Komoot's own API.
    pub const MIN_REQUEST_INTERVAL: std::time::Duration = std::time::Duration::from_millis(350);
    /// Backoff applied after a `429` response with no (or unparseable)
    /// `Retry-After` header.
    pub const DEFAULT_RATE_LIMIT_BACKOFF: std::time::Duration = std::time::Duration::from_secs(5);

    /// Page size used when paginating Komoot's tours and tour-photos
    /// endpoints (`server::komoot_sync`).
    pub const PAGE_SIZE: u32 = 200;
}

/// Thumbnail generation (US-5, ADR-0020).
pub mod thumbnail {
    /// Maximum long-edge dimension of a generated thumbnail, in pixels.
    pub const MAX_DIMENSION: u32 = 400;
    /// JPEG quality (0-100) for the re-encoded thumbnail.
    pub const JPEG_QUALITY: u8 = 80;
}
