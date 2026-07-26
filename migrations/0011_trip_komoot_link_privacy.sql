-- US-35 (ADR-0021): mirror Komoot's tour `status` (privacy) onto the link row.
-- NULL means "not known yet" — a link row created before this column existed,
-- or one whose tour hasn't been seen in a listing since. The pull phase fills
-- it in from the tour listing it already fetches.

ALTER TABLE trip_komoot_link ADD COLUMN privacy_status TEXT;
