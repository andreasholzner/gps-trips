//! Centralized configuration defaults for the SPA, the UI crate's
//! counterpart of the server's `src/config.rs`, which this crate cannot
//! depend on: rarely-changed values kept in one place rather than scattered
//! as inline literals across the modules that use them.

/// The trip list's map (US-63, US-73).
pub mod trip_map {
    /// The Leaflet zoom level from which the map draws the trips in view as
    /// their tracks instead of heat marks: 7 frames a small country, several
    /// hundred kilometres across the map, where the marks have already
    /// piled into blobs that say nothing about which way the trips went.
    pub const LINES_FROM_ZOOM: f64 = 7.0;
}

/// The elevation profile's speed and incline (US-79).
pub mod elevation_profile {
    /// The time the speed at a point is averaged over, centred on it: GPS
    /// jitter between two points a second apart reads as wild speeds, and
    /// half a minute either side settles it without hiding a real change.
    pub const SPEED_WINDOW_S: f64 = 30.0;

    /// A gap between two timed points longer than this is a break in the
    /// recording — a stop, or a tunnel — and no speed window reaches across it.
    pub const PAUSE_GAP_S: f64 = 60.0;

    /// A gap that covered less distance than this was a pause, and reads as
    /// 0 km/h; one that covered more (a tunnel with no reception) reads as its
    /// average speed. Wide enough for the jump a receiver makes when it fixes
    /// again after a stop.
    pub const PAUSE_GAP_MAX_M: f64 = 50.0;

    /// A smoothed speed below this is GPS drift while standing still, and
    /// reads as 0 km/h — low enough that a slow scramble still counts as moving.
    pub const STANDSTILL_KMH: f64 = 0.5;

    /// The distance the incline at a point is measured over, centred on it —
    /// shared with the server, which finds climbs on the elevation averaged
    /// over the same distance (US-81).
    pub use trip_archive_types::INCLINE_WINDOW_M;

    /// A run shorter than this gives no incline, rather than a rise divided by
    /// next to nothing.
    pub const MIN_INCLINE_RUN_M: f64 = 1.0;
}
