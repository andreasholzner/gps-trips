-- US-33: tag trips for organization. `tag` rows are created on-demand and
-- kept even when unused (no owner-facing "delete tag" action yet) so a tag
-- stays suggestible after its last trip is untagged. `trip_tag` is the join
-- table; a trip/tag pair can only appear once.
CREATE TABLE IF NOT EXISTS tag (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT    NOT NULL UNIQUE  -- normalized: trimmed, lowercased, no whitespace
);

CREATE TABLE IF NOT EXISTS trip_tag (
    trip_id INTEGER NOT NULL REFERENCES trip(id) ON DELETE CASCADE,
    tag_id  INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
    PRIMARY KEY (trip_id, tag_id)
);

-- Trips are looked up by tag_id when listing which trips carry a given tag
-- (US-34); trip_id is already covered by the PK's leading column.
CREATE INDEX IF NOT EXISTS idx_trip_tag_tag_id ON trip_tag(tag_id);
