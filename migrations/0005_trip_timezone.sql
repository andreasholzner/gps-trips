-- US-4: the trip's IANA timezone (e.g. "Europe/Oslo"), used to resolve a photo's
-- EXIF DateTimeOriginal (which has no embedded offset) to UTC for timestamp-based
-- map placement (ADR-0009, ADR-0019). Nullable: existing trips predate this
-- migration and get it lazily backfilled the first time photos are added to
-- them; every newly-imported trip always gets a concrete value (auto-guessed
-- from the track's start coordinate, or an explicit owner override).
ALTER TABLE trip ADD COLUMN tz_name TEXT;
