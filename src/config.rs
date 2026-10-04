//! Centralized configuration defaults.
//!
//! These are rarely-changed values that don't warrant a settings file or env
//! var of their own (beyond the paths and the listen address that are already
//! env-overridable — see `server::paths`) — kept here as one place to find and
//! adjust them, instead of scattered as inline literals across the modules
//! that use them.

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
    /// The place-name database (US-74, ADR-0027), under the data directory
    /// but no part of the archive: built by `places_build`, copied there by
    /// hand, and optional.
    pub const PLACES_DB_FILENAME: &str = "places.sqlite";
}

/// HTTP server networking (US-10 on the laptop, US-45 in a container).
pub mod server {
    /// Env var overriding the listen address, as `IP:port`. See
    /// `server::paths::bind_addr`.
    pub const BIND_ADDR_ENV_VAR: &str = "TRIP_ARCHIVE_BIND_ADDR";
    /// Listen address when `BIND_ADDR_ENV_VAR` isn't set: loopback only, so
    /// a laptop run is never reachable from the network by accident.
    pub const DEFAULT_BIND_ADDR: ([u8; 4], u16) = ([127, 0, 0, 1], 3000);

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

    /// The `Content-Security-Policy` every response carries. What the SPA
    /// needs and no more: its own scripts, styles and API; OSM's tiles and
    /// the `data:` icons Pico's CSS draws with. `'unsafe-eval'` because
    /// `document::eval` (ADR-0025) runs its scripts through `new Function`;
    /// inline scripts and event-handler attributes stay refused, which is
    /// what stops markup that reaches the page from running.
    pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; \
        script-src 'self' 'unsafe-eval' 'wasm-unsafe-eval'; \
        style-src 'self'; \
        img-src 'self' data: https://*.tile.openstreetmap.org; \
        connect-src 'self'; \
        object-src 'none'; \
        base-uri 'none'; \
        form-action 'self'; \
        frame-ancestors 'none'";
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

    /// The file, under the data directory, holding the salt the session
    /// signing key is derived under (US-55). Created on first boot; not part
    /// of the backup, so a restore ends every session once.
    pub const SALT_FILENAME: &str = "session-salt";
    /// Salt length in bytes.
    pub const SALT_LEN: usize = 16;

    /// Argon2id's cost for deriving the signing key — RFC 9106's
    /// low-memory recommendation. Paid once at boot and once per login
    /// attempt, and by an attacker once per guess should the salt leak.
    pub const ARGON2_MEMORY_KIB: u32 = 64 * 1024;
    pub const ARGON2_ITERATIONS: u32 = 3;
    pub const ARGON2_LANES: u32 = 1;
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

/// The access log (US-70).
pub mod access_log {
    /// Records waiting for the writer. A request hands its record over
    /// without waiting; one arriving while this many are queued is dropped
    /// rather than delaying the request — a burst far beyond any page load,
    /// or a database that has stopped answering.
    pub const QUEUE_CAPACITY: usize = 10_000;
    /// Records written per transaction, at most.
    pub const BATCH_SIZE: usize = 500;
}

/// Shares — read-only links to a few trips (US-53).
pub mod share {
    /// Random bytes in a share's token: 256 bits, so a link cannot be guessed
    /// however many are tried.
    pub const TOKEN_BYTES: usize = 32;
    /// Where a share's own routes live: `/s/<token>/…`.
    pub const PATH_PREFIX: &str = "/s/";
    /// Where the SPA shows a share's screens: `/app/s/<token>…`.
    pub const PAGE_PREFIX: &str = "/app/s/";
    /// The lifetimes the owner picks between.
    pub const ONE_MONTH: time::Duration = time::Duration::days(30);
    pub const SIX_MONTHS: time::Duration = time::Duration::days(182);
    /// Longest label, in characters — it is a title, not a message.
    pub const LABEL_MAX_CHARS: usize = 100;
}

/// The stored copy of each photo (US-54, ADR-0026).
pub mod photo {
    /// Maximum long-edge dimension of the stored copy, in pixels. A photo
    /// within it is stored as uploaded.
    pub const MAX_DIMENSION: u32 = 2048;
    /// JPEG quality (0-100) for a photo re-encoded to fit the bound.
    pub const JPEG_QUALITY: u8 = 80;
    /// Largest decoded pixel buffer, in bytes, an import may allocate for one
    /// photo — a 48 MP camera photo is 144 MB as RGB. Well inside the
    /// deployed machine's memory next to a full upload request
    /// (`server::PHOTO_IMPORT_BODY_LIMIT`); a photo beyond it is stored as
    /// uploaded.
    pub const MAX_DECODE_BYTES: u64 = 256 * 1024 * 1024;
}

/// Thumbnail generation (US-5, ADR-0020).
pub mod thumbnail {
    /// Maximum long-edge dimension of a generated thumbnail, in pixels.
    pub const MAX_DIMENSION: u32 = 200;
    /// JPEG quality (0-100) for the re-encoded thumbnail.
    pub const JPEG_QUALITY: u8 = 80;
}

/// Moving time (US-77): the time between consecutive timed track points
/// counts as moving when the speed between them reaches the activity's
/// threshold here, so breaks and stops drop out. Per activity, because a
/// slow scramble is still moving where a slow bike is standing.
pub mod moving_time {
    use crate::models::ActivityType;

