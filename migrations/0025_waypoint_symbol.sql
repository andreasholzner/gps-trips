-- US-95: a waypoint's GPX `<sym>`, as its file names it (`Summit`,
-- `Triangle, Blue`). The map decides which icon it gets; NULL — no symbol,
-- or a waypoint imported before this — keeps US-86's marker.

ALTER TABLE waypoint ADD COLUMN symbol TEXT;
