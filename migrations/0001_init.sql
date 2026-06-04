-- Trip Archive — initial schema
-- ADR-0002 (SQLite), ADR-0003 (track geometry as a GeoJSON blob)

CREATE TABLE IF NOT EXISTS trip (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT    NOT NULL,
    activity_type TEXT    NOT NULL,
    start_time    TEXT,                       -- RFC-3339 UTC, nullable (GPX may lack times)
    end_time      TEXT,
    duration_secs INTEGER,
    distance_m    REAL    NOT NULL,
    ascent_m      REAL,
    descent_m     REAL,
    min_lat       REAL,
    min_lon       REAL,
    max_lat       REAL,
    max_lon       REAL,
    created_at    TEXT    NOT NULL
);

-- 1:1 with trip — kept in its own table so list queries never load the blob
CREATE TABLE IF NOT EXISTS track (
    trip_id INTEGER PRIMARY KEY REFERENCES trip(id) ON DELETE CASCADE,
    geojson TEXT NOT NULL   -- GeoJSON Feature: LineString + elevation/distance arrays in properties
);
