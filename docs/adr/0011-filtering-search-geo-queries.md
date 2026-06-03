# ADR-0011 — Filtering, search & geographic queries on SQLite (no PostGIS)

## Status

Accepted

## Context

The requirements added trip-list filtering and search ([US-13](../requirements.md)) by activity
type, date interval, distance, and free-text name; plus a **geographic-region filter**
([US-14](../requirements.md)) where the owner draws an area on a map and sees only trips in that
area. A geographic query naturally suggests a spatial database (PostgreSQL + PostGIS), which
would conflict with the SQLite-only, single-file, self-hosted choice
([ADR-0002](./0002-sqlite-local-disk.md)). The data set is small (one user's trips — hundreds),
and the track geometry lives in a blob that is never queried internally
([ADR-0003](./0003-track-as-geojson-blob-in-sqlite.md)).

## Decision

Implement all filtering as **SQL `WHERE` clauses over indexed `trip` columns** — no PostGIS:

- **Activity type** — equality on `trip.activity_type`.
- **Date interval** — range on `trip.start_time`.
- **Distance** — range on `trip.distance_m`.
- **Free-text name** — `name LIKE '%q%'` for v1 (upgrade to SQLite FTS5 only if needed).
- **Geographic region** — a **bounding-box overlap** test between the region rectangle the owner
  draws on the map and each trip's stored bbox columns (`min_lat`/`min_lon`/`max_lat`/`max_lon`):
  two trips' boxes overlap iff they overlap on both axes. The region selection is a rectangle
  (lon/lat min/max), not a free polygon, in v1.

Add supporting indexes on `trip(activity_type)`, `trip(start_time)`, `trip(distance_m)`, and the
bbox columns. If bbox filtering ever needs acceleration, adopt SQLite's built-in **R\*Tree**
module rather than introducing a new database.

## Consequences

- Stays single-file SQLite; no spatial DB, no extra service — preserves
  [ADR-0002](./0002-sqlite-local-disk.md) and the self-hosted/portability goals.
- Filtering scans only lightweight `trip` rows (the geometry blob is untouched), which is exactly
  why the blob is in a separate table ([ADR-0003](./0003-track-as-geojson-blob-in-sqlite.md)).
- The bbox region filter is **coarse**: a trip whose bounding box overlaps the region but whose
  actual track never enters it is a false positive. Accepted for v1; can be refined later by
  point-in-rectangle testing the LineString server-side for the surviving candidates.
- Rectangle-only region selection in v1; arbitrary polygons are a future enhancement.
- Filters compose as a single parameterized query, exposed via `GET /api/trips` query parameters
  ([ADR-0008](./0008-json-first-api.md)).
