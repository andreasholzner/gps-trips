-- US-93: tags on collections and their tracks, through links of their own
-- beside `trip_tag` (ADR-0028), so nothing that reads a trip's tags sees
-- them. Deleting a collection, a track or a tag takes its links with it.

CREATE TABLE IF NOT EXISTS collection_tag (
    collection_id INTEGER NOT NULL REFERENCES collection(id) ON DELETE CASCADE,
    tag_id        INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_collection_tag_tag_id ON collection_tag(tag_id);

CREATE TABLE IF NOT EXISTS collection_track_tag (
    collection_track_id INTEGER NOT NULL REFERENCES collection_track(id) ON DELETE CASCADE,
    tag_id              INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
    PRIMARY KEY (collection_track_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_collection_track_tag_tag_id ON collection_track_tag(tag_id);
