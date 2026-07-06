-- ADR-0011: filtering/search over indexed trip columns (US-13).
CREATE INDEX IF NOT EXISTS idx_trip_activity_type ON trip(activity_type);
CREATE INDEX IF NOT EXISTS idx_trip_start_time ON trip(start_time);
CREATE INDEX IF NOT EXISTS idx_trip_distance_m ON trip(distance_m);
