-- US-12: the two-phase import. Phase one parses the uploaded GPX and parks
-- what it derived here; phase two promotes that row into a real trip once the
-- owner has confirmed the name, activity type, kind and timezone. The point is
-- that the file is parsed once: the archive cannot suggest a `YYYY-mm-dd`
-- prefix before the name is entered without having read the track first, and
-- re-reading it at confirmation time would do the same work twice.
--
-- A separate table rather than a `draft` flag on `trip`, deliberately. A flag
-- would have to be filtered out of every query that reads trips — including
-- the `qmapshack_export` and `komoot_backfill` binaries, which open this
-- database directly (ADR-0022, ADR-0021) — with "an abandoned draft was
-- exported to QMapShack" as the failure nobody would notice. Nothing selects
-- from this table but the import handlers.
--
-- A row here is not a trip and never becomes visible as one: it is deleted in
-- the same transaction that inserts the trip, deleted outright when the owner
-- picks a different file, and swept after `config::server::STAGED_IMPORT_TTL`
-- otherwise. Navigating away does not delete it — nothing reliably runs on
-- the way out of a page — so the sweeper is what bounds this table, not the
-- cancel.

CREATE TABLE IF NOT EXISTS import_staging (
    id         INTEGER PRIMARY KEY,
    -- RFC-3339 UTC (ADR-0009), like every other timestamp here; the sweeper's
    -- only input.
    created_at TEXT    NOT NULL,
    -- The parse's own output as JSON: the track's `<name>`, its computed
    -- stats and the guessed timezone. JSON rather than a column per statistic
    -- so this table does not have to change every time `TrackStats` does —
    -- nothing queries it, it is written once and read once.
    derived    TEXT    NOT NULL,
    -- The track geometry (ADR-0003) and the original upload (US-21), already
    -- in the shape the `track` row wants, so promotion is a copy rather than
    -- a re-derivation.
    geojson    TEXT    NOT NULL,
    gpx        BLOB    NOT NULL
);
