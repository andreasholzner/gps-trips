-- US-103 (ADR-0030): a link the archive proposed from the tracks' geometry
-- is a link marked as proposed, kept as links are until the owner confirms
-- it (the mark cleared) or dismisses it (the row deleted). A proposed link
-- counts as a link nowhere. Its coverage, as shares from 0 to 1, is what the
-- owner reads it by: the line's two shares for a plan followed along its
-- line, the points' share for one judged by its own points, none for a
-- waypoint passed — whose table has the columns only so every link reads
-- alike. A link made or confirmed by hand ignores what it had.

ALTER TABLE trip_trip_link ADD COLUMN proposed       INTEGER NOT NULL DEFAULT 0;
ALTER TABLE trip_trip_link ADD COLUMN plan_covered   REAL;
ALTER TABLE trip_trip_link ADD COLUMN trip_on_plan   REAL;
ALTER TABLE trip_trip_link ADD COLUMN points_reached REAL;

ALTER TABLE trip_track_link ADD COLUMN proposed       INTEGER NOT NULL DEFAULT 0;
ALTER TABLE trip_track_link ADD COLUMN plan_covered   REAL;
ALTER TABLE trip_track_link ADD COLUMN trip_on_plan   REAL;
ALTER TABLE trip_track_link ADD COLUMN points_reached REAL;

ALTER TABLE trip_waypoint_link ADD COLUMN proposed       INTEGER NOT NULL DEFAULT 0;
ALTER TABLE trip_waypoint_link ADD COLUMN plan_covered   REAL;
ALTER TABLE trip_waypoint_link ADD COLUMN trip_on_plan   REAL;
ALTER TABLE trip_waypoint_link ADD COLUMN points_reached REAL;
