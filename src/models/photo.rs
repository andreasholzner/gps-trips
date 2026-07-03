use crate::models::LocationSource;

/// A photo attached to a trip (US-2). The image *bytes* live in the `BlobStore`
/// under `blob_key` (ADR-0007); this is the metadata row that associates them
/// with a trip. `lat`/`lon`/`location_source` place the photo on the map
/// (US-3: EXIF GPS; US-4 will add timestamp-based interpolation for photos
/// without GPS). Thumbnails (US-5) are added to this model as that story lands.
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
    pub created_at: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub location_source: LocationSource,
}
