-- US-77: how long a trip was actually under way, apart from duration_secs,
-- which counts every break. Computed from the track under the activity's
-- speed threshold, and recomputed when the activity changes. NULL for a
-- track without times, and for a trip imported before this column existed
-- until the startup backfill reaches it.
ALTER TABLE trip ADD COLUMN moving_secs INTEGER;
