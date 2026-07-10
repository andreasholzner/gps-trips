# ADR-0021 — Reverse-engineered Komoot client for automated import & edit sync

## Status

Accepted

## Context

Per the "Determine import pipeline" section of [`requirements.md`](../requirements.md): all future
recorded trips (Garmin GPS unit *and* Komoot mobile app) must be ingested without a manual GPX
export/upload step, edits (name/activity_type) must sync back to Komoot, and historical trips
already in Komoot must be bulk-importable (incl. photos). Since Garmin-recorded trips already flow
into Komoot via its own Garmin Connect integration, and app-recorded trips are native to Komoot,
**Komoot is the single upstream source** that covers both ingestion paths — no separate Garmin
Connect integration is needed for this (that stays deferred to US-18 as a possible alternate
source). Komoot has no official API suited to this; the only option is an **unofficial/
reverse-engineered API**, which is fragile and carries ToS risk the owner accepts for personal,
single-user use. Endpoint and auth details are in [`docs/komoot-api.md`](../komoot-api.md), kept
separately since they're protocol reference, not architecture.

[ADR-0014](./0014-defer-deployment-topology.md) commits the app to on-demand/local-first, no
background daemon — so ingestion must be a **pull triggered by an explicit action**, not a poller.

## Decision

- Introduce a **`KomootClient` trait** (`src/server/komoot.rs`), the same seam pattern as
  `BlobStore`/`Clock` ([ADR-0007](./0007-blobstore-abstraction.md),
  [ADR-0012](./0012-tdd-test-strategy.md)), wrapping the unofficial HTTP calls (list tours, fetch
  GPX + metadata, fetch photos, update tour name/sport). Ship one real implementation; mock it in
  tests.
- **Auth** via `KOMOOT_EMAIL` / `KOMOOT_PASSWORD` env vars, read at startup, consistent with the
  existing `TRIP_ARCHIVE_DATA_DIR` config pattern. v1 uses **HTTP Basic Auth per request** (see
  `docs/komoot-api.md`) rather than a reused session — simpler, no session lifecycle to manage.
  `KomootClient` still logs in once per sync invocation — a "Sync now" run or a `komoot_backfill`
  run — to resolve the Komoot username and validate credentials up front, failing fast before any
  sync work starts. All auth attachment is routed through one internal seam inside `KomootClient`
  (not threaded through each call site), so switching to Komoot's cookie-based session auth later,
  if Basic Auth ever stops working, doesn't require touching every method.
- **Rate limiting** lives inside `KomootClient` itself (min delay / backoff between requests) so
  every call site — the small routine sync and the large historical backfill — gets it
  automatically.
- **Activity type mapping** is a hard-coded, exhaustive `match` between `ActivityType`
  ([ADR-0018](./0018-enums-for-closed-string-sets.md)) and Komoot's sport strings, in each
  direction. Push always has a defined outgoing string (the match is exhaustive over the closed
  enum). Pull maps an unrecognized/unmapped Komoot sport to `ActivityType::Unknown` rather than
  rejecting the tour.
- Schema: add a new **`trip_komoot_link`** table — `trip_id` (FK → `trip`, nullable,
  `ON DELETE SET NULL`), `komoot_tour_id` (unique, not null), `edit_pending` (bool, default
  false), `delete_pending` (bool, default false). A row's existence *is* "this trip is
  Komoot-sourced (or was, and its Komoot-side deletion is still pending)"; `trip` itself stays
  untouched ([ADR-0015](./0015-db-model-response-type-separation.md)).
  - Pull dedup: anti-join `trip_komoot_link.komoot_tour_id` against the Komoot tour list.
  - Pull insert: the link row is inserted in the **same transaction** as the imported
    trip/track/photos — never as a separate step — so a crash mid-pull cannot leave an imported
    trip without its link row (which would re-import as a duplicate on the next pull) or vice
    versa.
  - Edit: `repo::update_trip` also sets `edit_pending = true` on the trip's link row, if one
    exists, in the same statement/transaction as the name/activity_type update — so an edit to a
    Komoot-sourced trip is never silently lost.
  - Delete (US-9): deleting a Komoot-linked trip does **not** drop its link row. The link row's
    `delete_pending` is set `true` (in the same transaction as the trip delete); the FK's
    `ON DELETE SET NULL` then nulls `trip_id` when the `trip` row disappears, leaving an orphaned,
    delete-pending link row behind as the record of "still needs deleting on Komoot". Only once a
    later sync's push phase successfully calls Komoot to delete the tour does the link row itself
    get deleted — at which point the tour is gone from both sides and cannot resurrect the trip on
    a subsequent pull.
  - Push phase: `SELECT ... FROM trip_komoot_link WHERE edit_pending OR delete_pending`.
    `delete_pending` rows call Komoot's delete-tour API, then delete the link row on success.
    `edit_pending` rows (joined to `trip` for current name/activity_type) call Komoot's
    update-tour API, then clear the flag on success.
  - Also gives future alternate sources (e.g. a direct Garmin Connect import, US-18) the same
    pattern — their own link table — without ever touching `trip`.
