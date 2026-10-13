-- US-94: a share may name collections, or collection tracks, instead of
-- trips or tags — through links of their own beside `share_trip` (ADR-0028).
-- Deleting a collection or a track takes it out of every share; a share left
-- naming nothing answers like one that does not exist.
CREATE TABLE IF NOT EXISTS share_collection (
    share_id      INTEGER NOT NULL REFERENCES share(id)      ON DELETE CASCADE,
    collection_id INTEGER NOT NULL REFERENCES collection(id) ON DELETE CASCADE,
    PRIMARY KEY (share_id, collection_id)
);

CREATE INDEX IF NOT EXISTS idx_share_collection_collection_id
    ON share_collection(collection_id);

CREATE TABLE IF NOT EXISTS share_collection_track (
    share_id            INTEGER NOT NULL REFERENCES share(id)            ON DELETE CASCADE,
    collection_track_id INTEGER NOT NULL REFERENCES collection_track(id) ON DELETE CASCADE,
    PRIMARY KEY (share_id, collection_track_id)
);

CREATE INDEX IF NOT EXISTS idx_share_collection_track_track_id
    ON share_collection_track(collection_track_id);

-- Every collection track a share reaches: the ones it names, and every track
-- of the collections it names, whenever it is asked — so a track added later
-- is reached and a deleted one is not. The one place that answers this, as
-- `share_reach` is for trips.
CREATE VIEW IF NOT EXISTS share_track_reach AS
    SELECT share_id, collection_track_id FROM share_collection_track
    UNION
    SELECT sc.share_id, k.id
    FROM share_collection sc
    JOIN collection_track k ON k.collection_id = sc.collection_id;
