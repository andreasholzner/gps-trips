# ADR-0002 — SQLite (sqlx), local disk only

## Status

Accepted

## Context

Single user, self-hosted. The data set is small (one person's trips). A future extension may
move bulk blobs (photos) to a private ownCloud instance — see
[ADR-0007](./0007-blobstore-abstraction.md) — which raises the question of whether the database
could also live on network storage.

The requirements now include filtering and searching the trip list by attributes (activity
type, date interval, distance, free-text name) and by **geographic region** (US-13/US-14),
which raises whether a spatial database (e.g. PostgreSQL + PostGIS) is needed instead of SQLite.

## Decision

Use **embedded SQLite via `sqlx`** (compile-time-checked queries with a committed `.sqlx`
offline cache; migrations via `sqlx migrate`). The database file **always lives on local disk**
— never on a network/WebDAV mount. Apply connect-time pragmas: `foreign_keys = ON`,
`journal_mode = WAL`, and a `busy_timeout`.

## Consequences

- Zero database service to operate; backup is copying a single file.
- Compile-time query checking catches schema drift.
- Network filesystems are explicitly excluded for the DB, because SQLite relies on real POSIX
  file locking and can corrupt over network mounts.
- Because tracks are stored in the DB ([ADR-0003](./0003-track-as-geojson-blob-in-sqlite.md)),
  they remain local even after photos move to ownCloud.
- Filtering/search (incl. the coarse geographic-region filter) stays well within SQLite's
  capabilities at this scale — indexed `trip` columns plus bounding-box comparison, no PostGIS.
  See [ADR-0011](./0011-filtering-search-geo-queries.md).
