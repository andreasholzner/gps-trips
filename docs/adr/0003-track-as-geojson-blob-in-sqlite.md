# ADR-0003 — Track geometry as a GeoJSON blob in SQLite (separate `track` table)

## Status

Accepted. *Supersedes an earlier draft that stored track GeoJSON as on-disk files.*

## Context

A track is a polyline of hundreds to tens of thousands of points (lat, lon, elevation, time).
The geometry *blob* itself is never queried relationally — there is no in-track spatial search,
because discovery stays in komoot. (Trip-level filtering, including the geographic-region filter
in US-14, operates on lightweight bounding-box columns on `trip`, **not** by querying inside the
blob — see [ADR-0011](./0011-filtering-search-geo-queries.md).) The browser needs the geometry as
GeoJSON to render it with Leaflet and to draw the elevation profile. The owner prefers tracks
stored **in the database** alongside the rest of the trip data (rather than as separate files on
disk).

Options considered:
- **Per-point rows** in a `track_point` table — bloats the DB, complicates the schema, gains
  nothing without relational queries.
- **On-disk GeoJSON files** — lean DB, but introduces file/DB consistency and orphan-cleanup concerns.
- **GeoJSON blob in the DB** — one transactional unit, single-file backup.

## Decision

Store **one GeoJSON blob per trip** (a LineString with per-coordinate elevation plus
distance/time arrays in `properties`) as a `TEXT` column in a dedicated **`track` table**, 1:1
with `trip` (`track.trip_id PRIMARY KEY REFERENCES trip(id) ON DELETE CASCADE`). The blob is kept
in its **own table** so the trip-list query (`SELECT … FROM trip`) never loads it; `trip` holds
only summary stats. The trip-detail endpoint reads the blob and serves it raw to the client,
which feeds it to both Leaflet and the elevation chart.

The **original uploaded GPX file** is retained in the same `track` table as a `BLOB` column
(`gpx`), for byte-for-byte download (US-21). It is track data — the source the geometry was
derived from — so it belongs with the track and stays in the DB (local, transactional), **not**
in the photo `BlobStore`/ownCloud path ([ADR-0007](./0007-blobstore-abstraction.md)). It is heavy
and likewise never loaded by list queries.

## Consequences

- Each trip is one atomic, transactional unit; backup is a single DB file.
- No orphaned track files; deletion is a cascade.
- SQLite comfortably handles blobs of this size (tens of KB to a few MB).
- Tracks ride with the DB, so they stay local ([ADR-0002](./0002-sqlite-local-disk.md)) even
  after photos move to ownCloud ([ADR-0007](./0007-blobstore-abstraction.md)).
- Editing a track means rewriting the blob (acceptable — tracks are imported, rarely edited).
- Keeping the blob out of `trip` is what lets the filter/search queries (US-13/US-14) scan only
  lightweight rows; this reinforces, rather than conflicts with, the new filtering requirements.
- The original GPX (US-21) cascades away with the trip too, so deleting a trip leaves nothing behind.
