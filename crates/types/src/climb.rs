//! US-81 — the climbs of a trip: the stretches whose elevation rises from a
//! low point to a high point, significant for the trip's activity.

use serde::{Deserialize, Serialize};

/// The distance, centred on a point, over which the elevation profile is
/// looked at: the incline at a point is measured across it (US-79), and the
/// elevation climbs are found on is averaged over it (US-81). One value,
/// here where the server and the SPA both reach it, so the two agree.
pub const INCLINE_WINDOW_M: f64 = 50.0;

/// One climb, in track order, as `GET /api/trips/:id/climbs` answers it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Climb {
    /// Where it starts and ends, as distances along the track — the
    /// elevation profile's own x-axis.
    pub start_m: f64,
    pub end_m: f64,
    /// The height it gains: its rises on the smoothed elevation added up, so
    /// the height won back after a drop within it counts.
    pub gain_m: f64,
    /// The time spent moving on it (US-77); `None` for a stretch without
    /// times, which has no climbing rate.
    pub moving_secs: Option<i64>,
}