    /// The lowest speed, in km/h, that still counts as moving.
    pub const fn min_speed_kmh(activity: ActivityType) -> f64 {
        match activity {
            ActivityType::Mountaineering => 0.5,
            ActivityType::SnowShoe | ActivityType::SkiTouring | ActivityType::Bikepacking => 0.8,
            ActivityType::Hiking | ActivityType::Unknown => 1.0,
            ActivityType::Kayaking => 1.5,
            ActivityType::CrossCountrySkiing => 2.0,
            ActivityType::Cycling => 3.0,
        }
    }

    /// The rate, in metres an hour, at which the smoothed elevation changing
    /// counts as moving whatever the speed over the ground (US-81): on a
    /// steep slope a hiker climbing 400 m/h covers under 1 km/h, which the
    /// speed alone would read as standing still.
    pub const MIN_VERTICAL_MH: f64 = 100.0;

    /// The time the elevation's rate of change is measured over, centred on
    /// each stretch: a minute, so GPS noise while standing still averages
    /// out and a slow climb still shows.
    pub const VERTICAL_WINDOW_S: i64 = 60;
}

/// What counts as a climb (US-81): a rise significant for the activity, and
/// how far the elevation may fall below a climb's highest point before the
/// climb ends — so a short dip or a flat does not split one hill in two.
pub mod climbs {
    use crate::models::ActivityType;

    /// The rule one activity's climbs are found by.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct ClimbRule {
        /// The least height a climb gains.
        pub min_gain_m: f64,
        /// The least average gradient, in percent, if the activity has one.
        pub min_gradient_pct: Option<f64>,
        /// A fall below the highest point so far ends it if it is greater
        /// than this, or than `max_dip_share` of the height gained so far,
        /// whichever is more — so a long climb is not split by a short drop.
        pub max_dip_m: f64,
        pub max_dip_share: f64,
        /// A stretch flatter than this at either end of a climb, measured
        /// over [`TRIM_WINDOW_M`], is cut off if it is longer than
        /// `trim_tolerance_m`: a gentle approach, or a plateau, is not part
        /// of the hill — though a summit that flattens out within the
        /// tolerance is.
        pub trim_gradient_pct: f64,
        pub trim_tolerance_m: f64,
    }

    /// The distance a climb's ends are judged gentle or not over: long
    /// enough that a bump on a plateau does not read as climbing.
    pub const TRIM_WINDOW_M: f64 = 200.0;

    /// The rule for `activity`; `None` for an activity that has no climbs.
    pub const fn rule(activity: ActivityType) -> Option<ClimbRule> {
        match activity {
            ActivityType::Cycling | ActivityType::Bikepacking => Some(ClimbRule {
                min_gain_m: 30.0,
                min_gradient_pct: Some(3.0),
                max_dip_m: 10.0,
                max_dip_share: 0.1,
                trim_gradient_pct: 3.0,
                trim_tolerance_m: 1000.0,
            }),
            ActivityType::CrossCountrySkiing | ActivityType::SkiTouring => Some(ClimbRule {
                min_gain_m: 15.0,
                min_gradient_pct: Some(3.0),
                max_dip_m: 10.0,
                max_dip_share: 0.1,
                trim_gradient_pct: 3.0,
                trim_tolerance_m: 1000.0,
            }),
            // Summits often sit at the end of a long, nearly flat ridge.
            ActivityType::Hiking | ActivityType::Mountaineering | ActivityType::SnowShoe => {
                Some(ClimbRule {
                    min_gain_m: 75.0,
                    min_gradient_pct: None,
                    max_dip_m: 20.0,
                    max_dip_share: 0.1,
                    trim_gradient_pct: 3.0,
                    trim_tolerance_m: 3000.0,
                })
            }
            ActivityType::Kayaking | ActivityType::Unknown => None,
        }
    }
}

/// The place-based name suggestion (US-74): how far each kind of place
/// reaches, what it weighs, and how a name is put together from them.
pub mod name_suggestion {
    use crate::server::places::PlaceKind;

