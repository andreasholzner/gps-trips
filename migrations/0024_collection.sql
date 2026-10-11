-- US-86: guide-book collections, kept apart from trips (ADR-0028). A
-- collection owns its tracks and waypoints; deleting it deletes them with it.

CREATE TABLE IF NOT EXISTS collection (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT    NOT NULL,
    description   TEXT    NOT NULL DEFAULT '',
    -- NULL for a book with no activity focus: its tracks are `unknown` until
    -- given an activity of their own.
    activity_type TEXT,
    created_at    TEXT    NOT NULL,             -- RFC-3339 UTC (ADR-0009)
    -- The file as it was uploaded, once.
    gpx           BLOB    NOT NULL
);

-- No times, durations or speeds: a guide book's timestamps are not when
-- anyone went. The geometry and the track's own GPX sit on the row itself —
-- nothing lists collection tracks without their collection, and the
-- collection page's track list names its columns.
CREATE TABLE IF NOT EXISTS collection_track (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    collection_id INTEGER NOT NULL REFERENCES collection(id) ON DELETE CASCADE,
    -- 1-based, in the order the file holds the tracks.
    position      INTEGER NOT NULL,
    name          TEXT    NOT NULL,
    -- The track's own activity; NULL takes its collection's.
    activity_type TEXT,
    distance_m    REAL    NOT NULL,
    ascent_m      REAL    NOT NULL,
    descent_m     REAL    NOT NULL,
    min_lat       REAL    NOT NULL,
    min_lon       REAL    NOT NULL,
    max_lat       REAL    NOT NULL,
    max_lon       REAL    NOT NULL,
    geojson       TEXT    NOT NULL,             -- ADR-0003, without times
    gpx           BLOB    NOT NULL              -- this track alone, as GPX
);

CREATE INDEX IF NOT EXISTS idx_collection_track_collection
    ON collection_track(collection_id, position);

-- A waypoint from a file with exactly one track belongs to that track as
-- well; deleting the track leaves it to the collection alone.
CREATE TABLE IF NOT EXISTS waypoint (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    collection_id       INTEGER NOT NULL REFERENCES collection(id) ON DELETE CASCADE,
    collection_track_id INTEGER REFERENCES collection_track(id) ON DELETE SET NULL,
    position            INTEGER NOT NULL,
    name                TEXT,
    description         TEXT,
    lat                 REAL    NOT NULL,
    lon                 REAL    NOT NULL,
    ele                 REAL
);

CREATE INDEX IF NOT EXISTS idx_waypoint_collection ON waypoint(collection_id, position);
CREATE INDEX IF NOT EXISTS idx_waypoint_collection_track ON waypoint(collection_track_id);
