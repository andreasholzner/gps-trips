use crate::models::LocationSource;

/// A photo attached to a trip (US-2). The image *bytes* live in the `BlobStore`
/// under `blob_key` (ADR-0007); this is the metadata row that associates them
/// with a trip. `lat`/`lon`/`location_source` place the photo on the map
/// (US-3: EXIF GPS; US-4: timestamp-based interpolation for photos without
/// GPS). `thumbnail_key` (US-5, ADR-0020) is `None` for a photo whose
/// thumbnail generation failed, or a legacy photo imported before US-5
/// shipped — callers fall back to the full-size original in that case.
///
/// This is a pure DB record. HTTP handlers project it into a `PhotoResponse`
/// (defined in `server::http`) that adds the public serving URL.
#[derive(Debug, Clone)]
pub struct Photo {
    pub id: i64,
    pub trip_id: i64,
    pub original_name: String,
    pub content_type: Option<String>,
    pub byte_len: i64,
    pub blob_key: String,
    pub thumbnail_key: Option<String>,
    pub created_at: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub location_source: LocationSource,
}
