# ADR-0004 — Import via native Axum multipart handler, not a Leptos server function

## Status

Accepted — amended once:
- 2026-09-01 — photos may land after the trip, when the UI reports progress
  ([Amendment](#amendment-2026-09-01--photos-may-land-after-the-trip-when-the-ui-reports-progress)).

## Context

Import is the heaviest operation: a single request carries one GPX file plus N photo files
(potentially large), followed by CPU-intensive work (image decode/resize, EXIF parsing,
haversine over thousands of points). Leptos server functions exist but serialize their
arguments and are awkward for a mixed, large, multi-file payload.

## Decision

Expose a plain **`POST /api/import` Axum route handler** (not a Leptos server function) using
`axum`'s `Multipart` extractor (backed by `multer`) to **stream each field to disk**, so large
photos are never fully buffered in memory. Run CPU-heavy work on `tokio::task::spawn_blocking`
(optionally `rayon` for parallel per-photo processing). The import page is a normal HTML
`<form enctype="multipart/form-data" action="/api/import">`; on success the handler writes all
rows in **one transaction** and redirects to the trip page. The form also carries the chosen
activity type and trip name (US-11/US-12), which are stored on the `trip` row.

Because photos can also be added to a trip **after** the initial import (US-2), a second
endpoint **`POST /api/trips/:id/photos`** reuses the very same multipart-streaming + EXIF +
thumbnail + time-matching pipeline; the only difference is that it time-matches against the
trip's already-stored track instead of one parsed in the same request, and inserts only `photo`
rows. The shared logic lives in one photo-ingestion function used by both entry points.

## Consequences

- Large uploads do not exhaust RAM; the async runtime is not blocked by CPU work.
- The import endpoint is a clean HTTP contract reusable by future clients
  ([ADR-0008](./0008-json-first-api.md)) and by a future Garmin ingestion source.
- One photo-ingestion path serves both initial import and later additions, so EXIF/thumbnail/
  time-match behavior cannot drift between the two.
- Upload size is capped with `tower-http`'s `RequestBodyLimitLayer`.
- Slightly more manual wiring than a server function, but far more robust for this payload shape.

## Amendment (2026-09-01) — photos may land after the trip, when the UI reports progress

US-43 rebuilt the import screen in the SPA and US-12 made it two-step. A single multipart POST
issued from WASM reports no upload progress at all — the browser's `fetch` exposes none, and
`reqwest` cannot invent it — so the operation that most needs something to watch, a large
multi-photo import, gave the owner a frozen button and nothing else.

The import screen therefore creates the trip first and uploads the photos in batches to
`POST /api/trips/:id/photos` — the endpoint this ADR already defines for later additions — so it
can report "6 of 12". The owner's experience of choosing photos is unchanged: one file dialog,
multi-select, chosen once.

**What still holds.** Multipart streaming rather than buffering; CPU work off the async runtime;
the body cap; and — unchanged and now load-bearing — *one* photo-ingestion function behind both
entry points, so EXIF, thumbnailing and time-matching cannot drift between them. `POST /api/import`
still accepts photos and still commits them with the trip, for the one-shot clients this ADR
anticipated (the test helpers today, Garmin ingestion later, US-18).

**What no longer holds.** "A failed import leaves no partial trip" now means exactly that: no
partial *trip*. The trip and its track still commit in one transaction — at the confirm step of
US-12's two-phase flow — but a failed photo batch leaves a complete trip carrying fewer photos than
the owner chose. That is reported as what it is, not as a failure, with the rest addable from the
trip page (US-2).

The trade was accepted because the two outcomes are not comparable in cost. A photo that did not
upload is repaired in one click on a screen that already exists; a half-written trip is not
repairable by hand at all. The transactional guarantee is kept where it protects something
irreparable, and spent where it does not.

**Consequences.** The import screen must distinguish partial success from failure in its own words,
and a trip can briefly exist with none of its photos — a state US-2's "photos can be added at a
later time" already permits.
