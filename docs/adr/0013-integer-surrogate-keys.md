# ADR-0013 — Integer surrogate primary keys (revisit only for offline-first replication)

## Status

Accepted

## Context

Every table needs a primary key. The two candidates are an autoincrement
`INTEGER PRIMARY KEY` (a surrogate `rowid` key) or a UUID (`TEXT`/`BLOB`). The
generic "prefer UUIDs" advice targets distributed, multi-writer systems; this
project is single-user and self-hosted on SQLite ([ADR-0002](./0002-sqlite-local-disk.md)),
with ids appearing in URLs (`/trips/:id`) and a JSON-first API
([ADR-0008](./0008-json-first-api.md)).

The decisive factor is **not** "single vs multi user", nor even "single vs
multiple devices" — it is **how many places independently create records and
later merge them** (i.e. how many writers/databases exist):

- **Centralized** — multiple devices (phone, PC) are thin clients of the *one*
  self-hosted server with its *one* SQLite database. Still a single writer.
- **Replicated / offline-first** — each device holds its own copy that is edited
  offline and later synced/merged. Multiple writers.

For this app, trips are created by **server-side GPX import** and recording stays
in komoot, so the centralized model is the natural fit and covers "use it from my
phone and my PC" fully. There is no current requirement for offline creation/merge.

## Decision

Use an autoincrement **`INTEGER PRIMARY KEY`** (SQLite `rowid` alias) as the
primary key for `trip` (and future tables).

**Revisit this decision only if we adopt offline-first replication** — devices
holding local copies that are edited offline and merged. Multi-device access via
one central server does **not** count as a trigger.

If that trigger occurs, the preferred path is **not** to change the primary key
but to **keep the integer key internal and add a separate global identifier**
(UUIDv7 or an opaque slug) for sync/merge and, if needed, non-guessable URLs.

## Consequences

- Fastest, smallest key in SQLite (clustered on `rowid`, no extra index);
  human-friendly URLs and logs; no dependency or id-generation code.
- Ids are **enumerable** and leak creation order/count. Acceptable because the
  instance is single-user and can be gated by the optional shared password
  ([ADR-0010](./0010-single-user-optional-auth.md)). Note that exposing the
  server to the network for multi-device access raises the priority of enabling
  that auth — an auth concern, not a key-design one.
- Ids are DB-assigned at insert (no client-side/offline generation) and would
  **collide if two databases were merged** — exactly the situation the "revisit"
  trigger guards against.
- The dual-key escape hatch (internal integer + external UUIDv7/slug) keeps the
  door open without paying the cost now (YAGNI).
