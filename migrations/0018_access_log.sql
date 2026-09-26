-- US-70: how the archive is used — one row per request, except the bundle's
-- content-hashed files. Kept indefinitely; US-71 bounds it if that is ever
-- needed.
--
-- Deliberately without the caller's IP address: the backup (US-40) and the
-- volume snapshots carry this table, and a recipient's address has no
-- business outliving the short-lived platform log that shows it.

-- Each user agent once: a phone's makes every request of a visit.
CREATE TABLE IF NOT EXISTS user_agent (
    id    INTEGER PRIMARY KEY,
    value TEXT    NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS access_log (
    id            INTEGER PRIMARY KEY,
    -- RFC-3339 UTC (ADR-0009): when the request arrived. Rows are ordered by
    -- id, which follows arrival, rather than by this text.
    at            TEXT    NOT NULL,
    method        TEXT    NOT NULL,
    -- A share's token is blanked out before it gets here.
    path          TEXT    NOT NULL,
    status        INTEGER NOT NULL,
    duration_ms   INTEGER NOT NULL,
    -- owner | anonymous | share | unknown_link
    caller        TEXT    NOT NULL,
    -- No foreign key: a stopped share's row goes (US-69), its records stay.
    -- Share ids are never reused (`share.id` is AUTOINCREMENT), so an old
    -- id cannot come to name a new share.
    share_id      INTEGER,
    -- Copied, not joined: once the share is stopped this is the only place
    -- its label survives.
    share_label   TEXT,
    user_agent_id INTEGER REFERENCES user_agent(id)
);

-- The Shares screen's question: how often, and when last, per share.
CREATE INDEX IF NOT EXISTS idx_access_log_share_id ON access_log(share_id, id);
