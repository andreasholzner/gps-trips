//! Centralized configuration defaults for the SPA, the UI crate's
//! counterpart of the server's `src/config.rs`, which this crate cannot
//! depend on: rarely-changed values kept in one place rather than scattered
//! as inline literals across the modules that use them.

/// The trip list's map (US-63, US-73).
pub mod trip_map {
    /// The Leaflet zoom level from which the map draws the trips in view as
    /// their tracks instead of heat marks: 9 frames a larger region, a few
    /// hundred kilometres across the map, where the marks have piled into
    /// blobs that say nothing about which way the trips went.
    pub const LINES_FROM_ZOOM: f64 = 9.0;
}