    /// At most this many main places in a name.
    pub const MAX_MAIN_PLACES: usize = 3;

    /// A trip that ends within this distance of its start is a round trip.
    pub const ROUND_TRIP_M: f64 = 1_000.0;

    /// On a round trip, a main place that weighs this many times another
    /// leaves that one out: one this much weightier than all the rest is
    /// named alone.
    pub const DOMINANCE_RATIO: f64 = 3.0;

    /// On a round trip, a main place at the turning point weighs this much
    /// more than its importance alone, falling off with the cube of its
    /// nearness to it — the share of the turning point's distance from the
    /// start it is not away from it: one halfway back an eighth as much. A
    /// round trip goes somewhere.
    pub const TURNING_POINT_BONUS: f64 = 4.0;

    /// Where a round trip turned is its point farthest from the start, a
    /// metre climbed above the start counting as this many metres out —
    /// Naismith's rule, about an hour for 600 m up or 5 km on — so a hike
    /// turns at its summit.
    pub const TURNING_CLIMB_FACTOR: f64 = 8.0;

    /// A hut or campsite this close to an end is where the trip stopped.
    pub const STOP_REACH_M: f64 = 150.0;

    /// How far from a hut or campsite the place it is named after may lie.
    pub const NAMESAKE_REACH_M: f64 = 5_000.0;

    /// Two places of a kind and a name this close are the same place.
    pub const DUPLICATE_REACH_M: f64 = 300.0;

    /// Two places of a kind this close from different sources are the same
    /// place under two names — the register's and OSM's, in another
    /// language. A register's lake within OSM's outline is 0 m from it.
    pub const RENAMED_DUPLICATE_REACH_M: f64 = 50.0;

    /// How far from an end a place of `kind` may lie and still name it;
    /// `None` for a kind that never names an end.
    pub const fn end_reach_m(kind: PlaceKind) -> Option<f64> {
        match kind {
            PlaceKind::City => Some(5_000.0),
            PlaceKind::Town => Some(3_000.0),
            PlaceKind::Village => Some(1_500.0),
            PlaceKind::Hamlet => Some(600.0),
            PlaceKind::Lake | PlaceKind::Bay => Some(300.0),
            PlaceKind::Glacier | PlaceKind::Summit | PlaceKind::Pass => Some(200.0),
            PlaceKind::Hut | PlaceKind::Campsite => Some(STOP_REACH_M),
            // A trailhead is often a named spot with nothing on it.
            PlaceKind::Locality => Some(400.0),
            PlaceKind::Farm => None,
        }
    }

    /// How close the track must pass a place of `kind` for it to be one of
    /// the trip's main places; `None` for a kind that never is.
    pub const fn main_reach_m(kind: PlaceKind) -> Option<f64> {
        match kind {
            PlaceKind::City => Some(2_000.0),
            PlaceKind::Town => Some(1_000.0),
            PlaceKind::Village => Some(500.0),
            PlaceKind::Hamlet => Some(200.0),
            PlaceKind::Summit | PlaceKind::Pass => Some(150.0),
            PlaceKind::Bay => Some(300.0),
            PlaceKind::Hut | PlaceKind::Lake => Some(100.0),
            PlaceKind::Glacier => Some(50.0),
            PlaceKind::Campsite | PlaceKind::Farm | PlaceKind::Locality => None,
        }
    }

    /// What a place of `kind` weighs before what the sources say about its
    /// size: a settlement's kind is all there is to it.
    pub const fn base_weight(kind: PlaceKind) -> f64 {
        match kind {
            PlaceKind::City => 100.0,
            PlaceKind::Town => 40.0,
            PlaceKind::Village => 15.0,
            PlaceKind::Hamlet => 4.0,
            PlaceKind::Hut => 10.0,
            PlaceKind::Pass => 8.0,
            PlaceKind::Campsite => 2.0,
            PlaceKind::Summit | PlaceKind::Lake | PlaceKind::Bay | PlaceKind::Glacier => 2.0,
            PlaceKind::Farm | PlaceKind::Locality => 1.0,
        }
    }

    /// A lake, bay or glacier weighs this much per square root of its area
    /// in km², on top of its base weight: a 1 km² lake 10, a 25 km² one 50.
    pub const AREA_WEIGHT_PER_KM: f64 = 10.0;

    /// A summit weighs its prominence divided by this, where a source gives
    /// it — 300 m weighs 30.
    pub const PROMINENCE_PER_WEIGHT_M: f64 = 10.0;

    /// A summit without a known prominence weighs its height divided by
    /// this — 884 m weighs about 9.
    pub const ELEVATION_PER_WEIGHT_M: f64 = 100.0;
}
