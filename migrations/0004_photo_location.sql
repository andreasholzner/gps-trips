-- US-3: place geotagged photos on the map at their EXIF GPS coordinates.
-- lat/lon nullable (most photos have neither, or are placed later by US-4).
-- location_source: 'exif' (this story) | 'interpolated' (US-4) | 'none' (no
-- GPS, or GPS present but unparseable/out-of-range). Plain TEXT, no enum, no
-- CHECK constraint — matches the `activity_type` convention (validated only
-- by the fixed set of values application code ever writes).
-- The DEFAULT on location_source only exists to satisfy adding a NOT NULL
-- column via ALTER; ingest_photos always supplies 'exif' or 'none' on insert.
ALTER TABLE photo ADD COLUMN lat REAL;
ALTER TABLE photo ADD COLUMN lon REAL;
ALTER TABLE photo ADD COLUMN location_source TEXT NOT NULL DEFAULT 'none';
