/// A photo attached to a trip (US-2). The image *bytes* live in the `BlobStore`
/// under `blob_key` (ADR-0007); this is the metadata row that associates them
/// with a trip. Per-photo map coordinates (US-3/US-4) and thumbnails (US-5) are
/// added to this model as those stories land.
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
}
