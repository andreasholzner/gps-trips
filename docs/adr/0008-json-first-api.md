# ADR-0008 — JSON-first API

## Status

Accepted

## Context

A future Android client — a PWA first, possibly a native app later
([US-16](../requirements.md)) — needs a stable contract to read and write trips. If the UI
relied solely on Leptos server functions, a separate client would have no clean API to consume.

## Decision

Implement reads and writes as **plain Axum JSON handlers**. The Leptos web UI consumes these
**same endpoints**, and the shared serde structs in `models.rs` are the API contract. The v1
surface:

- `GET /api/trips` — list, with filter query parameters: `activity`, `from`/`to` (date interval),
  `min_dist`/`max_dist`, free-text `q` (name), and a geographic region `bbox=minLon,minLat,maxLon,maxLat`
  (US-13/US-14; see [ADR-0011](./0011-filtering-search-geo-queries.md)). Returns lightweight rows
  only (no track geometry).
- `GET /api/trips/:id` — trip detail metadata.
- `GET /api/trips/:id/track.geojson` — the track GeoJSON blob.
- `GET /api/trips/:id/gpx` — download the original uploaded GPX byte-for-byte (US-21).
  Not JSON — serves the stored file with `Content-Type: application/gpx+xml` and an RFC 6266
  `Content-Disposition` attachment filename.
- `POST /api/import` — create a trip from GPX (+ optional photos), activity type and name included.
- `POST /api/trips/:id/photos` — add photos to an existing trip (US-2).
- `PATCH /api/trips/:id` — edit trip name and activity type (US-15).
- `DELETE /api/trips/:id` — delete a trip and its blobs.

## Consequences

- A future native or PWA client is purely additive — it reuses the existing endpoints.
- Minor duplication compared to using Leptos server functions everywhere, accepted for the
  decoupling benefit.
- The contract is centralized in `models.rs`, keeping server and any client in sync.
