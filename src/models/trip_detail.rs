use serde::{Deserialize, Serialize};

/// Full trip metadata. The track geometry (a GeoJSON blob) lives in a separate
/// table and is not part of this struct (ADR-0003).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripDetail {
    pub id: i64,
    pub name: String,
    pub activity_type: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    pub duration_secs: Option<i64>,
    pub min_lat: Option<f64>,
    pub min_lon: Option<f64>,
    pub max_lat: Option<f64>,
    pub max_lon: Option<f64>,
}
