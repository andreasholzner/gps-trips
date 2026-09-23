use serde::{Deserialize, Serialize};

/// The `PATCH /api/trips/:id/photos/:photo_id` request body (US-30): where
/// the owner placed the photo on the map, in decimal degrees. The server
/// refuses a position off the globe.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhotoPlacement {
    pub lat: f64,
    pub lon: f64,
}
