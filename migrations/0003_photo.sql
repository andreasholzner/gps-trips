-- US-2: attach photos to a trip so they are stored alongside the track.
--
-- The photo *bytes* (originals, and later thumbnails) live in the BlobStore
-- (ADR-0007); this table records the association to a trip plus the metadata we
-- need to list and serve them. Per-photo geographic coordinates (US-3/US-4),
-- the `location_source`, thumbnails (US-5) and the EXIF capture time are added
-- by later migrations as those stories land — this one stays minimal.
CREATE TABLE IF NOT EXISTS photo (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    trip_id       INTEGER NOT NULL REFERENCES trip(id) ON DELETE CASCADE,
    original_name TEXT    NOT NULL,
    content_type  TEXT,                 -- as declared by the upload; nullable
    byte_len      INTEGER NOT NULL,
    blob_key      TEXT    NOT NULL UNIQUE,  -- where the bytes live in the BlobStore
    created_at    TEXT    NOT NULL          -- RFC-3339 UTC
);

-- Photos are always queried by their trip (list a trip's gallery, cascade delete).
CREATE INDEX IF NOT EXISTS idx_photo_trip ON photo(trip_id);