- **"Sync now"** — a new action, invoked explicitly by the owner (no auto-run on startup). When
  triggered: (1) *push* — for every `trip_komoot_link` row with `delete_pending` or
  `edit_pending` set, call Komoot accordingly (see schema bullet above); (2) *pull* — list Komoot
  tours, and for any tour ID not yet in `trip_komoot_link`, download its GPX and run it through
  the **existing** import pipeline ([ADR-0004](./0004-import-via-axum-multipart.md)) unchanged.
  - **Halt-on-first-failure**: the first failed Komoot call in either phase stops the sync
    immediately (no further push/pull items are attempted) and surfaces a visible error naming the
    trip/tour that failed — deliberately not "best effort" here, to avoid hammering Komoot with a
    run of requests that are likely all failing for the same reason (e.g. an expired session).
  - **Concurrency guard**: only one sync runs at a time — a single in-process flag in `AppState`,
    since this is a single-process, single-user app (ADR-0010/0014). While a
    sync is in flight, edit (`PATCH /api/trips/:id`) and delete requests are rejected (409) rather
    than racing the push phase's read of `edit_pending`/`delete_pending`.
- **Historical bulk import** is a **separate one-off CLI binary** (`src/bin/komoot_backfill.rs`),
  reusing the same lib crate (`KomootClient`, import pipeline, repos) but run manually from the
  terminal, not exposed in the web UI. Rationale: rate-limited fetching of potentially hundreds of
  tours doesn't belong inside a synchronous HTTP request/response cycle. It's idempotent (dedup via
  `komoot_tour_id`), so a rerun after interruption is safe. Includes photos, per the requirement.
  Expected to be run once, to seed `trip_komoot_link` from the owner's existing Komoot history;
  ongoing ingestion afterward happens via "Sync now". Rate limiting is per-process (inside
  `KomootClient`), so this and a concurrent "Sync now" would not share a rate-limit budget — an
  accepted risk given the single-actor, run-once nature of the backfill.
- **Integration check** — a third, minimal CLI binary (`src/bin/komoot_check.rs`) sharing only the
  `KomootClient` trait and its real implementation: no DB, no `BlobStore`, no import pipeline. It
  logs in and makes one cheap call (e.g. list tours) to confirm the reverse-engineered API still
  works, independently of the full app or a real sync — a fast way to notice Komoot has changed
  something before it surfaces as a confusing failure mid-sync.
- Nice-to-haves (photo upload to Komoot, one-way planned-trip sync) are **not built now** — they're
  additive `KomootClient` methods / a separate read-only pull, left for later.

## Consequences

- Reuses the existing import pipeline and trait-seam testing approach; no HTTP-handler duplication
  between the button and the CLI tool.
- The unofficial API can break silently on Komoot's end with no warning — sync failures must
  surface to the owner, not fail silently or corrupt trip state.
- Halting on first failure means one persistently-failing item (e.g. a tour Komoot rejects) blocks
  all later items until it's resolved — a deliberate ordering/simplicity trade-off over a
  "collect all failures" batch design.
- A deleted-but-not-yet-Komoot-deleted trip leaves a lingering, trip-less `trip_komoot_link` row
  until the next successful sync; the tour still exists on Komoot in the meantime.
- Editing or deleting a trip is blocked for the (short) duration of an in-flight sync.
- Plaintext Komoot credentials sit in local env vars, same trust boundary as the rest of the
  local-only deployment ([ADR-0014](./0014-defer-deployment-topology.md),
  [ADR-0010](./0010-single-user-optional-auth.md)) — not a new class of exposure.
- Bulk historical import requires terminal access; acceptable since the owner is the sole,
  technical user (US-10).
- One extra table + join for sync-state queries (negligible at personal scale,
  [ADR-0011](./0011-filtering-search-geo-queries.md)'s territory), in exchange for zero
  Komoot-specific surface on the core `trip` model.
