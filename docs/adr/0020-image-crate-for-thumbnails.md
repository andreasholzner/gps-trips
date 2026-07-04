# ADR-0020 — Use the `image` crate for thumbnail generation

## Status

Accepted

## Context

[US-5](../requirements.md) requires a generated thumbnail per photo so the gallery and map
popups (which today load the full-size original for every small `<img>`, per
[ADR-0007](./0007-blobstore-abstraction.md)'s original + thumbnail blob design) load fast. This
needs a decode → resize → re-encode pipeline for the photo formats this project already accepts
(`content_type_from_path` in `src/server/http.rs`: JPEG, PNG, GIF, WebP).

## Decision

Depend on **`image`** (`default-features = false, features = ["jpeg", "png", "gif", "webp"]`,
trimming the roughly ten other codecs the crate ships by default that this project never accepts
as an upload). It is the de facto standard pure-Rust image crate, consistent with this project's
preference for pure-Rust dependencies over C/FFI ones ([ADR-0016](./0016-assets-relative-to-executable.md),
[ADR-0017](./0017-kamadak-exif-for-gps-extraction.md)).

Thumbnail generation is isolated behind a single new module, `src/server/thumbnail.rs`, mirroring
the isolation principle ADR-0017/[ADR-0019](./0019-tzf-rs-time-tz-for-timezone-lookup.md)
established for `kamadak-exif`/`tzf-rs`: it exposes only a narrow
`bytes + orientation -> Option<Vec<u8>>` surface. EXIF orientation itself is read by the
already-open `kamadak-exif` pass in `location.rs` (widened, not duplicated) rather than opening
the file's EXIF container a third time.

## Consequences

- One new pure-Rust dependency; no C/FFI, no impact on the single-binary deploy story.
- `image` does not read or apply EXIF orientation — `thumbnail.rs` must apply the rotation/flip
  itself from the `Orientation` tag before resizing, or a sideways/upside-down thumbnail results.
- Thumbnail generation is best-effort: a photo `image` cannot decode (corrupt bytes, or a format
  outside the 4 compiled in) simply gets no thumbnail (`thumbnail_key = NULL`), never fails the
  import — the same non-fatal pattern ADR-0017 established for EXIF extraction.
- Thumbnails are always re-encoded as JPEG (quality 80) at a 400px long edge regardless of source
  format — this project's photos are effectively always opaque camera JPEGs, so no
  format-preservation branch is needed.
