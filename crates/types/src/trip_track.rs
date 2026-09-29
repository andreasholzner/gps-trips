use serde::{Deserialize, Serialize};

/// One trip's track as `GET /api/trips/tracks` answers it (US-73): the
/// stored positions alone, as `[lon, lat]` in GeoJSON's order, for drawing
/// many trips' lines at once. The elevation, times and chart series stay in
/// `track.geojson`, which only the detail screen needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TripTrack {
    pub id: i64,
    pub coordinates: Vec<[f64; 2]>,
}
