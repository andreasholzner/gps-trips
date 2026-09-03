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
    /// Env var overriding the static-assets directory — since US-44 that is
    /// the SPA's built bundle and nothing else. See
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

    /// Request-body cap for the multipart upload routes (`/api/import`,
    /// `/api/import/staged` and `/api/trips/:id/photos`, ADR-0004). Axum's
    /// 2 MB default is far too small for a GPX plus a batch of camera photos
    /// — or, on the staging route, for a recorded track of tens of thousands
    /// of points on its own; every other route keeps the default.
    pub const PHOTO_IMPORT_BODY_LIMIT: usize = 256 * 1024 * 1024;

    /// How long a two-phase import's parked parse survives unconfirmed
    /// (US-12, migration 0014). Long enough that a distracted owner can come
    /// back to a half-finished import, short enough that abandoned ones do
    /// not accumulate. Swept on the way into the next staging request.
    pub const STAGED_IMPORT_TTL: time::Duration = time::Duration::hours(24);
}

/// The shared-password gate (US-19, [ADR-0010]'s 2026-09-02 amendment).
///
/// [ADR-0010]: ../../docs/adr/0010-single-user-optional-auth.md
pub mod auth {
    /// Env var holding the one shared password. Unlike the Komoot
    /// credentials above, this one is **required**: a missing or empty value
    /// makes the server refuse to start rather than serve the archive
    /// unauthenticated (US-19/US-48). There is deliberately no local-
    /// development exemption — an exemption reachable in production would
    /// defeat the story.
    pub const PASSWORD_ENV_VAR: &str = "TRIP_ARCHIVE_PASSWORD";

    /// Name of the session cookie the browser attaches by itself — which is
    /// the whole reason the session is a cookie: photos load as `<img src>`
    /// and the GPX download is an `<a href>`, plain URL loads that can carry
    /// no `Authorization` header.
    pub const COOKIE_NAME: &str = "trip_archive_session";

    /// How long a session lasts. Long, and slid forward by
    /// [`SESSION_REFRESH_AFTER`], because the phone is the primary client
    /// and a login screen there is friction, not security.
    pub const SESSION_TTL: time::Duration = time::Duration::days(90);

    /// Once more than this much of a session's life has passed, the next
    /// authenticated request re-issues the cookie for a fresh
    /// [`SESSION_TTL`]. Half the lifetime: an archive opened at any interval
    /// shorter than 45 days never sees a login screen, and one left alone
    /// dies within 90 days of its last use.
    pub const SESSION_REFRESH_AFTER: time::Duration =
        time::Duration::seconds(SESSION_TTL.whole_seconds() / 2);

    /// How many consecutive failed logins are tolerated before the archive
    /// stops answering them for [`LOGIN_LOCKOUT`]. One secret on the public
    /// internet, and on a scale-to-zero machine (ADR-0023) each attempt is
    /// also a wake-up the owner pays for.
    pub const LOGIN_FAILURE_LIMIT: u32 = 5;

    /// How long logins are refused once [`LOGIN_FAILURE_LIMIT`] consecutive
    /// attempts have failed. Counted for the instance as a whole, not per
    /// client address: there is one user, so there is no other legitimate
    /// caller to lock out, and it needs no decision about whether to trust a
    /// proxy's `X-Forwarded-For`.
    ///
    /// The accepted cost is that **anyone** can lock the owner out for this
    /// long with five wrong guesses — an availability attack that per-IP
    /// counting would blunt and that this deliberately does not. The trade
    /// was made knowingly: guessing the secret is the risk worth stopping,
    /// the owner can wait fifteen minutes, and on a scale-to-zero machine
    /// (ADR-0023) an attacker who is being answered at all is one the owner
    /// is paying to wake.
    pub const LOGIN_LOCKOUT: std::time::Duration = std::time::Duration::from_secs(15 * 60);
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
