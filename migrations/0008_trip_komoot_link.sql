-- US-22/US-20/US-24 (ADR-0021): dedup + sync-state for Komoot-sourced trips.
-- A row's existence is "this trip is Komoot-sourced (or was, and its
-- Komoot-side deletion is still pending)". `trip` itself stays untouched.

CREATE TABLE IF NOT EXISTS trip_komoot_link (
    trip_id        INTEGER REFERENCES trip(id) ON DELETE SET NULL,
    komoot_tour_id TEXT    NOT NULL UNIQUE,
    edit_pending   INTEGER NOT NULL DEFAULT 0,
    delete_pending INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_trip_komoot_link_trip_id ON trip_komoot_link(trip_id);
