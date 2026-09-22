-- US-62: when each photo was taken, for the caption under its thumbnail.
-- The EXIF capture time was always read at ingestion (ADR-0017) — to place the
-- photo by the track (US-4) — and then thrown away; `created_at` is when the
-- row was inserted, not when the shutter went. RFC-3339 UTC (ADR-0009).
-- Nullable: a photo whose EXIF names no capture time has none, and photos
-- stored before this have none until `photo_taken_at_backfill` reads it back
-- out of the stored copy (ADR-0026 keeps its EXIF verbatim).
ALTER TABLE photo ADD COLUMN taken_at TEXT;
