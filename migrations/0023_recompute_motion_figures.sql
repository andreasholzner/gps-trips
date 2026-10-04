-- US-81: moving time now also counts slow, steep climbing, and climbs are
-- found by revised rules — a drop relative to the height gained, gentle ends
-- trimmed, a climb's height its rises added up, missing elevations filled in.
-- Every stored figure is cleared, so the startup backfill works each timed
-- trip out again from its track.
UPDATE trip SET moving_secs = NULL, moving_distance_m = NULL, climb_gain_m = NULL,
    climb_secs = NULL;
