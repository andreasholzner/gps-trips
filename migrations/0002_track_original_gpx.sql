-- US-21: retain the original uploaded GPX file for download (ADR-0003 — it is
-- track data, so it lives in the DB alongside the derived geometry, not in the
-- photo BlobStore).
--
-- The DEFAULT only exists to satisfy adding a NOT NULL column via ALTER; the
-- application always supplies the real bytes on insert, and no rows exist yet.
ALTER TABLE track ADD COLUMN gpx BLOB NOT NULL DEFAULT x'';
