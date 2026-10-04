-- US-81: what a trip's climbing rate is worked out from — the height its
-- climbs gain, added up, and the time spent moving on them. Worked out and
-- recomputed with moving_secs, under the activity's climb rule; 0 and 0 for
-- a timed trip without a climb, NULL for a track without times and for a
-- trip stored before these columns existed until the startup backfill
-- reaches it. The climbs themselves are found again whenever asked for.
ALTER TABLE trip ADD COLUMN climb_gain_m REAL;
ALTER TABLE trip ADD COLUMN climb_secs INTEGER;
