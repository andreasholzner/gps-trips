-- US-82: a share may name tags instead of trips — their summary. It reaches
-- the recorded trips under them whenever its link is opened, so nothing is
-- stored per trip. `position` keeps the order the owner chose the tags in,
-- which decides each tag's color. Deleting a tag takes it out of every share
-- (US-83); a share left naming nothing answers like one that does not exist.
CREATE TABLE IF NOT EXISTS share_tag (
    share_id INTEGER NOT NULL REFERENCES share(id) ON DELETE CASCADE,
    tag_id   INTEGER NOT NULL REFERENCES tag(id)   ON DELETE CASCADE,
    position INTEGER NOT NULL,
    PRIMARY KEY (share_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_share_tag_tag_id ON share_tag(tag_id);

-- Every trip a share reaches, whichever kind it is: the one place that
-- answers this, so every check the recipient's routes make agrees. Planned
-- trips are reached only by being named.
CREATE VIEW IF NOT EXISTS share_reach AS
    SELECT share_id, trip_id FROM share_trip
    UNION
    SELECT sg.share_id, tt.trip_id
    FROM share_tag sg
    JOIN trip_tag tt ON tt.tag_id = sg.tag_id
    JOIN trip t ON t.id = tt.trip_id
    WHERE t.trip_kind = 'recorded';
