# ADR-0021 — Reverse-engineered Komoot client for automated import & edit sync

## Status

Proposed

## Context

Per the "Determine import pipeline" section of [`requirements.md`](../requirements.md): all future
recorded trips (Garmin GPS unit *and* Komoot mobile app) must be ingested without a manual GPX
export/upload step, edits (name/activity_type) must sync back to Komoot, and historical trips
already in Komoot must be bulk-importable (incl. photos). Since Garmin-recorded trips already flow
into Komoot via its own Garmin Connect integration, and app-recorded trips are native to Komoot,
**Komoot is the single upstream source** that covers both ingestion paths — no separate Garmin
Connect integration is needed for this (that stays deferred to US-18 as a possible alternate
source). Komoot has no official API suited to this; the only option is an **unofficial/
reverse-engineered API** (session-based auth), which is fragile and carries ToS risk the owner
accepts for personal, single-user use.

[ADR-0014](./0014-defer-deployment-topology.md) commits the app to on-demand/local-first, no
background daemon — so ingestion must be a **pull triggered by an explicit action**, not a poller.

## Decision

- Introduce a **`KomootClient` trait** (`src/server/komoot.rs`), the same seam pattern as
  `BlobStore`/`Clock` ([ADR-0007](./0007-blobstore-abstraction.md),
  [ADR-0012](./0012-tdd-test-strategy.md)), wrapping the unofficial HTTP calls (list tours, fetch
  GPX + metadata, fetch photos, update tour name/sport). Ship one real implementation; mock it in
  tests.
- **Auth** via `KOMOOT_EMAIL` / `KOMOOT_PASSWORD` env vars, read at startup, consistent with the
  existing `TRIP_ARCHIVE_DATA_DIR` config pattern.
- **Rate limiting** lives inside `KomootClient` itself (min delay / backoff between requests) so
  every call site — the small routine sync and the large historical backfill — gets it
  automatically.
- Schema: add a new **`trip_komoot_link`** table — `trip_id` (FK → `trip`, unique/PK),
  `komoot_tour_id` (unique, not null), `edit_pending` (bool, default false). A row's existence
  *is* "this trip is Komoot-sourced"; `trip` itself stays untouched
  ([ADR-0015](./0015-db-model-response-type-separation.md)).
  - Pull dedup: anti-join `trip_komoot_link.komoot_tour_id` against the Komoot tour list.
  - Push phase: `SELECT ... FROM trip_komoot_link WHERE edit_pending`, joined to `trip` for the
    current name/activity_type.
  - Also gives future alternate sources (e.g. a direct Garmin Connect import, US-18) the same
    pattern — their own link table — without ever touching `trip`.
- **"Sync now"** — a new button/endpoint, invoked explicitly by the owner (no auto-run on
  startup). On click: (1) *push* — for every trip with `edit_pending` in `trip_komoot_link`, call
  Komoot to update name/sport, clear the flag; (2) *pull* — list Komoot tours, and for any tour ID
  not yet in `trip_komoot_link`, download its GPX and run it through the **existing** import
  pipeline ([ADR-0004](./0004-import-via-axum-multipart.md)) unchanged.
- **Historical bulk import** is a **separate one-off CLI binary** (`src/bin/komoot_backfill.rs`),
  reusing the same lib crate (`KomootClient`, import pipeline, repos) but run manually from the
  terminal, not exposed in the web UI. Rationale: rate-limited fetching of potentially hundreds of
  tours doesn't belong inside a synchronous HTTP request/response cycle. It's idempotent (dedup via
  `komoot_tour_id`), so a rerun after interruption is safe. Includes photos, per the requirement.
- Nice-to-haves (photo upload to Komoot, one-way planned-trip sync) are **not built now** — they're
  additive `KomootClient` methods / a separate read-only pull, left for later.

## Consequences

- Reuses the existing import pipeline and trait-seam testing approach; no HTTP-handler duplication
  between the button and the CLI tool.
- The unofficial API can break silently on Komoot's end with no warning — sync failures must
  surface to the owner, not fail silently or corrupt trip state.
- Plaintext Komoot credentials sit in local env vars, same trust boundary as the rest of the
  local-only deployment ([ADR-0014](./0014-defer-deployment-topology.md),
  [ADR-0010](./0010-single-user-optional-auth.md)) — not a new class of exposure.
- Bulk historical import requires terminal access; acceptable since the owner is the sole,
  technical user (US-10).
- One extra table + join for sync-state queries (negligible at personal scale,
  [ADR-0011](./0011-filtering-search-geo-queries.md)'s territory), in exchange for zero
  Komoot-specific surface on the core `trip` model.
