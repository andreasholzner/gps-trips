-- US-5: the blob key of a photo's generated thumbnail (ADR-0007, ADR-0020).
-- Nullable: legacy photos imported before this feature has no thumbnail and
-- falls back to the full-size original; every newly-ingested photo gets one
-- unless thumbnail generation failed (corrupt/unsupported image bytes).
ALTER TABLE photo ADD COLUMN thumbnail_key TEXT;
