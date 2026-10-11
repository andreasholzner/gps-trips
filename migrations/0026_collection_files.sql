-- US-88: every file a collection was imported or added from, kept as it was
-- uploaded and under its own name, in the order the files came. Replaces
-- `collection.gpx`, which held the first file only; a collection imported
-- before this keeps that one, named after the collection, since its file's
-- own name was not stored.
--
-- SQLite's DROP COLUMN cannot parse back migration 0024's definition of
-- `collection` — the comment after its last column but one — so that
-- definition is first stored again without comments, the same columns and
-- constraints. That leaves the stored rows as they are, the case SQLite's
-- documentation gives `writable_schema` for ("Making Other Kinds Of Table
-- Schema Changes"); rebuilding the table instead would need foreign keys
-- off, or dropping it would delete every track and waypoint, and sqlx runs
-- each migration in a transaction, inside which that pragma does nothing.

CREATE TABLE IF NOT EXISTS collection_file (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    collection_id INTEGER NOT NULL REFERENCES collection(id) ON DELETE CASCADE,
    position      INTEGER NOT NULL,
    file_name     TEXT    NOT NULL,
    gpx           BLOB    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_collection_file_collection
    ON collection_file(collection_id, position);

INSERT INTO collection_file (collection_id, position, file_name, gpx)
    SELECT id, 1, name || '.gpx', gpx FROM collection;

PRAGMA writable_schema = ON;

UPDATE sqlite_schema
    SET sql = 'CREATE TABLE collection (id INTEGER PRIMARY KEY AUTOINCREMENT, '
           || 'name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '''', '
           || 'activity_type TEXT, created_at TEXT NOT NULL, gpx BLOB NOT NULL)'
    WHERE type = 'table' AND name = 'collection';

PRAGMA writable_schema = RESET;

ALTER TABLE collection DROP COLUMN gpx;

-- US-88: files waiting for the owner's review before they are added to a
-- collection — or make a new one of several files. Parked rather than sent
-- again with every change of a decision, as US-12's trip import parks its
-- file (migration 0014, ADR-0004), and swept after the same
-- `config::server::STAGED_IMPORT_TTL`. AUTOINCREMENT, so a handle once spent
-- never names another upload (migration 0017).

CREATE TABLE IF NOT EXISTS collection_import_staging (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at    TEXT    NOT NULL,             -- RFC-3339 UTC (ADR-0009)
    -- The collection the files are added to; NULL for a new one. Deleting
    -- the collection drops what was waiting to be added to it.
    collection_id INTEGER REFERENCES collection(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS collection_import_staging_file (
    staging_id INTEGER NOT NULL REFERENCES collection_import_staging(id) ON DELETE CASCADE,
    -- 0-based, in the order the files are taken: by their names.
    position   INTEGER NOT NULL,
    file_name  TEXT    NOT NULL,
    gpx        BLOB    NOT NULL,
    PRIMARY KEY (staging_id, position)
);
