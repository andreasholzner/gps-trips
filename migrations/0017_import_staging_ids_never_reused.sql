-- US-12: a staging id, once spent, never names another parse. Without
-- AUTOINCREMENT SQLite hands out the highest id plus one, so a parse swept,
-- taken or cancelled at the top of the table gave its id to the next file
-- parked — and a tab still holding the old handle would confirm that file
-- under its own name. The sweeper runs just before every insert, which made
-- this the usual outcome of coming back to an import after a day, not a
-- rare one.
--
-- SQLite cannot add AUTOINCREMENT to an existing table, so it is rebuilt;
-- the parses parked at the moment of the upgrade are kept.

CREATE TABLE import_staging_new (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT    NOT NULL,
    derived    TEXT    NOT NULL,
    geojson    TEXT    NOT NULL,
    gpx        BLOB    NOT NULL
);

INSERT INTO import_staging_new (id, created_at, derived, geojson, gpx)
    SELECT id, created_at, derived, geojson, gpx FROM import_staging;

DROP TABLE import_staging;

ALTER TABLE import_staging_new RENAME TO import_staging;
