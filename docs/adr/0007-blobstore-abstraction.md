# ADR-0007 — `BlobStore` storage abstraction (local now, ownCloud later)

## Status

Accepted

## Context

Photos (originals + generated thumbnails) are bulk binary blobs. The owner may later want them
stored on a **private ownCloud instance** ([US-17](../requirements.md)) rather than local disk,
without rewriting the import pipeline or UI. (Tracks are *not* blobs here — they live in the DB
per [ADR-0003](./0003-track-as-geojson-blob-in-sqlite.md).)

## Decision

Route **all photo file I/O through a `BlobStore` trait** (`src/server/storage.rs`) with methods
roughly `put(key, bytes)`, `get(key)`, and `url_for(key)`. Ship a **`LocalDisk`** implementation
in v1 (files under the data dir, served via `tower-http`'s `ServeDir`). A future
**`OwnCloudWebDav`** implementation swaps in with no changes to the import pipeline or UI; photo
serving then redirects to ownCloud links or proxies through the trait, with thumbnails kept in a
local cache for speed.

## Consequences

- Small upfront indirection in exchange for making ownCloud a backend swap, not a rewrite.
- Network-backed blobs imply a local thumbnail cache and redirect/proxy serving (decode/resize
  over a network mount would be slow).
- The SQLite DB (including tracks) stays local ([ADR-0002](./0002-sqlite-local-disk.md))
  regardless of where photo blobs live.
