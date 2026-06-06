use serde::{Deserialize, Serialize};

/// A photo attached to a trip (US-2). The image *bytes* live in the `BlobStore`
/// under `blob_key` (ADR-0007); this is the metadata row that associates them
/// with a trip. Per-photo map coordinates (US-3/US-4) and thumbnails (US-5) are
/// added to this model as those stories land.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Photo {
    pub id: i64,
    pub trip_id: i64,
    pub original_name: String,
    pub content_type: Option<String>,
    pub byte_len: i64,
    /// Where the bytes live in the `BlobStore`. An internal storage detail, not
    /// part of the API — the public photo URL (via `BlobStore::url_for`) arrives
    /// with the gallery/serving story (US-7).
    #[serde(skip_serializing)]
    pub blob_key: String,
    pub created_at: String,
}
