-- US-80: how far a trip went while moving, counted over the same stretches
-- as moving_secs, so their ratio is the average speed in motion. Worked out
-- and recomputed with moving_secs; NULL for a track without times, and for a
-- trip stored before this column existed until the startup backfill reaches
-- it.
ALTER TABLE trip ADD COLUMN moving_distance_m REAL;
