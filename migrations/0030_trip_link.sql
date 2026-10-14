-- US-99: links from a trip to what it followed or was planned from — a
-- recorded trip to a planned trip, and a recorded or planned trip to a
-- collection track or a waypoint. Links of their own beside the tags' and
-- shares' (ADR-0028); deleting either end takes its links with it. Which
-- kind of trip may stand at which end is the application's to check: a
-- trip's kind never changes.

CREATE TABLE IF NOT EXISTS trip_trip_link (
    recorded_trip_id INTEGER NOT NULL REFERENCES trip(id) ON DELETE CASCADE,
    planned_trip_id  INTEGER NOT NULL REFERENCES trip(id) ON DELETE CASCADE,
    PRIMARY KEY (recorded_trip_id, planned_trip_id)
);

CREATE INDEX IF NOT EXISTS idx_trip_trip_link_planned ON trip_trip_link(planned_trip_id);

CREATE TABLE IF NOT EXISTS trip_track_link (
    trip_id             INTEGER NOT NULL REFERENCES trip(id)             ON DELETE CASCADE,
    collection_track_id INTEGER NOT NULL REFERENCES collection_track(id) ON DELETE CASCADE,
    PRIMARY KEY (trip_id, collection_track_id)
);

CREATE INDEX IF NOT EXISTS idx_trip_track_link_track ON trip_track_link(collection_track_id);

CREATE TABLE IF NOT EXISTS trip_waypoint_link (
    trip_id     INTEGER NOT NULL REFERENCES trip(id)     ON DELETE CASCADE,
    waypoint_id INTEGER NOT NULL REFERENCES waypoint(id) ON DELETE CASCADE,
    PRIMARY KEY (trip_id, waypoint_id)
);

CREATE INDEX IF NOT EXISTS idx_trip_waypoint_link_waypoint ON trip_waypoint_link(waypoint_id);
