-- US-32: distinguish recorded from planned trips so the list can show them
-- separately. Existing rows have no notion of "planned" yet, so the DEFAULT
-- backfills every pre-existing trip as recorded.
ALTER TABLE trip ADD COLUMN trip_kind TEXT NOT NULL DEFAULT 'recorded';
