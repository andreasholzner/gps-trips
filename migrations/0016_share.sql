-- US-53: read-only access to a few trips through a link. The token is the
-- whole credential and is kept as-is, so the owner can copy a link again
-- (US-69). `expires_at` is RFC-3339 UTC (ADR-0009), NULL for never; a share
-- ends when its row goes, and with it every `share_trip` row.
CREATE TABLE IF NOT EXISTS share (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    token      TEXT    NOT NULL UNIQUE,
    label      TEXT,
    created_at TEXT    NOT NULL,
    expires_at TEXT
);

-- Deleting a trip takes it out of every share; a share left with no trips
-- answers like one that does not exist.
CREATE TABLE IF NOT EXISTS share_trip (
    share_id INTEGER NOT NULL REFERENCES share(id) ON DELETE CASCADE,
    trip_id  INTEGER NOT NULL REFERENCES trip(id)  ON DELETE CASCADE,
    PRIMARY KEY (share_id, trip_id)
);

-- A trip's deletion cascades by trip_id; share_id is the PK's leading column.
CREATE INDEX IF NOT EXISTS idx_share_trip_trip_id ON share_trip(trip_id);
