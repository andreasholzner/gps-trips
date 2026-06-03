# ADR-0004 — Import via native Axum multipart handler, not a Leptos server function

## Status

Accepted

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
