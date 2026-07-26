-- US-35 (ADR-0021): at most one link row per trip, enforced by the schema.
-- The trip queries LEFT JOIN this table to surface a trip's Komoot privacy, so
-- a second row for the same trip would silently duplicate that trip on the
-- list page. Only `sync_one_tour` ever inserts links (one per freshly created
-- trip), so this codifies an invariant that already holds rather than changing
-- behaviour.
--
-- Replaces migration 0008's non-unique index on the same column: a UNIQUE
-- index serves those lookups just as well. NULLs stay distinct under SQLite's
-- UNIQUE semantics, so the orphaned `trip_id IS NULL` rows left behind by
-- delete-pending tours (US-24) can still coexist.

DROP INDEX IF EXISTS idx_trip_komoot_link_trip_id;

CREATE UNIQUE INDEX IF NOT EXISTS idx_trip_komoot_link_trip_id
    ON trip_komoot_link(trip_id);
